//! Port of Twig\Token.php.
//!
//! A token's value is bytes, or a number for `NUMBER_TYPE` (PHP's
//! `0 + $string`: an int, or a float when it has a fraction or an
//! exponent or does not fit). `test($type, $values)` is split by the shape
//! of `$values`: [`Token::test`] (null), [`Token::test_value`] (one value,
//! compared with PHP's `==`) and [`Token::test_values`] (an array,
//! compared strictly). `test('name')` is [`Token::test_name`].
//!
//! Upstream: twig/twig src/Token.php

use std::fmt;

use php_value::{Value, compare::loose_equals, numeric::Number, string::PhpString};
use symfony_deprecation_contracts::trigger_deprecation;

/// `Token::*_TYPE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenType {
    Eof,
    Text,
    BlockStart,
    VarStart,
    BlockEnd,
    VarEnd,
    Name,
    Number,
    String,
    Operator,
    Punctuation,
    InterpolationStart,
    InterpolationEnd,
    /// Deprecated since Twig 3.21: `=>` is an operator.
    Arrow,
    /// Deprecated since Twig 3.21: `...` is an operator.
    Spread,
}

impl TokenType {
    /// The PHP constant's value.
    pub fn code(self) -> i64 {
        match self {
            Self::Eof => -1,
            Self::Text => 0,
            Self::BlockStart => 1,
            Self::VarStart => 2,
            Self::BlockEnd => 3,
            Self::VarEnd => 4,
            Self::Name => 5,
            Self::Number => 6,
            Self::String => 7,
            Self::Operator => 8,
            Self::Punctuation => 9,
            Self::InterpolationStart => 10,
            Self::InterpolationEnd => 11,
            Self::Arrow => 12,
            Self::Spread => 13,
        }
    }

    /// `Token::typeToString($type, $short)`.
    pub fn to_php_string(self, short: bool) -> String {
        let name = match self {
            Self::Eof => "EOF_TYPE",
            Self::Text => "TEXT_TYPE",
            Self::BlockStart => "BLOCK_START_TYPE",
            Self::VarStart => "VAR_START_TYPE",
            Self::BlockEnd => "BLOCK_END_TYPE",
            Self::VarEnd => "VAR_END_TYPE",
            Self::Name => "NAME_TYPE",
            Self::Number => "NUMBER_TYPE",
            Self::String => "STRING_TYPE",
            Self::Operator => "OPERATOR_TYPE",
            Self::Punctuation => "PUNCTUATION_TYPE",
            Self::InterpolationStart => "INTERPOLATION_START_TYPE",
            Self::InterpolationEnd => "INTERPOLATION_END_TYPE",
            Self::Arrow => "ARROW_TYPE",
            Self::Spread => "SPREAD_TYPE",
        };
        if short {
            name.to_owned()
        } else {
            format!("Twig\\Token::{name}")
        }
    }

    /// `Token::typeToEnglish($type)`.
    pub fn to_english(self) -> &'static str {
        match self {
            Self::Eof => "end of template",
            Self::Text => "text",
            Self::BlockStart => "begin of statement block",
            Self::VarStart => "begin of print statement",
            Self::BlockEnd => "end of statement block",
            Self::VarEnd => "end of print statement",
            Self::Name => "name",
            Self::Number => "number",
            Self::String => "string",
            Self::Operator => "operator",
            Self::Punctuation => "punctuation",
            Self::InterpolationStart => "begin of string interpolation",
            Self::InterpolationEnd => "end of string interpolation",
            Self::Arrow => "arrow function",
            Self::Spread => "spread operator",
        }
    }
}

/// A token's value.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenValue {
    Bytes(Vec<u8>),
    Number(Number),
}

impl TokenValue {
    /// `(string) $value`.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::Bytes(b) => b.clone(),
            Self::Number(n) => number_to_bytes(*n),
        }
    }

    /// The bytes of a string-valued token; `None` for a number.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bytes(b) => Some(b),
            Self::Number(_) => None,
        }
    }

    /// PHP truthiness (`'' ` and `'0'` and `0` are false).
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Bytes(b) => php_value::string::truthy(b),
            Self::Number(Number::Int(i)) => *i != 0,
            Self::Number(Number::Float(f)) => *f != 0.0,
        }
    }

    fn to_value(&self) -> Value {
        match self {
            Self::Bytes(b) => Value::String(PhpString::new(b.as_slice())),
            Self::Number(Number::Int(i)) => Value::Int(*i),
            Self::Number(Number::Float(f)) => Value::Float(*f),
        }
    }

    /// `in_array($this->value, $values, true)` for one element.
    fn strict_equals(&self, value: &[u8]) -> bool {
        matches!(self, Self::Bytes(b) if b == value)
    }
}

pub(crate) fn number_to_bytes(n: Number) -> Vec<u8> {
    match n {
        Number::Int(i) => i.to_string().into_bytes(),
        Number::Float(f) => php_value::numeric::float_to_string(f).into_bytes(),
    }
}

/// The values that are now operators but were punctuation before 3.21.
const FORMER_PUNCTUATION: &[&[u8]] = &[b"(", b"[", b"|", b".", b"?", b"?:"];

fn is_former_punctuation(value: &[u8]) -> bool {
    FORMER_PUNCTUATION.contains(&value)
}

/// What `test()` compares the value with.
#[derive(Clone, Copy)]
enum Values<'a> {
    Any,
    One(&'a [u8]),
    Many(&'a [&'a [u8]]),
}

impl Values<'_> {
    /// Whether any of the values passes `f`.
    fn any(self, f: impl Fn(&[u8]) -> bool) -> bool {
        match self {
            Self::Any => false,
            Self::One(v) => f(v),
            Self::Many(vs) => vs.iter().any(|v| f(v)),
        }
    }

    /// PHP truthiness of `$values` (a non-empty string other than `'0'`,
    /// or a non-empty array).
    fn is_truthy(self) -> bool {
        match self {
            Self::Any => false,
            Self::One(v) => php_value::string::truthy(v),
            Self::Many(v) => !v.is_empty(),
        }
    }
}

/// A token: type, value, line, byte offset in the source and the
/// documentation comments (`{## ... ##}`, `## ...`) before it.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    ty: TokenType,
    value: TokenValue,
    lineno: usize,
    offset: Option<usize>,
    documentation: Option<Vec<u8>>,
}

impl Token {
    /// `new Token($type, $value, $lineno)`.
    pub fn new(ty: TokenType, value: impl Into<Vec<u8>>, lineno: usize) -> Self {
        Self::with_value(ty, TokenValue::Bytes(value.into()), lineno, None, None)
    }

    /// `new Token($type, $value, $lineno, $offset, $documentation)`.
    pub fn with_value(
        ty: TokenType,
        value: TokenValue,
        lineno: usize,
        offset: Option<usize>,
        documentation: Option<Vec<u8>>,
    ) -> Self {
        if ty == TokenType::Arrow {
            trigger_deprecation(
                b"twig/twig",
                b"3.21",
                b"The \"12\" token type is deprecated, \"arrow\" is now an operator.",
            );
        }
        if ty == TokenType::Spread {
            trigger_deprecation(
                b"twig/twig",
                b"3.21",
                b"The \"13\" token type is deprecated, \"spread\" is now an operator.",
            );
        }
        Self {
            ty,
            value,
            lineno,
            offset,
            documentation,
        }
    }

    /// `test($type)`: the type matches.
    pub fn test(&self, ty: TokenType) -> bool {
        self.test_impl(ty, Values::Any)
    }

    /// `test($type, $value)`: the type matches and the value `==` it.
    pub fn test_value(&self, ty: TokenType, value: &[u8]) -> bool {
        self.test_impl(ty, Values::One(value))
    }

    /// `test($type, [$value, ...])`: the type matches and the value is
    /// one of them (strictly).
    pub fn test_values(&self, ty: TokenType, values: &[&[u8]]) -> bool {
        self.test_impl(ty, Values::Many(values))
    }

    /// `test('name')`: a name token with this value.
    pub fn test_name(&self, value: &[u8]) -> bool {
        self.test_value(TokenType::Name, value)
    }

    /// `test(['a', 'b'])`: a name token with one of these values.
    pub fn test_names(&self, values: &[&[u8]]) -> bool {
        self.test_values(TokenType::Name, values)
    }

    fn test_impl(&self, ty: TokenType, values: Values<'_>) -> bool {
        if ty == TokenType::Arrow {
            trigger_deprecation(
                b"twig/twig",
                b"3.21",
                b"The \"arrow function\" token type is deprecated, \"arrow\" is now an operator.",
            );
            return self.ty == TokenType::Operator && self.value.strict_equals(b"=>");
        }
        if ty == TokenType::Spread {
            trigger_deprecation(
                b"twig/twig",
                b"3.21",
                b"The \"spread operator\" token type is deprecated, \"spread\" is now an operator.",
            );
            return self.ty == TokenType::Operator && self.value.strict_equals(b"...");
        }

        let mut type_matches = self.ty == ty;
        if type_matches
            && ty == TokenType::Punctuation
            && self.value.as_bytes().is_some_and(is_former_punctuation)
            && values.is_truthy()
            && values.any(is_former_punctuation)
        {
            let message = format!(
                "The \"{}\" token is now an \"{}\" token instead of a \"{}\" one.",
                String::from_utf8_lossy(&self.value.to_bytes()),
                TokenType::Operator.to_english(),
                self.to_english()
            );
            trigger_deprecation(b"twig/twig", b"3.21", message.as_bytes());
        }
        if !type_matches && ty == TokenType::Operator && self.ty == TokenType::Punctuation {
            type_matches = if values.is_truthy() {
                values.any(is_former_punctuation)
            } else {
                true
            };
        }

        type_matches
            && match values {
                Values::Any => true,
                Values::Many(vs) => vs.iter().any(|v| self.value.strict_equals(v)),
                Values::One(v) => {
                    loose_equals(&self.value.to_value(), &Value::String(PhpString::new(v)))
                }
            }
    }

    /// `getLine()`.
    pub fn line(&self) -> usize {
        self.lineno
    }

    /// `getOffset()`: the byte offset in the source, `None` for a token
    /// that was not lexed from a source.
    pub fn offset(&self) -> Option<usize> {
        self.offset
    }

    /// `getDocumentation()`.
    pub fn documentation(&self) -> Option<&[u8]> {
        self.documentation.as_deref()
    }

    /// The type (`getType()`, deprecated upstream; the Rust parser needs
    /// it to match on tokens).
    pub fn token_type(&self) -> TokenType {
        self.ty
    }

    /// `getValue()`.
    pub fn value(&self) -> &TokenValue {
        &self.value
    }

    /// `getValue()` of a string-valued token, or the number as text.
    pub fn value_bytes(&self) -> Vec<u8> {
        self.value.to_bytes()
    }

    /// `toEnglish()`.
    pub fn to_english(&self) -> &'static str {
        self.ty.to_english()
    }
}

/// `__toString()`: `NAME_TYPE(foo)`.
impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}({})",
            self.ty.to_php_string(true),
            String::from_utf8_lossy(&self.value.to_bytes())
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_test_accepts_former_punctuation() {
        let token = Token::new(TokenType::Punctuation, "(", 1);
        assert!(token.test(TokenType::Operator));
        assert!(token.test_value(TokenType::Operator, b"("));
        assert!(!token.test_value(TokenType::Operator, b"+"));
        let (matched, deprecations) = symfony_deprecation_contracts::collect(|| {
            token.test_value(TokenType::Punctuation, b"(")
        });
        assert!(matched);
        assert_eq!(
            deprecations[0].message,
            b"Since twig/twig 3.21: The \"(\" token is now an \"operator\" token instead of a \"punctuation\" one."
        );
    }

    #[test]
    fn single_value_is_compared_loosely() {
        let token = Token::with_value(
            TokenType::Number,
            TokenValue::Number(Number::Int(1)),
            1,
            None,
            None,
        );
        assert!(token.test_value(TokenType::Number, b"1"));
        assert!(!token.test_values(TokenType::Number, &[b"1"]));
        assert_eq!(token.to_string(), "NUMBER_TYPE(1)");
    }
}
