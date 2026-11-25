//! Port of Twig\Runtime\EscaperRuntime.php: the `escape` filter's
//! strategies (`html`, `js`, `css`, `html_attr`, `html_attr_relaxed`,
//! `url`), custom escapers and safe classes.
//!
//! Charsets other than UTF-8 are not converted (PHP uses iconv): the
//! strategies work on the bytes as UTF-8.
//!
//! Upstream: twig/twig src/Runtime/EscaperRuntime.php

use std::{collections::HashMap, fmt, fmt::Write as _};

use php_value::{PhpString, Value};

use crate::{error::Error, markup::Markup};

/// A custom escaper: the string and the charset.
pub type EscaperFn = fn(&[u8], &[u8]) -> Result<Vec<u8>, Error>;

/// `EscaperRuntime`.
#[derive(Clone)]
pub struct EscaperRuntime {
    charset: Vec<u8>,
    escapers: Vec<(Vec<u8>, EscaperFn)>,
    safe_classes: HashMap<String, Vec<Vec<u8>>>,
}

impl fmt::Debug for EscaperRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EscaperRuntime").finish_non_exhaustive()
    }
}

const BUILT_IN: [&str; 6] = ["html", "js", "url", "css", "html_attr", "html_attr_relaxed"];

impl EscaperRuntime {
    #[must_use]
    pub fn new(charset: &[u8]) -> Self {
        Self {
            charset: charset.to_vec(),
            escapers: Vec::new(),
            safe_classes: HashMap::new(),
        }
    }

    /// `setEscaper()`.
    pub fn set_escaper(&mut self, strategy: &[u8], escaper: EscaperFn) {
        if let Some(slot) = self.escapers.iter_mut().find(|(s, _)| s == strategy) {
            slot.1 = escaper;
        } else {
            self.escapers.push((strategy.to_vec(), escaper));
        }
    }

    /// `array_keys(getEscapers())`.
    #[must_use]
    pub fn escaper_names(&self) -> Vec<Vec<u8>> {
        self.escapers.iter().map(|(s, _)| s.clone()).collect()
    }

    /// `addSafeClass()`.
    pub fn add_safe_class(&mut self, class: &str, strategies: &[&[u8]]) {
        let class = class.trim_start_matches('\\').to_owned();
        let entry = self.safe_classes.entry(class).or_default();
        entry.extend(strategies.iter().map(|s| s.to_vec()));
    }

    /// `setSafeClasses()`.
    pub fn set_safe_classes(&mut self, classes: &[(&str, &[&[u8]])]) {
        self.safe_classes.clear();
        for (class, strategies) in classes {
            self.add_safe_class(class, strategies);
        }
    }

    fn is_safe_class(&self, object: &php_value::Object, strategy: &[u8]) -> bool {
        let matches = |strategies: &Vec<Vec<u8>>| {
            strategies
                .iter()
                .any(|s| s.as_slice() == strategy || s.as_slice() == b"all")
        };
        if let Some(strategies) = self.safe_classes.get(object.class()) {
            return matches(strategies);
        }
        self.safe_classes
            .iter()
            .any(|(class, strategies)| object.instance_of(class) && matches(strategies))
    }

    /// `escape($string, $strategy, $charset, $autoescape)`.
    pub fn escape(
        &self,
        value: &Value,
        strategy: &[u8],
        charset: Option<&[u8]>,
        autoescape: bool,
    ) -> Result<Value, Error> {
        if autoescape && Markup::is(value) {
            return Ok(value.clone());
        }
        let string: PhpString = match value {
            Value::String(s) => s.clone(),
            Value::Object(o) if o.is_stringable() => {
                if autoescape && self.is_safe_class(o, strategy) {
                    return Ok(Value::String(o.to_php_string().unwrap_or_default()));
                }
                o.to_php_string().unwrap_or_default()
            }
            other if BUILT_IN.iter().any(|s| s.as_bytes() == strategy) => {
                // the input as is (which can be of any type)
                return Ok(other.clone());
            }
            other => crate::runtime::to_php_string(other)?,
        };
        if string.is_empty() {
            return Ok(Value::String(PhpString::default()));
        }
        let charset = match charset {
            Some(c) if !c.is_empty() => c,
            _ => &self.charset,
        };
        let s = string.as_bytes();
        let escaped = match strategy {
            b"html" => html(s),
            b"js" => js(valid_utf8(s)?),
            b"css" => css(valid_utf8(s)?),
            b"html_attr" => html_attr(valid_utf8(s)?, false),
            b"html_attr_relaxed" => html_attr(valid_utf8(s)?, true),
            b"url" => php_value::url::rawurlencode(s),
            _ => {
                if let Some((_, escaper)) = self.escapers.iter().find(|(name, _)| name == strategy)
                {
                    escaper(s, charset)?
                } else {
                    let mut valid: Vec<String> = BUILT_IN.iter().map(|s| (*s).to_owned()).collect();
                    valid.extend(
                        self.escapers
                            .iter()
                            .map(|(n, _)| String::from_utf8_lossy(n).into_owned()),
                    );
                    return Err(Error::runtime(
                        format!(
                            "Invalid escaping strategy \"{}\" (valid ones: \"{}\").",
                            String::from_utf8_lossy(strategy),
                            valid.join("\", \"")
                        ),
                        -1,
                        None,
                    ));
                }
            }
        };
        Ok(Value::String(PhpString::from(escaped)))
    }
}

fn valid_utf8(s: &[u8]) -> Result<&str, Error> {
    std::str::from_utf8(s).map_err(|_| {
        Error::runtime(
            "The string to escape is not a valid UTF-8 string.",
            -1,
            None,
        )
    })
}

/// `htmlspecialchars($string, ENT_QUOTES | ENT_SUBSTITUTE, 'UTF-8')`.
#[must_use]
pub fn html(s: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(s);
    let mut out = Vec::with_capacity(s.len());
    for b in text.bytes() {
        match b {
            b'&' => out.extend_from_slice(b"&amp;"),
            b'"' => out.extend_from_slice(b"&quot;"),
            b'\'' => out.extend_from_slice(b"&#039;"),
            b'<' => out.extend_from_slice(b"&lt;"),
            b'>' => out.extend_from_slice(b"&gt;"),
            b => out.push(b),
        }
    }
    out
}

fn js(s: &str) -> Vec<u8> {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, ',' | '.' | '_') {
            out.push(c);
            continue;
        }
        let short = match c {
            '\\' => Some("\\\\"),
            '/' => Some("\\/"),
            '\u{8}' => Some("\\b"),
            '\u{c}' => Some("\\f"),
            '\n' => Some("\\n"),
            '\r' => Some("\\r"),
            '\t' => Some("\\t"),
            _ => None,
        };
        if let Some(short) = short {
            out.push_str(short);
            continue;
        }
        let codepoint = u32::from(c);
        if codepoint < 0x10000 {
            let _ = write!(out, "\\u{codepoint:04X}");
        } else {
            let u = codepoint - 0x10000;
            let high = 0xD800 | (u >> 10);
            let low = 0xDC00 | (u & 0x3FF);
            let _ = write!(out, "\\u{high:04X}\\u{low:04X}");
        }
    }
    out.into_bytes()
}

fn css(s: &str) -> Vec<u8> {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            let _ = write!(out, "\\{:X} ", u32::from(c));
        }
    }
    out.into_bytes()
}

fn html_attr(s: &str, relaxed: bool) -> Vec<u8> {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let kept = c.is_ascii_alphanumeric()
            || matches!(c, ',' | '.' | '-' | '_')
            || (relaxed && matches!(c, ':' | '@' | '[' | ']'));
        if kept {
            out.push(c);
            continue;
        }
        let ord = u32::from(c);
        // the first byte of the UTF-8 sequence, as PHP reads ord($chr[0])
        let first = c
            .to_string()
            .as_bytes()
            .first()
            .copied()
            .map_or(0, u32::from);
        if (first <= 0x1F && !matches!(c, '\t' | '\n' | '\r')) || (0x7F..=0x9F).contains(&first) {
            out.push_str("&#xFFFD;");
            continue;
        }
        if c.len_utf8() == 1 {
            match ord {
                34 => out.push_str("&quot;"),
                38 => out.push_str("&amp;"),
                60 => out.push_str("&lt;"),
                62 => out.push_str("&gt;"),
                _ => {
                    let _ = write!(out, "&#x{ord:02X};");
                }
            }
            continue;
        }
        let _ = write!(out, "&#x{ord:04X};");
    }
    out.into_bytes()
}
