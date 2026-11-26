//! Port of the test classes of tests/IntegrationTest.php that the
//! fixtures' `--DATA--` sections create: `TwigTestFoo`, `ToStringStub`,
//! `CountableStub`, `IteratorAggregateStub`, `SimpleIteratorForTesting`
//! and `MagicCallStub`, as `php_value` objects. An object is immutable
//! here, so an iterator's position is not kept between two loops (each
//! loop rewinds, as PHP's `foreach` does).
//!
//! Upstream: twig/twig tests/IntegrationTest.php

use std::sync::Arc;

use php_value::{
    Key, PhpArray, PhpString,
    value::{PhpObject, Thrown, Value},
};

fn s(text: &str) -> Value {
    Value::String(PhpString::from(text.as_bytes().to_vec()))
}

fn arg(arguments: &[Value], i: usize) -> Value {
    arguments.get(i).cloned().unwrap_or(Value::Null)
}

/// `(string) $value` (an array gives `"Array"`, as PHP does with a
/// warning).
fn string_of(value: &Value) -> Vec<u8> {
    match value {
        Value::Array(_) => b"Array".to_vec(),
        other => other
            .to_php_string_or_stringable()
            .map(|s| s.as_bytes().to_vec())
            .unwrap_or_default(),
    }
}

fn pairs(array: &PhpArray) -> Vec<(Value, Value)> {
    array
        .iter()
        .map(|(k, v)| (k.to_value(), v.clone()))
        .collect()
}

/// `Twig\Tests\TwigTestFoo`.
#[derive(Debug, Clone)]
pub struct TwigTestFoo {
    pub position: i64,
    pub array: PhpArray,
}

impl Default for TwigTestFoo {
    fn default() -> Self {
        let mut array = PhpArray::new();
        let _ = array.push(Value::Int(1));
        let _ = array.push(Value::Int(2));
        Self { position: 0, array }
    }
}

impl TwigTestFoo {
    pub const CLASS: &'static str = "Twig\\Tests\\TwigTestFoo";
}

impl PhpObject for TwigTestFoo {
    fn class(&self) -> &'static str {
        Self::CLASS
    }

    fn properties(&self) -> PhpArray {
        let mut out = PhpArray::new();
        out.insert(Key::from_bytes(b"position"), Value::Int(self.position));
        out.insert(Key::from_bytes(b"array"), Value::Array(self.array.clone()));
        out
    }

    fn instance_of(&self, class: &str) -> bool {
        matches!(class.trim_start_matches('\\'), "Iterator" | "Traversable")
    }

    fn methods(&self) -> Vec<String> {
        [
            "bar",
            "getFoo",
            "getEmpty",
            "getNull",
            "getSelf",
            "is",
            "in",
            "not",
            "strToLower",
            "rewind",
            "current",
            "key",
            "next",
            "valid",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    fn call_method(&self, name: &str, arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        let result = match name.to_ascii_lowercase().as_str() {
            "bar" => {
                let mut out = b"bar".to_vec();
                let (p1, p2) = (arg(arguments, 0), arg(arguments, 1));
                if p1.is_truthy() {
                    out.push(b'_');
                    out.extend(string_of(&p1));
                }
                if p2.is_truthy() {
                    out.push(b'-');
                    out.extend(string_of(&p2));
                }
                Ok(Value::String(PhpString::from(out)))
            }
            "getfoo" => Ok(s("foo")),
            "getempty" => Ok(s("")),
            "getnull" | "rewind" | "next" => Ok(Value::Null),
            "getself" => Ok(Value::Object(Arc::new(self.clone()))),
            "is" => Ok(s("is")),
            "in" => Ok(s("in")),
            "not" => Ok(s("not")),
            "strtolower" => Ok(Value::String(PhpString::from(
                string_of(&arg(arguments, 0)).to_ascii_lowercase(),
            ))),
            "current" => Ok(self
                .array
                .get(&Key::Int(self.position))
                .cloned()
                .unwrap_or(Value::Null)),
            "key" => Ok(s("a")),
            "valid" => Ok(Value::Bool(
                self.array.get(&Key::Int(self.position)).is_some(),
            )),
            _ => return None,
        };
        Some(result)
    }

    /// `foreach`: `rewind()`, then `current()` under the key `'a'` while
    /// `isset($this->array[$this->position])`.
    fn traverse(&self) -> Option<Result<Vec<(Value, Value)>, Thrown>> {
        let mut out = Vec::new();
        let mut position = 0;
        while let Some(v) = self.array.get(&Key::Int(position)) {
            out.push((s("a"), v.clone()));
            position += 1;
        }
        Some(Ok(out))
    }
}

/// `Twig\Tests\ToStringStub`.
#[derive(Debug, Clone)]
pub struct ToStringStub {
    pub string: PhpString,
}

impl PhpObject for ToStringStub {
    fn class(&self) -> &'static str {
        "Twig\\Tests\\ToStringStub"
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn instance_of(&self, class: &str) -> bool {
        class.trim_start_matches('\\') == "Stringable"
    }

    fn methods(&self) -> Vec<String> {
        vec!["__construct".to_owned(), "__toString".to_owned()]
    }

    fn to_php_string(&self) -> Option<PhpString> {
        Some(self.string.clone())
    }
}

/// `Twig\Tests\CountableStub`: its `__toString()` throws, so it is not
/// stringable here.
#[derive(Debug, Clone)]
pub struct CountableStub {
    pub count: i64,
}

impl PhpObject for CountableStub {
    fn class(&self) -> &'static str {
        "Twig\\Tests\\CountableStub"
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn instance_of(&self, class: &str) -> bool {
        matches!(class.trim_start_matches('\\'), "Countable" | "Stringable")
    }

    fn methods(&self) -> Vec<String> {
        vec![
            "__construct".to_owned(),
            "count".to_owned(),
            "__toString".to_owned(),
        ]
    }

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        match name.to_ascii_lowercase().as_str() {
            "count" => Some(Ok(Value::Int(self.count))),
            "__tostring" => Some(Err(Thrown::new(
                "Exception",
                PhpString::from(b"__toString shall not be called on \\Countables.".to_vec()),
                0,
            ))),
            _ => None,
        }
    }

    fn php_count(&self) -> Option<Result<i64, Thrown>> {
        Some(Ok(self.count))
    }
}

/// `Twig\Tests\IteratorAggregateStub`.
#[derive(Debug, Clone)]
pub struct IteratorAggregateStub {
    pub data: PhpArray,
}

impl PhpObject for IteratorAggregateStub {
    fn class(&self) -> &'static str {
        "Twig\\Tests\\IteratorAggregateStub"
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn instance_of(&self, class: &str) -> bool {
        matches!(
            class.trim_start_matches('\\'),
            "IteratorAggregate" | "Traversable"
        )
    }

    fn methods(&self) -> Vec<String> {
        vec!["__construct".to_owned(), "getIterator".to_owned()]
    }

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        (name.eq_ignore_ascii_case("getIterator")).then(|| {
            Ok(Value::Object(Arc::new(
                php_value::object::ArrayIterator::new(self.data.clone()),
            )))
        })
    }

    fn traverse(&self) -> Option<Result<Vec<(Value, Value)>, Thrown>> {
        Some(Ok(pairs(&self.data)))
    }
}

/// `Twig\Tests\SimpleIteratorForTesting`: the keys 0 to 6 as values; its
/// string is 17 `X` (`iterator_count($this) + 10`).
#[derive(Debug, Clone, Default)]
pub struct SimpleIteratorForTesting;

impl PhpObject for SimpleIteratorForTesting {
    fn class(&self) -> &'static str {
        "Twig\\Tests\\SimpleIteratorForTesting"
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn instance_of(&self, class: &str) -> bool {
        matches!(
            class.trim_start_matches('\\'),
            "Iterator" | "Traversable" | "Stringable"
        )
    }

    fn methods(&self) -> Vec<String> {
        ["current", "next", "key", "valid", "rewind", "__toString"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    fn to_php_string(&self) -> Option<PhpString> {
        Some(PhpString::from(vec![b'X'; 17]))
    }

    fn traverse(&self) -> Option<Result<Vec<(Value, Value)>, Thrown>> {
        Some(Ok((0..7).map(|i| (Value::Int(i), Value::Int(i))).collect()))
    }
}

/// `Twig\Tests\MagicCallStub`: `__call()` throws.
#[derive(Debug, Clone, Default)]
pub struct MagicCallStub;

impl PhpObject for MagicCallStub {
    fn class(&self) -> &'static str {
        "Twig\\Tests\\MagicCallStub"
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn methods(&self) -> Vec<String> {
        vec!["__call".to_owned()]
    }

    fn call_method(&self, _name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        Some(Err(Thrown::new(
            "Exception",
            PhpString::from(b"__call shall not be called.".to_vec()),
            0,
        )))
    }
}
