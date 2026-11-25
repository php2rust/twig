//! NodeVisitor/OptimizerTest.php at twig/twig 8c737079b726af72ff8ef3c595be9f6a810ea1ef.
//! Names and assertions are PHPUnit's; data-provider rows are written out
//! as cases of one test.
//!
//! Upstream: twig/twig tests/NodeVisitor/OptimizerTest.php
#![allow(non_snake_case, clippy::unwrap_used, clippy::expect_used)]

use twig::{
    Autoescape, Environment, EnvironmentOptions, Source,
    node::{Attr, Node, NodeClass},
    node_visitor::optimizer::{OPTIMIZE_FOR, OptimizerNodeVisitor},
};

/// `new Environment(new ArrayLoader(), ['cache' => false, 'autoescape' => false])`.
fn env_without_autoescape() -> Environment {
    Environment::with_options(
        Vec::new(),
        EnvironmentOptions {
            autoescape: Autoescape::Off,
            ..EnvironmentOptions::default()
        },
    )
    .unwrap()
}

fn parse(env: &Environment, template: &str) -> Node {
    env.parse_source(Source::new(template, "index"))
        .unwrap_or_else(|e| panic!("{template}: {e}"))
}

#[test]
fn testConstructor() {
    OptimizerNodeVisitor::check_mode(OPTIMIZE_FOR).unwrap();
    let _ = OptimizerNodeVisitor::new(OPTIMIZE_FOR);
}

#[test]
fn testRenderBlockOptimizer() {
    let stream = parse(&env_without_autoescape(), "{{ block(\"foo\") }}");
    let node = stream.get_node("body").unwrap().get_node("0").unwrap();

    assert!(node.is(NodeClass::BlockReferenceExpression));
    assert!(matches!(node.get_attr("output").unwrap(), Attr::Bool(true)));
}

#[test]
fn testRenderParentBlockOptimizer() {
    let stream = parse(
        &env_without_autoescape(),
        "{% extends \"foo\" %}{% block content %}{{ parent() }}{% endblock %}",
    );
    let node = stream
        .get_node("blocks")
        .unwrap()
        .get_node("content")
        .unwrap()
        .get_node("0")
        .unwrap()
        .get_node("body")
        .unwrap();

    assert!(node.is(NodeClass::ParentExpression));
    assert!(matches!(node.get_attr("output").unwrap(), Attr::Bool(true)));
}

#[test]
fn testForVarOptimizer() {
    let template = "{% for i, j in foo %}{{ loop.index }}{{ i }}{{ j }}{% endfor %}";
    let stream = parse(&env_without_autoescape(), template);

    for target in ["loop", "i", "j"] {
        check_for_var_configuration(&stream, target);
    }
}

/// `checkForVarConfiguration()`: upstream compares the exact class with
/// `NameExpression`, which no parsed variable has.
fn check_for_var_configuration(node: &Node, target: &str) {
    for n in node.child_nodes() {
        if n.class() == NodeClass::NameExpression
            && n.attr_bytes("name").as_deref() == Some(target.as_bytes())
        {
            assert!(matches!(
                n.get_attr("always_defined").unwrap(),
                Attr::Bool(true)
            ));
        } else {
            check_for_var_configuration(&n, target);
        }
    }
}

#[test]
fn testForLoopOptimizer() {
    let env = Environment::new(Vec::new());
    let cases: Vec<(&str, Vec<(&str, bool)>)> = vec![
        ("{% for i in foo %}{% endfor %}", vec![("i", false)]),
        (
            "{% for i in foo %}{{ loop.index }}{% endfor %}",
            vec![("i", true)],
        ),
        (
            "{% for i in foo %}{% for j in foo %}{% endfor %}{% endfor %}",
            vec![("i", false), ("j", false)],
        ),
        (
            "{% for i in foo %}{% include \"foo\" %}{% endfor %}",
            vec![("i", true)],
        ),
        (
            "{% for i in foo %}{% include \"foo\" only %}{% endfor %}",
            vec![("i", false)],
        ),
        (
            "{% for i in foo %}{% include \"foo\" with { \"foo\": \"bar\" } only %}{% endfor %}",
            vec![("i", false)],
        ),
        (
            "{% for i in foo %}{% include \"foo\" with { \"foo\": loop.index } only %}{% endfor %}",
            vec![("i", true)],
        ),
        (
            "{% for i in foo %}{% for j in foo %}{{ loop.index }}{% endfor %}{% endfor %}",
            vec![("i", false), ("j", true)],
        ),
        (
            "{% for i in foo %}{% for j in foo %}{{ loop.parent.loop.index }}{% endfor %}{% endfor %}",
            vec![("i", true), ("j", true)],
        ),
        (
            "{% for i in foo %}{% set l = loop %}{% for j in foo %}{{ l.index }}{% endfor %}{% endfor %}",
            vec![("i", true), ("j", false)],
        ),
        (
            "{% for i in foo %}{% for j in foo %}{{ foo.parent.loop.index }}{% endfor %}{% endfor %}",
            vec![("i", false), ("j", false)],
        ),
        (
            "{% for i in foo %}{% for j in foo %}{{ loop[\"parent\"].loop.index }}{% endfor %}{% endfor %}",
            vec![("i", true), ("j", true)],
        ),
        (
            "{% for i in foo %}{{ include(\"foo\") }}{% endfor %}",
            vec![("i", true)],
        ),
        (
            "{% for i in foo %}{{ include(\"foo\", with_context = false) }}{% endfor %}",
            vec![("i", false)],
        ),
        (
            "{% for i in foo %}{{ include(\"foo\", with_context = true) }}{% endfor %}",
            vec![("i", true)],
        ),
        (
            "{% for i in foo %}{{ include(\"foo\", { \"foo\": \"bar\" }, with_context = false) }}{% endfor %}",
            vec![("i", false)],
        ),
        (
            "{% for i in foo %}{{ include(\"foo\", { \"foo\": loop.index }, with_context = false) }}{% endfor %}",
            vec![("i", true)],
        ),
    ];
    for (template, expected) in cases {
        let stream = parse(&env, template);
        for (target, with_loop) in expected {
            assert_eq!(
                check_for_loop_configuration(&stream, target, with_loop),
                Some(true),
                "variable {target} is {}optimized in {template}",
                if with_loop { "not " } else { "" }
            );
        }
    }
}

/// `checkForLoopConfiguration()`.
fn check_for_loop_configuration(node: &Node, target: &str, with_loop: bool) -> Option<bool> {
    for n in node.child_nodes() {
        if n.is(NodeClass::ForNode)
            && n.get_node("value_target")
                .unwrap()
                .attr_bytes("name")
                .as_deref()
                == Some(target.as_bytes())
        {
            return Some(n.attr("with_loop").is_some_and(|v| v.is_truthy()) == with_loop);
        }
        if let Some(ret) = check_for_loop_configuration(&n, target, with_loop) {
            return Some(ret);
        }
    }
    None
}
