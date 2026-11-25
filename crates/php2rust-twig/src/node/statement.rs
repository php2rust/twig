//! The constructors of the statement node classes of `src/Node`.
//!
//! Each function copies one PHP constructor: the child nodes, the
//! attributes and their order, the checks and the errors.
//!
//! Upstream: twig/twig src/Node/AutoEscapeNode.php src/Node/BlockNode.php src/Node/BlockReferenceNode.php src/Node/BodyNode.php src/Node/CaptureNode.php src/Node/CheckToStringNode.php src/Node/ConfigNode.php src/Node/DeprecatedNode.php src/Node/DoNode.php src/Node/EmbedNode.php src/Node/FlushNode.php src/Node/ForElseNode.php src/Node/ForLoopNode.php src/Node/ForNode.php src/Node/IfNode.php src/Node/ImportNode.php src/Node/IncludeNode.php src/Node/MacroDeclarationNode.php src/Node/MacroNode.php src/Node/MacrosNode.php src/Node/ModuleNode.php src/Node/PrintNode.php src/Node/SandboxNode.php src/Node/SetNode.php src/Node/TextNode.php src/Node/TypeNode.php src/Node/TypesNode.php src/Node/WithNode.php

use std::sync::Arc;

use super::{
    Attr, Key, Node, NodeClass,
    expression::{self, RESERVED_NAME_PREFIX},
};
use crate::{error::Error, source::Source};

fn key(name: &str) -> Key {
    Key::Str(name.as_bytes().to_vec())
}

fn named(nodes: Vec<(&str, Node)>) -> Vec<(Key, Node)> {
    nodes.into_iter().map(|(k, n)| (key(k), n)).collect()
}

/// `new AutoEscapeNode($value, $body, $lineno)`.
pub fn auto_escape(value: Attr, body: Node, lineno: i64) -> Node {
    Node::new(
        NodeClass::AutoEscapeNode,
        named(vec![("body", body)]),
        vec![("value", value)],
        lineno,
    )
}

/// `new BlockNode($name, $body, $lineno)`.
pub fn block(name: &[u8], body: Node, lineno: i64) -> Node {
    Node::new(
        NodeClass::BlockNode,
        named(vec![("body", body)]),
        vec![("name", Attr::Str(name.to_vec()))],
        lineno,
    )
}

/// `new BlockReferenceNode($name, $lineno)`.
pub fn block_reference(name: &[u8], lineno: i64) -> Node {
    Node::new(
        NodeClass::BlockReferenceNode,
        Vec::new(),
        vec![("name", Attr::Str(name.to_vec()))],
        lineno,
    )
}

/// `new BodyNode([$a, ...], [], $lineno)`.
pub fn body(nodes: Vec<Node>, lineno: i64) -> Node {
    Node::new(
        NodeClass::BodyNode,
        nodes
            .into_iter()
            .enumerate()
            .map(|(i, n)| (Key::from(i), n))
            .collect(),
        Vec::new(),
        lineno,
    )
}

/// `new CaptureNode($body, $lineno)`.
pub fn capture(body: Node, lineno: i64) -> Node {
    Node::new(
        NodeClass::CaptureNode,
        named(vec![("body", body)]),
        vec![("raw", Attr::Bool(false))],
        lineno,
    )
}

/// `new CheckToStringNode($expr, $spread)`.
pub fn check_to_string(expr: Node, spread: bool) -> Node {
    let lineno = expr.line();
    Node::new(
        NodeClass::CheckToStringNode,
        named(vec![("expr", expr)]),
        vec![("spread", Attr::Bool(spread))],
        lineno,
    )
}

/// `new DeprecatedNode($expr, $lineno)`.
pub fn deprecated(expr: Node, lineno: i64) -> Node {
    Node::new(
        NodeClass::DeprecatedNode,
        named(vec![("expr", expr)]),
        Vec::new(),
        lineno,
    )
}

/// `new DoNode($expr, $lineno)`.
pub fn do_node(expr: Node, lineno: i64) -> Node {
    Node::new(
        NodeClass::DoNode,
        named(vec![("expr", expr)]),
        Vec::new(),
        lineno,
    )
}

/// `new IncludeNode($expr, $variables, $only, $ignoreMissing, $lineno)`.
pub fn include(
    expr: Node,
    variables: Option<Node>,
    only: bool,
    ignore_missing: bool,
    lineno: i64,
) -> Node {
    include_of_class(
        NodeClass::IncludeNode,
        expr,
        variables,
        only,
        ignore_missing,
        lineno,
    )
}

fn include_of_class(
    class: NodeClass,
    expr: Node,
    variables: Option<Node>,
    only: bool,
    ignore_missing: bool,
    lineno: i64,
) -> Node {
    let mut nodes = vec![("expr", expr)];
    if let Some(variables) = variables {
        nodes.push(("variables", variables));
    }
    Node::new(
        class,
        named(nodes),
        vec![
            ("only", Attr::Bool(only)),
            ("ignore_missing", Attr::Bool(ignore_missing)),
        ],
        lineno,
    )
}

/// `new EmbedNode($name, $index, $variables, $only, $ignoreMissing, $lineno)`.
pub fn embed(
    name: &[u8],
    index: i64,
    variables: Option<Node>,
    only: bool,
    ignore_missing: bool,
    lineno: i64,
) -> Node {
    let node = include_of_class(
        NodeClass::EmbedNode,
        expression::constant("not_used", lineno),
        variables,
        only,
        ignore_missing,
        lineno,
    );
    node.set_attr("name", Attr::Str(name.to_vec()));
    node.set_attr("index", index);
    node
}

/// `new FlushNode($lineno)`.
pub fn flush(lineno: i64) -> Node {
    Node::new(NodeClass::FlushNode, Vec::new(), Vec::new(), lineno)
}

/// `new ForElseNode($body, $lineno)`.
pub fn for_else(body: Node, lineno: i64) -> Node {
    Node::new(
        NodeClass::ForElseNode,
        named(vec![("body", body)]),
        Vec::new(),
        lineno,
    )
}

/// `new ForNode($keyTarget, $valueTarget, $seq, null, $body, $else, $lineno)`.
pub fn for_node(
    key_target: Node,
    value_target: Node,
    seq: Node,
    body: Node,
    else_node: Option<Node>,
    lineno: i64,
) -> Node {
    let for_loop = Node::new(
        NodeClass::ForLoopNode,
        Vec::new(),
        vec![
            ("with_loop", Attr::Bool(false)),
            ("ifexpr", Attr::Bool(false)),
            ("else", Attr::Bool(false)),
        ],
        lineno,
    );
    let body = Node::list(vec![body, for_loop], 0);
    let mut nodes = vec![
        ("key_target", key_target),
        ("value_target", value_target),
        ("seq", seq),
        ("body", body),
    ];
    if let Some(else_node) = else_node {
        let else_node = if else_node.is(NodeClass::ForElseNode) {
            else_node
        } else {
            let line = else_node.line();
            for_else(else_node, line)
        };
        nodes.push(("else", else_node));
    }
    Node::new(
        NodeClass::ForNode,
        named(nodes),
        vec![("with_loop", Attr::Bool(true))],
        lineno,
    )
}

/// `new IfNode($tests, $else, $lineno)`.
pub fn if_node(tests: Node, else_node: Option<Node>, lineno: i64) -> Result<Node, Error> {
    let count = tests.count();
    let mut i = 0;
    while i < count {
        let test = tests.get_node(Key::from(i))?;
        tests.set_node(Key::from(i), expression::true_test_wrap(test))?;
        i += 2;
    }
    let mut nodes = vec![("tests", tests)];
    if let Some(else_node) = else_node {
        nodes.push(("else", else_node));
    }
    Ok(Node::new(
        NodeClass::IfNode,
        named(nodes),
        Vec::new(),
        lineno,
    ))
}

/// `new ImportNode($expr, $var, $lineno)`.
pub fn import(expr: Node, var: Node, lineno: i64) -> Node {
    Node::new(
        NodeClass::ImportNode,
        named(vec![("expr", expr), ("var", var)]),
        Vec::new(),
        lineno,
    )
}

/// `new MacroDeclarationNode($name, $lineno)`.
pub fn macro_declaration(name: &[u8], lineno: i64) -> Node {
    let node = Node::new(
        NodeClass::MacroDeclarationNode,
        Vec::new(),
        Vec::new(),
        lineno,
    );
    node.set_attr("name", Attr::Str(name.to_vec()));
    node
}

/// `MacroNode::VARARGS_NAME`.
pub const VARARGS_NAME: &str = "varargs";

fn strip_reserved_prefix(name: &[u8]) -> &[u8] {
    name.strip_prefix(RESERVED_NAME_PREFIX).unwrap_or(name)
}

/// `new MacroNode($name, $body, $arguments, $lineno, $variadicName)`.
pub fn macro_node(
    name: &[u8],
    body: Node,
    arguments: Node,
    lineno: i64,
    variadic_name: Option<&[u8]>,
) -> Result<Node, Error> {
    let display = String::from_utf8_lossy(name).into_owned();
    let mut seen: Vec<Vec<u8>> = Vec::new();
    let mut reserved = RESERVED_NAME_PREFIX.to_vec();
    reserved.extend_from_slice(VARARGS_NAME.as_bytes());
    for (key, value) in arguments.key_value_pairs() {
        let arg_name = key.get_attr("name")?.to_php_string().unwrap_or_default();
        if arg_name == reserved {
            return Err(Error::syntax(
                format!(
                    "The argument \"{VARARGS_NAME}\" in macro \"{display}\" cannot be defined because the variable \"{VARARGS_NAME}\" is reserved for arbitrary arguments."
                ),
                value.line(),
                value.source_context(),
            ));
        }
        if let Some(variadic) = variadic_name
            && variadic == strip_reserved_prefix(&arg_name)
        {
            return Err(Error::syntax(
                format!(
                    "The variadic argument \"{}\" in macro \"{display}\" cannot have the same name as another argument.",
                    String::from_utf8_lossy(variadic)
                ),
                value.line(),
                value.source_context(),
            ));
        }
        if seen.contains(&arg_name) {
            return Err(Error::syntax(
                format!(
                    "Argument \"{}\" is defined twice for macro \"{display}\".",
                    String::from_utf8_lossy(strip_reserved_prefix(&arg_name))
                ),
                value.line(),
                value.source_context(),
            ));
        }
        seen.push(arg_name);
    }
    Ok(Node::new(
        NodeClass::MacroNode,
        named(vec![("body", body), ("arguments", arguments)]),
        vec![
            ("name", Attr::Str(name.to_vec())),
            (
                "variadic_name",
                variadic_name.map_or(Attr::Null, |v| Attr::Str(v.to_vec())),
            ),
        ],
        lineno,
    ))
}

/// `new MacrosNode($macros)`.
pub fn macros(macros: Vec<(Key, Node)>) -> Node {
    Node::new(NodeClass::MacrosNode, macros, Vec::new(), 0)
}

/// `new ModuleNode($body, $parent, $blocks, $macros, $traits, $embeddedTemplates, $source)`.
pub fn module(
    body: Node,
    parent: Option<Node>,
    blocks: Node,
    macros: Node,
    traits: Node,
    embedded_templates: Node,
    source: &Arc<Source>,
) -> Node {
    let mut nodes = vec![
        ("body", body),
        ("blocks", blocks),
        ("macros", macros),
        ("traits", traits),
        ("display_start", Node::nodes(Vec::new(), 0)),
        ("display_end", Node::nodes(Vec::new(), 0)),
        ("constructor_start", Node::nodes(Vec::new(), 0)),
        ("constructor_end", Node::nodes(Vec::new(), 0)),
        ("class_end", Node::nodes(Vec::new(), 0)),
    ];
    if let Some(parent) = parent {
        nodes.push(("parent", parent));
    }
    let node = Node::new(
        NodeClass::ModuleNode,
        named(nodes),
        vec![
            ("index", Attr::Null),
            ("embedded_templates", Attr::Node(embedded_templates)),
            ("strategy", Attr::Bool(false)),
            ("escaper", Attr::Bool(false)),
        ],
        1,
    );
    node.set_source_context(source);
    node
}

/// `new PrintNode($expr, $lineno)`.
pub fn print(expr: Node, lineno: i64) -> Node {
    Node::new(
        NodeClass::PrintNode,
        named(vec![("expr", expr)]),
        Vec::new(),
        lineno,
    )
}

/// `new SandboxNode($body, $lineno)`.
pub fn sandbox(body: Node, lineno: i64) -> Node {
    Node::new(
        NodeClass::SandboxNode,
        named(vec![("body", body)]),
        Vec::new(),
        lineno,
    )
}

/// `new SetNode($capture, $names, $values, $lineno)`.
pub fn set(capture: bool, names: Node, values: Node, lineno: i64) -> Result<Node, Error> {
    let mut capture = capture;
    let mut values = values;
    let mut safe = false;
    if capture {
        safe = true;
        if (values.class() == NodeClass::Nodes || values.class() == NodeClass::Node)
            && values.count() == 0
        {
            values = expression::constant("", values.line());
            capture = false;
        } else if values.is(NodeClass::TextNode) {
            values = expression::constant(values.get_attr("data")?, values.line());
            capture = false;
        } else if values.is(NodeClass::PrintNode)
            && values.get_node("expr")?.is(NodeClass::ConstantExpression)
        {
            values = values.get_node("expr")?;
            capture = false;
        } else {
            let line = values.line();
            values = capture_node(values, line);
        }
    }
    Ok(Node::new(
        NodeClass::SetNode,
        named(vec![("names", names), ("values", values)]),
        vec![("capture", Attr::Bool(capture)), ("safe", Attr::Bool(safe))],
        lineno,
    ))
}

fn capture_node(body: Node, lineno: i64) -> Node {
    capture(body, lineno)
}

/// `new TextNode($data, $lineno)`.
pub fn text(data: &[u8], lineno: i64) -> Node {
    Node::new(
        NodeClass::TextNode,
        Vec::new(),
        vec![("data", Attr::Str(data.to_vec()))],
        lineno,
    )
}

impl Node {
    /// `TextNode::isBlank()`.
    pub fn is_blank(&self) -> bool {
        let data = self.attr_bytes("data").unwrap_or_default();
        if ctype_space(&data) {
            return true;
        }
        if self.repr().windows(3).any(|w| w == b"\xEF\xBB\xBF") {
            let rest = data.get(3..).unwrap_or_default();
            if rest.is_empty() || ctype_space(rest) {
                return true;
            }
        }
        false
    }
}

/// `ctype_space()`: not empty, and only " \t\n\r\x0b\x0c".
fn ctype_space(text: &[u8]) -> bool {
    !text.is_empty() && text.iter().all(|b| b" \t\n\r\x0b\x0c".contains(b))
}

/// One `{% types %}` entry: the type and whether it is optional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeEntry {
    pub name: Vec<u8>,
    pub type_name: Vec<u8>,
    pub optional: bool,
}

/// `new TypesNode($types, $lineno)`.
pub fn types(types: Vec<TypeEntry>, lineno: i64) -> Node {
    let mut nodes = Vec::new();
    let mut mapping = Vec::new();
    for entry in types {
        nodes.push((
            Key::from_bytes(&entry.name),
            Node::new(
                NodeClass::TypeNode,
                Vec::new(),
                vec![
                    ("name", Attr::Str(entry.name.clone())),
                    ("type", Attr::Str(entry.type_name.clone())),
                    ("optional", Attr::Bool(entry.optional)),
                ],
                lineno,
            ),
        ));
        mapping.push((
            Key::from_bytes(&entry.name),
            Attr::Array(vec![
                (key("type"), Attr::Str(entry.type_name)),
                (key("optional"), Attr::Bool(entry.optional)),
            ]),
        ));
    }
    Node::new(
        NodeClass::TypesNode,
        nodes,
        vec![("mapping", Attr::Array(mapping))],
        lineno,
    )
}

/// `new WithNode($body, $variables, $only, $lineno)`.
pub fn with(body: Node, variables: Option<Node>, only: bool, lineno: i64) -> Node {
    let mut nodes = vec![("body", body)];
    if let Some(variables) = variables {
        nodes.push(("variables", variables));
    }
    Node::new(
        NodeClass::WithNode,
        named(nodes),
        vec![("only", Attr::Bool(only))],
        lineno,
    )
}
