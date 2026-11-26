//! Port of the extension of tests/IntegrationTest.php
//! (`Twig\Tests\TwigTestExtension`): its filters, functions, tests and
//! the `§` tag, each callable bound to a Rust function of
//! [`crate::callables`] by path. The build script includes this file too,
//! to compile the fixtures with the same definitions.
//!
//! [`extensions`] are the ones `IntegrationTest::getExtensions()` gives:
//! the sandbox extension (off, with the test's policy) and this one.
//! `DebugExtension` (`dump`) and `StringLoaderExtension`
//! (`template_from_string`) are not ported: their callables have no Rust
//! function, so a fixture that calls them does not compile.
//!
//! Upstream: twig/twig tests/IntegrationTest.php

use std::rc::Rc;

use twig::{
    Attr, TwigCallable,
    callable::DeprecatedCallableInfo,
    error::Error,
    extension::Extension,
    extension::sandbox::SandboxExtension,
    node::{Node, expression, statement},
    parser::Parser,
    sandbox::SecurityPolicy,
    token::Token,
    token::TokenType,
    token_parser::TokenParser,
    util::callable_arguments::Parameter as P,
};

const CLOSURE: Option<&str> = Some("{closure}");

/// The path of a function of [`crate::callables`].
fn path(name: &str) -> String {
    format!("::twig_integration::callables::{name}")
}

/// `TwigTestTokenParser_§`.
pub struct FixtureTokenParser;

impl TokenParser for FixtureTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, _token: &Token) -> Result<Node, Error> {
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        Ok(statement::print(expression::constant("§", -1), -1))
    }

    fn tag(&self) -> &'static str {
        "§"
    }
}

/// `TwigTestExtension`.
pub struct FixtureExtension;

/// `IntegrationTest::getExtensions()`: `new SandboxExtension(new
/// SecurityPolicy([], [], [], [], ['dump']), false)` and
/// `TwigTestExtension`.
#[must_use]
pub fn extensions() -> Vec<Rc<dyn Extension>> {
    vec![
        Rc::new(SandboxExtension::new(
            Rc::new(SecurityPolicy::new(&[], &[], &[], &[], &["dump"], &[])),
            false,
            None,
        )),
        Rc::new(FixtureExtension),
    ]
}

fn filter(name: &str, rust: &str, parameters: Vec<P>) -> TwigCallable {
    TwigCallable::filter(name, CLOSURE).rust(&path(rust), parameters)
}

fn function(name: &str, rust: &str, parameters: Vec<P>) -> TwigCallable {
    TwigCallable::function(name, CLOSURE).rust(&path(rust), parameters)
}

fn test(name: &str, rust: &str, parameters: Vec<P>) -> TwigCallable {
    TwigCallable::test(name, CLOSURE).rust(&path(rust), parameters)
}

fn req(name: &str) -> P {
    P::required(name)
}

fn br_default() -> P {
    P::with_default("sep", Attr::str("<br />"))
}

impl Extension for FixtureExtension {
    fn name(&self) -> &'static str {
        "Twig\\Tests\\TwigTestExtension"
    }

    fn token_parsers(&self) -> Vec<Rc<dyn TokenParser>> {
        vec![Rc::new(FixtureTokenParser)]
    }

    fn filters(&self) -> Vec<TwigCallable> {
        vec![
            filter("§", "section", vec![req("value")]),
            filter(
                "escape_and_nl2br",
                "escape_and_nl2br",
                vec![req("env"), req("value"), br_default()],
            )
            .needs_environment()
            .safe(&["html"]),
            filter("nl2br", "nl2br", vec![req("value"), br_default()])
                .pre_escape("html")
                .safe(&["html"]),
            filter("escape_something", "upper", vec![req("value")]).safe(&["something"]),
            filter("preserves_safety", "upper", vec![req("value")]).preserves_safety(&["html"]),
            filter("static_call_string", "static_call", vec![req("value")]),
            filter("static_call_array", "static_call", vec![req("value")]),
            filter("magic_call", "magic_call", vec![req("value")]),
            filter("magic_call_closure", "magic_call", vec![req("value")]),
            filter("magic_call_string", "magic_static_call", vec![req("value")]),
            filter("magic_call_array", "magic_static_call", vec![req("value")]),
            filter("*_path", "dynamic_path", vec![req("element"), req("item")]),
            filter(
                "*_foo_*_bar",
                "dynamic_foo",
                vec![req("foo"), req("bar"), req("item")],
            ),
            filter("not", "not", vec![req("value")]),
            filter("anon_foo", "anon_foo", vec![req("name")]),
        ]
    }

    fn functions(&self) -> Vec<TwigCallable> {
        vec![
            // DebugExtension and StringLoaderExtension: no Rust function
            TwigCallable::function("dump", Some("Twig\\Extension\\DebugExtension::dump"))
                .safe(&["html"])
                .needs_context()
                .needs_environment()
                .variadic(),
            TwigCallable::function(
                "template_from_string",
                Some("Twig\\Extension\\StringLoaderExtension::templateFromString"),
            )
            .needs_environment(),
            function("§", "section", vec![req("value")]),
            function("safe_br", "br", Vec::new()).safe(&["html"]),
            function("unsafe_br", "br", Vec::new()),
            function("static_call_string", "static_call", vec![req("value")]),
            function("static_call_array", "static_call", vec![req("value")]),
            function("*_path", "dynamic_path", vec![req("element"), req("item")]),
            function(
                "*_foo_*_bar",
                "dynamic_foo",
                vec![req("foo"), req("bar"), req("item")],
            ),
            function("anon_foo", "anon_foo", vec![req("name")]),
            function("deprecated_function", "deprecated_function", Vec::new()).deprecated(
                DeprecatedCallableInfo::new("foo/bar", "1.1").alternative(
                    Some("not_deprecated_function"),
                    None,
                    None,
                ),
            ),
        ]
    }

    fn tests(&self) -> Vec<TwigCallable> {
        vec![
            test("multi word", "is_multi_word", vec![req("value")]),
            test("test_*", "dynamic_test", vec![req("element"), req("item")]),
        ]
    }
}

/// `Twig\Tests\DummyBackedEnum` and `Twig\Tests\DummyUnitEnum`, the enums
/// the fixtures name, registered for `enum()` and `enum_cases()`.
///
/// Upstream: twig/twig tests/DummyBackedEnum.php tests/DummyUnitEnum.php
pub fn register_enums(env: &twig::Environment) {
    use php_value::object::EnumCase;
    let backed = "Twig\\Tests\\DummyBackedEnum";
    let unit = "Twig\\Tests\\DummyUnitEnum";
    let mut options = env.core_options_mut();
    options.enums.define_enum(
        backed,
        vec![
            EnumCase::new(backed, "FOO", Some("foo".into())),
            EnumCase::new(backed, "BAR", Some("bar".into())),
        ],
    );
    options.enums.define_enum(
        unit,
        vec![
            EnumCase::new(unit, "BAR", None),
            EnumCase::new(unit, "BAZ", None),
        ],
    );
}
