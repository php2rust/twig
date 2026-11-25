//! NodeVisitor/CorrectnessTest.php at twig/twig 8c737079b726af72ff8ef3c595be9f6a810ea1ef.
//! Names and assertions are PHPUnit's; data-provider rows are written out
//! as cases of one test. `assertEquals()` on two nodes compares their
//! `__toString()` dumps.
//!
//! Upstream: twig/twig tests/NodeVisitor/CorrectnessTest.php
#![allow(non_snake_case, clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use twig::{
    Environment, Source,
    error::ErrorKind,
    node::{Node, expression, statement},
    node_visitor::{self, NodeVisitor, correctness::CorrectnessNodeVisitor},
};

/// `traverse()`: the node as the body of a child template, through a
/// traverser with the correctness visitor only.
fn traverse(input: Node) -> Result<Node, twig::Error> {
    let source = Arc::new(Source::new("", "index"));
    let module = statement::module(
        statement::body(vec![input], 0),
        Some(expression::constant("parent", 1)),
        Node::empty(0),
        statement::macros(Vec::new()),
        Node::empty(0),
        Node::empty(0),
        &source,
    );
    let env = Environment::new(Vec::new());
    let mut visitors: Vec<Box<dyn NodeVisitor>> = vec![Box::new(CorrectnessNodeVisitor::default())];
    let module = node_visitor::traverse(&env, &mut visitors, module)?;
    Ok(module.get_node("body").unwrap().get_node("0").unwrap())
}

fn text(data: &str) -> Node {
    statement::text(data.as_bytes(), 1)
}

#[test]
fn testFilterBodyNodes() {
    let inputs = [
        Node::list(
            vec![statement::set(false, Node::empty(0), Node::empty(0), 1).unwrap()],
            0,
        ),
        Node::list(
            vec![
                statement::set(
                    true,
                    Node::empty(0),
                    Node::list(vec![Node::list(vec![text("foo")], 0)], 0),
                    1,
                )
                .unwrap(),
            ],
            0,
        ),
    ];
    for input in inputs {
        let expected = input.repr();
        let result = traverse(input).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            String::from_utf8_lossy(&result.repr()),
            String::from_utf8_lossy(&expected)
        );
    }
}

#[test]
fn testFilterBodyNodesThrowsException() {
    for input in [
        text("foo"),
        statement::print(expression::constant("foo", 1), 1),
        statement::include(expression::constant("foo", 1), None, false, false, 1),
        Node::list(vec![Node::list(vec![text("foo")], 0)], 0),
    ] {
        let dump = input.to_string();
        let e = traverse(input).expect_err(&dump);
        assert_eq!(e.kind(), ErrorKind::Syntax, "{dump}");
    }
}

#[test]
fn testFilterBodyNodesWithBOM() {
    for empty_text in [" ", "\t", "\n", "\n\t\n   "] {
        let mut data = b"\xEF\xBB\xBF".to_vec();
        data.extend_from_slice(empty_text.as_bytes());
        let input = statement::text(&data, 1);

        // a child template whose root content is only a BOM followed by blanks is valid:
        // the visitor must accept it (no SyntaxError) and leave it untouched
        let result = traverse(input.clone()).unwrap_or_else(|e| panic!("{e}"));
        assert!(result.ptr_eq(&input), "{empty_text:?}");
    }
}
