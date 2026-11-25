//! Port of Twig\NodeVisitor\OptimizerNodeVisitor.php: constant prints
//! become text, block and parent prints output directly, and `for` loops
//! drop the `loop` variable when nothing can read it.
//!
//! Upstream: twig/twig src/NodeVisitor/OptimizerNodeVisitor.php

use symfony_deprecation_contracts::trigger_deprecation;

use super::NodeVisitor;
use crate::{
    environment::Environment,
    error::Error,
    node::{Attr, Node, NodeClass, statement},
};

/// `OptimizerNodeVisitor::OPTIMIZE_ALL`.
pub const OPTIMIZE_ALL: i64 = -1;
/// `OptimizerNodeVisitor::OPTIMIZE_NONE`.
pub const OPTIMIZE_NONE: i64 = 0;
/// `OptimizerNodeVisitor::OPTIMIZE_FOR`.
pub const OPTIMIZE_FOR: i64 = 2;
/// `OptimizerNodeVisitor::OPTIMIZE_RAW_FILTER` (deprecated, does nothing).
pub const OPTIMIZE_RAW_FILTER: i64 = 4;
/// `OptimizerNodeVisitor::OPTIMIZE_TEXT_NODES` (deprecated, does nothing).
pub const OPTIMIZE_TEXT_NODES: i64 = 8;

/// `OptimizerNodeVisitor`.
pub struct OptimizerNodeVisitor {
    optimizers: i64,
    loops: Vec<Node>,
    loops_targets: Vec<Attr>,
}

impl OptimizerNodeVisitor {
    /// The constructor's checks and deprecations, for `new
    /// Environment(['optimizations' => $optimizers])`.
    pub fn check_mode(optimizers: i64) -> Result<(), Error> {
        if optimizers > (OPTIMIZE_FOR | OPTIMIZE_RAW_FILTER | OPTIMIZE_TEXT_NODES) {
            return Err(Error::invalid_argument(format!(
                "Optimizer mode \"{optimizers}\" is not valid."
            )));
        }
        if optimizers != -1 && OPTIMIZE_RAW_FILTER == (OPTIMIZE_RAW_FILTER & optimizers) {
            trigger_deprecation(
                b"twig/twig",
                b"3.11",
                b"The \"Twig\\NodeVisitor\\OptimizerNodeVisitor::OPTIMIZE_RAW_FILTER\" option is deprecated and does nothing.",
            );
        }
        if optimizers != -1 && OPTIMIZE_TEXT_NODES == (OPTIMIZE_TEXT_NODES & optimizers) {
            trigger_deprecation(
                b"twig/twig",
                b"3.12",
                b"The \"Twig\\NodeVisitor\\OptimizerNodeVisitor::OPTIMIZE_TEXT_NODES\" option is deprecated and does nothing.",
            );
        }
        Ok(())
    }

    /// `new OptimizerNodeVisitor($optimizers)`, for a mode that
    /// [`OptimizerNodeVisitor::check_mode`] accepted.
    pub fn new(optimizers: i64) -> Self {
        Self {
            optimizers,
            loops: Vec::new(),
            loops_targets: Vec::new(),
        }
    }

    fn optimizes_for(&self) -> bool {
        OPTIMIZE_FOR == (OPTIMIZE_FOR & self.optimizers)
    }

    /// `enterOptimizeFor()`.
    fn enter_optimize_for(&mut self, node: &Node) -> Result<(), Error> {
        if node.is(NodeClass::ForNode) {
            // disable the loop variable by default
            node.set_attr("with_loop", false);
            self.loops.insert(0, node.clone());
            self.loops_targets
                .insert(0, node.get_node("value_target")?.get_attr("name")?);
            self.loops_targets
                .insert(0, node.get_node("key_target")?.get_attr("name")?);
            return Ok(());
        }
        if self.loops.is_empty() {
            // we are outside a loop
            return Ok(());
        }

        // when do we need to add the loop variable back?
        let is_context_variable = node.is(NodeClass::ContextVariable);
        let name = if is_context_variable {
            Some(node.get_attr("name")?)
        } else {
            None
        };
        if is_context_variable && name.as_ref().and_then(Attr::as_bytes) == Some(b"loop") {
            // the loop variable is referenced for the current loop
            node.set_attr("always_defined", true);
            self.add_loop_to_current();
        } else if name
            .as_ref()
            .is_some_and(|n| self.loops_targets.iter().any(|t| same(t, n)))
        {
            // optimize access to loop targets
            node.set_attr("always_defined", true);
        } else if is_context_variable {
            // a context variable matches none of the cases below
        } else if node.is(NodeClass::BlockReferenceNode)
            || node.is(NodeClass::BlockReferenceExpression)
        {
            // block reference
            self.add_loop_to_current();
        } else if node.is(NodeClass::IncludeNode) {
            // include without the only attribute
            if !node.get_attr("only")?.is_truthy() {
                self.add_loop_to_all();
            }
        } else if node.is(NodeClass::FunctionExpression) {
            // include function without the with_context=false parameter
            if node.get_attr("name")?.as_bytes() == Some(b"include") {
                let arguments = node.get_node("arguments")?;
                let with_context = if arguments.has_node("with_context") {
                    !matches!(
                        arguments.get_node("with_context")?.get_attr("value")?,
                        Attr::Bool(false)
                    )
                } else {
                    true
                };
                if with_context {
                    self.add_loop_to_all();
                }
            }
        } else if node.is(NodeClass::GetAttrExpression) {
            // the loop variable is referenced via an attribute
            let attribute = node.get_node("attribute")?;
            let dynamic_or_parent = !attribute.is(NodeClass::ConstantExpression)
                || attribute.get_attr("value")?.as_bytes() == Some(b"parent");
            if dynamic_or_parent {
                let current_has_loop = matches!(
                    self.loops.first().and_then(|l| l.attr("with_loop")),
                    Some(Attr::Bool(true))
                );
                let inner = node.get_node("node")?;
                let on_loop = inner.is(NodeClass::ContextVariable)
                    && inner.get_attr("name")?.as_bytes() == Some(b"loop");
                if current_has_loop || on_loop {
                    self.add_loop_to_all();
                }
            }
        }
        Ok(())
    }

    /// `leaveOptimizeFor()`.
    fn leave_optimize_for(&mut self, node: &Node) {
        if node.is(NodeClass::ForNode) {
            if !self.loops.is_empty() {
                self.loops.remove(0);
            }
            for _ in 0..2 {
                if !self.loops_targets.is_empty() {
                    self.loops_targets.remove(0);
                }
            }
        }
    }

    fn add_loop_to_current(&self) {
        if let Some(current) = self.loops.first() {
            current.set_attr("with_loop", true);
        }
    }

    fn add_loop_to_all(&self) {
        for l in &self.loops {
            l.set_attr("with_loop", true);
        }
    }
}

/// `in_array($name, $this->loopsTargets, true)` for name attributes.
fn same(a: &Attr, b: &Attr) -> bool {
    match (a, b) {
        (Attr::Str(a), Attr::Str(b)) => a == b,
        (Attr::Null, Attr::Null) => true,
        (Attr::Bool(a), Attr::Bool(b)) => a == b,
        (Attr::Int(a), Attr::Int(b)) => a == b,
        _ => false,
    }
}

/// `optimizePrintNode()`: a constant string print becomes text; a block
/// or parent print outputs directly.
fn optimize_print_node(node: Node) -> Result<Node, Error> {
    if !node.is(NodeClass::PrintNode) {
        return Ok(node);
    }
    let expr = node.get_node("expr")?;
    if expr.is(NodeClass::ConstantExpression)
        && let Some(Attr::Str(value)) = expr.attr("value")
    {
        return Ok(statement::text(&value, expr.line()));
    }
    if expr.is(NodeClass::BlockReferenceExpression) || expr.is(NodeClass::ParentExpression) {
        expr.set_attr("output", true);
        return Ok(expr);
    }
    Ok(node)
}

impl NodeVisitor for OptimizerNodeVisitor {
    fn enter_node(&mut self, node: Node, _env: &Environment) -> Result<Node, Error> {
        if self.optimizes_for() {
            self.enter_optimize_for(&node)?;
        }
        Ok(node)
    }

    fn leave_node(&mut self, node: Node, _env: &Environment) -> Result<Option<Node>, Error> {
        if self.optimizes_for() {
            self.leave_optimize_for(&node);
        }
        optimize_print_node(node).map(Some)
    }

    fn priority(&self) -> i64 {
        255
    }
}
