//! Port of the definitions in Twig\Extension\EscaperExtension.php: the
//! `autoescape` tag and the `escape`, `e` and `raw` filters.
//!
//! The default strategy and the escaper visitor come with the emitter.
//!
//! Upstream: twig/twig src/Extension/EscaperExtension.php

use std::rc::Rc;

use super::Extension;
use crate::{
    callable::TwigCallable,
    node::{Node, NodeClass},
    token_parser::{TokenParser, auto_escape::AutoEscapeTokenParser},
};

/// `EscaperExtension`.
#[derive(Debug, Default)]
pub struct EscaperExtension;

const RUNTIME: &str = "Twig\\Runtime\\EscaperRuntime::escape";

impl Extension for EscaperExtension {
    fn name(&self) -> &'static str {
        "Twig\\Extension\\EscaperExtension"
    }

    fn token_parsers(&self) -> Vec<Rc<dyn TokenParser>> {
        vec![Rc::new(AutoEscapeTokenParser)]
    }

    fn filters(&self) -> Vec<TwigCallable> {
        vec![
            TwigCallable::filter("escape", Some(RUNTIME))
                .safe_callback(escape_filter_is_safe)
                .node_class(NodeClass::EscapeFilter),
            TwigCallable::filter("e", Some(RUNTIME))
                .safe_callback(escape_filter_is_safe)
                .node_class(NodeClass::EscapeFilter),
            TwigCallable::filter("raw", None)
                .safe(&["all"])
                .node_class(NodeClass::RawFilter),
        ]
    }
}

/// `EscaperExtension::escapeFilterIsSafe()`: the strategy when the first
/// argument is a constant, nothing when it is not, `html` without one.
pub fn escape_filter_is_safe(filter_args: &Node) -> Option<Vec<Vec<u8>>> {
    match filter_args.child_nodes().first() {
        Some(arg) if arg.is(NodeClass::ConstantExpression) => Some(vec![
            arg.attr("value")
                .and_then(|v| v.to_php_string())
                .unwrap_or_default(),
        ]),
        Some(_) => Some(Vec::new()),
        None => Some(vec![b"html".to_vec()]),
    }
}
