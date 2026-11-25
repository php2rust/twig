//! Port of Twig\Node\Node.php, Nodes.php, EmptyNode.php and
//! NodeDocumentation.php: the generic node tree.
//!
//! As in PHP, a node is a class, named child nodes, named attributes, a
//! line, a tag and documentation, and nodes are handles: [`Node`] clones
//! share the node (PHP object assignment), [`Node::deep_clone`] is PHP's
//! `clone`. Upstream relies on sharing: `NullCoalesceBinary` puts its left
//! operand in two places and a later change to it shows in both.
//!
//! The constructors of the node classes are in the sibling modules; the
//! `compile()` methods are the Rust emitter's job. Node names and
//! attribute names that upstream deprecates (`NameDeprecation`) are not
//! tracked: no ported code reads them.
//!
//! Upstream: twig/twig src/Node/Node.php src/Node/Nodes.php src/Node/EmptyNode.php src/Node/NodeDocumentation.php

pub mod class;
pub mod expression;
pub mod statement;

use std::{cell::RefCell, fmt, rc::Rc, sync::Arc};

pub use class::{NodeClass, NodeInterface};

use crate::{callable::TwigCallable, error::Error, source::Source, token::Token};

/// A child node's or array element's key: PHP's int or string array key.
/// A string that is a canonical decimal int is stored as the int, as PHP
/// does (`$nodes['0']` is `$nodes[0]`). Strings are bytes: a name token
/// can hold any byte from 0x7f up.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Int(i64),
    Str(Vec<u8>),
}

impl Key {
    /// The key as PHP's `(string)` gives it.
    pub fn to_php_string(&self) -> Vec<u8> {
        match self {
            Self::Int(i) => i.to_string().into_bytes(),
            Self::Str(s) => s.clone(),
        }
    }

    /// PHP's key cast of a string.
    pub fn from_bytes(s: &[u8]) -> Self {
        let digits = s.strip_prefix(b"-").unwrap_or(s);
        let canonical = !digits.is_empty()
            && digits.iter().all(u8::is_ascii_digit)
            && (digits == b"0" || digits[0] != b'0')
            && s != b"-0";
        if canonical
            && let Some(i) = std::str::from_utf8(s)
                .ok()
                .and_then(|t| t.parse::<i64>().ok())
        {
            return Self::Int(i);
        }
        Self::Str(s.to_vec())
    }
}

impl From<&str> for Key {
    fn from(s: &str) -> Self {
        Self::from_bytes(s.as_bytes())
    }
}

impl From<&[u8]> for Key {
    fn from(s: &[u8]) -> Self {
        Self::from_bytes(s)
    }
}

impl From<Vec<u8>> for Key {
    fn from(s: Vec<u8>) -> Self {
        Self::from_bytes(&s)
    }
}

impl From<i64> for Key {
    fn from(i: i64) -> Self {
        Self::Int(i)
    }
}

impl From<usize> for Key {
    fn from(i: usize) -> Self {
        Self::Int(i64::try_from(i).unwrap_or(i64::MAX))
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.to_php_string()))
    }
}

/// A node attribute value.
#[derive(Debug, Clone)]
pub enum Attr {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Vec<u8>),
    /// A PHP array.
    Array(Vec<(Key, Attr)>),
    /// A `TwigFilter`, `TwigFunction` or `TwigTest`.
    Callable(Rc<TwigCallable>),
    /// A node used as a value (`embedded_templates`, a dynamic macro name).
    Node(Node),
}

impl Attr {
    pub fn str(s: impl Into<Vec<u8>>) -> Self {
        Self::Str(s.into())
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_callable(&self) -> Option<&Rc<TwigCallable>> {
        match self {
            Self::Callable(c) => Some(c),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// PHP truthiness of a scalar or array (a callable or node is true).
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(b) => *b,
            Self::Int(i) => *i != 0,
            Self::Float(f) => *f != 0.0,
            Self::Str(s) => php_value::string::truthy(s),
            Self::Array(a) => !a.is_empty(),
            Self::Callable(_) | Self::Node(_) => true,
        }
    }

    /// `(string) $value` for a scalar (`true` is "1", `null` and `false`
    /// are ""); `None` for an array, callable or node.
    pub fn to_php_string(&self) -> Option<Vec<u8>> {
        match self {
            Self::Null | Self::Bool(false) => Some(Vec::new()),
            Self::Bool(true) => Some(b"1".to_vec()),
            Self::Int(i) => Some(i.to_string().into_bytes()),
            Self::Float(f) => Some(php_value::numeric::float_to_string(*f).into_bytes()),
            Self::Str(s) => Some(s.clone()),
            Self::Array(_) | Self::Callable(_) | Self::Node(_) => None,
        }
    }

    /// `var_export()` without newlines, as `Node::__toString()` shows it.
    fn export(&self, out: &mut Vec<u8>) {
        match self {
            Self::Null => out.extend_from_slice(b"NULL"),
            Self::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
            Self::Int(i) => out.extend_from_slice(i.to_string().as_bytes()),
            Self::Float(f) => {
                out.extend_from_slice(php_value::numeric::var_export_float(*f).as_bytes());
            }
            Self::Str(s) => {
                out.push(b'\'');
                for &b in s {
                    if b == b'\'' || b == b'\\' {
                        out.push(b'\\');
                    }
                    if b != b'\n' {
                        out.push(b);
                    }
                }
                out.push(b'\'');
            }
            Self::Array(items) => {
                out.extend_from_slice(b"array (");
                for (k, v) in items {
                    out.extend_from_slice(b"  ");
                    match k {
                        Key::Int(i) => out.extend_from_slice(i.to_string().as_bytes()),
                        Key::Str(s) => Self::Str(s.clone()).export(out),
                    }
                    out.extend_from_slice(b" => ");
                    v.export(out);
                    out.push(b',');
                }
                out.push(b')');
            }
            Self::Callable(c) => out.extend_from_slice(c.to_string().as_bytes()),
            // a `Stringable` value: `(string) $value`, newlines kept
            Self::Node(n) => out.extend(n.repr()),
        }
    }
}

impl From<bool> for Attr {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<i64> for Attr {
    fn from(i: i64) -> Self {
        Self::Int(i)
    }
}

impl From<&str> for Attr {
    fn from(s: &str) -> Self {
        Self::Str(s.as_bytes().to_vec())
    }
}

impl From<Vec<u8>> for Attr {
    fn from(s: Vec<u8>) -> Self {
        Self::Str(s)
    }
}

impl From<Option<&str>> for Attr {
    fn from(s: Option<&str>) -> Self {
        s.map_or(Self::Null, Self::from)
    }
}

#[derive(Debug)]
struct Data {
    class: NodeClass,
    nodes: Vec<(Key, Node)>,
    attributes: Vec<(String, Attr)>,
    lineno: i64,
    tag: Option<String>,
    documentation: Option<Vec<u8>>,
    source: Option<Arc<Source>>,
    // private properties of node subclasses
    /// `ArrayExpression::$index`.
    index: i64,
    /// `MacroReferenceExpression::$hasCallParentheses`.
    has_call_parentheses: bool,
    /// `SupportDefinedTestTrait::$definedTest`.
    defined_test: bool,
    /// `Parser::$expressionRefs`: the expression parser that made this
    /// node (a clone is a new object, so it has none).
    parsed_by: Option<usize>,
}

/// A node handle. `clone()` shares the node; [`Node::deep_clone`] copies it.
#[derive(Clone)]
pub struct Node(Rc<RefCell<Data>>);

impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string())
    }
}

impl Node {
    /// `parent::__construct($nodes, $attributes, $lineno)` for a class.
    pub fn new(
        class: NodeClass,
        nodes: Vec<(Key, Node)>,
        attributes: Vec<(&str, Attr)>,
        lineno: i64,
    ) -> Self {
        Self(Rc::new(RefCell::new(Data {
            class,
            nodes,
            attributes: attributes
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
            lineno,
            tag: None,
            documentation: None,
            source: None,
            index: -1,
            has_call_parentheses: true,
            defined_test: false,
            parsed_by: None,
        })))
    }

    /// `new Nodes($nodes, $lineno)`.
    pub fn nodes(nodes: Vec<(Key, Node)>, lineno: i64) -> Self {
        Self::new(NodeClass::Nodes, nodes, Vec::new(), lineno)
    }

    /// `new Nodes([$a, $b, ...], $lineno)`: a list keyed 0, 1, ...
    pub fn list(nodes: Vec<Node>, lineno: i64) -> Self {
        Self::nodes(
            nodes
                .into_iter()
                .enumerate()
                .map(|(i, n)| (Key::from(i), n))
                .collect(),
            lineno,
        )
    }

    /// `new EmptyNode($lineno)`.
    pub fn empty(lineno: i64) -> Self {
        Self::new(NodeClass::EmptyNode, Vec::new(), Vec::new(), lineno)
    }

    /// The node's identity, for maps keyed by node (PHP's `WeakMap`,
    /// `SplObjectStorage`). Stable while a handle to the node is kept.
    pub fn id(&self) -> usize {
        Rc::as_ptr(&self.0).addr()
    }

    /// Whether both handles are the same node (`===`).
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn class(&self) -> NodeClass {
        self.0.borrow().class
    }

    /// `instanceof` a class.
    pub fn is(&self, class: NodeClass) -> bool {
        self.class().is_a(class)
    }

    /// `instanceof` an interface.
    pub fn implements(&self, interface: NodeInterface) -> bool {
        self.class().implements(interface)
    }

    /// `instanceof AbstractExpression`.
    pub fn is_expression(&self) -> bool {
        self.is(NodeClass::AbstractExpression)
    }

    /// `getTemplateLine()`.
    pub fn line(&self) -> i64 {
        self.0.borrow().lineno
    }

    /// `getNodeTag()`.
    pub fn tag(&self) -> Option<String> {
        self.0.borrow().tag.clone()
    }

    /// `setNodeTag()`.
    pub fn set_tag(&self, tag: &str) -> Result<(), Error> {
        let mut data = self.0.borrow_mut();
        if data.tag.as_deref().is_some_and(|t| !t.is_empty()) {
            return Err(Error::logic("The tag of a node can only be set once."));
        }
        data.tag = Some(tag.to_owned());
        Ok(())
    }

    /// `getDocumentation()`.
    pub fn documentation(&self) -> Option<Vec<u8>> {
        self.0.borrow().documentation.clone()
    }

    /// `setDocumentation()`.
    pub fn set_documentation(&self, documentation: Option<Vec<u8>>) {
        self.0.borrow_mut().documentation = documentation;
    }

    /// `hasAttribute()`.
    pub fn has_attr(&self, name: &str) -> bool {
        self.0.borrow().attributes.iter().any(|(k, _)| k == name)
    }

    /// `getAttribute()`, `None` where upstream throws.
    pub fn attr(&self, name: &str) -> Option<Attr> {
        self.0
            .borrow()
            .attributes
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    }

    /// `getAttribute()` or upstream's `LogicException`.
    pub fn get_attr(&self, name: &str) -> Result<Attr, Error> {
        self.attr(name).ok_or_else(|| {
            Error::logic(format!(
                "Attribute \"{name}\" does not exist for Node \"{}\".",
                self.class().php_class()
            ))
        })
    }

    /// A boolean attribute that is set and true.
    pub fn attr_true(&self, name: &str) -> bool {
        self.attr(name).is_some_and(|a| a.as_bool() == Some(true))
    }

    /// A string attribute's bytes.
    pub fn attr_bytes(&self, name: &str) -> Option<Vec<u8>> {
        self.attr(name)
            .and_then(|a| a.as_bytes().map(<[u8]>::to_vec))
    }

    /// `setAttribute()`: replaces in place or appends.
    pub fn set_attr(&self, name: &str, value: impl Into<Attr>) {
        let value = value.into();
        let mut data = self.0.borrow_mut();
        if let Some(slot) = data.attributes.iter_mut().find(|(k, _)| k == name) {
            slot.1 = value;
        } else {
            data.attributes.push((name.to_owned(), value));
        }
    }

    /// `removeAttribute()`.
    pub fn remove_attr(&self, name: &str) {
        self.0.borrow_mut().attributes.retain(|(k, _)| k != name);
    }

    /// The attributes, in order.
    pub fn attributes(&self) -> Vec<(String, Attr)> {
        self.0.borrow().attributes.clone()
    }

    /// `hasNode()`.
    pub fn has_node(&self, name: impl Into<Key>) -> bool {
        let name = name.into();
        self.0.borrow().nodes.iter().any(|(k, _)| *k == name)
    }

    /// `getNode()`, `None` where upstream throws.
    pub fn node(&self, name: impl Into<Key>) -> Option<Node> {
        let name = name.into();
        self.0
            .borrow()
            .nodes
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, n)| n.clone())
    }

    /// `getNode()` or upstream's `LogicException`.
    pub fn get_node(&self, name: impl Into<Key>) -> Result<Node, Error> {
        let name = name.into();
        self.node(name.clone()).ok_or_else(|| {
            Error::logic(format!(
                "Node \"{name}\" does not exist for Node \"{}\".",
                self.class().php_class()
            ))
        })
    }

    /// `setNode()`: replaces in place or appends, and gives the node this
    /// node's source.
    pub fn set_node(&self, name: impl Into<Key>, node: Node) -> Result<(), Error> {
        let name = name.into();
        match self.class() {
            NodeClass::EmptyNode => return Err(Error::logic("EmptyNode cannot have children.")),
            NodeClass::MacrosNode if !node.is(NodeClass::MacroNode) => {
                return Err(Error::logic(format!(
                    "A \"Twig\\Node\\MacrosNode\" can only contain \"Twig\\Node\\MacroNode\" nodes; replacing the macro \"{name}\" with a \"{}\" node is not supported.",
                    node.class().php_class()
                )));
            }
            _ => {}
        }
        if let Some(source) = self.source_context() {
            node.set_source_context(&source);
        }
        let mut data = self.0.borrow_mut();
        if let Some(slot) = data.nodes.iter_mut().find(|(k, _)| *k == name) {
            slot.1 = node;
        } else {
            data.nodes.push((name, node));
        }
        Ok(())
    }

    /// Appends a child without the checks of `setNode()` (an
    /// `array_push()` on `$this->nodes`).
    pub(crate) fn push_node(&self, name: Key, node: Node) {
        self.0.borrow_mut().nodes.push((name, node));
    }

    /// `removeNode()`.
    pub fn remove_node(&self, name: impl Into<Key>) {
        let name = name.into();
        self.0.borrow_mut().nodes.retain(|(k, _)| *k != name);
    }

    /// `count()`.
    pub fn count(&self) -> usize {
        self.0.borrow().nodes.len()
    }

    /// `getIterator()`: the children with their keys.
    pub fn children(&self) -> Vec<(Key, Node)> {
        self.0.borrow().nodes.clone()
    }

    /// The child nodes without their keys.
    pub fn child_nodes(&self) -> Vec<Node> {
        self.0
            .borrow()
            .nodes
            .iter()
            .map(|(_, n)| n.clone())
            .collect()
    }

    /// `getTemplateName()`.
    pub fn template_name(&self) -> Option<String> {
        self.source_context().map(|s| s.name().to_owned())
    }

    /// `getSourceContext()`.
    pub fn source_context(&self) -> Option<Arc<Source>> {
        self.0.borrow().source.clone()
    }

    /// `setSourceContext()`: this node and all its descendants.
    pub fn set_source_context(&self, source: &Arc<Source>) {
        self.0.borrow_mut().source = Some(source.clone());
        for child in self.child_nodes() {
            child.set_source_context(source);
        }
    }

    /// PHP's `clone`: a copy of this node and its descendants (attributes
    /// are copied as values; a node or callable in an attribute stays
    /// shared, as PHP's shallow property copy leaves it).
    /// `MacroReferenceExpression` keeps its `template` node shared.
    #[must_use]
    pub fn deep_clone(&self) -> Self {
        let data = self.0.borrow();
        let keep_template = data.class == NodeClass::MacroReferenceExpression;
        let nodes = data
            .nodes
            .iter()
            .map(|(k, n)| {
                let copy = if keep_template && *k == Key::Str(b"template".to_vec()) {
                    n.clone()
                } else {
                    n.deep_clone()
                };
                (k.clone(), copy)
            })
            .collect();
        Self(Rc::new(RefCell::new(Data {
            class: data.class,
            nodes,
            attributes: data.attributes.clone(),
            lineno: data.lineno,
            tag: data.tag.clone(),
            documentation: data.documentation.clone(),
            source: data.source.clone(),
            index: data.index,
            has_call_parentheses: data.has_call_parentheses,
            defined_test: data.defined_test,
            parsed_by: None,
        })))
    }

    pub(crate) fn index(&self) -> i64 {
        self.0.borrow().index
    }

    pub(crate) fn set_index(&self, index: i64) {
        self.0.borrow_mut().index = index;
    }

    /// `MacroReferenceExpression::hasCallParentheses()`.
    pub fn has_call_parentheses(&self) -> bool {
        self.0.borrow().has_call_parentheses
    }

    /// `MacroReferenceExpression::setHasCallParentheses()`.
    pub fn set_has_call_parentheses(&self, value: bool) {
        self.0.borrow_mut().has_call_parentheses = value;
    }

    /// `SupportDefinedTestTrait::isDefinedTestEnabled()`.
    pub fn is_defined_test_enabled(&self) -> bool {
        self.0.borrow().defined_test
    }

    pub(crate) fn set_defined_test(&self, value: bool) {
        self.0.borrow_mut().defined_test = value;
    }

    pub(crate) fn parsed_by(&self) -> Option<usize> {
        self.0.borrow().parsed_by
    }

    pub(crate) fn set_parsed_by(&self, parser: usize) {
        self.0.borrow_mut().parsed_by = Some(parser);
    }

    /// `NodeDocumentation::add($node, ...$tokens)`.
    pub fn add_documentation(&self, tokens: &[&Token]) {
        let mut documentation: Vec<Vec<u8>> = tokens
            .iter()
            .filter_map(|t| t.documentation().map(<[u8]>::to_vec))
            .collect();
        if let Some(comment) = self.documentation() {
            documentation.push(comment);
        }
        if !documentation.is_empty() {
            self.set_documentation(Some(documentation.join(&b'\n')));
        }
    }

    /// `NodeDocumentation::move($source, $target)`.
    pub fn move_documentation(&self, target: &Node) {
        if let Some(mut documentation) = self.documentation() {
            if let Some(current) = target.documentation() {
                documentation.push(b'\n');
                documentation.extend_from_slice(&current);
            }
            target.set_documentation(Some(documentation));
            self.set_documentation(None);
        }
    }
}

impl Node {
    /// `__toString()`: the class, tag, documentation, attributes
    /// (`var_export()` without newlines) and children, indented.
    pub fn repr(&self) -> Vec<u8> {
        let data = self.0.borrow();
        let mut repr: Vec<u8> = data.class.php_class().as_bytes().to_vec();
        if let Some(tag) = data.tag.as_deref().filter(|t| !t.is_empty() && *t != "0") {
            repr.extend_from_slice(format!("\n  tag: {tag}").as_bytes());
        }
        if let Some(doc) = &data.documentation {
            repr.extend_from_slice(b"\n  documentation: ");
            for &b in doc {
                if b == b'\n' {
                    repr.extend_from_slice(b"\\n");
                } else {
                    repr.push(b);
                }
            }
        }
        if !data.attributes.is_empty() {
            repr.extend_from_slice(b"\n  attributes:");
            for (name, value) in &data.attributes {
                repr.extend_from_slice(b"\n    ");
                repr.extend_from_slice(name.as_bytes());
                repr.extend_from_slice(b": ");
                value.export(&mut repr);
            }
        }
        if !data.nodes.is_empty() {
            repr.extend_from_slice(b"\n  nodes:");
            for (name, node) in &data.nodes {
                let name = name.to_php_string();
                let pad = vec![b' '; name.len() + 6];
                repr.extend_from_slice(b"\n    ");
                repr.extend_from_slice(&name);
                repr.extend_from_slice(b": ");
                for (i, line) in node.repr().split(|&b| b == b'\n').enumerate() {
                    if i > 0 {
                        repr.push(b'\n');
                        repr.extend_from_slice(&pad);
                    }
                    repr.extend_from_slice(line);
                }
            }
        }
        repr
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&String::from_utf8_lossy(&self.repr()))
    }
}
