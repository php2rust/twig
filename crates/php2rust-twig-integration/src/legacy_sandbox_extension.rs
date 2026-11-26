//! The callables and tags that tests/Extension/SandboxTest.php adds to
//! its environments (`addFunction()`, `addFilter()`, `addTest()`,
//! `addTokenParser()`), as one extension: a Rust environment takes its
//! callables when it is made, and the templates are compiled with them.
//! Each callable is bound to a Rust function of [`crate::legacy_sandbox`]
//! by path. The build script includes this file too.
//!
//! Some tests add a callable of the same name with other PHP parameter
//! types; the sandbox visitor reads these types when it compiles. A
//! template set selects these callables with its `variant`.
//!
//! Upstream: twig/twig tests/Extension/SandboxTest.php

use std::rc::Rc;

use twig::{
    Error, TwigCallable,
    extension::Extension,
    node::{Node, statement},
    parser::Parser,
    token::{Token, TokenType},
    token_parser::{TokenParser, line_of},
    util::callable_arguments::Parameter as P,
};

const CLOSURE: Option<&str> = Some("{closure}");

fn path(name: &str) -> String {
    format!("::twig_integration::legacy_sandbox::{name}")
}

fn closure(
    kind: fn(&str, Option<&str>) -> TwigCallable,
    name: &str,
    rust: &str,
    params: Vec<P>,
) -> TwigCallable {
    kind(name, CLOSURE).rust(&path(rust), params)
}

fn filter(name: &str, callable: Option<&str>) -> TwigCallable {
    TwigCallable::filter(name, callable)
}

fn function(name: &str, callable: Option<&str>) -> TwigCallable {
    TwigCallable::function(name, callable)
}

fn test(name: &str, callable: Option<&str>) -> TwigCallable {
    TwigCallable::test(name, callable)
}

fn upper(name: &str) -> TwigCallable {
    TwigCallable::filter(name, Some("strtoupper")).rust(
        "::twig_integration::sandbox::strtoupper",
        vec![P::required("string")],
    )
}

/// The test's callables and tags. `variant` selects the callables of
/// one test where two tests add callables of the same name:
///
/// - `safe_variadic`: `safe_fn(int ...$x)`;
/// - `unsafe_variadic`: `unsafe_fn(...$x)`;
/// - `named_variadic`: `unsafe_fn($fooBar, int ...$rest)`;
/// - `not_sandboxed_test`: the `sandbox_aware` test is
///   `!$isSandboxed && 'foo' === $value`;
/// - `range`: `range` is always allowed in a sandbox.
pub struct LegacySandboxTestExtension {
    pub variant: String,
}

impl Extension for LegacySandboxTestExtension {
    fn name(&self) -> &'static str {
        "Twig\\Tests\\Extension\\LegacySandboxTestExtension"
    }

    fn filters(&self) -> Vec<TwigCallable> {
        vec![
            closure(filter, "to_obj", "make_obj", Vec::new()),
            upper("safe_upper").always_allowed_in_sandbox(),
            upper("gated_upper"),
            closure(
                filter,
                "sandbox_aware",
                "sandbox_aware",
                vec![P::required("value")],
            )
            .needs_is_sandboxed(),
            closure(
                filter,
                "unsafe_filter",
                "second_to_string",
                vec![
                    P::required("value"),
                    P::required("fooBar"),
                    P::variadic("rest").coercion_safe(),
                ],
            )
            .variadic(),
            closure(
                filter,
                "safe_filter",
                "ok",
                vec![P::required("n").coercion_safe()],
            ),
        ]
    }

    fn functions(&self) -> Vec<TwigCallable> {
        let mut functions = vec![
            closure(function, "my_func", "to_string", vec![P::required("a")]),
            closure(function, "make_obj", "make_obj", Vec::new()),
            closure(
                function,
                "render_traversable",
                "render_traversable",
                vec![P::required("obj")],
            ),
            closure(function, "safe_greet", "greet", vec![P::required("name")])
                .always_allowed_in_sandbox(),
            closure(function, "gated_greet", "greet", vec![P::required("name")]),
            closure(
                function,
                "sandbox_aware",
                "sandbox_aware",
                vec![P::required("value")],
            )
            .needs_is_sandboxed(),
        ];
        functions.push(match self.variant.as_str() {
            "safe_variadic" => closure(
                function,
                "safe_fn",
                "ok",
                vec![P::variadic("x").coercion_safe()],
            ),
            _ => closure(
                function,
                "safe_fn",
                "ok",
                vec![P::required("n").coercion_safe()],
            ),
        });
        functions.push(match self.variant.as_str() {
            "unsafe_variadic" => {
                closure(function, "unsafe_fn", "to_string", vec![P::variadic("x")])
            }
            "named_variadic" => closure(
                function,
                "unsafe_fn",
                "to_string",
                vec![P::required("fooBar"), P::variadic("rest").coercion_safe()],
            )
            .variadic(),
            _ => closure(function, "unsafe_fn", "to_string", vec![P::required("x")]),
        });
        if self.variant == "range" {
            functions.push(function("range", Some("range")).always_allowed_in_sandbox());
        }
        functions
    }

    fn tests(&self) -> Vec<TwigCallable> {
        let sandbox_aware = if self.variant == "not_sandboxed_test" {
            "not_sandboxed_test"
        } else {
            "sandbox_aware_test"
        };
        vec![
            closure(test, "unsafe", "unsafe_test", vec![P::required("value")]),
            closure(test, "safe_even", "even", vec![P::required("value")])
                .always_allowed_in_sandbox(),
            closure(test, "gated_even", "even", vec![P::required("value")]),
            closure(
                test,
                "sandbox_aware",
                sandbox_aware,
                vec![P::required("value")],
            )
            .needs_is_sandboxed(),
            closure(
                test,
                "my_test",
                "second_is_x",
                vec![P::required("value").coercion_safe(), P::required("arg")],
            ),
            closure(
                test,
                "unsafe_test",
                "second_is_x",
                vec![
                    P::required("value"),
                    P::required("fooBar"),
                    P::variadic("rest").coercion_safe(),
                ],
            )
            .variadic(),
        ]
    }

    fn token_parsers(&self) -> Vec<Rc<dyn TokenParser>> {
        vec![
            Rc::new(TextTag {
                tag: "always_allowed_tag",
                text: b"always-allowed",
                always_allowed: true,
            }),
            Rc::new(TextTag {
                tag: "gated_tag",
                text: b"gated",
                always_allowed: false,
            }),
        ]
    }
}

/// `AlwaysAllowedSandboxTokenParser` and `GatedSandboxTokenParser`: a
/// tag that prints a text.
struct TextTag {
    tag: &'static str,
    text: &'static [u8],
    always_allowed: bool,
}

impl TokenParser for TextTag {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        Ok(statement::text(self.text, line_of(token)))
    }

    fn tag(&self) -> &str {
        self.tag
    }

    fn is_always_allowed_in_sandbox(&self) -> bool {
        self.always_allowed
    }
}
