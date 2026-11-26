//! The callables that tests/Sandbox/SandboxTest.php adds to its
//! environments (`addFunction()`, `addTest()`), as one extension: a Rust
//! environment takes its callables when it is made, and the templates are
//! compiled with them. Each callable is bound to a Rust function of
//! [`crate::sandbox`] by path. The build script includes this file too.
//!
//! Upstream: twig/twig tests/Sandbox/SandboxTest.php

use twig::{TwigCallable, extension::Extension, util::callable_arguments::Parameter as P};

const CLOSURE: Option<&str> = Some("{closure}");

fn path(name: &str) -> String {
    format!("::twig_integration::sandbox::{name}")
}

/// `shout` (`strtoupper`), `render_trusted` and the `funky` test.
pub struct SandboxTestExtension;

impl Extension for SandboxTestExtension {
    fn name(&self) -> &'static str {
        "Twig\\Tests\\Sandbox\\SandboxTestExtension"
    }

    fn functions(&self) -> Vec<TwigCallable> {
        vec![
            TwigCallable::function("shout", Some("strtoupper"))
                .rust(&path("strtoupper"), vec![P::required("string")]),
            TwigCallable::function("render_trusted", CLOSURE)
                .rust(&path("render_trusted"), Vec::new()),
        ]
    }

    fn tests(&self) -> Vec<TwigCallable> {
        vec![TwigCallable::test("funky", CLOSURE).rust(&path("funky"), vec![P::required("value")])]
    }
}
