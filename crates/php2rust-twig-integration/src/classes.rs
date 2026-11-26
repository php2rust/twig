//! Port of the test classes of tests/IntegrationTest.php that the
//! fixtures' `--DATA--` sections create: `TwigTestFoo`, `ToStringStub`,
//! `CountableStub`, `IteratorAggregateStub`, `SimpleIteratorForTesting`
//! and `MagicCallStub`, and the classes that some fixtures declare in
//! their own `--DATA--`, as `php_value` objects. An object is immutable
//! here, so an iterator's position is not kept between two loops (each
//! loop rewinds, as PHP's `foreach` does).
//!
//! Upstream: twig/twig tests/IntegrationTest.php

use std::sync::Arc;

use php_value::{
    Key, PhpArray, PhpString,
    args::Parameter,
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

    fn method_parameters(&self, name: &str) -> Option<Vec<Parameter>> {
        Some(match name.to_ascii_lowercase().as_str() {
            "bar" => vec![
                Parameter::optional("param1", Value::Null),
                Parameter::optional("param2", Value::Null),
            ],
            "strtolower" => vec![Parameter::required("value")],
            "getfoo" | "getempty" | "getnull" | "getself" | "is" | "in" | "not" | "rewind"
            | "current" | "key" | "next" | "valid" => Vec::new(),
            _ => return None,
        })
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

/// `TestClassForMagicCallAttributes` (`expressions/magic_call.test`):
/// `getBar()`, and `__call()` that answers `foo` (else `false`).
#[derive(Debug, Clone, Default)]
pub struct TestClassForMagicCallAttributes;

impl PhpObject for TestClassForMagicCallAttributes {
    fn class(&self) -> &'static str {
        "TestClassForMagicCallAttributes"
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn methods(&self) -> Vec<String> {
        vec!["getBar".to_owned(), "__call".to_owned()]
    }

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        Some(Ok(match name {
            _ if name.eq_ignore_ascii_case("getBar") => s("bar_from_getbar"),
            "foo" => s("foo_from_call"),
            _ => Value::Bool(false),
        }))
    }
}

/// A class of a fixture's `--DATA--` with only `__toString()`:
/// `MyObj` (`regression/block-inheritance-issue.test`).
#[derive(Debug, Clone)]
pub struct StringOnly {
    pub class: &'static str,
    pub string: &'static str,
}

impl PhpObject for StringOnly {
    fn class(&self) -> &'static str {
        self.class
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn instance_of(&self, class: &str) -> bool {
        class.trim_start_matches('\\') == "Stringable"
    }

    fn methods(&self) -> Vec<String> {
        vec!["__toString".to_owned()]
    }

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        name.eq_ignore_ascii_case("__toString")
            .then(|| Ok(s(self.string)))
    }

    fn to_php_string(&self) -> Option<PhpString> {
        Some(PhpString::from(self.string.as_bytes().to_vec()))
    }
}

/// `UserForAutoEscapeTest` (`tags/autoescape/objects.test`): `getName()`
/// and `__toString()` give `Fabien<br />`.
#[derive(Debug, Clone, Default)]
pub struct UserForAutoEscapeTest;

impl PhpObject for UserForAutoEscapeTest {
    fn class(&self) -> &'static str {
        "UserForAutoEscapeTest"
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn instance_of(&self, class: &str) -> bool {
        class.trim_start_matches('\\') == "Stringable"
    }

    fn methods(&self) -> Vec<String> {
        vec!["getName".to_owned(), "__toString".to_owned()]
    }

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        (name.eq_ignore_ascii_case("getName") || name.eq_ignore_ascii_case("__toString"))
            .then(|| Ok(s("Fabien<br />")))
    }

    fn to_php_string(&self) -> Option<PhpString> {
        Some(PhpString::from(b"Fabien<br />".to_vec()))
    }
}

/// `ItemsIterator` and `ItemsIteratorCountable` (`tags/for/objects*.test`):
/// an `Iterator` over `['foo' => 'bar', 'bar' => 'foo']`, `Countable` or
/// not.
#[derive(Debug, Clone)]
pub struct ItemsIterator {
    pub countable: bool,
}

impl ItemsIterator {
    fn values() -> PhpArray {
        let mut values = PhpArray::new();
        values.insert(Key::from_bytes(b"foo"), s("bar"));
        values.insert(Key::from_bytes(b"bar"), s("foo"));
        values
    }
}

impl PhpObject for ItemsIterator {
    fn class(&self) -> &'static str {
        if self.countable {
            "ItemsIteratorCountable"
        } else {
            "ItemsIterator"
        }
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn instance_of(&self, class: &str) -> bool {
        match class.trim_start_matches('\\') {
            "Iterator" | "Traversable" => true,
            "Countable" => self.countable,
            _ => false,
        }
    }

    fn methods(&self) -> Vec<String> {
        let mut methods: Vec<String> = ["current", "key", "next", "rewind", "valid"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        if self.countable {
            methods.push("count".to_owned());
        }
        methods
    }

    fn php_count(&self) -> Option<Result<i64, Thrown>> {
        self.countable.then_some(Ok(2))
    }

    fn traverse(&self) -> Option<Result<Vec<(Value, Value)>, Thrown>> {
        Some(Ok(pairs(&Self::values())))
    }
}

/// `TwigTestStringy` (`expressions/array_dynamic_keys.test`): a
/// `Stringable` with a private `$v`.
#[derive(Debug, Clone)]
pub struct TwigTestStringy {
    pub v: PhpString,
}

impl PhpObject for TwigTestStringy {
    fn class(&self) -> &'static str {
        "TwigTestStringy"
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

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        name.eq_ignore_ascii_case("__toString")
            .then(|| Ok(Value::String(self.v.clone())))
    }

    fn to_php_string(&self) -> Option<PhpString> {
        Some(self.v.clone())
    }
}

/// `TwigTestHolder` (`expressions/array_dynamic_keys.test`): `$name =
/// 'attr'`, `$stringable` (`attr_obj`), `getStringable()` (`method_obj`).
#[derive(Debug, Clone, Default)]
pub struct TwigTestHolder;

impl PhpObject for TwigTestHolder {
    fn class(&self) -> &'static str {
        "TwigTestHolder"
    }

    fn properties(&self) -> PhpArray {
        let mut out = PhpArray::new();
        out.insert(Key::from_bytes(b"name"), s("attr"));
        out.insert(
            Key::from_bytes(b"stringable"),
            Value::Object(Arc::new(TwigTestStringy {
                v: PhpString::from(b"attr_obj".to_vec()),
            })),
        );
        out
    }

    fn methods(&self) -> Vec<String> {
        vec!["__construct".to_owned(), "getStringable".to_owned()]
    }

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        name.eq_ignore_ascii_case("getStringable").then(|| {
            Ok(Value::Object(Arc::new(TwigTestStringy {
                v: PhpString::from(b"method_obj".to_vec()),
            })))
        })
    }
}

/// The anonymous class of `expressions/set.test` with `getName()`.
#[derive(Debug, Clone, Default)]
pub struct AnonymousNamed;

impl PhpObject for AnonymousNamed {
    fn class(&self) -> &'static str {
        "class@anonymous"
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn methods(&self) -> Vec<String> {
        vec!["getName".to_owned()]
    }

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        name.eq_ignore_ascii_case("getName")
            .then(|| Ok(s("Fabien")))
    }
}
