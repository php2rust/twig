//! Port of Twig\Markup.php: a string that is safe to output as is.
//!
//! Upstream: twig/twig src/Markup.php

use std::sync::Arc;

use php_value::{PhpArray, PhpObject, PhpString, Thrown, Value};

/// `Twig\Markup`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Markup {
    content: PhpString,
    charset: PhpString,
}

impl Markup {
    pub const CLASS: &'static str = "Twig\\Markup";

    /// `new Markup($content, $charset)` for a content already cast to a
    /// string.
    pub fn new(content: impl Into<PhpString>, charset: impl Into<PhpString>) -> Self {
        Self {
            content: content.into(),
            charset: charset.into(),
        }
    }

    /// `new Markup(...)` as a value.
    pub fn value(content: impl Into<PhpString>, charset: impl Into<PhpString>) -> Value {
        Value::Object(Arc::new(Self::new(content, charset)))
    }

    /// `__toString()`.
    #[must_use]
    pub fn content(&self) -> &PhpString {
        &self.content
    }

    /// `getCharset()`.
    #[must_use]
    pub fn charset(&self) -> &PhpString {
        &self.charset
    }

    /// `count()`: `mb_strlen($content, $charset)`.
    #[must_use]
    pub fn count(&self) -> i64 {
        crate::runtime::string::mb_strlen(self.content.as_bytes(), self.charset.as_bytes())
    }

    /// Whether a value is a `Markup`.
    #[must_use]
    pub fn is(value: &Value) -> bool {
        matches!(value, Value::Object(o) if o.downcast_ref::<Self>().is_some())
    }
}

impl PhpObject for Markup {
    fn class(&self) -> &str {
        Self::CLASS
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn to_php_string(&self) -> Option<PhpString> {
        Some(self.content.clone())
    }

    fn json_serialize(&self) -> Option<Result<Value, Thrown>> {
        Some(Ok(Value::String(self.content.clone())))
    }

    fn instance_of(&self, class: &str) -> bool {
        matches!(
            class.trim_start_matches('\\'),
            "Countable" | "JsonSerializable" | "Stringable"
        )
    }

    fn methods(&self) -> Vec<String> {
        [
            "__construct",
            "__toString",
            "getCharset",
            "count",
            "jsonSerialize",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        Some(Ok(match name.to_ascii_lowercase().as_str() {
            "__tostring" | "jsonserialize" => Value::String(self.content.clone()),
            "getcharset" => Value::String(self.charset.clone()),
            "count" => Value::Int(self.count()),
            _ => return None,
        }))
    }

    fn php_count(&self) -> Option<Result<i64, Thrown>> {
        Some(Ok(self.count()))
    }
}
