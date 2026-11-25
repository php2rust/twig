//! Port of Twig\NodeVisitor\EscaperNodeVisitor.php: auto-escaping. Each
//! print gets an `escape` filter for the current strategy unless the safe
//! analysis shows the value is already safe; filters with a `pre_escape`
//! option get their input escaped.
//!
//! Upstream: twig/twig src/NodeVisitor/EscaperNodeVisitor.php

use std::collections::HashMap;

use super::{
    NodeVisitor,
    safe_analysis::{SafeAnalysisNodeVisitor, operand_names_to_escape},
    traverse_for_visitor,
};
use crate::{
    environment::Environment,
    error::Error,
    node::{Attr, Node, NodeClass, NodeInterface, expression},
};

/// A strategy as the visitor holds it: `false` (no escaping) or a value
/// PHP passes to a `string` parameter.
#[derive(Clone, Debug)]
enum Strategy {
    Off,
    On(Vec<u8>),
    /// A value PHP cannot pass as a `string` (null, an array): a
    /// `TypeError` when it is used.
    Invalid(&'static str),
}

impl Strategy {
    /// An `autoescape` tag value.
    fn from_attr(value: &Attr) -> Self {
        match value {
            Attr::Bool(false) => Self::Off,
            Attr::Null => Self::Invalid("null"),
            Attr::Array(_) => Self::Invalid("array"),
            other => other
                .to_php_string()
                .map_or(Self::Invalid("object"), Self::On),
        }
    }
}

/// `EscaperNodeVisitor`.
#[derive(Default)]
pub struct EscaperNodeVisitor {
    status_stack: Vec<Strategy>,
    blocks: HashMap<Vec<u8>, Strategy>,
    safe_analysis: SafeAnalysisNodeVisitor,
    /// `$defaultStrategy`: a strategy or `false`.
    default_strategy: Option<Vec<u8>>,
    safe_vars: Vec<Vec<u8>>,
    uses_escaper: bool,
}

impl NodeVisitor for EscaperNodeVisitor {
    fn enter_node(&mut self, node: Node, env: &Environment) -> Result<Node, Error> {
        if node.is(NodeClass::ModuleNode) {
            let name = node.template_name().unwrap_or_default();
            if let Some(strategy) = env.default_escaping_strategy(&name).filter(|s| truthy(s)) {
                self.default_strategy = Some(strategy);
            }
            node.set_attr(
                "strategy",
                self.default_strategy
                    .clone()
                    .map_or(Attr::Bool(false), Attr::Str),
            );
            self.safe_vars.clear();
            self.blocks.clear();
            self.uses_escaper = false;
        } else if node.is(NodeClass::AutoEscapeNode) {
            self.status_stack
                .push(Strategy::from_attr(&node.get_attr("value")?));
        } else if node.is(NodeClass::BlockNode) {
            let name = node.attr_bytes("name").unwrap_or_default();
            // `??`: a stored null counts as unset
            let status = match self.blocks.get(&name) {
                Some(Strategy::Invalid("null")) | None => self.need_escaping(),
                Some(status) => status.clone(),
            };
            self.status_stack.push(status);
        } else if node.is(NodeClass::ImportNode)
            && let Attr::Str(name) = node.get_node("var")?.get_node("var")?.get_attr("name")? {
                self.safe_vars.push(name);
            }
        Ok(node)
    }

    fn leave_node(&mut self, node: Node, env: &Environment) -> Result<Option<Node>, Error> {
        if node.is(NodeClass::ModuleNode) {
            node.set_attr("escaper", self.uses_escaper);
            self.default_strategy = None;
            self.safe_vars.clear();
            self.blocks.clear();
        } else if node.is(NodeClass::FilterExpression) {
            if node.is(NodeClass::EscapeFilter) {
                self.use_template_escaper(&node);
            }
            return self.pre_escape_filter_node(node, env).map(Some);
        } else if node.is(NodeClass::PrintNode) {
            let strategy = self.need_escaping();
            if !matches!(strategy, Strategy::Off) {
                let expression = node.get_node("expr")?;
                if expression.implements(NodeInterface::OperatorEscape) {
                    self.escape_conditional(&expression, env, &strategy)?;
                } else {
                    let escaped = self.escape_expression(expression, env, &strategy)?;
                    node.set_node("expr", escaped)?;
                }
                return Ok(Some(node));
            }
        }

        if node.is(NodeClass::AutoEscapeNode) || node.is(NodeClass::BlockNode) {
            self.status_stack.pop();
        } else if node.is(NodeClass::BlockReferenceNode) {
            let name = node.attr_bytes("name").unwrap_or_default();
            let status = self.need_escaping();
            self.blocks.insert(name, status);
        }
        Ok(Some(node))
    }

    fn priority(&self) -> i64 {
        0
    }
}

/// PHP truthiness of a string.
fn truthy(s: &[u8]) -> bool {
    !s.is_empty() && s != b"0"
}

/// The `string $type` parameter: PHP's `TypeError` for a value it cannot
/// coerce.
fn strategy_name(strategy: &Strategy) -> Result<&[u8], Error> {
    match strategy {
        Strategy::On(s) => Ok(s),
        Strategy::Off => Ok(b""),
        Strategy::Invalid(given) => Err(Error::logic(format!(
            "Twig\\NodeVisitor\\EscaperNodeVisitor::escapeExpression(): Argument #3 ($type) must be of type string, {given} given"
        ))),
    }
}

impl EscaperNodeVisitor {
    /// `escapeConditional()`.
    fn escape_conditional(
        &mut self,
        expression: &Node,
        env: &Environment,
        strategy: &Strategy,
    ) -> Result<(), Error> {
        for &name in operand_names_to_escape(expression) {
            let operand = expression.get_node(name)?;
            if operand.implements(NodeInterface::OperatorEscape) {
                self.escape_conditional(&operand, env, strategy)?;
            } else {
                let escaped = self.escape_expression(operand, env, strategy)?;
                expression.set_node(name, escaped)?;
            }
        }
        Ok(())
    }

    /// `escapeExpression()`.
    fn escape_expression(
        &mut self,
        expression: Node,
        env: &Environment,
        strategy: &Strategy,
    ) -> Result<Node, Error> {
        let name = strategy_name(strategy)?.to_vec();
        if self.is_safe_for(&name, &expression, env)? {
            Ok(expression)
        } else {
            self.get_escaper_filter(env, &name, expression)
        }
    }

    /// `preEscapeFilterNode()`.
    fn pre_escape_filter_node(&mut self, filter: Node, env: &Environment) -> Result<Node, Error> {
        let callable = match filter.attr("twig_callable") {
            Some(Attr::Callable(c)) => Some(c),
            // legacy
            _ => filter.attr_bytes("name").and_then(|name| env.filter(&name)),
        };
        let Some(strategy) = callable.and_then(|c| c.get_pre_escape().map(<[u8]>::to_vec)) else {
            return Ok(filter);
        };
        let node = filter.get_node("node")?;
        if self.is_safe_for(&strategy, &node, env)? {
            return Ok(filter);
        }
        let escaped = self.get_escaper_filter(env, &strategy, node)?;
        filter.set_node("node", escaped)?;
        Ok(filter)
    }

    /// `isSafeFor()`.
    fn is_safe_for(
        &mut self,
        strategy: &[u8],
        expression: &Node,
        env: &Environment,
    ) -> Result<bool, Error> {
        let mut safe = self.safe_analysis.get_safe(expression);
        if safe.is_empty() {
            self.safe_analysis.set_safe_vars(self.safe_vars.clone());
            traverse_for_visitor(env, &mut self.safe_analysis, expression.clone())?;
            safe = self.safe_analysis.get_safe(expression);
        }
        Ok(safe.iter().any(|s| s == strategy || s == b"all"))
    }

    /// `needEscaping()`.
    fn need_escaping(&self) -> Strategy {
        if let Some(top) = self.status_stack.last() {
            return top.clone();
        }
        self.default_strategy
            .clone()
            .filter(|s| truthy(s))
            .map_or(Strategy::Off, Strategy::On)
    }

    /// `getEscaperFilter()`.
    fn get_escaper_filter(
        &mut self,
        env: &Environment,
        strategy: &[u8],
        node: Node,
    ) -> Result<Node, Error> {
        let line = node.line();
        let Some(filter) = env.filter(b"escape") else {
            return Err(Error::logic(
                "Call to a member function getNodeClass() on null",
            ));
        };
        let arguments = Node::list(
            vec![
                expression::constant(Attr::Str(strategy.to_vec()), line),
                expression::constant(Attr::Null, line),
                expression::constant(true, line),
            ],
            0,
        );
        let class = filter.get_node_class();
        let escaped = expression::filter(class, node, filter, arguments, line)?;
        if escaped.is(NodeClass::EscapeFilter) {
            self.use_template_escaper(&escaped);
        }
        Ok(escaped)
    }

    /// `useTemplateEscaper()`.
    fn use_template_escaper(&mut self, filter: &Node) {
        filter.set_attr("template_escaper", true);
        self.uses_escaper = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autoescape_values_coerce_as_php_strings() {
        assert!(matches!(
            Strategy::from_attr(&Attr::Bool(false)),
            Strategy::Off
        ));
        assert!(matches!(Strategy::from_attr(&Attr::Bool(true)), Strategy::On(s) if s == b"1"));
        assert!(matches!(Strategy::from_attr(&Attr::Int(3)), Strategy::On(s) if s == b"3"));
        assert!(matches!(
            Strategy::from_attr(&Attr::Null),
            Strategy::Invalid("null")
        ));
    }
}
