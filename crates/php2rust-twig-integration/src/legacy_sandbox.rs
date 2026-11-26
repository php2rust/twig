//! The Rust side of tests/Extension/SandboxTest.php: the test's classes
//! (`FooObject`, `MagicCallObject`, ...) and the functions of the
//! callables it adds to its environments (see
//! [`crate::legacy_sandbox_extension`]).
//!
//! Upstream: twig/twig tests/Extension/SandboxTest.php

// The callables share one signature (`fn(&Call, Vec<Value>) -> R`).
#![allow(clippy::needless_pass_by_value)]

use std::{
    cell::Cell,
    sync::{Arc, Weak},
};

use php_value::{
    PhpArray, PhpString,
    value::{PhpObject, Thrown, Value},
};
use twig::{
    Error,
    runtime::{Call, R, bytes, s},
};

/// The namespace of the test's classes.
pub const NS: &str = "Twig\\Tests\\Extension\\";

thread_local! {
    static FOO_CALLED: Cell<[i64; 3]> = const { Cell::new([0; 3]) };
    static UNSAFE_CALLED: Cell<i64> = const { Cell::new(0) };
}

fn count(index: usize) {
    FOO_CALLED.with(|c| {
        let mut called = c.get();
        called[index] += 1;
        c.set(called);
    });
}

/// `FooObject`, or `CountableFooObject` (a child class that implements
/// `\Countable`).
#[derive(Debug, Default)]
pub struct FooObject {
    countable: bool,
}

impl FooObject {
    pub const CLASS: &'static str = "Twig\\Tests\\Extension\\FooObject";
    pub const COUNTABLE_CLASS: &'static str = "Twig\\Tests\\Extension\\CountableFooObject";

    #[must_use]
    pub fn value() -> Value {
        Value::Object(Arc::new(Self::default()))
    }

    #[must_use]
    pub fn countable() -> Value {
        Value::Object(Arc::new(Self { countable: true }))
    }

    /// `FooObject::reset()`.
    pub fn reset() {
        FOO_CALLED.with(|c| c.set([0; 3]));
    }

    /// `FooObject::$called[$method]` for `__toString`, `foo` and
    /// `getFooBar`.
    #[must_use]
    pub fn called(method: &str) -> i64 {
        let called = FOO_CALLED.with(Cell::get);
        match method {
            "__toString" => called[0],
            "foo" => called[1],
            "getFooBar" => called[2],
            _ => 0,
        }
    }
}

impl PhpObject for FooObject {
    fn class(&self) -> &str {
        if self.countable {
            Self::COUNTABLE_CLASS
        } else {
            Self::CLASS
        }
    }

    fn properties(&self) -> PhpArray {
        let mut properties = PhpArray::new();
        properties.insert("bar", s(b"bar"));
        properties
    }

    fn to_php_string(&self) -> Option<PhpString> {
        count(0);
        Some(PhpString::from(b"foo".to_vec()))
    }

    fn is_stringable(&self) -> bool {
        true
    }

    fn instance_of(&self, class: &str) -> bool {
        class.eq_ignore_ascii_case("Stringable")
            || (self.countable
                && (class.eq_ignore_ascii_case(Self::CLASS)
                    || class.eq_ignore_ascii_case("Countable")))
    }

    fn methods(&self) -> Vec<String> {
        let mut methods = Vec::new();
        if self.countable {
            methods.push("count".to_owned());
        }
        methods.extend(
            [
                "reset",
                "__toString",
                "foo",
                "getFooBar",
                "getAnotherFooObject",
            ]
            .map(str::to_owned),
        );
        methods
    }

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        match name.to_ascii_lowercase().as_str() {
            "__tostring" => {
                count(0);
                Some(Ok(s(b"foo")))
            }
            "foo" => {
                count(1);
                Some(Ok(s(b"foo")))
            }
            "getfoobar" => {
                count(2);
                Some(Ok(s(b"foobar")))
            }
            "getanotherfooobject" => Some(Ok(Self::value())),
            "reset" => {
                Self::reset();
                Some(Ok(Value::Null))
            }
            "count" if self.countable => Some(Ok(Value::Int(0))),
            _ => None,
        }
    }

    fn php_count(&self) -> Option<Result<i64, Thrown>> {
        self.countable.then_some(Ok(0))
    }
}

/// `ChildClass` (extends `ParentClass`).
#[derive(Debug, Default)]
pub struct ChildClass;

impl PhpObject for ChildClass {
    fn class(&self) -> &'static str {
        "Twig\\Tests\\Extension\\ChildClass"
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn instance_of(&self, class: &str) -> bool {
        class.eq_ignore_ascii_case("Twig\\Tests\\Extension\\ParentClass")
    }

    fn methods(&self) -> Vec<String> {
        vec!["ChildMethod".to_owned(), "ParentMethod".to_owned()]
    }

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        matches!(
            name.to_ascii_lowercase().as_str(),
            "childmethod" | "parentmethod"
        )
        .then_some(Ok(Value::Null))
    }
}

fn not_called(what: &str) -> Thrown {
    Thrown::new("BadMethodCallException", what.to_owned(), 0)
}

/// `ArrayLikeObject` (extends `\ArrayObject`): its `offsetGet()` and
/// `offsetExists()` must not be called.
#[derive(Debug, Default)]
pub struct ArrayLikeObject;

impl PhpObject for ArrayLikeObject {
    fn class(&self) -> &'static str {
        "Twig\\Tests\\Extension\\ArrayLikeObject"
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn instance_of(&self, class: &str) -> bool {
        [
            "ArrayObject",
            "ArrayAccess",
            "IteratorAggregate",
            "Traversable",
            "Countable",
        ]
        .iter()
        .any(|c| c.eq_ignore_ascii_case(class))
    }

    fn offset_get(&self, _offset: &Value) -> Option<Result<Value, Thrown>> {
        Some(Err(not_called("Should not be called.")))
    }

    fn offset_exists(&self, _offset: &Value) -> Option<Result<bool, Thrown>> {
        Some(Err(not_called("Should not be called.")))
    }

    fn php_count(&self) -> Option<Result<i64, Thrown>> {
        Some(Ok(0))
    }

    fn traverse(&self) -> Option<Result<Vec<(Value, Value)>, Thrown>> {
        Some(Ok(Vec::new()))
    }
}

/// `MagicObject`: `__get()` and `__isset()` must not be called in the
/// sandbox. It has no real properties, so a read never reaches them
/// here.
#[derive(Debug, Default)]
pub struct MagicObject;

impl PhpObject for MagicObject {
    fn class(&self) -> &'static str {
        "Twig\\Tests\\Extension\\MagicObject"
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn methods(&self) -> Vec<String> {
        vec!["__get".to_owned(), "__isset".to_owned()]
    }

    fn property(&self, _name: &[u8]) -> Option<Value> {
        None
    }
}

/// `MagicCallObject`: `$secret` and a `__call()` that returns
/// `call:<name>`.
#[derive(Debug, Default)]
pub struct MagicCallObject;

impl MagicCallObject {
    pub const CLASS: &'static str = "Twig\\Tests\\Extension\\MagicCallObject";
}

impl PhpObject for MagicCallObject {
    fn class(&self) -> &str {
        Self::CLASS
    }

    fn properties(&self) -> PhpArray {
        let mut properties = PhpArray::new();
        properties.insert("secret", s(b"secret"));
        properties
    }

    fn methods(&self) -> Vec<String> {
        vec!["__call".to_owned()]
    }

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        Some(Ok(s(format!("call:{name}").as_bytes())))
    }
}

/// `ColumnObject`: `$bar = 'bar'`, no `__toString()`.
#[derive(Debug, Default)]
pub struct ColumnObject;

impl ColumnObject {
    pub const CLASS: &'static str = "Twig\\Tests\\Extension\\ColumnObject";
}

impl PhpObject for ColumnObject {
    fn class(&self) -> &str {
        Self::CLASS
    }

    fn properties(&self) -> PhpArray {
        let mut properties = PhpArray::new();
        properties.insert("bar", s(b"bar"));
        properties
    }
}

/// `StringablePlainObject` (extends `PlainBaseObject`).
#[derive(Debug, Default)]
pub struct StringablePlainObject;

impl StringablePlainObject {
    pub const CLASS: &'static str = "Twig\\Tests\\Extension\\StringablePlainObject";
}

impl PhpObject for StringablePlainObject {
    fn class(&self) -> &str {
        Self::CLASS
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn to_php_string(&self) -> Option<PhpString> {
        Some(PhpString::from(b"plain".to_vec()))
    }

    fn instance_of(&self, class: &str) -> bool {
        class.eq_ignore_ascii_case("Twig\\Tests\\Extension\\PlainBaseObject")
            || class.eq_ignore_ascii_case("Stringable")
    }

    fn methods(&self) -> Vec<String> {
        vec!["__toString".to_owned()]
    }

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        name.eq_ignore_ascii_case("__toString")
            .then(|| Ok(s(b"plain")))
    }
}

/// `StringableTraversableObject`: a `\Stringable` `\IteratorAggregate`
/// over its items.
#[derive(Debug)]
pub struct StringableTraversableObject {
    items: PhpArray,
}

impl StringableTraversableObject {
    pub const CLASS: &'static str = "Twig\\Tests\\Extension\\StringableTraversableObject";

    #[must_use]
    pub fn value(items: PhpArray) -> Value {
        Value::Object(Arc::new(Self { items }))
    }
}

impl PhpObject for StringableTraversableObject {
    fn class(&self) -> &str {
        Self::CLASS
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn to_php_string(&self) -> Option<PhpString> {
        Some(PhpString::from(b"stringable-traversable".to_vec()))
    }

    fn instance_of(&self, class: &str) -> bool {
        ["IteratorAggregate", "Traversable", "Stringable"]
            .iter()
            .any(|c| c.eq_ignore_ascii_case(class))
    }

    fn methods(&self) -> Vec<String> {
        vec![
            "__construct".to_owned(),
            "__toString".to_owned(),
            "getIterator".to_owned(),
        ]
    }

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        name.eq_ignore_ascii_case("__toString")
            .then(|| Ok(s(b"stringable-traversable")))
    }

    fn traverse(&self) -> Option<Result<Vec<(Value, Value)>, Thrown>> {
        Some(Ok(self
            .items
            .iter()
            .map(|(k, v)| (k.to_value(), v.clone()))
            .collect()))
    }
}

/// `CyclicTraversableObject`: `getIterator()` yields the object itself.
#[derive(Debug)]
pub struct CyclicTraversableObject {
    this: Weak<Self>,
}

impl CyclicTraversableObject {
    #[must_use]
    pub fn value() -> Value {
        Value::Object(Arc::new_cyclic(|this| Self { this: this.clone() }))
    }
}

impl PhpObject for CyclicTraversableObject {
    fn class(&self) -> &'static str {
        "Twig\\Tests\\Extension\\CyclicTraversableObject"
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn instance_of(&self, class: &str) -> bool {
        ["IteratorAggregate", "Traversable"]
            .iter()
            .any(|c| c.eq_ignore_ascii_case(class))
    }

    fn traverse(&self) -> Option<Result<Vec<(Value, Value)>, Thrown>> {
        let this = self.this.upgrade()?;
        Some(Ok(vec![(Value::Int(0), Value::Object(this))]))
    }
}

fn first(arguments: &[Value]) -> Value {
    arguments.first().cloned().unwrap_or(Value::Null)
}

/// `static fn ($a) => (string) $a`: `my_func`, and the `unsafe_fn`
/// variants.
pub fn to_string(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    Ok(s(&bytes(&first(&arguments))?))
}

/// `static fn () => new FooObject()`: `make_obj` and `to_obj`.
pub fn make_obj(_call: &Call<'_>, _arguments: Vec<Value>) -> R {
    Ok(FooObject::value())
}

/// `render_traversable`: the object must reach the function as it is.
pub fn render_traversable(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    let value = first(&arguments);
    match &value {
        Value::Object(o) if o.downcast_ref::<StringableTraversableObject>().is_some() => {
            Ok(s(&bytes(&value)?))
        }
        other => Err(Error::exception(
            "RuntimeException",
            format!(
                "Expected a StringableTraversableObject, got \"{}\".",
                other.debug_type()
            ),
        )),
    }
}

/// `static fn (...) => 'ok'`: `safe_fn` and `safe_filter`.
pub fn ok(_call: &Call<'_>, _arguments: Vec<Value>) -> R {
    Ok(s(b"ok"))
}

/// `static fn (string $name) => "hi $name"`.
pub fn greet(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    let mut out = b"hi ".to_vec();
    out.extend_from_slice(&bytes(&first(&arguments))?);
    Ok(s(&out))
}

/// `static fn ($value) => 0 === $value % 2`.
pub fn even(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    Ok(Value::Bool(
        php_value::ops::to_int(&first(&arguments)) % 2 == 0,
    ))
}

/// The `unsafe` test: counts its calls and is true.
pub fn unsafe_test(_call: &Call<'_>, _arguments: Vec<Value>) -> R {
    UNSAFE_CALLED.with(|c| c.set(c.get() + 1));
    Ok(Value::Bool(true))
}

/// How many times the `unsafe` test ran on this thread.
#[must_use]
pub fn unsafe_test_calls() -> i64 {
    UNSAFE_CALLED.with(Cell::get)
}

fn is_sandboxed(call: &Call<'_>) -> bool {
    twig::runtime::sandbox::is_sandboxed(call.env, call.source())
}

/// The `sandbox_aware` filter and function: `$value:on` or `$value:off`.
pub fn sandbox_aware(call: &Call<'_>, arguments: Vec<Value>) -> R {
    let mut out = bytes(&first(&arguments))?;
    out.extend_from_slice(if is_sandboxed(call) { b":on" } else { b":off" });
    Ok(s(&out))
}

/// The `sandbox_aware` test: `$isSandboxed && 'foo' === $value`.
pub fn sandbox_aware_test(call: &Call<'_>, arguments: Vec<Value>) -> R {
    Ok(Value::Bool(
        is_sandboxed(call) && first(&arguments) == s(b"foo"),
    ))
}

/// The `sandbox_aware` test of the not-sandboxed case:
/// `!$isSandboxed && 'foo' === $value`.
pub fn not_sandboxed_test(call: &Call<'_>, arguments: Vec<Value>) -> R {
    Ok(Value::Bool(
        !is_sandboxed(call) && first(&arguments) == s(b"foo"),
    ))
}

/// `static fn ($value, $arg) => 'x' === (string) $arg`: `my_test` and
/// `unsafe_test`.
pub fn second_is_x(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    let arg = arguments.get(1).cloned().unwrap_or(Value::Null);
    Ok(Value::Bool(bytes(&arg)? == b"x"))
}

/// `static fn ($value, $fooBar, int ...$rest) => (string) $fooBar`.
pub fn second_to_string(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    let arg = arguments.get(1).cloned().unwrap_or(Value::Null);
    Ok(s(&bytes(&arg)?))
}
