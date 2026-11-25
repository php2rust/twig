//! The constructors of the expression node classes of `src/Node/Expression`
//! and the methods the parser calls on them.
//!
//! Each function copies one PHP constructor: the child nodes, the
//! attributes and their order, the checks and the errors. The 3.15
//! deprecations for a non-expression operand are left out: the ported
//! parser only passes expressions.
//!
//! Upstream: twig/twig src/Node/Expression/AbstractExpression.php src/Node/Expression/ArrayExpression.php src/Node/Expression/ArrowFunctionExpression.php src/Node/Expression/BlockReferenceExpression.php src/Node/Expression/ConstantExpression.php src/Node/Expression/EmptyExpression.php src/Node/Expression/FilterExpression.php src/Node/Expression/FunctionExpression.php src/Node/Expression/GetAttrExpression.php src/Node/Expression/ListExpression.php src/Node/Expression/MacroReferenceExpression.php src/Node/Expression/NameExpression.php src/Node/Expression/ParentExpression.php src/Node/Expression/TempNameExpression.php src/Node/Expression/AssignNameExpression.php src/Node/Expression/TestExpression.php src/Node/Expression/Binary/AbstractBinary.php src/Node/Expression/Binary/ElvisBinary.php src/Node/Expression/Binary/NullCoalesceBinary.php src/Node/Expression/Binary/ObjectDestructuringSetBinary.php src/Node/Expression/Binary/SequenceDestructuringSetBinary.php src/Node/Expression/Binary/SetBinary.php src/Node/Expression/Filter/DefaultFilter.php src/Node/Expression/Filter/EscapeFilter.php src/Node/Expression/Filter/RawFilter.php src/Node/Expression/Ternary/ConditionalTernary.php src/Node/Expression/Test/DefinedTest.php src/Node/Expression/Test/TrueTest.php src/Node/Expression/Unary/AbstractUnary.php src/Node/Expression/Variable/AssignMacroVariable.php src/Node/Expression/Variable/MacroVariable.php src/Node/Expression/SupportDefinedTestTrait.php

use std::rc::Rc;

use super::{Attr, Key, Node, NodeClass, NodeInterface};
use crate::{callable::TwigCallable, error::Error};

/// `Template::ANY_CALL`.
pub const ANY_CALL: &str = "any";
/// `Template::ARRAY_CALL`.
pub const ARRAY_CALL: &str = "array";
/// `Template::METHOD_CALL`.
pub const METHOD_CALL: &str = "method";

/// `TempNameExpression::RESERVED_NAMES`.
pub const RESERVED_NAMES: [&str; 5] = ["varargs", "context", "macros", "blocks", "this"];
/// `TempNameExpression::RESERVED_NAME_PREFIX` (U+035C).
pub const RESERVED_NAME_PREFIX: &[u8] = "\u{035C}".as_bytes();

fn key(name: &str) -> Key {
    Key::Str(name.as_bytes().to_vec())
}

fn named(nodes: Vec<(&str, Node)>) -> Vec<(Key, Node)> {
    nodes.into_iter().map(|(k, n)| (key(k), n)).collect()
}

fn is_reserved_literal(name: &[u8]) -> bool {
    let lower = name.to_ascii_lowercase();
    [&b"true"[..], b"false", b"none", b"null"].contains(&lower.as_slice())
}

// ---- AbstractExpression ----

impl Node {
    /// `AbstractExpression::setExplicitParentheses()`.
    #[must_use]
    pub fn set_explicit_parentheses(&self) -> Self {
        self.set_attr("with_parentheses", true);
        self.clone()
    }

    /// `AbstractExpression::hasExplicitParentheses()`.
    pub fn has_explicit_parentheses(&self) -> bool {
        self.attr_true("with_parentheses")
    }

    /// `SupportDefinedTestInterface::enableDefinedTest()`.
    pub fn enable_defined_test(&self) {
        match self.class() {
            c if c.is_a(NodeClass::GetAttrExpression) => {
                self.set_defined_test(true);
                change_ignore_strict_check(self);
            }
            c if c.is_a(NodeClass::FunctionExpression) => {
                if self.attr_bytes("name").as_deref() == Some(b"constant") {
                    self.set_defined_test(true);
                }
            }
            _ => self.set_defined_test(true),
        }
    }
}

fn change_ignore_strict_check(node: &Node) {
    node.set_attr("optimizable", false);
    node.set_attr("ignore_strict_check", true);
    if let Some(inner) = node.node("node")
        && inner.is(NodeClass::GetAttrExpression)
    {
        change_ignore_strict_check(&inner);
    }
}

// ---- constants and names ----

/// `new ConstantExpression($value, $lineno)`.
pub fn constant(value: impl Into<Attr>, lineno: i64) -> Node {
    Node::new(
        NodeClass::ConstantExpression,
        Vec::new(),
        vec![("value", value.into())],
        lineno,
    )
}

/// `new EmptyExpression($lineno)`.
pub fn empty_expression(lineno: i64) -> Node {
    Node::new(NodeClass::EmptyExpression, Vec::new(), Vec::new(), lineno)
}

/// `new ContextVariable($name, $lineno)` (the `NameExpression` constructor).
pub fn context_variable(name: &[u8], lineno: i64) -> Node {
    name_expression(NodeClass::ContextVariable, name, lineno)
}

fn name_expression(class: NodeClass, name: &[u8], lineno: i64) -> Node {
    Node::new(
        class,
        Vec::new(),
        vec![
            ("name", Attr::Str(name.to_vec())),
            ("ignore_strict_check", Attr::Bool(false)),
            ("always_defined", Attr::Bool(false)),
        ],
        lineno,
    )
}

/// `new AssignContextVariable($name, $lineno)`.
pub fn assign_context_variable(name: &[u8], lineno: i64) -> Result<Node, Error> {
    if is_reserved_literal(name) {
        let mut message = b"You cannot assign a value to \"".to_vec();
        message.extend_from_slice(name);
        message.extend_from_slice(b"\".");
        return Err(Error::syntax(message, lineno, None));
    }
    Ok(name_expression(
        NodeClass::AssignContextVariable,
        name,
        lineno,
    ))
}

/// The `TempNameExpression` constructor, for `LocalVariable` and
/// `MacroVariable`. `name` is a string, an int or null.
pub fn temp_name(class: NodeClass, name: Attr, lineno: i64) -> Result<Node, Error> {
    if name.is_truthy()
        && let Some(text) = name.to_php_string()
        && is_reserved_literal(&text)
    {
        let mut message = b"You cannot assign a value to \"".to_vec();
        message.extend_from_slice(&text);
        message.extend_from_slice(b"\".");
        return Err(Error::syntax(message, lineno, None));
    }
    let name = match name {
        Attr::Str(s) if !s.is_empty() && s.iter().all(u8::is_ascii_digit) => {
            Attr::Int(saturating_int(&s))
        }
        Attr::Str(s) if RESERVED_NAMES.iter().any(|r| r.as_bytes() == s.as_slice()) => {
            let mut prefixed = RESERVED_NAME_PREFIX.to_vec();
            prefixed.extend_from_slice(&s);
            Attr::Str(prefixed)
        }
        other => other,
    };
    Ok(Node::new(class, Vec::new(), vec![("name", name)], lineno))
}

/// `(int) $digits`: saturates as PHP does.
fn saturating_int(digits: &[u8]) -> i64 {
    digits.iter().fold(0i64, |acc, &d| {
        acc.saturating_mul(10).saturating_add(i64::from(d - b'0'))
    })
}

/// `new LocalVariable($name, $lineno)`.
pub fn local_variable(name: Attr, lineno: i64) -> Result<Node, Error> {
    temp_name(NodeClass::LocalVariable, name, lineno)
}

/// `new MacroVariable($name, $lineno)`.
pub fn macro_variable(name: Attr, lineno: i64) -> Result<Node, Error> {
    temp_name(NodeClass::MacroVariable, name, lineno)
}

/// `new AssignMacroVariable($var, $global)`.
pub fn assign_macro_variable(var: Node, global: bool) -> Node {
    let lineno = var.line();
    Node::new(
        NodeClass::AssignMacroVariable,
        named(vec![("var", var)]),
        vec![("global", Attr::Bool(global))],
        lineno,
    )
}

// ---- arrays and lists ----

/// `new ArrayExpression($elements, $lineno)`: `elements` alternate keys
/// and values.
pub fn array(elements: Vec<Node>, lineno: i64) -> Node {
    array_of_class(NodeClass::ArrayExpression, elements, lineno)
}

/// `new ArrayExpression` for a subclass (`VariadicExpression`).
pub fn array_of_class(class: NodeClass, elements: Vec<Node>, lineno: i64) -> Node {
    let node = Node::new(
        class,
        elements
            .into_iter()
            .enumerate()
            .map(|(i, n)| (Key::from(i), n))
            .collect(),
        Vec::new(),
        lineno,
    );
    let mut index = -1;
    for (key, _) in node.key_value_pairs() {
        if key.is(NodeClass::ConstantExpression)
            && let Some(value) = key.attr("value").and_then(|v| array_index_of(&v))
            && value > index
        {
            index = value;
        }
    }
    node.set_index(index);
    node
}

/// The value of a key for which `ctype_digit((string) $value)` is true.
fn array_index_of(value: &Attr) -> Option<i64> {
    let text = value.to_php_string()?;
    if !text.is_empty() && text.iter().all(u8::is_ascii_digit) {
        Some(saturating_int(&text))
    } else {
        None
    }
}

impl Node {
    /// `ArrayExpression::getKeyValuePairs()`.
    pub fn key_value_pairs(&self) -> Vec<(Node, Node)> {
        let nodes = self.child_nodes();
        nodes
            .chunks(2)
            .filter_map(|pair| match pair {
                [k, v] => Some((k.clone(), v.clone())),
                _ => None,
            })
            .collect()
    }

    /// `ArrayExpression::hasElement()`.
    pub fn has_element(&self, key: &Node) -> bool {
        let wanted = key.repr();
        self.key_value_pairs()
            .iter()
            .any(|(k, _)| k.repr() == wanted)
    }

    /// `ArrayExpression::isSequence()`.
    pub fn is_sequence(&self) -> bool {
        for (i, (key, _)) in self.key_value_pairs().into_iter().enumerate() {
            let value = if key.is(NodeClass::TempNameExpression) {
                key.attr("name")
            } else if key.is(NodeClass::ConstantExpression) {
                key.attr("value")
            } else {
                return false;
            };
            if value.and_then(|v| v.as_int()) != i64::try_from(i).ok() {
                return false;
            }
        }
        true
    }

    /// `ArrayExpression::addElement()`.
    pub fn add_element(&self, value: Node, key: Option<Node>) {
        let key = key.unwrap_or_else(|| {
            let index = self.index().saturating_add(1);
            self.set_index(index);
            constant(index, value.line())
        });
        let next = self.count();
        self.push_node(Key::from(next), key);
        self.push_node(Key::from(next + 1), value);
    }
}

/// `new ListExpression($items, $lineno)`.
pub fn list(items: Vec<Node>, lineno: i64) -> Node {
    Node::new(
        NodeClass::ListExpression,
        items
            .into_iter()
            .enumerate()
            .map(|(i, n)| (Key::from(i), n))
            .collect(),
        Vec::new(),
        lineno,
    )
}

/// `new ArrowFunctionExpression($expr, $names, $lineno)`.
pub fn arrow_function(expr: Node, names: Node, lineno: i64) -> Result<Node, Error> {
    let names = if names.is(NodeClass::ContextVariable) {
        let name = names.attr_bytes("name").unwrap_or_default();
        list(vec![assign_context_variable(&name, names.line())?], lineno)
    } else {
        names
    };
    if !names.is(NodeClass::ListExpression) {
        return Err(Error::syntax(
            "The arrow function argument must be a list of variables or a single variable.",
            names.line(),
            names.source_context(),
        ));
    }
    Ok(Node::new(
        NodeClass::ArrowFunctionExpression,
        named(vec![("expr", expr), ("names", names)]),
        Vec::new(),
        lineno,
    ))
}

// ---- references ----

/// `new BlockReferenceExpression($name, $template, $lineno)`.
pub fn block_reference(name: Node, template: Option<Node>, lineno: i64) -> Node {
    let mut nodes = vec![("name", name)];
    if let Some(template) = template {
        nodes.push(("template", template));
    }
    Node::new(
        NodeClass::BlockReferenceExpression,
        named(nodes),
        vec![("output", Attr::Bool(false))],
        lineno,
    )
}

/// `new ParentExpression($name, $lineno)`.
pub fn parent(name: &[u8], lineno: i64) -> Node {
    Node::new(
        NodeClass::ParentExpression,
        Vec::new(),
        vec![
            ("output", Attr::Bool(false)),
            ("name", Attr::Str(name.to_vec())),
        ],
        lineno,
    )
}

/// The name of a macro reference: a string or an expression.
pub enum MacroName {
    Str(Vec<u8>),
    Expr(Node),
}

/// `new MacroReferenceExpression($template, $name, $arguments, $lineno)`.
pub fn macro_reference(template: Node, name: MacroName, arguments: Node, lineno: i64) -> Node {
    let mut nodes = vec![("template", template), ("arguments", arguments)];
    let mut name_attr = Attr::Null;
    match name {
        MacroName::Str(s) => name_attr = Attr::Str(s),
        MacroName::Expr(n) => nodes.push(("name", n)),
    }
    Node::new(
        NodeClass::MacroReferenceExpression,
        named(nodes),
        vec![("name", name_attr)],
        lineno,
    )
}

/// `new GetAttrExpression($node, $attribute, $arguments, $type, $lineno, $nullSafe)`.
pub fn get_attr(
    node: Node,
    attribute: Node,
    arguments: Option<Node>,
    call_type: &str,
    lineno: i64,
    null_safe: bool,
) -> Node {
    let mut nodes = vec![("node", node), ("attribute", attribute)];
    if let Some(arguments) = arguments {
        nodes.push(("arguments", arguments));
    }
    Node::new(
        NodeClass::GetAttrExpression,
        named(nodes),
        vec![
            ("type", Attr::from(call_type)),
            ("ignore_strict_check", Attr::Bool(false)),
            ("optimizable", Attr::Bool(!null_safe)),
            ("null_safe", Attr::Bool(null_safe)),
            ("is_short_circuited", Attr::Bool(false)),
            ("var_name", Attr::Null),
        ],
        lineno,
    )
}

// ---- callables ----

/// `new $class($node, $filter, $arguments, $lineno)` for a filter node
/// class: `FilterExpression`, `DefaultFilter`, `EscapeFilter`, `RawFilter`.
pub fn filter(
    class: NodeClass,
    node: Node,
    filter: Rc<TwigCallable>,
    arguments: Node,
    lineno: i64,
) -> Result<Node, Error> {
    match class {
        NodeClass::DefaultFilter => default_filter(node, filter, arguments, lineno),
        NodeClass::EscapeFilter => {
            let n = filter_expression(class, node, filter, arguments, lineno);
            n.set_attr("template_escaper", false);
            Ok(n)
        }
        _ => Ok(filter_expression(class, node, filter, arguments, lineno)),
    }
}

/// The `FilterExpression` constructor.
fn filter_expression(
    class: NodeClass,
    node: Node,
    filter: Rc<TwigCallable>,
    arguments: Node,
    lineno: i64,
) -> Node {
    let name = filter.name().to_vec();
    let n = Node::new(
        class,
        named(vec![
            ("node", node),
            ("filter", constant(Attr::Str(name.clone()), lineno)),
            ("arguments", arguments),
        ]),
        vec![("name", Attr::Str(name)), ("type", Attr::from("filter"))],
        lineno,
    );
    n.set_attr("twig_callable", Attr::Callable(filter));
    n
}

#[allow(clippy::needless_pass_by_value)]
fn default_filter(
    node: Node,
    filter: Rc<TwigCallable>,
    arguments: Node,
    lineno: i64,
) -> Result<Node, Error> {
    let line = node.line();
    let default = filter_expression(
        NodeClass::FilterExpression,
        node.clone(),
        filter.clone(),
        arguments.clone(),
        line,
    );
    let node = if filter.name() == b"default"
        && (node.is(NodeClass::ContextVariable) || node.is(NodeClass::GetAttrExpression))
    {
        let test = defined_test(
            node.deep_clone(),
            Rc::new(builtin_test("defined")),
            Some(Node::empty(0)),
            line,
        )?;
        let otherwise = if arguments.count() > 0 {
            arguments.get_node(0i64)?.deep_clone()
        } else {
            constant("", line)
        };
        conditional_ternary(test, default, otherwise, line)
    } else {
        default
    };
    Ok(filter_expression(
        NodeClass::DefaultFilter,
        node,
        filter,
        arguments,
        lineno,
    ))
}

/// `new RawFilter($node)`: the `raw` filter with no arguments.
pub fn raw_filter(node: Node) -> Node {
    let line = node.line();
    filter_expression(
        NodeClass::RawFilter,
        node,
        Rc::new(TwigCallable::filter("raw", None).safe(&["all"])),
        Node::empty(0),
        line,
    )
}

/// `new $class($function, $arguments, $lineno)` for a function node class.
pub fn function(
    class: NodeClass,
    function: Rc<TwigCallable>,
    arguments: Node,
    lineno: i64,
) -> Node {
    let n = Node::new(
        class,
        named(vec![("arguments", arguments)]),
        vec![
            ("name", Attr::Str(function.name().to_vec())),
            ("type", Attr::from("function")),
        ],
        lineno,
    );
    n.set_attr("twig_callable", Attr::Callable(function));
    n
}

/// `new $class($node, $test, $arguments, $lineno)` for a test node class.
pub fn test(
    class: NodeClass,
    node: Node,
    test: Rc<TwigCallable>,
    arguments: Option<Node>,
    lineno: i64,
) -> Result<Node, Error> {
    if class == NodeClass::DefinedTest {
        return defined_test(node, test, arguments, lineno);
    }
    Ok(test_expression(class, node, test, arguments, lineno))
}

fn test_expression(
    class: NodeClass,
    node: Node,
    test: Rc<TwigCallable>,
    arguments: Option<Node>,
    lineno: i64,
) -> Node {
    let mut nodes = vec![("node", node)];
    if let Some(arguments) = arguments {
        nodes.push(("arguments", arguments));
    }
    let n = Node::new(
        class,
        named(nodes),
        vec![
            ("name", Attr::Str(test.name().to_vec())),
            ("type", Attr::from("test")),
        ],
        lineno,
    );
    n.set_attr("twig_callable", Attr::Callable(test));
    n
}

/// `new DefinedTest($node, $test, $arguments, $lineno)`.
pub fn defined_test(
    node: Node,
    test: Rc<TwigCallable>,
    arguments: Option<Node>,
    lineno: i64,
) -> Result<Node, Error> {
    if !node.implements(NodeInterface::SupportDefinedTest) {
        return Err(Error::syntax(
            "The \"defined\" test only works with simple variables.",
            lineno,
            None,
        ));
    }
    node.enable_defined_test();
    Ok(test_expression(
        NodeClass::DefinedTest,
        node,
        test,
        arguments,
        lineno,
    ))
}

/// `new TwigTest($name, null, ['always_allowed_in_sandbox' => true])`, as
/// the node classes create for `defined`, `null` and `true`.
fn builtin_test(name: &str) -> TwigCallable {
    TwigCallable::test(name, None).always_allowed_in_sandbox()
}

/// `TrueTest::wrap($node)`.
pub fn true_test_wrap(node: Node) -> Node {
    if node.implements(NodeInterface::ReturnPrimitiveType) {
        return node;
    }
    let line = node.line();
    test_expression(
        NodeClass::TrueTest,
        node,
        Rc::new(builtin_test("true")),
        None,
        line,
    )
}

/// `new ConditionalTernary($test, $left, $right, $lineno)`.
pub fn conditional_ternary(test: Node, left: Node, right: Node, lineno: i64) -> Node {
    Node::new(
        NodeClass::ConditionalTernary,
        named(vec![
            ("test", true_test_wrap(test)),
            ("left", left),
            ("right", right),
        ]),
        Vec::new(),
        lineno,
    )
}

// ---- operators ----

/// `new $class($left, $right, $lineno)` for a binary node class.
pub fn binary(class: NodeClass, left: Node, right: Node, lineno: i64) -> Result<Node, Error> {
    match class {
        NodeClass::SetBinary => {
            let Some(Attr::Str(name)) = left.get_attr("name").ok() else {
                return Err(Error::logic("The \"name\" attribute must be a string."));
            };
            let left = assign_context_variable(&name, left.line())?;
            Ok(abstract_binary(class, left, right, lineno))
        }
        NodeClass::ElvisBinary => {
            let node = abstract_binary(class, left.clone(), right, lineno);
            node.set_node("test", true_test_wrap(left.deep_clone()))?;
            left.set_attr("always_defined", true);
            Ok(node)
        }
        NodeClass::NullCoalesceBinary => {
            let node = abstract_binary(class, left.clone(), right, lineno);
            let line = left.line();
            let mut test = defined_test(
                left.deep_clone(),
                Rc::new(builtin_test("defined")),
                Some(Node::empty(0)),
                line,
            )?;
            if !left.is(NodeClass::BlockReferenceExpression) {
                let null = test_expression(
                    NodeClass::NullTest,
                    left.clone(),
                    Rc::new(builtin_test("null")),
                    Some(Node::empty(0)),
                    line,
                );
                test = binary(
                    NodeClass::AndBinary,
                    test,
                    unary(NodeClass::NotUnary, null, line),
                    line,
                )?;
            }
            left.set_attr("always_defined", true);
            node.set_node("test", test)?;
            Ok(node)
        }
        NodeClass::ObjectDestructuringSetBinary => {
            if !left.is(NodeClass::ArrayExpression) {
                return Err(Error::logic(
                    "Left side must be ArrayExpression for object/mapping destructuring.",
                ));
            }
            for (key, value) in left.key_value_pairs() {
                if !value.is(NodeClass::AssignContextVariable) {
                    return Err(Error::syntax(
                        format!(
                            "Cannot assign to \"{}\", only variables can be assigned in object/mapping destructuring.",
                            value.class().php_class()
                        ),
                        lineno,
                        None,
                    ));
                }
                key.get_attr("value")?;
            }
            Ok(abstract_binary(class, left, right, lineno))
        }
        NodeClass::SequenceDestructuringSetBinary => {
            for (_, value) in left.key_value_pairs() {
                if !value.is(NodeClass::EmptyExpression)
                    && !value.is(NodeClass::AssignContextVariable)
                {
                    return Err(Error::syntax(
                        format!(
                            "Cannot assign to \"{}\", only variables can be assigned in sequence destructuring.",
                            value.class().php_class()
                        ),
                        lineno,
                        None,
                    ));
                }
            }
            Ok(abstract_binary(class, left, right, lineno))
        }
        NodeClass::AndBinary | NodeClass::OrBinary | NodeClass::XorBinary => Ok(abstract_binary(
            class,
            true_test_wrap(left),
            true_test_wrap(right),
            lineno,
        )),
        NodeClass::MatchesBinary => {
            if right.is(NodeClass::ConstantExpression) {
                let regexp = right
                    .attr("value")
                    .and_then(|v| v.to_php_string())
                    .unwrap_or_default();
                if let Err(php_pcre::PcreError::Compile(message)) = php_pcre::Pcre::new(&regexp) {
                    let mut text = b"Regexp \"".to_vec();
                    text.extend_from_slice(&regexp);
                    text.extend_from_slice(b"\" passed to \"matches\" is not valid: ");
                    text.extend_from_slice(message.as_bytes());
                    text.push(b'.');
                    return Err(Error::syntax(text, lineno, None));
                }
            }
            Ok(abstract_binary(class, left, right, lineno))
        }
        _ => Ok(abstract_binary(class, left, right, lineno)),
    }
}

/// The `AbstractBinary` constructor.
fn abstract_binary(class: NodeClass, left: Node, right: Node, lineno: i64) -> Node {
    Node::new(
        class,
        named(vec![("left", left), ("right", right)]),
        Vec::new(),
        lineno,
    )
}

/// `new $class($node, $lineno)` for a unary node class.
pub fn unary(class: NodeClass, node: Node, lineno: i64) -> Node {
    let node = if class == NodeClass::NotUnary {
        true_test_wrap(node)
    } else {
        node
    };
    Node::new(
        class,
        named(vec![("node", node)]),
        vec![("with_parentheses", Attr::Bool(false))],
        lineno,
    )
}
