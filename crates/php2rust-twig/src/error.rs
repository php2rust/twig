//! Port of Twig\Error\Error.php and its subclasses SyntaxError.php,
//! LoaderError.php and RuntimeError.php.
//!
//! One type with a [`ErrorKind`] stands for the class hierarchy. The
//! message is rebuilt from the raw message, the template name, line and
//! column each time one of them changes, as `updateRepr()` does. Not
//! ported: `guess()` (it reads PHP's backtrace and the compiled PHP
//! template's debug info; compiled Rust templates keep the line of the
//! last debug info they passed and give it to the error, see
//! [`Error::guess_line`]) and the PHP file/line of the exception.
//!
//! Exceptions that are not Twig errors (a `\LogicException`, or what a
//! PHP method or operator threw, `ErrorKind::Exception` with its class)
//! use the same type; their message is not decorated.
//!
//! The sandbox's `Twig\Sandbox\SecurityError` and its `SecurityNotAllowed*`
//! subclasses are kinds too; the name (and class) they carry is
//! [`Error::security_name`] (and [`Error::security_class_name`]).
//!
//! Upstream: twig/twig src/Error/Error.php src/Error/SyntaxError.php src/Error/LoaderError.php src/Error/RuntimeError.php src/Sandbox/SecurityError.php src/Sandbox/SecurityNotAllowedTagError.php src/Sandbox/SecurityNotAllowedFilterError.php src/Sandbox/SecurityNotAllowedFunctionError.php src/Sandbox/SecurityNotAllowedTestError.php src/Sandbox/SecurityNotAllowedMethodError.php src/Sandbox/SecurityNotAllowedPropertyError.php

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
    /// `Twig\Sandbox\SecurityError`.
    Security,
    /// `Twig\Sandbox\SecurityNotAllowedTagError`.
    SecurityNotAllowedTag,
    /// `Twig\Sandbox\SecurityNotAllowedFilterError`.
    SecurityNotAllowedFilter,
    /// `Twig\Sandbox\SecurityNotAllowedFunctionError`.
    SecurityNotAllowedFunction,
    /// `Twig\Sandbox\SecurityNotAllowedTestError`.
    SecurityNotAllowedTest,
    /// `Twig\Sandbox\SecurityNotAllowedMethodError`.
    SecurityNotAllowedMethod,
    /// `Twig\Sandbox\SecurityNotAllowedPropertyError`.
    SecurityNotAllowedProperty,
    /// PHP's `\LogicException`: a misuse of the API (upstream throws it
    /// for states a template cannot reach).
    Logic,
    /// PHP's `\InvalidArgumentException`: an invalid option value.
    InvalidArgument,
    /// Any other PHP exception or error (`TypeError`, `\Exception`, ...):
    /// the class is [`Error::class`].
    Exception,
}

impl ErrorKind {
    /// The fully qualified PHP class name.
    #[must_use]
    pub fn class(self) -> &'static str {
        match self {
            Self::Error => "Twig\\Error\\Error",
            Self::Syntax => "Twig\\Error\\SyntaxError",
            Self::Loader => "Twig\\Error\\LoaderError",
            Self::Runtime => "Twig\\Error\\RuntimeError",
            Self::Security => "Twig\\Sandbox\\SecurityError",
            Self::SecurityNotAllowedTag => "Twig\\Sandbox\\SecurityNotAllowedTagError",
            Self::SecurityNotAllowedFilter => "Twig\\Sandbox\\SecurityNotAllowedFilterError",
            Self::SecurityNotAllowedFunction => "Twig\\Sandbox\\SecurityNotAllowedFunctionError",
            Self::SecurityNotAllowedTest => "Twig\\Sandbox\\SecurityNotAllowedTestError",
            Self::SecurityNotAllowedMethod => "Twig\\Sandbox\\SecurityNotAllowedMethodError",
            Self::SecurityNotAllowedProperty => "Twig\\Sandbox\\SecurityNotAllowedPropertyError",
            Self::Logic => "LogicException",
            Self::InvalidArgument => "InvalidArgumentException",
            Self::Exception => "Exception",
        }
    }

    /// Whether the class is `Twig\Error\Error` or a subclass.
    #[must_use]
    pub fn is_twig_error(self) -> bool {
        matches!(
            self,
            Self::Error | Self::Syntax | Self::Loader | Self::Runtime
        ) || self.is_security_error()
    }

    /// Whether the class is `Twig\Sandbox\SecurityError` or a subclass.
    #[must_use]
    pub fn is_security_error(self) -> bool {
        matches!(
            self,
            Self::Security
                | Self::SecurityNotAllowedTag
                | Self::SecurityNotAllowedFilter
                | Self::SecurityNotAllowedFunction
                | Self::SecurityNotAllowedTest
                | Self::SecurityNotAllowedMethod
                | Self::SecurityNotAllowedProperty
        )
    }
}

/// A Twig error: `Twig\Error\Error` or one of its subclasses.
#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    kind: ErrorKind,
    /// The class of an `ErrorKind::Exception`.
    class: Option<String>,
    raw_message: Vec<u8>,
    lineno: i64,
    columnno: Option<usize>,
    source: Option<Arc<Source>>,
    message: Vec<u8>,
    /// What a `SecurityNotAllowed*Error` refused.
    security: Option<Box<Refused>>,
}

/// The name (`getTagName()`, `getFilterName()`, ..., `getMethodName()`,
/// `getPropertyName()`) and class (`getClassName()`) of a
/// `SecurityNotAllowed*Error`.
#[derive(Debug, Clone, PartialEq)]
struct Refused {
    name: Vec<u8>,
    class_name: Option<String>,
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
            class: None,
            raw_message: message.into(),
            lineno,
            columnno: None,
            source,
            message: Vec::new(),
            security: None,
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

    /// An `\InvalidArgumentException`: no template location, the message as is.
    pub fn invalid_argument(message: impl Into<Vec<u8>>) -> Self {
        Self::new(ErrorKind::InvalidArgument, message, -1, None)
    }

    /// `new RuntimeError($message, $lineno, $source)`.
    pub fn runtime(message: impl Into<Vec<u8>>, lineno: i64, source: Option<Arc<Source>>) -> Self {
        Self::new(ErrorKind::Runtime, message, lineno, source)
    }

    /// `new LoaderError($message, $lineno, $source)`.
    pub fn loader(message: impl Into<Vec<u8>>, lineno: i64, source: Option<Arc<Source>>) -> Self {
        Self::new(ErrorKind::Loader, message, lineno, source)
    }

    /// A PHP exception of another class (`TypeError`, `\Exception`, ...).
    pub fn exception(class: impl Into<String>, message: impl Into<Vec<u8>>) -> Self {
        let mut error = Self::new(ErrorKind::Exception, message, -1, None);
        error.class = Some(class.into());
        error
    }

    /// `new SecurityNotAllowed<Kind>Error($message, $name)` for a tag,
    /// filter, function or test kind.
    pub fn security_not_allowed(kind: ErrorKind, message: impl Into<Vec<u8>>, name: &[u8]) -> Self {
        let mut error = Self::new(kind, message, -1, None);
        error.security = Some(Box::new(Refused {
            name: name.to_vec(),
            class_name: None,
        }));
        error
    }

    /// `new SecurityNotAllowedMethodError($message, $className, $methodName)`
    /// or the property error (`kind`).
    pub fn security_not_allowed_member(
        kind: ErrorKind,
        message: impl Into<Vec<u8>>,
        class_name: &str,
        name: &[u8],
    ) -> Self {
        let mut error = Self::new(kind, message, -1, None);
        error.security = Some(Box::new(Refused {
            name: name.to_vec(),
            class_name: Some(class_name.to_owned()),
        }));
        error
    }

    /// The name a `SecurityNotAllowed*Error` carries (`getTagName()`,
    /// `getFilterName()`, `getFunctionName()`, `getTestName()`,
    /// `getMethodName()`, `getPropertyName()`).
    #[must_use]
    pub fn security_name(&self) -> Option<&[u8]> {
        self.security.as_ref().map(|r| r.name.as_slice())
    }

    /// `getClassName()` of a `SecurityNotAllowedMethodError` or
    /// `SecurityNotAllowedPropertyError`.
    #[must_use]
    pub fn security_class_name(&self) -> Option<&str> {
        self.security.as_ref().and_then(|r| r.class_name.as_deref())
    }

    /// The PHP class (`$e::class`).
    #[must_use]
    pub fn class(&self) -> &str {
        match (&self.class, self.kind) {
            (Some(class), ErrorKind::Exception) => class,
            (_, kind) => kind.class(),
        }
    }

    /// `Template::handleException()`: a Twig error gets the template's
    /// source when it has none, and `line` when its line is unknown (PHP
    /// guesses it from the debug info of the code that threw); any other
    /// exception becomes a `RuntimeError` "An exception has been thrown
    /// during the rendering of a template".
    #[must_use]
    pub fn guess_line(mut self, line: i64, source: &Arc<Source>) -> Self {
        if self.kind.is_twig_error() {
            if self.source.is_none() {
                self.source = Some(Arc::clone(source));
            }
            if self.lineno == -1 {
                self.lineno = line;
            }
            self.update_repr();
            return self;
        }
        let mut message =
            b"An exception has been thrown during the rendering of a template (\"".to_vec();
        message.extend_from_slice(&self.message);
        message.extend_from_slice(b"\").");
        Self::new(ErrorKind::Runtime, message, line, Some(Arc::clone(source)))
    }

    /// The same error with the `columnno` constructor argument.
    #[must_use]
    pub fn with_column(mut self, columnno: Option<usize>) -> Self {
        self.set_template_column(columnno);
        self
    }

    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// `getMessage()`.
    #[must_use]
    pub fn message(&self) -> &[u8] {
        &self.message
    }

    /// `getRawMessage()`.
    #[must_use]
    pub fn raw_message(&self) -> &[u8] {
        &self.raw_message
    }

    /// `getTemplateLine()`.
    #[must_use]
    pub fn template_line(&self) -> i64 {
        self.lineno
    }

    /// `setTemplateLine()`.
    pub fn set_template_line(&mut self, lineno: i64) {
        self.lineno = lineno;
        self.update_repr();
    }

    /// `getTemplateColumn()`.
    #[must_use]
    pub fn template_column(&self) -> Option<usize> {
        self.columnno
    }

    /// `setTemplateColumn()`.
    pub fn set_template_column(&mut self, columnno: Option<usize>) {
        self.columnno = columnno;
        self.update_repr();
    }

    /// `getSourceContext()`.
    #[must_use]
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
        if !self.kind.is_twig_error() {
            self.message = message;
            return;
        }
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

impl From<php_value::Thrown> for Error {
    fn from(thrown: php_value::Thrown) -> Self {
        Self::exception(thrown.class, thrown.message.as_bytes().to_vec())
    }
}

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
