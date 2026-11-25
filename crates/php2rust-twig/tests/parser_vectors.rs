//! The parser against real Twig 3.30: every template of upstream's
//! `tests/Fixtures/**/*.test` and the cases in `data/parser_extra.json`,
//! parsed by `Twig\Parser` with the node visitors off and the extensions
//! of `Twig\Tests\IntegrationTest` (`oracle/recorders/twig_parser.php`,
//! `scripts/record.sh twig-parser`). The tree dump (`dump_node()` in the
//! recorder) must match, or the error's class, message and line, and the
//! deprecations.
//!
//! Upstream: none
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::rc::Rc;

use php_value::base64::decode;
use serde_json::Value as Json;
use twig::{
    Environment, Source, TwigCallable,
    callable::DeprecatedCallableInfo,
    error::Error,
    extension::Extension,
    node::{Attr, Key, Node, expression, statement},
    parser::Parser,
    token::{Token, TokenType},
    token_parser::{TokenParser, sandbox::SandboxTokenParser},
};

/// `TwigTestTokenParser_§`.
struct FixtureTokenParser;

impl TokenParser for FixtureTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, _token: &Token) -> Result<Node, Error> {
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        Ok(statement::print(expression::constant("§", -1), -1))
    }

    fn tag(&self) -> &'static str {
        "§"
    }
}

/// The extensions of `IntegrationTest::getExtensions()`, definitions only.
struct FixtureExtension;

const CLOSURE: Option<&str> = Some("{closure}");

impl Extension for FixtureExtension {
    fn name(&self) -> &'static str {
        "FixtureExtension"
    }

    fn token_parsers(&self) -> Vec<Rc<dyn TokenParser>> {
        vec![Rc::new(SandboxTokenParser), Rc::new(FixtureTokenParser)]
    }

    fn filters(&self) -> Vec<TwigCallable> {
        let f = |name: &str| TwigCallable::filter(name, CLOSURE);
        vec![
            f("§"),
            f("escape_and_nl2br").needs_environment().safe(&["html"]),
            f("nl2br").pre_escape("html").safe(&["html"]),
            f("escape_something").safe(&["something"]),
            f("preserves_safety").preserves_safety(&["html"]),
            f("static_call_string"),
            f("static_call_array"),
            f("magic_call"),
            f("magic_call_closure"),
            f("magic_call_string"),
            f("magic_call_array"),
            f("*_path"),
            f("*_foo_*_bar"),
            f("not"),
            f("anon_foo"),
        ]
    }

    fn functions(&self) -> Vec<TwigCallable> {
        let f = |name: &str| TwigCallable::function(name, CLOSURE);
        vec![
            // DebugExtension
            TwigCallable::function("dump", Some("Twig\\Extension\\DebugExtension::dump"))
                .safe(&[])
                .needs_context()
                .needs_environment()
                .variadic(),
            // StringLoaderExtension
            TwigCallable::function(
                "template_from_string",
                Some("Twig\\Extension\\StringLoaderExtension::templateFromString"),
            )
            .needs_environment(),
            f("§"),
            f("safe_br").safe(&["html"]),
            f("unsafe_br"),
            f("static_call_string"),
            f("static_call_array"),
            f("*_path"),
            f("*_foo_*_bar"),
            f("anon_foo"),
            f("deprecated_function").deprecated(
                DeprecatedCallableInfo::new("foo/bar", "1.1").alternative(
                    Some("not_deprecated_function"),
                    None,
                    None,
                ),
            ),
        ]
    }

    fn tests(&self) -> Vec<TwigCallable> {
        vec![
            TwigCallable::test("multi word", CLOSURE),
            TwigCallable::test("test_*", CLOSURE),
        ]
    }
}

fn bytes(json: &Json) -> Vec<u8> {
    decode(json.as_str().expect("base64").as_bytes())
}

/// `var_export($value, true)`.
fn var_export(value: &Attr, indent: usize, out: &mut Vec<u8>) {
    match value {
        Attr::Null => out.extend_from_slice(b"NULL"),
        Attr::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Attr::Int(i) => out.extend_from_slice(i.to_string().as_bytes()),
        Attr::Float(f) => {
            out.extend_from_slice(php_value::numeric::var_export_float(*f).as_bytes())
        }
        Attr::Str(s) => {
            out.push(b'\'');
            for &b in s {
                if b == b'\'' || b == b'\\' {
                    out.push(b'\\');
                }
                out.push(b);
            }
            out.push(b'\'');
        }
        Attr::Array(items) => {
            let pad = " ".repeat(indent);
            out.extend_from_slice(b"array (\n");
            for (k, v) in items {
                out.extend_from_slice(pad.as_bytes());
                out.extend_from_slice(b"  ");
                match k {
                    Key::Int(i) => out.extend_from_slice(i.to_string().as_bytes()),
                    Key::Str(s) => var_export(&Attr::Str(s.clone()), 0, out),
                }
                out.extend_from_slice(b" => ");
                if matches!(v, Attr::Array(_)) {
                    out.push(b'\n');
                    out.extend_from_slice(pad.as_bytes());
                    out.extend_from_slice(b"  ");
                }
                var_export(v, indent + 2, out);
                out.extend_from_slice(b",\n");
            }
            out.extend_from_slice(pad.as_bytes());
            out.push(b')');
        }
        Attr::Callable(c) => out.extend_from_slice(c.to_string().as_bytes()),
        Attr::Node(_) => out.extend_from_slice(b"node"),
    }
}

fn export_value(value: &Attr) -> Vec<u8> {
    let mut out = Vec::new();
    match value {
        Attr::Node(_) => return b"node".to_vec(),
        Attr::Callable(c) => return c.to_string().into_bytes(),
        _ => var_export(value, 0, &mut out),
    }
    let mut escaped = Vec::with_capacity(out.len());
    for b in out {
        if b == b'\n' {
            escaped.extend_from_slice(b"\\n");
        } else {
            escaped.push(b);
        }
    }
    escaped
}

/// The recorder's `dump_node()`.
fn dump_node(
    node: &Node,
    key: &[u8],
    depth: usize,
    parent_source: Option<&str>,
    lines: &mut Vec<Vec<u8>>,
) {
    let mut line = format!("{depth} ").into_bytes();
    line.extend_from_slice(key);
    let class = node.class().php_class();
    line.extend_from_slice(
        format!(
            " {} {}",
            class.strip_prefix("Twig\\Node\\").unwrap_or(class),
            node.line()
        )
        .as_bytes(),
    );
    if let Some(tag) = node.tag().filter(|t| !t.is_empty() && t != "0") {
        line.extend_from_slice(format!(" tag={tag}").as_bytes());
    }
    if let Some(doc) = node.documentation() {
        line.extend_from_slice(b" doc=");
        line.extend_from_slice(&export_value(&Attr::Str(doc)));
    }
    let mut nested = Vec::new();
    for (name, value) in node.attributes() {
        line.push(b' ');
        line.extend_from_slice(name.as_bytes());
        line.push(b'=');
        line.extend_from_slice(&export_value(&value));
        if let Attr::Node(n) = value {
            nested.push((name, n));
        }
    }
    if node.is_defined_test_enabled() {
        line.extend_from_slice(b" +defined");
    }
    if !node.has_call_parentheses() {
        line.extend_from_slice(b" -parentheses");
    }
    let source = node.source_context().map(|s| s.name().to_owned());
    if source.as_deref() != parent_source {
        line.extend_from_slice(b" src=");
        line.extend_from_slice(source.as_deref().unwrap_or("-").as_bytes());
    }
    lines.push(line);
    for (k, child) in node.children() {
        dump_node(
            &child,
            &k.to_php_string(),
            depth + 1,
            source.as_deref(),
            lines,
        );
    }
    for (k, child) in nested {
        dump_node(
            &child,
            format!("@{k}").as_bytes(),
            depth + 1,
            source.as_deref(),
            lines,
        );
    }
}

fn tree_of(json: &Json) -> Vec<u8> {
    match json {
        Json::String(s) => s.as_bytes().to_vec(),
        Json::Array(a) => bytes(&a[1]),
        _ => panic!("bad tree {json}"),
    }
}

#[test]
fn trees_match_php() {
    let data: Vec<Json> =
        serde_json::from_str(include_str!("data/parser.json")).expect("parser.json");
    assert!(data.len() > 600, "{} cases", data.len());
    let env = Environment::new(vec![Rc::new(FixtureExtension)]);
    let mut failures = Vec::new();
    for case in &data {
        let name = case["name"].as_str().unwrap();
        let source = case["source"].as_str().unwrap();
        let template = bytes(&case["template"]);
        let (result, deprecations) = symfony_deprecation_contracts::collect(|| {
            env.parse_source(Source::new(template.clone(), source))
        });
        let deprecations: Vec<String> = deprecations
            .into_iter()
            .map(|d| String::from_utf8_lossy(&d.message).into_owned())
            .collect();
        let expected_deprecations: Vec<String> = case["deprecations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| String::from_utf8_lossy(&bytes(d)).into_owned())
            .collect();
        if deprecations != expected_deprecations {
            failures.push(format!(
                "{name}: deprecations\n  got  {deprecations:?}\n  want {expected_deprecations:?}"
            ));
        }
        match (case.get("tree"), result) {
            (Some(tree), Ok(module)) => {
                let mut lines = Vec::new();
                dump_node(&module, b"", 0, None, &mut lines);
                let got = lines.join(&b'\n');
                let want = tree_of(tree);
                if got != want {
                    let got = String::from_utf8_lossy(&got);
                    let want = String::from_utf8_lossy(&want);
                    let diff = got
                        .lines()
                        .zip(want.lines())
                        .find(|(g, w)| g != w)
                        .map_or_else(
                            || {
                                format!(
                                    "lengths {} / {}",
                                    got.lines().count(),
                                    want.lines().count()
                                )
                            },
                            |(g, w)| format!("\n  got  {g}\n  want {w}"),
                        );
                    failures.push(format!("{name}: tree {diff}"));
                }
            }
            (None, Err(e)) => {
                let want = &case["error"];
                let got = (
                    e.kind().class().to_owned(),
                    e.message().to_vec(),
                    e.template_line(),
                );
                let expected = (
                    want[0].as_str().unwrap().to_owned(),
                    bytes(&want[1]),
                    want[2].as_i64().unwrap_or(-1),
                );
                if got != expected {
                    failures.push(format!(
                        "{name}: error\n  got  {} {:?} {}\n  want {} {:?} {}",
                        got.0,
                        String::from_utf8_lossy(&got.1),
                        got.2,
                        expected.0,
                        String::from_utf8_lossy(&expected.1),
                        expected.2
                    ));
                }
            }
            (Some(_), Err(e)) => failures.push(format!(
                "{name}: unexpected error {:?}",
                String::from_utf8_lossy(e.message())
            )),
            (None, Ok(_)) => failures.push(format!(
                "{name}: no error, want {:?}",
                String::from_utf8_lossy(&bytes(&case["error"][1]))
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        data.len(),
        failures.join("\n")
    );
}
