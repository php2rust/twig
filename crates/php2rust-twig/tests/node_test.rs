//! Node/NodeTest.php at twig/twig 8c737079b726af72ff8ef3c595be9f6a810ea1ef.
//! Names and assertions are PHPUnit's. The port has no user node
//! classes, so `NodeForTest` is the base `Twig\Node\Node` class and the
//! expected dumps name that class.
//!
//! Not ported: testToString (a closure as an attribute value; the port's
//! attributes cannot hold one), testAttributeDeprecationIgnore,
//! testAttributeDeprecationWithoutAlternative, testAttributeDeprecationWithAlternative,
//! testNodeDeprecationIgnore, testNodeDeprecationWithoutAlternative,
//! testNodeAttributeDeprecationWithAlternative (`deprecateAttribute()` and
//! `deprecateNode()` are not ported, see the `node` module).
//!
//! Upstream: twig/twig tests/Node/NodeTest.php
#![allow(non_snake_case, clippy::unwrap_used, clippy::expect_used)]

use std::rc::Rc;

use twig::{
    TwigCallable,
    node::{Attr, Node, NodeClass},
};

fn node_for_test(attributes: Vec<(&str, Attr)>) -> Node {
    Node::new(NodeClass::Node, Vec::new(), attributes, 0)
}

#[test]
fn testToStringWithTwigCallables() {
    let node = node_for_test(vec![
        (
            "function",
            Attr::Callable(Rc::new(TwigCallable::function("a_function", None))),
        ),
        (
            "filter",
            Attr::Callable(Rc::new(TwigCallable::filter("a_filter", None))),
        ),
        (
            "test",
            Attr::Callable(Rc::new(TwigCallable::test("a_test", None))),
        ),
    ]);

    assert_eq!(
        node.to_string(),
        "Twig\\Node\\Node\n  attributes:\n    function: Twig\\TwigFunction(a_function)\n    filter: Twig\\TwigFilter(a_filter)\n    test: Twig\\TwigTest(a_test)"
    );
}

#[test]
fn testToStringWithTag() {
    let node = node_for_test(Vec::new());
    node.set_tag("tag").unwrap();

    assert_eq!(node.to_string(), "Twig\\Node\\Node\n  tag: tag");
}

#[test]
fn testToStringWithDocumentation() {
    let node = node_for_test(Vec::new());
    node.set_documentation(Some(b"First line\nSecond line".to_vec()));

    assert_eq!(
        node.to_string(),
        "Twig\\Node\\Node\n  documentation: First line\\nSecond line"
    );
}
