//! Port of Twig\Error\Error.php and its subclasses SyntaxError.php,
//! LoaderError.php and RuntimeError.php.
//!
//! One type with a [`ErrorKind`] stands for the class hierarchy. The
//! message is rebuilt from the raw message, the template name, line and
//! column each time one of them changes, as `updateRepr()` does. Not
//! ported: `guess()` (it reads PHP's backtrace and the compiled PHP
//! template's debug info; compiled Rust templates set the line when they
//! raise the error) and the PHP file/line of the exception.
//!
//! Upstream: twig/twig src/Error/Error.php src/Error/SyntaxError.php src/Error/LoaderError.php src/Error/RuntimeError.php

use std::{fmt, sync::Arc};

use crate::source::Source;

/// The PHP class of an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// `Twig\Error\Error`.
    Error,
    /// `Twig\Error\SyntaxError`: lexing or parsing failed.
    Syntax,
    /// `Twig\Error\LoaderError`: a template could not be found or loaded.
    Loader,
    /// `Twig\Error\RuntimeError`: rendering failed.
    Runtime,
    /// PHP's `\LogicException`: a misuse of the API (upstream throws it
    /// for states a template cannot reach).
    Logic,
}

impl ErrorKind {
    /// The fully qualified PHP class name.
    pub fn class(self) -> &'static str {
        match self {
            Self::Error => "Twig\\Error\\Error",
            Self::Syntax => "Twig\\Error\\SyntaxError",
            Self::Loader => "Twig\\Error\\LoaderError",
            Self::Runtime => "Twig\\Error\\RuntimeError",
            Self::Logic => "LogicException",
        }
    }
}

/// A Twig error: `Twig\Error\Error` or one of its subclasses.
#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    kind: ErrorKind,
    raw_message: Vec<u8>,
    lineno: i64,
    columnno: Option<usize>,
    source: Option<Arc<Source>>,
    message: Vec<u8>,
}

impl Error {
    /// `new <Class>($message, $lineno, $source)`; `lineno` -1 means unknown.
    pub fn new(
        kind: ErrorKind,
        message: impl Into<Vec<u8>>,
        lineno: i64,
        source: Option<Arc<Source>>,
    ) -> Self {
        let mut error = Self {
            kind,
            raw_message: message.into(),
            lineno,
            columnno: None,
            source,
            message: Vec::new(),
        };
        error.update_repr();
        error
    }

    /// `new SyntaxError($message, $lineno, $source)`.
    pub fn syntax(message: impl Into<Vec<u8>>, lineno: i64, source: Option<Arc<Source>>) -> Self {
        Self::new(ErrorKind::Syntax, message, lineno, source)
    }

    /// A `\LogicException`: no template location, the message as is.
    pub fn logic(message: impl Into<Vec<u8>>) -> Self {
        Self::new(ErrorKind::Logic, message, -1, None)
    }

    /// The same error with the `columnno` constructor argument.
    #[must_use]
    pub fn with_column(mut self, columnno: Option<usize>) -> Self {
        self.set_template_column(columnno);
        self
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// `getMessage()`.
    pub fn message(&self) -> &[u8] {
        &self.message
    }

    /// `getRawMessage()`.
    pub fn raw_message(&self) -> &[u8] {
        &self.raw_message
    }

    /// `getTemplateLine()`.
    pub fn template_line(&self) -> i64 {
        self.lineno
    }

    /// `setTemplateLine()`.
    pub fn set_template_line(&mut self, lineno: i64) {
        self.lineno = lineno;
        self.update_repr();
    }

    /// `getTemplateColumn()`.
    pub fn template_column(&self) -> Option<usize> {
        self.columnno
    }

    /// `setTemplateColumn()`.
    pub fn set_template_column(&mut self, columnno: Option<usize>) {
        self.columnno = columnno;
        self.update_repr();
    }

    /// `getSourceContext()`.
    pub fn source_context(&self) -> Option<&Arc<Source>> {
        self.source.as_ref()
    }

    /// `setSourceContext()`.
    pub fn set_source_context(&mut self, source: Option<Arc<Source>>) {
        self.source = source;
        self.update_repr();
    }

    /// `appendMessage()`.
    pub fn append_message(&mut self, raw_message: &[u8]) {
        self.raw_message.extend_from_slice(raw_message);
        self.update_repr();
    }

    /// `SyntaxError::addSuggestions()`: appends ` Did you mean "a", "b"?`
    /// with the items at a Levenshtein distance of at most a third of the
    /// name's length, or containing the name, closest first.
    pub fn add_suggestions(&mut self, name: &[u8], items: &[&[u8]]) {
        let mut alternatives: Vec<(&[u8], usize)> = Vec::new();
        for &item in items {
            let lev = php_value::string::levenshtein(name, item);
            #[allow(clippy::cast_precision_loss)]
            let close = lev as f64 <= name.len() as f64 / 3.0;
            if close || contains(item, name) {
                // array keys: a repeated item keeps its first position
                if !alternatives.iter().any(|(seen, _)| *seen == item) {
                    alternatives.push((item, lev));
                }
            }
        }
        if alternatives.is_empty() {
            return;
        }
        // asort() is stable since PHP 8.0
        alternatives.sort_by_key(|&(_, lev)| lev);
        let mut text = b" Did you mean \"".to_vec();
        for (i, (item, _)) in alternatives.iter().enumerate() {
            if i > 0 {
                text.extend_from_slice(b"\", \"");
            }
            text.extend_from_slice(item);
        }
        text.extend_from_slice(b"\"?");
        self.append_message(&text);
    }

    fn update_repr(&mut self) {
        let mut message = self.raw_message.clone();
        let punctuation = match message.last() {
            Some(&last @ (b'.' | b'?')) => {
                message.pop();
                Some(last)
            }
            _ => None,
        };
        if let Some(source) = &self.source
            && !source.name().is_empty()
        {
            message.extend_from_slice(format!(" in \"{}\"", source.name()).as_bytes());
        }
        if self.lineno > 0 {
            message.extend_from_slice(format!(" at line {}", self.lineno).as_bytes());
            if let Some(column) = self.columnno {
                message.extend_from_slice(format!(" column {column}").as_bytes());
            }
        }
        if let Some(punctuation) = punctuation {
            message.push(punctuation);
        }
        self.message = message;
    }
}

/// `str_contains($haystack, $needle)`; the empty needle is in every string.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || haystack.windows(needle.len()).any(|w| w == needle)
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.message))
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> Arc<Source> {
        Arc::new(Source::new("", "index"))
    }

    #[test]
    fn message_moves_the_final_punctuation_after_the_location() {
        let e = Error::syntax("Unclosed \"(\".", 3, Some(source()));
        assert_eq!(e.message(), b"Unclosed \"(\" in \"index\" at line 3.");
        let e = Error::syntax("Did it fail?", 3, Some(source())).with_column(Some(7));
        assert_eq!(e.message(), b"Did it fail in \"index\" at line 3 column 7?");
        let e = Error::syntax("No line", -1, None);
        assert_eq!(e.message(), b"No line");
    }

    #[test]
    fn suggestions_are_sorted_by_distance() {
        let mut e = Error::syntax("Unknown \"lenght\" filter.", 1, Some(source()));
        e.add_suggestions(b"lenght", &[b"length", b"lower", b"lenghtx", b"upper"]);
        assert_eq!(
            e.raw_message(),
            b"Unknown \"lenght\" filter. Did you mean \"lenghtx\", \"length\"?"
        );
    }
}
