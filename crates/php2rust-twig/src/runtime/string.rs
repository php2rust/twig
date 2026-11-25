//! The multibyte string functions the core filters call (`mb_strlen()`,
//! `mb_substr()`, `mb_strtoupper()`, `mb_strtolower()`,
//! `mb_convert_case(MB_CASE_TITLE)`) and the character split of
//! `CoreExtension::splitIntoCharacters()` (`preg_match_all('/./us')`).
//!
//! Only UTF-8 is read: another charset is read as UTF-8 (PHP converts
//! with mbstring). An invalid byte counts as one character, as mbstring
//! counts it.
//!
//! Upstream: none

use crate::error::Error;

/// The characters of a string: each valid UTF-8 sequence, or each byte
/// of an invalid one.
pub fn chars(s: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        match std::str::from_utf8(rest) {
            Ok(valid) => {
                let mut start = 0;
                for (i, c) in valid.char_indices() {
                    let end = i + c.len_utf8();
                    out.push(&rest[start..end]);
                    start = end;
                }
                break;
            }
            Err(e) => {
                let valid_up_to = e.valid_up_to();
                let valid = std::str::from_utf8(&rest[..valid_up_to]).unwrap_or_default();
                let mut start = 0;
                for (i, c) in valid.char_indices() {
                    let end = i + c.len_utf8();
                    out.push(&rest[start..end]);
                    start = end;
                }
                let bad = e.error_len().unwrap_or(rest.len() - valid_up_to).max(1);
                for i in 0..bad {
                    out.push(&rest[valid_up_to + i..=valid_up_to + i]);
                }
                rest = &rest[valid_up_to + bad..];
            }
        }
    }
    out
}

/// `mb_strlen($s, $charset)`.
pub fn mb_strlen(s: &[u8], _charset: &[u8]) -> i64 {
    i64::try_from(chars(s).len()).unwrap_or(i64::MAX)
}

/// `mb_substr($s, $start, $length, $charset)`.
pub fn mb_substr(s: &[u8], start: i64, length: Option<i64>, _charset: &[u8]) -> Vec<u8> {
    let chars = chars(s);
    let len = i64::try_from(chars.len()).unwrap_or(i64::MAX);
    let mut from = if start < 0 {
        (len + start).max(0)
    } else {
        start.min(len)
    };
    let to = match length {
        None => len,
        Some(l) if l < 0 => (len + l).max(0),
        Some(l) => from.saturating_add(l).min(len),
    };
    if to < from {
        from = to;
    }
    let (from, to) = (
        usize::try_from(from).unwrap_or(0),
        usize::try_from(to).unwrap_or(0),
    );
    chars[from..to].concat()
}

fn map_valid(s: &[u8], f: impl Fn(&str) -> String) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in chars(s) {
        match std::str::from_utf8(c) {
            Ok(text) => out.extend_from_slice(f(text).as_bytes()),
            Err(_) => out.push(b'?'),
        }
    }
    out
}

/// `mb_strtoupper($s, $charset)`.
pub fn mb_strtoupper(s: &[u8], _charset: &[u8]) -> Vec<u8> {
    map_valid(s, str::to_uppercase)
}

/// `mb_strtolower($s, $charset)`.
pub fn mb_strtolower(s: &[u8], _charset: &[u8]) -> Vec<u8> {
    map_valid(s, str::to_lowercase)
}

fn is_case_ignorable(c: char) -> bool {
    matches!(
        c,
        '\'' | '.'
            | ':'
            | '^'
            | '`'
            | '\u{00AD}'
            | '\u{00B7}'
            | '\u{2018}'
            | '\u{2019}'
            | '\u{2024}'
            | '\u{2027}'
    ) || ('\u{0300}'..='\u{036F}').contains(&c)
}

fn is_cased(c: char) -> bool {
    c.is_lowercase() || c.is_uppercase()
}

/// `mb_convert_case($s, MB_CASE_TITLE, $charset)`: the first cased letter
/// of each word in title case, the others in lower case.
pub fn mb_title_case(s: &[u8], _charset: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut in_word = false;
    for c in chars(s) {
        let Ok(text) = std::str::from_utf8(c) else {
            out.push(b'?');
            in_word = false;
            continue;
        };
        let Some(ch) = text.chars().next() else {
            continue;
        };
        if is_cased(ch) {
            if in_word {
                out.extend_from_slice(text.to_lowercase().as_bytes());
            } else {
                out.extend_from_slice(title_case_char(ch).as_bytes());
                in_word = true;
            }
        } else {
            out.extend_from_slice(text.as_bytes());
            if !is_case_ignorable(ch) {
                in_word = false;
            }
        }
    }
    out
}

fn title_case_char(c: char) -> String {
    match c {
        // the title case forms of the digraphs
        '\u{01C4}'..='\u{01C6}' => "\u{01C5}".to_owned(),
        '\u{01C7}'..='\u{01C9}' => "\u{01C8}".to_owned(),
        '\u{01CA}'..='\u{01CC}' => "\u{01CB}".to_owned(),
        '\u{01F1}'..='\u{01F3}' => "\u{01F2}".to_owned(),
        'ß' => "Ss".to_owned(),
        _ => c.to_uppercase().collect(),
    }
}

/// `CoreExtension::splitIntoCharacters()`: the characters of a UTF-8
/// string, an empty string giving one empty character.
pub fn split_into_characters(s: &[u8], name: &str) -> Result<Vec<Vec<u8>>, Error> {
    if std::str::from_utf8(s).is_err() {
        return Err(Error::runtime(
            format!(
                "Unable to split the string passed to \"{name}\" into characters: Malformed UTF-8 characters, possibly incorrectly encoded."
            ),
            -1,
            None,
        ));
    }
    if s.is_empty() {
        return Ok(vec![Vec::new()]);
    }
    Ok(chars(s).into_iter().map(<[u8]>::to_vec).collect())
}
