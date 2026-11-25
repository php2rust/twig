//! Port of Twig\NodeVisitor\SafeAnalysisNodeVisitor.php: for each
//! expression, the escaping strategies its value is already safe for.
//!
//! Upstream: twig/twig src/NodeVisitor/SafeAnalysisNodeVisitor.php

use std::collections::HashMap;

use symfony_deprecation_contracts::trigger_deprecation;

use super::NodeVisitor;
use crate::{
    callable::TwigCallable,
    environment::Environment,
    error::Error,
    node::{Attr, Node, NodeClass, NodeInterface, WeakNode},
};

/// A list of strategy names (`['html', 'js']`, `['all']`).
pub type Safe = Vec<Vec<u8>>;

/// `SafeAnalysisNodeVisitor`.
#[derive(Default)]
pub struct SafeAnalysisNodeVisitor {
    /// `$data`, a `WeakMap`: by node identity, with a weak handle (it
    /// keeps the identity from being reused and tells a dead entry).
    data: HashMap<usize, (WeakNode, Safe)>,
    /// The size of `data` after the last removal of dead entries.
    live: usize,
    safe_vars: Vec<Vec<u8>>,
}

impl SafeAnalysisNodeVisitor {
    /// `setSafeVars()`: the names of imported macro templates.
    pub fn set_safe_vars(&mut self, safe_vars: Vec<Vec<u8>>) {
        self.safe_vars = safe_vars;
    }

    /// `getSafe()`.
    #[must_use]
    pub fn get_safe(&self, node: &Node) -> Safe {
        let mut safe = self
            .data
            .get(&node.id())
            .filter(|(weak, _)| weak.upgrade().is_some())
            .map(|(_, s)| s.clone())
            .unwrap_or_default();
        if contains(&safe, b"html_attr") {
            safe.push(b"html".to_vec());
            safe.push(b"html_attr_relaxed".to_vec());
        }
        if contains(&safe, b"html_attr_relaxed") {
            safe.push(b"html".to_vec());
        }
        safe
    }

    fn set_safe(&mut self, node: &Node, safe: Safe) {
        if self.data.len() >= 2 * self.live.max(512) {
            self.data.retain(|_, (weak, _)| weak.upgrade().is_some());
            self.live = self.data.len();
        }
        self.data.insert(node.id(), (node.downgrade(), safe));
    }

    /// `$filter->getSafe()` with the deprecation for a `null` result.
    fn callable_safe(callable: &TwigCallable, arguments: &Node) -> Safe {
        callable.get_safe(arguments).unwrap_or_else(|| {
            trigger_deprecation(
                b"twig/twig",
                b"3.16",
                format!(
                    "The \"{}::getSafe()\" method should not return \"null\" anymore, return \"[]\" instead.",
                    callable.callable_type().php_class()
                )
                .as_bytes(),
            );
            Vec::new()
        })
    }

    fn twig_callable(
        node: &Node,
        env: &Environment,
        filter: bool,
    ) -> Option<std::rc::Rc<TwigCallable>> {
        if let Some(Attr::Callable(c)) = node.attr("twig_callable") {
            return Some(c);
        }
        // legacy
        let name = node.attr_bytes("name")?;
        if filter {
            env.filter(&name)
        } else {
            env.function(&name)
        }
    }
}

fn contains(safe: &Safe, name: &[u8]) -> bool {
    safe.iter().any(|s| s == name)
}

/// `intersectSafe()`.
fn intersect_safe(a: &Safe, b: &Safe) -> Safe {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    if contains(a, b"all") {
        return b.clone();
    }
    if contains(b, b"all") {
        return a.clone();
    }
    a.iter().filter(|s| contains(b, s)).cloned().collect()
}

/// `OperatorEscapeInterface::getOperandNamesToEscape()`.
pub(crate) fn operand_names_to_escape(node: &Node) -> &'static [&'static str] {
    if node.is(NodeClass::ConditionalExpression) {
        &["expr2", "expr3"]
    } else if node.implements(NodeInterface::OperatorEscape) {
        &["left", "right"]
    } else {
        &[]
    }
}

impl NodeVisitor for SafeAnalysisNodeVisitor {
    fn enter_node(&mut self, node: Node, _env: &Environment) -> Result<Node, Error> {
        Ok(node)
    }

    fn leave_node(&mut self, node: Node, env: &Environment) -> Result<Option<Node>, Error> {
        if node.is(NodeClass::ConstantExpression)
            || node.is(NodeClass::BlockReferenceExpression)
            || node.is(NodeClass::ParentExpression)
        {
            // constants are marked safe for all; blocks and the parent block
            // are safe by definition
            self.set_safe(&node, vec![b"all".to_vec()]);
        } else if node.implements(NodeInterface::OperatorEscape) {
            // intersect safeness of operands
            let operands = operand_names_to_escape(&node);
            if operands.len() == 2 {
                let safe = intersect_safe(
                    &self.get_safe(&node.get_node(operands[0])?),
                    &self.get_safe(&node.get_node(operands[1])?),
                );
                self.set_safe(&node, safe);
            }
        } else if node.is(NodeClass::FilterExpression) {
            // filter expression is safe when the filter is safe
            if let Some(filter) = Self::twig_callable(&node, env, true) {
                let mut safe = Self::callable_safe(&filter, &node.get_node("arguments")?);
                if safe.is_empty() {
                    safe = intersect_safe(
                        &self.get_safe(&node.get_node("node")?),
                        &filter.get_preserves_safety(),
                    );
                }
                self.set_safe(&node, safe);
            }
        } else if node.is(NodeClass::FunctionExpression) {
            // function expression is safe when the function is safe
            if let Some(function) = Self::twig_callable(&node, env, false) {
                let safe = Self::callable_safe(&function, &node.get_node("arguments")?);
                self.set_safe(&node, safe);
            }
        } else if node.is(NodeClass::MethodCallExpression)
            || node.is(NodeClass::MacroReferenceExpression)
        {
            // all macro calls are safe
            self.set_safe(&node, vec![b"all".to_vec()]);
        } else if node.is(NodeClass::GetAttrExpression) {
            let inner = node.get_node("node")?;
            if inner.is(NodeClass::ContextVariable)
                && let Attr::Str(name) = inner.get_attr("name")?
                && self.safe_vars.contains(&name)
            {
                self.set_safe(&node, vec![b"all".to_vec()]);
            }
        }
        Ok(Some(node))
    }

    fn priority(&self) -> i64 {
        0
    }
}
