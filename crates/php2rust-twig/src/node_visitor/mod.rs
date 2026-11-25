//! Port of Twig\NodeVisitor\NodeVisitorInterface.php and Twig\NodeTraverser.php:
//! node visitors and the traverser that runs them on a parsed module.
//!
//! A visitor is made for each parser (`Environment::node_visitors()`), as
//! PHP's parser takes the environment's visitors once. `enter_node()` runs
//! before the children, `leave_node()` after them; a `None` from
//! `leave_node()` removes the node from its parent.
//!
//! Not ported: `AbstractNodeVisitor` (deprecated since 3.9) and
//! `YieldNotReadyNodeVisitor`, which only reports node classes without the
//! `#[YieldReady]` attribute; every node class of the port is ready.
//! `SandboxNodeVisitor` comes with the sandbox.
//!
//! Upstream: twig/twig src/NodeVisitor/NodeVisitorInterface.php src/NodeTraverser.php

pub mod correctness;
pub mod escaper;
pub mod optimizer;
pub mod safe_analysis;

use crate::{environment::Environment, error::Error, node::Node};

/// `NodeVisitorInterface`.
pub trait NodeVisitor {
    /// `enterNode()`: called before the child nodes are visited.
    fn enter_node(&mut self, node: Node, env: &Environment) -> Result<Node, Error>;

    /// `leaveNode()`: called after the child nodes are visited; `None`
    /// removes the node.
    fn leave_node(&mut self, node: Node, env: &Environment) -> Result<Option<Node>, Error>;

    /// `getPriority()`: from -10 to 10 by convention, lower runs first.
    fn priority(&self) -> i64;
}

/// `NodeTraverser::traverse()`: every visitor in priority order (stable
/// for equal priorities), each over the whole tree.
pub fn traverse(
    env: &Environment,
    visitors: &mut [Box<dyn NodeVisitor>],
    mut node: Node,
) -> Result<Node, Error> {
    let mut order: Vec<usize> = (0..visitors.len()).collect();
    order.sort_by_key(|&i| visitors[i].priority());
    for i in order {
        // The root is never removed: PHP returns null from traverse() and
        // the caller's ModuleNode type check fails; keep the node.
        if let Some(n) = traverse_for_visitor(env, visitors[i].as_mut(), node.clone())? {
            node = n;
        }
    }
    Ok(node)
}

/// `NodeTraverser::traverseForVisitor()`.
pub fn traverse_for_visitor(
    env: &Environment,
    visitor: &mut dyn NodeVisitor,
    node: Node,
) -> Result<Option<Node>, Error> {
    let node = visitor.enter_node(node, env)?;
    // PHP iterates over a copy of the children.
    for (key, child) in node.children() {
        match traverse_for_visitor(env, visitor, child.clone())? {
            Some(m) => {
                if !m.ptr_eq(&child) {
                    node.set_node(key, m)?;
                }
            }
            None => node.remove_node(key),
        }
    }
    visitor.leave_node(node, env)
}

/// `getNodeTag()` as a PHP condition: a tag that is neither `""` nor `"0"`.
pub(crate) fn truthy_tag(node: &Node) -> Option<String> {
    node.tag().filter(|t| !t.is_empty() && t != "0")
}
