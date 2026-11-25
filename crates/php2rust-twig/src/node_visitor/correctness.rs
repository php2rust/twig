//! Port of Twig\NodeVisitor\CorrectnessNodeVisitor.php: the checks that
//! need the whole module (content outside blocks in a child template,
//! `extends` placement, `block` under an output tag, `extends`/`use`
//! outside the root, macro calls without parentheses).
//!
//! Upstream: twig/twig src/NodeVisitor/CorrectnessNodeVisitor.php

use std::collections::HashSet;

use symfony_deprecation_contracts::trigger_deprecation;

use super::{NodeVisitor, truthy_tag};
use crate::{
    environment::Environment,
    error::Error,
    node::{Node, NodeClass, NodeInterface},
};

/// `CorrectnessNodeVisitor`.
#[derive(Default)]
pub struct CorrectnessNodeVisitor {
    /// `$rootNodes` (a `WeakMap` by identity); `None` outside a module.
    root_nodes: Option<Vec<Node>>,
    reported_macro_call_sites: HashSet<Vec<u8>>,
    /// The output-wrapping tags currently open.
    tag_stack: Vec<Node>,
    has_parent: bool,
    block_depth: usize,
    macro_depth: usize,
    capturing_node_depth: usize,
    has_extends: bool,
}

impl NodeVisitor for CorrectnessNodeVisitor {
    fn enter_node(&mut self, node: Node, _env: &Environment) -> Result<Node, Error> {
        if node.is(NodeClass::ModuleNode) {
            self.enter_module(&node)?;
            return Ok(node);
        }
        self.enter_scope(&node);
        if node.is(NodeClass::ConfigNode) {
            self.check_config_tag(&node)?;
        }
        if node.is(NodeClass::MacroReferenceExpression) {
            self.check_macro_call_parentheses(&node);
        }
        if node.is(NodeClass::BlockReferenceNode) {
            self.check_block_definition_nesting(&node)?;
        }
        Ok(node)
    }

    fn leave_node(&mut self, node: Node, _env: &Environment) -> Result<Option<Node>, Error> {
        if node.is(NodeClass::ModuleNode) {
            self.reset_state();
            return Ok(Some(node));
        }
        self.leave_scope(&node);
        Ok(Some(node))
    }

    fn priority(&self) -> i64 {
        -255
    }
}

fn source_name(node: &Node) -> String {
    node.template_name().unwrap_or_default()
}

impl CorrectnessNodeVisitor {
    fn enter_module(&mut self, node: &Node) -> Result<(), Error> {
        self.reset_state();
        let mut roots = Vec::new();
        self.has_parent = node.has_node("parent");
        for n in root_nodes_of(node)? {
            if self.has_parent && !is_empty_output_node(&n) {
                return Err(Error::syntax(
                    "A template that extends another one cannot include content outside Twig blocks. Did you forget to put the content inside a {% block %} tag?",
                    n.line(),
                    n.source_context(),
                ));
            }
            roots.push(n);
        }
        self.root_nodes = Some(roots);
        Ok(())
    }

    fn reset_state(&mut self) {
        *self = Self::default();
    }

    fn enter_scope(&mut self, node: &Node) {
        if node.implements(NodeInterface::NodeCapture) {
            self.capturing_node_depth += 1;
        }
        if node.is(NodeClass::BlockNode) {
            self.block_depth += 1;
        } else if node.is(NodeClass::MacroNode) {
            self.macro_depth += 1;
        } else if truthy_tag(node).is_some() && !node.is(NodeClass::BlockReferenceNode) {
            self.tag_stack.push(node.clone());
        }
    }

    fn leave_scope(&mut self, node: &Node) {
        if node.implements(NodeInterface::NodeCapture) {
            self.capturing_node_depth = self.capturing_node_depth.saturating_sub(1);
        }
        if node.is(NodeClass::BlockNode) {
            self.block_depth = self.block_depth.saturating_sub(1);
        } else if node.is(NodeClass::MacroNode) {
            self.macro_depth = self.macro_depth.saturating_sub(1);
        } else if truthy_tag(node).is_some() && !node.is(NodeClass::BlockReferenceNode) {
            self.tag_stack.pop();
        }
    }

    fn check_macro_call_parentheses(&mut self, node: &Node) {
        if node.has_call_parentheses() || node.is_defined_test_enabled() {
            return;
        }
        let source_name = source_name(node);
        let line = node.line();
        // A dynamic macro name is only known at runtime.
        let name = node.attr_bytes("name");

        // A single call site can be visited more than once: "??" reuses its left node,
        // while "?:" and the "default" filter put a clone of it in the compiled tree.
        let mut call_site = source_name.as_bytes().to_vec();
        call_site.push(0);
        call_site.extend_from_slice(line.to_string().as_bytes());
        call_site.push(0);
        call_site.extend_from_slice(name.as_deref().unwrap_or_default());
        if !self.reported_macro_call_sites.insert(call_site) {
            return;
        }

        let message = match name {
            None => format!(
                "Omitting parentheses when calling a macro is deprecated and will throw a SyntaxError in Twig 4.0; add parentheses after the macro name in \"{source_name}\" at line {line}."
            )
            .into_bytes(),
            Some(name) => {
                let mut m = b"Omitting parentheses when calling the macro \"".to_vec();
                m.extend_from_slice(&name);
                m.extend_from_slice(
                    format!(
                        "\" is deprecated and will throw a SyntaxError in Twig 4.0; add parentheses after the macro name in \"{source_name}\" at line {line}."
                    )
                    .as_bytes(),
                );
                m
            }
        };
        trigger_deprecation(b"twig/twig", b"3.29", &message);
    }

    fn check_config_tag(&mut self, node: &Node) -> Result<(), Error> {
        let tag = node.tag().unwrap_or_default();
        if tag == "extends" {
            self.check_extends_tag(node)?;
        }
        let at_root = self
            .root_nodes
            .as_ref()
            .is_some_and(|roots| roots.iter().any(|r| r.ptr_eq(node)));
        if !at_root {
            trigger_deprecation(
                b"twig/twig",
                b"3.28",
                format!(
                    "Using the \"{tag}\" tag outside the root of a template is deprecated in {} at line {}.",
                    source_name(node),
                    node.line()
                )
                .as_bytes(),
            );
        }
        Ok(())
    }

    fn check_extends_tag(&mut self, node: &Node) -> Result<(), Error> {
        // "extends" inside a "block" or a "macro" has always been a hard error; keep it
        let message = if self.block_depth > 0 {
            Some("Cannot use \"extend\" in a block.")
        } else if self.macro_depth > 0 {
            Some("Cannot use \"extend\" in a macro.")
        } else if self.has_extends {
            Some("Multiple extends tags are forbidden.")
        } else {
            None
        };
        if let Some(message) = message {
            return Err(Error::syntax(message, node.line(), node.source_context()));
        }
        self.has_extends = true;
        Ok(())
    }

    fn check_block_definition_nesting(&self, node: &Node) -> Result<(), Error> {
        // A "block" definition nested under an output-wrapping tag is registered globally
        // regardless of that tag, so the nesting is misleading. This only matters at the root
        // of a child template's body: once inside a block, a macro, an output capture, or in
        // a standalone template, the block is rendered in place and behaves like any other.
        if !self.has_parent
            || self.block_depth > 0
            || self.macro_depth > 0
            || self.capturing_node_depth > 0
        {
            return Ok(());
        }
        let Some(tag) = self.tag_stack.last() else {
            return Ok(());
        };
        Err(Error::syntax(
            format!(
                "A \"block\" tag cannot be under a \"{}\" tag (line {}).",
                tag.tag().unwrap_or_default(),
                tag.line()
            ),
            node.line(),
            node.source_context(),
        ))
    }
}

/// `getRootNodes()`.
fn root_nodes_of(module: &Node) -> Result<Vec<Node>, Error> {
    let body = module.get_node("body")?.get_node(0_i64)?;
    // Parser::subparse() does not wrap the parsed nodes when there is only one,
    // so $body can be a single "real" node instead of a Nodes container; in that
    // case it is the only root node and must not be iterated over its children.
    Ok(
        if body.is(NodeClass::Nodes) || body.class() == NodeClass::Node {
            body.child_nodes()
        } else {
            vec![body]
        },
    )
}

/// `isEmptyOutputNode()`: whether the node never outputs anything or its
/// output is empty.
fn is_empty_output_node(node: &Node) -> bool {
    if node.implements(NodeInterface::NodeCapture) {
        // a "block" tag in such a node will serve as a block definition AND be displayed in place as well
        return true;
    }
    // Can the text be considered "empty" (only whitespace)?
    if node.is(NodeClass::TextNode) {
        return node.is_blank();
    }
    if !node.is(NodeClass::BlockReferenceNode) && node.implements(NodeInterface::NodeOutput) {
        return false;
    }
    node.child_nodes().iter().all(is_empty_output_node)
}
