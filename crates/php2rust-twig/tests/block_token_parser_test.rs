//! BlockTokenParserTest.php at twig/twig 8c737079b726af72ff8ef3c595be9f6a810ea1ef.
//! Names and assertions are PHPUnit's; data-provider rows are written out
//! as cases of one test. `render()` in testDocumentationCommentRequiresALineBreak
//! fails in the parser, so the test parses.
//!
//! Not here (pending, they need the runtime): testDocumentationWhitespaceControl,
//! testShortcutAssignmentNamedDocsRemainsValid.
//!
//! Upstream: twig/twig tests/TokenParser/BlockTokenParserTest.php
#![allow(non_snake_case, clippy::unwrap_used, clippy::expect_used)]

use twig::{
    Environment, Source,
    error::ErrorKind,
    node::{Node, NodeClass},
};

/// `new Environment(new ArrayLoader(), ['autoescape' => false, 'optimizations' => $optimizations])`.
fn env(optimizations: i64) -> Environment {
    Environment::with_options(
        Vec::new(),
        twig::EnvironmentOptions {
            autoescape: twig::Autoescape::Off,
            optimizations,
            ..twig::EnvironmentOptions::default()
        },
    )
    .unwrap()
}

fn parse_with(env: &Environment, template: &str, name: &str) -> Node {
    env.parse_source(Source::new(template, name))
        .unwrap_or_else(|e| panic!("{template}: {e}"))
}

/// The environment of most of the tests: no auto-escaping, no optimizations.
fn parse(template: &str, name: &str) -> Node {
    parse_with(&env(0), template, name)
}

fn doc(node: &Node) -> Option<String> {
    node.documentation()
        .map(|d| String::from_utf8_lossy(&d).into_owned())
}

fn content_block(module: &Node) -> Node {
    module
        .get_node("blocks")
        .unwrap()
        .get_node("content")
        .unwrap()
        .get_node("0")
        .unwrap()
}

/// `parseBlock()`.
fn parse_block(template: &str) -> Node {
    content_block(&parse_with(&env(-1), template, ""))
}

#[test]
fn testDocumentation() {
    for (template, expected) in [
        // documentation comment before block
        (
            "{## The main content #}\n{% block content %}Hello{% endblock %}",
            Some("The main content"),
        ),
        // symmetric closing marker
        (
            "{## The main content ##}\n{% block content %}Hello{% endblock %}",
            Some("The main content"),
        ),
        // multiple documentation comments
        (
            "{## The main content #}\n{## Displayed on every page #}\n{% block content %}Hello{% endblock %}",
            Some("The main content\nDisplayed on every page"),
        ),
        // empty documentation comment
        ("{## #}\n{% block content %}Hello{% endblock %}", None),
        // zero documentation comment
        (
            "{## 0 #}\n{% block content %}Hello{% endblock %}",
            Some("0"),
        ),
        // regular comment
        (
            "{# The main content #}\n{% block content %}Hello{% endblock %}",
            None,
        ),
        // unrelated documentation comment
        (
            "{## Not block documentation #}\nHello\n{% block content %}Hello{% endblock %}",
            None,
        ),
    ] {
        assert_eq!(
            doc(&parse_block(template)).as_deref(),
            expected,
            "{template}"
        );
    }
}

#[test]
fn testDocumentationIsMovedFromTheBlockReference() {
    let module = parse(
        "{## The main content #}\n{% block content %}Hello{% endblock %}",
        "index",
    );
    let reference = module.get_node("body").unwrap().get_node("0").unwrap();

    assert!(reference.is(NodeClass::BlockReferenceNode));
    assert_eq!(doc(&reference), None);
    assert_eq!(
        doc(&content_block(&module)).as_deref(),
        Some("The main content")
    );
}

#[test]
fn testDocumentationIsPreservedInChildTemplates() {
    let module = parse(
        "{% extends 'base' %}\n{## The main content #}\n{% block content %}Hello{% endblock %}",
        "index",
    );
    assert_eq!(
        doc(&content_block(&module)).as_deref(),
        Some("The main content")
    );
}

#[test]
fn testDocumentationDoesNotCrossSpecialConstructs() {
    for template in [
        // line tag
        "{## Not do documentation #}\n{% line 10 %}{% do max() %}",
        // empty verbatim tag
        "{## Not do documentation #}\n{% verbatim %}{% endverbatim %}{% do max() %}",
        // whitespace verbatim tag
        "{## Not do documentation #}\n{% verbatim %} {% endverbatim %}{% do max() %}",
    ] {
        let module = parse(template, "index");
        let body = module.get_node("body").unwrap().get_node("0").unwrap();
        let node = if body.is(NodeClass::DoNode) {
            body
        } else {
            body.get_node("1").unwrap()
        };
        assert_eq!(doc(&node), None, "{template}");
    }
}

#[test]
fn testDocumentationCommentRequiresALineBreak() {
    let result = Environment::new(Vec::new()).parse_source(Source::new(
        "{% block ## Description %}Hello{% endblock %}",
        "index",
    ));
    assert_eq!(result.expect_err("a SyntaxError").kind(), ErrorKind::Syntax);
}
