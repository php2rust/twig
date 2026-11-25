//! NodeVisitor/SafeAnalysisTest.php at twig/twig 8c737079b726af72ff8ef3c595be9f6a810ea1ef.
//! Names and assertions are PHPUnit's. `WeakReference` is a weak node
//! handle; Rust frees a node as soon as its last handle is dropped, so
//! there is no `gc_collect_cycles()`.
//!
//! Upstream: twig/twig tests/NodeVisitor/SafeAnalysisTest.php
#![allow(non_snake_case, clippy::unwrap_used, clippy::expect_used)]

use twig::{Environment, Source, node::Node, node::WeakNode};

/// The `$collect` closure.
fn collect(node: &Node, refs: &mut Vec<WeakNode>) {
    refs.push(node.downgrade());
    for child in node.child_nodes() {
        collect(&child, refs);
    }

    #[test]
    fn testCompilingDoesNotRetainTheAst() {
        let env = Environment::new(Vec::new());
        let source = Source::new(
            "{{ \"a\"|upper }}{{ foo.bar }}{{ [1, 2]|join(\"-\") }}{{ b ? \"y\" : \"n\" }}",
            "index",
        );

        let ast = env.parse_source(source).unwrap();
    }
    let mut refs = Vec::new();
    collect(&ast, &mut refs);
    assert!(!refs.is_empty());

    drop(ast);

    let retained = refs.iter().filter(|r| r.upgrade().is_some()).count();
    assert_eq!(
        retained,
        0,
        "{retained} of {} AST nodes are still referenced after compilation.",
        refs.len()
    );
}
