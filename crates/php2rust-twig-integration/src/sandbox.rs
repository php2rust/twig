//! The Rust side of tests/Sandbox/SandboxTest.php: the functions of
//! [`crate::sandbox_extension`], the test's `SandboxTestObject` class, and
//! the environment `render_trusted()` renders with.
//!
//! Upstream: twig/twig tests/Sandbox/SandboxTest.php

// The callables share one signature (`fn(&Call, Vec<Value>) -> R`).
#![allow(clippy::needless_pass_by_value)]

use std::{cell::RefCell, rc::Rc};

use php_value::{
    PhpArray, PhpString,
    value::{PhpObject, Thrown, Value},
};
use twig::{
    Environment,
    runtime::{Call, R, bytes, s},
};

/// `strtoupper($string)`.
pub fn strtoupper(_call: &Call<'_>, arguments: Vec<Value>) -> R {
    let text = arguments.first().map_or_else(|| Ok(Vec::new()), bytes)?;
    Ok(s(&text.to_ascii_uppercase()))
}

/// `static fn ($value) => true`.
pub fn funky(_call: &Call<'_>, _arguments: Vec<Value>) -> R {
    Ok(Value::Bool(true))
}

thread_local! {
    static TRUSTED: RefCell<Option<Rc<Environment>>> = const { RefCell::new(None) };
}

/// Sets the environment (`$app`) that `render_trusted()` renders with.
pub fn set_trusted_environment(env: Option<Rc<Environment>>) {
    TRUSTED.with(|t| *t.borrow_mut() = env);
}

/// `static fn () => $app->render('trusted', ['value' => 'ok'])`.
pub fn render_trusted(_call: &Call<'_>, _arguments: Vec<Value>) -> R {
    let env = TRUSTED
        .with(|t| t.borrow().clone())
        .ok_or_else(|| twig::Error::logic("render_trusted() has no environment"))?;
    let mut context = PhpArray::new();
    context.insert("value", s(b"ok"));
    Ok(s(&env.render(b"trusted", context)?))
}

/// `SandboxTestObject`: `$name = 'fabien'`, `getName()`, `__toString()`.
#[derive(Debug, Default)]
pub struct SandboxTestObject;

impl SandboxTestObject {
    pub const CLASS: &'static str = "Twig\\Tests\\Sandbox\\SandboxTestObject";
}

impl PhpObject for SandboxTestObject {
    fn class(&self) -> &str {
        Self::CLASS
    }

    fn properties(&self) -> PhpArray {
        let mut properties = PhpArray::new();
        properties.insert("name", s(b"fabien"));
        properties
    }

    fn property(&self, name: &[u8]) -> Option<Value> {
        (name == b"name").then(|| s(b"fabien"))
    }

    fn to_php_string(&self) -> Option<PhpString> {
        Some(PhpString::from(b"object".to_vec()))
    }

    fn is_stringable(&self) -> bool {
        true
    }

    fn instance_of(&self, class: &str) -> bool {
        class.eq_ignore_ascii_case("Stringable")
    }

    fn methods(&self) -> Vec<String> {
        vec!["getName".to_owned(), "__toString".to_owned()]
    }

    fn call_method(&self, name: &str, _arguments: &[Value]) -> Option<Result<Value, Thrown>> {
        match name.to_ascii_lowercase().as_str() {
            "getname" => Some(Ok(s(b"fabien"))),
            "__tostring" => Some(Ok(s(b"object"))),
            _ => None,
        }
    }
}
