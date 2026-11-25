//! TypesTokenParserTest.php at twig/twig 8c737079b726af72ff8ef3c595be9f6a810ea1ef.
//! Names and assertions are PHPUnit's; data-provider rows are written out
//! as cases of one test. The `mapping` attribute is compared as
//! (name, type, optional) rows in PHP order.
//!
//! Upstream: twig/twig tests/TokenParser/TypesTokenParserTest.php
#![allow(non_snake_case, clippy::unwrap_used, clippy::expect_used)]

use twig::{
    Environment, Source,
    node::{Attr, Node},
};

fn types_node(template: &str) -> Node {
    Environment::new(Vec::new())
        .parse_source(Source::new(template, ""))
        .unwrap_or_else(|e| panic!("{template}: {e}"))
        .get_node("body")
        .unwrap()
        .get_node("0")
        .unwrap()
}

fn mapping(node: &Node) -> Vec<(String, String, bool)> {
    let Attr::Array(entries) = node.get_attr("mapping").unwrap() else {
        panic!("mapping is not an array");
    };
    entries
        .into_iter()
        .map(|(name, entry)| {
            let Attr::Array(fields) = entry else {
                panic!("mapping entry is not an array");
            };
            assert_eq!(
                fields
                    .iter()
                    .map(|(k, _)| k.to_php_string())
                    .collect::<Vec<_>>(),
                [b"type".to_vec(), b"optional".to_vec()]
            );
            (
                String::from_utf8_lossy(&name.to_php_string()).into_owned(),
                String::from_utf8_lossy(fields[0].1.as_bytes().unwrap()).into_owned(),
                fields[1].1.as_bool().unwrap(),
            )
        })
        .collect()
}

#[test]
fn testMappingParsing() {
    let row = |name: &str, ty: &str, optional: bool| (name.to_owned(), ty.to_owned(), optional);
    let cases = vec![
        // empty mapping
        ("{% types {} %}", vec![]),
        // simple
        ("{% types {foo: \"bar\"} %}", vec![row("foo", "bar", false)]),
        // trailing comma
        (
            "{% types {foo: \"bar\",} %}",
            vec![row("foo", "bar", false)],
        ),
        // optional name
        ("{% types {foo?: \"bar\"} %}", vec![row("foo", "bar", true)]),
        // multiple pairs, duplicate values
        (
            "{% types {foo: \"foo\", bar?: \"foo\", baz: \"baz\"} %}",
            vec![
                row("foo", "foo", false),
                row("bar", "foo", true),
                row("baz", "baz", false),
            ],
        ),
        // without {} enclosing
        (
            "{% types foo: \"foo\", bar: \"bar\" %}",
            vec![row("foo", "foo", false), row("bar", "bar", false)],
        ),
        // documentation comments before types
        (
            "{% types {\n## The foo variable\n## Used by the header\nfoo: \"bar\"\n} %}",
            vec![row("foo", "bar", false)],
        ),
        // inline documentation comments
        (
            "{% types {\n## The foo variable\nfoo: \"foo\",\n## The bar variable\nbar?: \"bar\",\n} %}",
            vec![row("foo", "foo", false), row("bar", "bar", true)],
        ),
        // regular comments
        (
            "{% types {\n# Not documentation\nfoo: \"bar\",\n} %}",
            vec![row("foo", "bar", false)],
        ),
    ];
    for (template, expected) in cases {
        assert_eq!(mapping(&types_node(template)), expected, "{template}");
    }
}

#[test]
fn testDocumentationIsAttachedToTypeNodes() {
    let node = types_node(
        "{% types {\n## The foo variable\nfoo: 'foo',\n## The bar variable\nbar?: 'bar',\n} %}",
    );
    let foo = node.get_node("foo").unwrap();
    let bar = node.get_node("bar").unwrap();

    assert_eq!(
        foo.documentation().as_deref(),
        Some(&b"The foo variable"[..])
    );
    assert_eq!(foo.line(), 3);
    assert_eq!(
        bar.documentation().as_deref(),
        Some(&b"The bar variable"[..])
    );
    assert_eq!(bar.line(), 5);
}
