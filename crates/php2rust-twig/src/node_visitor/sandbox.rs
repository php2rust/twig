//! Port of Twig\NodeVisitor\SandboxNodeVisitor.php: collects the tags,
//! filters, functions and tests a module uses for the security check,
//! and wraps the children that a node coerces to a string in a
//! `CheckToStringNode`.
//!
//! The lists go to the `CheckSecurityNode` added to `class_end`; PHP keeps
//! them in private properties, here they are the node's attributes
//! `usedFilters`, `usedTags`, `usedFunctions` and `usedTests` (name =>
//! line). `getStringCoercedChildNames()` of each node class
//! (`CoercesChildrenToStringInterface`) is [`string_coerced_child_names`].
//!
//! Upstream: twig/twig src/NodeVisitor/SandboxNodeVisitor.php src/Node/CoercesChildrenToStringInterface.php

use super::{NodeVisitor, truthy_tag};
use crate::{
    callable::TwigCallable,
    environment::Environment,
    error::Error,
    node::{Attr, Key, Node, NodeClass, NodeInterface, statement},
    template::ARRAY_CALL,
    util::{callable_arguments::Parameter, callable_parameters},
};

/// Names in first-use order with the line of the first use.
type Used = Vec<(Vec<u8>, i64)>;

/// `SandboxNodeVisitor`.
#[derive(Debug, Default)]
pub struct SandboxNodeVisitor {
    in_a_module: bool,
    tags: Used,
    filters: Used,
    functions: Used,
    tests: Used,
}

fn add(used: &mut Used, name: &[u8], line: i64) {
    used.push((name.to_vec(), line));
}

fn has(used: &Used, name: &[u8]) -> bool {
    used.iter().any(|(n, _)| n == name)
}

fn name_of(node: &Node) -> Vec<u8> {
    node.attr("name")
        .and_then(|a| a.to_php_string())
        .unwrap_or_default()
}

fn callable_attr(node: &Node, name: &str) -> Option<std::rc::Rc<TwigCallable>> {
    match node.attr(name) {
        Some(Attr::Callable(c)) => Some(c),
        _ => None,
    }
}

/// `isFilterAlwaysAllowedInSandbox()` and the function and test ones.
fn callable_always_allowed(
    node: &Node,
    lookup: impl Fn(&[u8]) -> Option<std::rc::Rc<TwigCallable>>,
) -> bool {
    callable_attr(node, "twig_callable")
        .or_else(|| lookup(&name_of(node)))
        .is_some_and(|c| c.is_always_allowed_in_sandbox())
}

/// `getStringCoercedChildNames()` of a node that implements
/// `CoercesChildrenToStringInterface`.
#[must_use]
pub fn string_coerced_child_names(node: &Node) -> Vec<Key> {
    use NodeClass as C;
    let class = node.class();
    let s = |name: &str| Key::from_bytes(name.as_bytes());
    let if_has = |name: &str| {
        if node.has_node(name) {
            vec![s(name)]
        } else {
            Vec::new()
        }
    };
    if !class.implements(NodeInterface::CoercesChildrenToString) {
        return Vec::new();
    }
    match class {
        C::ImportNode | C::IncludeNode | C::EmbedNode | C::PrintNode => vec![s("expr")],
        C::DeprecatedNode => {
            let mut names = vec![s("expr")];
            names.extend(if_has("package"));
            names.extend(if_has("version"));
            names
        }
        C::ModuleNode => if_has("parent"),
        C::BlockReferenceExpression => if_has("template"),
        C::MacroReferenceExpression => if_has("name"),
        C::DefinedTest
        | C::EvenTest
        | C::OddTest
        | C::NullTest
        | C::SameasTest
        | C::DivisiblebyTest
        | C::TrueTest => Vec::new(),
        C::ArrayExpression | C::VariadicExpression => {
            // dynamic mapping keys are coerced to strings; constant keys
            // and sequence indexes are literals
            let children = node.children();
            children
                .chunks(2)
                .enumerate()
                .filter(|(_, pair)| {
                    let key = pair[0].1.class();
                    !key.is_a(C::ConstantExpression) && !key.is_a(C::TempNameExpression)
                })
                .map(|(i, _)| Key::Int(i64::try_from(i * 2).unwrap_or(i64::MAX)))
                .collect()
        }
        C::GetAttrExpression => {
            let mut names = if_has("arguments");
            if node.attr("type").and_then(|t| t.to_php_string()).as_deref()
                == Some(ARRAY_CALL.as_bytes())
            {
                names.push(s("attribute"));
            }
            names
        }
        _ if class.is_a(C::FilterExpression) => vec![s("node"), s("arguments")],
        _ if class.is_a(C::FunctionExpression) => vec![s("arguments")],
        _ if class.is_a(C::TestExpression) => {
            let mut names = Vec::new();
            if name_of(node) == b"empty" {
                names.push(s("node"));
            }
            names.extend(if_has("arguments"));
            names
        }
        // the binary operators
        _ => vec![s("left"), s("right")],
    }
}

/// `normalizeName()`.
fn normalize_name(name: &[u8]) -> Vec<u8> {
    name.iter()
        .filter(|&&b| b != b'_')
        .map(u8::to_ascii_lowercase)
        .collect()
}

fn is_generator(node: &Node) -> bool {
    node.attr("is_generator").is_some_and(|a| a.is_truthy())
}

/// `wrapNode()`.
fn wrap_node(node: &Node, name: &Key) -> Result<(), Error> {
    use NodeClass as C;
    let Some(expr) = node.node(name.clone()) else {
        return Ok(());
    };
    let class = expr.class();
    // `_self` compiles to the template name, always a string
    if class.is_a(C::ContextVariable) && name_of(&expr) == b"_self" {
        return Ok(());
    }
    if (class.is_a(C::ContextVariable) || class.is_a(C::GetAttrExpression)) && !is_generator(&expr)
    {
        node.set_node(name.clone(), statement::check_to_string(expr, false))?;
    } else if class.is_a(C::SpreadUnary) {
        if let Some(operand) = expr.node("node") {
            expr.set_node("node", statement::check_to_string(operand, true))?;
        }
    } else if class.is_a(C::ArrayExpression) || class == C::Nodes {
        for (key, _) in expr.children() {
            wrap_node(&expr, &key)?;
        }
    } else if expr.implements(NodeInterface::OperatorEscape) {
        for &operand in super::safe_analysis::operand_names_to_escape(&expr) {
            wrap_node(&expr, &Key::from_bytes(operand.as_bytes()))?;
        }
    } else if class.is_a(C::FilterExpression) || class.is_a(C::FunctionExpression) {
        node.set_node(name.clone(), statement::check_to_string(expr, false))?;
    }
    Ok(())
}

/// `wrapArguments()`: wraps the arguments whose parameter can receive a
/// value that coerces to a string.
fn wrap_arguments(node: &Node, params: &[Parameter]) -> Result<(), Error> {
    let Some(arguments) = node.node("arguments") else {
        return Ok(());
    };
    let class = arguments.class();
    if class != NodeClass::Nodes && !class.is_a(NodeClass::ArrayExpression) {
        return wrap_node(node, &Key::from_bytes(b"arguments"));
    }
    // filters and tests pass their input as the first PHP parameter
    let positional = params
        .get(usize::from(node.has_node("node"))..)
        .unwrap_or_default();
    let mut variadic = None;
    let mut by_name: Vec<(Vec<u8>, &Parameter)> = Vec::new();
    for p in positional {
        if p.variadic {
            variadic = Some(p);
            break;
        }
        let normalized = normalize_name(p.name.as_bytes());
        if !by_name.iter().any(|(n, _)| *n == normalized) {
            by_name.push((normalized, p));
        }
    }
    let mut positional_index = 0;
    for (key, _) in arguments.children() {
        let param = match &key {
            Key::Int(_) => {
                let param = positional.get(positional_index).or(variadic);
                if param.is_some_and(|p| !p.variadic) {
                    positional_index += 1;
                }
                param
            }
            Key::Str(s) => {
                let normalized = normalize_name(s);
                by_name
                    .iter()
                    .find(|(n, _)| *n == normalized)
                    .map(|(_, p)| *p)
                    .or(variadic)
            }
        };
        if param.is_some_and(|p| p.coercion_safe) {
            continue;
        }
        wrap_node(&arguments, &key)?;
    }
    Ok(())
}

fn used_attr(used: &Used) -> Attr {
    Attr::Array(
        used.iter()
            .map(|(name, line)| (Key::from_bytes(name), Attr::Int(*line)))
            .collect(),
    )
}

impl NodeVisitor for SandboxNodeVisitor {
    fn enter_node(&mut self, node: Node, env: &Environment) -> Result<Node, Error> {
        use NodeClass as C;
        let class = node.class();
        if class.is_a(C::ModuleNode) {
            self.in_a_module = true;
            self.tags.clear();
            self.filters.clear();
            self.functions.clear();
            self.tests.clear();
        } else if self.in_a_module {
            let line = node.line();
            if let Some(tag) = truthy_tag(&node)
                && !has(&self.tags, tag.as_bytes())
                && !env
                    .token_parser(tag.as_bytes())
                    .is_some_and(|p| p.is_always_allowed_in_sandbox())
            {
                add(&mut self.tags, tag.as_bytes(), line);
            }
            if class.is_a(C::FilterExpression) {
                let name = name_of(&node);
                if !has(&self.filters, &name) && !callable_always_allowed(&node, |n| env.filter(n))
                {
                    add(&mut self.filters, &name, line);
                }
            }
            if class.is_a(C::FunctionExpression) {
                let name = name_of(&node);
                if !has(&self.functions, &name)
                    && !callable_always_allowed(&node, |n| env.function(n))
                {
                    add(&mut self.functions, &name, line);
                }
            }
            if class.is_a(C::TestExpression) {
                let name = name_of(&node);
                if !has(&self.tests, &name) && !callable_always_allowed(&node, |n| env.test(n)) {
                    add(&mut self.tests, &name, line);
                }
            }
            // a function whose parser callable made another node
            // (`parent`, `block`, `attribute`)
            if let Some(name) = node
                .attr("sandboxed_function_name")
                .and_then(|a| a.to_php_string())
                && !has(&self.functions, &name)
                && !callable_attr(&node, "sandboxed_function")
                    .or_else(|| env.function(&name))
                    .is_some_and(|c| c.is_always_allowed_in_sandbox())
            {
                add(&mut self.functions, &name, line);
            }
            // `..` is the range() function
            if class.is_a(C::RangeBinary)
                && !has(&self.functions, b"range")
                && !env
                    .function(b"range")
                    .is_some_and(|c| c.is_always_allowed_in_sandbox())
            {
                add(&mut self.functions, b"range", line);
            }
        }
        if self.in_a_module && class.implements(NodeInterface::CoercesChildrenToString) {
            let params = callable_parameters::from_node(&node, env);
            for child in string_coerced_child_names(&node) {
                if let Some(params) = &params {
                    if child == Key::from_bytes(b"arguments") {
                        wrap_arguments(&node, params)?;
                        continue;
                    }
                    // a filter's input is its first PHP parameter
                    if child == Key::from_bytes(b"node")
                        && class.is_a(C::FilterExpression)
                        && params.first().is_some_and(|p| p.coercion_safe)
                    {
                        continue;
                    }
                }
                wrap_node(&node, &child)?;
            }
        }
        Ok(node)
    }

    fn leave_node(&mut self, node: Node, _env: &Environment) -> Result<Option<Node>, Error> {
        if node.class().is_a(NodeClass::ModuleNode) {
            self.in_a_module = false;
            let call = Node::new(NodeClass::CheckSecurityCallNode, Vec::new(), Vec::new(), 0);
            let start = node.get_node("constructor_start")?;
            node.set_node(
                "constructor_start",
                Node::nodes(vec![(Key::Int(0), call), (Key::Int(1), start)], 0),
            )?;
            let check = Node::new(
                NodeClass::CheckSecurityNode,
                Vec::new(),
                vec![
                    ("usedFilters", used_attr(&self.filters)),
                    ("usedTags", used_attr(&self.tags)),
                    ("usedFunctions", used_attr(&self.functions)),
                    ("usedTests", used_attr(&self.tests)),
                ],
                0,
            );
            let end = node.get_node("class_end")?;
            node.set_node(
                "class_end",
                Node::nodes(vec![(Key::Int(0), check), (Key::Int(1), end)], 0),
            )?;
        }
        Ok(Some(node))
    }

    fn priority(&self) -> i64 {
        0
    }
}
