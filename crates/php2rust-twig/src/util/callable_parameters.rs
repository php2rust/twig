//! Port of Twig\Util\CallableParameters.php: the parameters of the PHP
//! callable behind a filter, function or test node, for the sandbox.
//!
//! PHP reflects on the callable and `isStringCoercionSafe()` reads the
//! declared types. Here the parameters come from the callable's Rust
//! function ([`TwigCallable::rust`]) or from the core table
//! ([`crate::compiler::core_callable`]), and each one carries the result
//! of `isStringCoercionSafe()` as [`Parameter::coercion_safe`].
//!
//! Upstream: twig/twig src/Util/CallableParameters.php

use crate::{
    callable::TwigCallable,
    compiler::core_callable,
    environment::Environment,
    node::{Node, NodeClass},
    util::callable_arguments::{Parameter, twig_parameters},
};

/// `(new ReflectionCallable($callable))->getTwigParameters()`, or `None`
/// for a callable without a PHP callable (compiled by its node class) or
/// one that this port cannot see the parameters of.
#[must_use]
pub fn of_callable(callable: &TwigCallable) -> Option<Vec<Parameter>> {
    let params = match callable.rust_callable() {
        Some(rust) => rust.parameters.clone(),
        None => core_callable(callable.callable()?)?.1,
    };
    Some(twig_parameters(callable, &params, false).to_vec())
}

/// `CallableParameters::fromNode()`.
#[must_use]
pub fn from_node(node: &Node, env: &Environment) -> Option<Vec<Parameter>> {
    let class = node.class();
    let filter = class.is_a(NodeClass::FilterExpression);
    let function = class.is_a(NodeClass::FunctionExpression);
    let test = class.is_a(NodeClass::TestExpression);
    if !filter && !function && !test {
        return None;
    }
    let callable = if let Some(crate::node::Attr::Callable(c)) = node.attr("twig_callable") {
        c
    } else {
        let name = node.attr("name")?.to_php_string()?;
        if filter {
            env.filter(&name)?
        } else if function {
            env.function(&name)?
        } else {
            env.test(&name)?
        }
    };
    of_callable(&callable)
}
