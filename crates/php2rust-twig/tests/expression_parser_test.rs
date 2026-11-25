//! ExpressionParserTest.php at twig/twig 8c737079b726af72ff8ef3c595be9f6a810ea1ef.
//! Names and assertions are PHPUnit's; data-provider rows are written out
//! as cases of one test. `expectExceptionMessage` checks that the message
//! contains the text. `assertEquals()` on two nodes compares their
//! `__toString()` dumps. `compileSource()` in testEmptyDestructuringThrows
//! fails in the parser, so the test parses.
//!
//! Not here (pending, they need the compiler or the runtime):
//! testSequenceCompilationError, testObjectDestructuringEvaluatesRightHandExpressionOnce,
//! testNullSafeOperator, testInvalidNullSafeOperatorShortCircuiting,
//! testCompiledCodeForDynamicTest, testCompiledCodeForDynamicFunction,
//! testCompiledCodeForDynamicFilter, testBindingPower.
//!
//! Not ported: testNotReadyFunctionWithNoConstructor, testNotReadyFilterWithNoConstructor,
//! testNotReadyTestWithNoConstructor, testNotReadyFunction, testNotReadyFilter,
//! testNotReadyTest, testReadyFunction, testReadyFilter, testReadyTest. They
//! check PHP subclasses of the expression nodes and the
//! `#[FirstClassTwigCallableReady]` attribute on their constructors; the
//! port has no user node classes.
//!
//! Upstream: twig/twig tests/ExpressionParserTest.php
#![allow(non_snake_case, clippy::unwrap_used, clippy::expect_used)]

use std::rc::Rc;

use twig::{
    Environment, Source, TwigCallable,
    error::ErrorKind,
    expression_parser::{ExpressionParser, ExpressionParserType, ParserKind},
    extension::Extension,
    node::{Node, NodeClass, expression},
};

fn env() -> Environment {
    Environment::new(Vec::new())
}

#[derive(Default)]
struct TestExtension {
    tests: Vec<TwigCallable>,
    expression_parsers: Vec<ExpressionParser>,
}

impl Extension for TestExtension {
    fn name(&self) -> &'static str {
        "TestExtension"
    }

    fn tests(&self) -> Vec<TwigCallable> {
        self.tests.clone()
    }

    fn expression_parsers(&self) -> Vec<ExpressionParser> {
        self.expression_parsers.clone()
    }
}

fn parse(env: &Environment, template: &str, name: &str) -> Result<Node, twig::error::Error> {
    env.parse_source(Source::new(template, name))
}

fn assert_syntax_error(env: &Environment, template: &str, text: &str) {
    match parse(env, template, "index") {
        Ok(node) => panic!("{template}: expected a SyntaxError, got {node}"),
        Err(e) => {
            assert_eq!(e.kind(), ErrorKind::Syntax, "{template}");
            let message = String::from_utf8_lossy(e.message());
            assert!(
                message.contains(text),
                "{template}: {message:?} does not contain {text:?}"
            );
        }
    }
}

fn print_expr(module: &Node) -> Node {
    module
        .get_node("body")
        .unwrap()
        .get_node("0")
        .unwrap()
        .get_node("expr")
        .unwrap()
}

fn assert_node_equals(expected: &Node, actual: &Node, template: &str) {
    assert_eq!(
        String::from_utf8_lossy(&expected.repr()),
        String::from_utf8_lossy(&actual.repr()),
        "{template}"
    );
}

fn c(value: impl Into<twig::node::Attr>) -> Node {
    expression::constant(value, 1)
}

fn var(name: &str) -> Node {
    expression::context_variable(name.as_bytes(), 1)
}

fn arr(elements: Vec<Node>) -> Node {
    expression::array(elements, 1)
}

fn spread(node: Node) -> Node {
    expression::unary(NodeClass::SpreadUnary, node, 1)
}

fn concat(left: Node, right: Node) -> Node {
    expression::binary(NodeClass::ConcatBinary, left, right, 1).unwrap()
}

#[test]
fn testCanOnlyAssignToNames() {
    let env = env();
    for template in [
        "{% set false = \"foo\" %}",
        "{% set FALSE = \"foo\" %}",
        "{% set true = \"foo\" %}",
        "{% set TRUE = \"foo\" %}",
        "{% set none = \"foo\" %}",
        "{% set NONE = \"foo\" %}",
        "{% set null = \"foo\" %}",
        "{% set NULL = \"foo\" %}",
        "{% set 3 = \"foo\" %}",
        "{% set 1 + 2 = \"foo\" %}",
        "{% set \"bar\" = \"foo\" %}",
        "{% set %}{% endset %}",
    ] {
        assert_syntax_error(&env, template, "");
    }
}

#[test]
fn testSequenceExpression() {
    let env = env();
    let cases: Vec<(&str, Node)> = vec![
        // simple sequence
        ("{{ [1, 2] }}", arr(vec![c(0), c(1), c(1), c(2)])),
        // sequence with trailing ,
        ("{{ [1, 2, ] }}", arr(vec![c(0), c(1), c(1), c(2)])),
        // simple mapping
        (
            "{{ {\"a\": \"b\", \"b\": \"c\"} }}",
            arr(vec![c("a"), c("b"), c("b"), c("c")]),
        ),
        // mapping with trailing ,
        (
            "{{ {\"a\": \"b\", \"b\": \"c\", } }}",
            arr(vec![c("a"), c("b"), c("b"), c("c")]),
        ),
        // mapping in a sequence
        (
            "{{ [1, {\"a\": \"b\", \"b\": \"c\"}] }}",
            arr(vec![
                c(0),
                c(1),
                c(1),
                arr(vec![c("a"), c("b"), c("b"), c("c")]),
            ]),
        ),
        // sequence in a mapping
        (
            "{{ {\"a\": [1, 2], \"b\": \"c\"} }}",
            arr(vec![
                c("a"),
                arr(vec![c(0), c(1), c(1), c(2)]),
                c("b"),
                c("c"),
            ]),
        ),
        (
            "{{ {a, b} }}",
            arr(vec![c("a"), var("a"), c("b"), var("b")]),
        ),
        // sequence with spread operator
        (
            "{{ [1, 2, ...foo] }}",
            arr(vec![c(0), c(1), c(1), c(2), c(2), spread(var("foo"))]),
        ),
        // mapping with spread operator
        (
            "{{ {\"a\": \"b\", \"b\": \"c\", ...otherLetters} }}",
            arr(vec![
                c("a"),
                c("b"),
                c("b"),
                c("c"),
                c(0),
                spread(var("otherLetters")),
            ]),
        ),
    ];
    for (template, expected) in cases {
        let module = parse(&env, template, "").unwrap_or_else(|e| panic!("{e}"));
        assert_node_equals(&expected, &print_expr(&module), template);
    }
}

#[test]
fn testSequenceSyntaxError() {
    let env = env();
    for template in [
        "{{ [1, \"a\": \"b\"] }}",
        "{{ {\"a\": \"b\", 2} }}",
        "{{ {\"a\"} }}",
    ] {
        assert_syntax_error(&env, template, "");
    }
}

#[test]
fn testStringExpressionDoesNotConcatenateTwoConsecutiveStrings() {
    assert_syntax_error(&env(), "{{ \"a\" \"b\" }}", "");
}

#[test]
fn testSequenceDestructuringUsesAssignmentTargets() {
    let module = parse(&env(), "{{ ([first, , third] = values) }}", "index").unwrap();
    let node = print_expr(&module);

    assert!(node.is(NodeClass::SequenceDestructuringSetBinary));
    let pairs = node.get_node("left").unwrap().key_value_pairs();
    assert_eq!(pairs[0].1.class(), NodeClass::AssignContextVariable);
    assert_eq!(pairs[0].1.attr_bytes("name").unwrap(), b"first");
    assert_eq!(pairs[1].1.class(), NodeClass::EmptyExpression);
    assert_eq!(pairs[2].1.class(), NodeClass::AssignContextVariable);
    assert_eq!(pairs[2].1.attr_bytes("name").unwrap(), b"third");
}

#[test]
fn testEmptyDestructuringThrows() {
    let env = env();
    for template in [
        "{% do [] = values %}",
        "{% do {} = values %}",
        "{% do [,] = values %}",
        "{% do [,,] = values %}",
    ] {
        assert_syntax_error(
            &env,
            template,
            "Cannot destructure to an empty list of variables in \"index\" at line 1.",
        );
    }
}

#[test]
fn testObjectDestructuringUsesAssignmentTargets() {
    let module = parse(&env(), "{{ ({name: user_name} = user) }}", "index").unwrap();
    let node = print_expr(&module);

    assert!(node.is(NodeClass::ObjectDestructuringSetBinary));
    let (key, value) = node.get_node("left").unwrap().key_value_pairs()[0].clone();
    assert_eq!(key.class(), NodeClass::ConstantExpression);
    assert_eq!(key.attr_bytes("value").unwrap(), b"name");
    assert_eq!(value.class(), NodeClass::AssignContextVariable);
    assert_eq!(value.attr_bytes("name").unwrap(), b"user_name");
}

#[test]
fn testStringExpression() {
    let env = env();
    let cases = vec![
        ("{{ \"foo #{bar}\" }}", concat(c("foo "), var("bar"))),
        (
            "{{ \"foo #{bar} baz\" }}",
            concat(concat(c("foo "), var("bar")), c(" baz")),
        ),
        (
            "{{ \"foo #{\"foo #{bar} baz\"} baz\" }}",
            concat(
                concat(c("foo "), concat(concat(c("foo "), var("bar")), c(" baz"))),
                c(" baz"),
            ),
        ),
    ];
    for (template, expected) in cases {
        let module = parse(&env, template, "").unwrap_or_else(|e| panic!("{e}"));
        assert_node_equals(&expected, &print_expr(&module), template);
    }
}

#[test]
fn testMacroDefinitionDoesNotSupportNonNameVariableName() {
    assert_syntax_error(
        &env(),
        "{% macro foo(\"a\") %}{% endmacro %}",
        "An argument must be a name. Unexpected token \"string\" of value \"a\" (\"name\" expected) in \"index\" at line 1 column 14.",
    );
}

#[test]
fn testMacroDefinitionDoesNotSupportNonConstantDefaultValues() {
    let env = env();
    for template in [
        "{% macro foo(name = \"a #{foo} a\") %}{% endmacro %}",
        "{% macro foo(name = [[\"b\", \"a #{foo} a\"]]) %}{% endmacro %}",
    ] {
        assert_syntax_error(
            &env,
            template,
            "A default value for an argument must be a constant (a boolean, a string, a number, a sequence, or a mapping) in \"index\" at line 1",
        );
    }
}

#[test]
fn testMacroDefinitionSupportsConstantDefaultValues() {
    let env = env();
    for template in [
        "{% macro foo(name = \"aa\") %}{% endmacro %}",
        "{% macro foo(name = 12) %}{% endmacro %}",
        "{% macro foo(name = true) %}{% endmacro %}",
        "{% macro foo(name = [\"a\"]) %}{% endmacro %}",
        "{% macro foo(name = [[\"a\"]]) %}{% endmacro %}",
        "{% macro foo(name = {a: \"a\"}) %}{% endmacro %}",
        "{% macro foo(name = {a: {b: \"a\"}}) %}{% endmacro %}",
    ] {
        parse(&env, template, "index").unwrap_or_else(|e| panic!("{template}: {e}"));
    }
}

#[test]
fn testUnknownFunction() {
    assert_syntax_error(
        &env(),
        "{{ cycl() }}",
        "Unknown \"cycl\" function. Did you mean \"cycle\" in \"index\" at line 1?",
    );
}

#[test]
fn testUnknownFunctionWithoutSuggestions() {
    assert_syntax_error(
        &env(),
        "{{ foobar() }}",
        "Unknown \"foobar\" function in \"index\" at line 1.",
    );
}

#[test]
fn testUnknownFilter() {
    assert_syntax_error(
        &env(),
        "{{ 1|lowe }}",
        "Unknown \"lowe\" filter. Did you mean \"lower\" in \"index\" at line 1?",
    );
}

#[test]
fn testUnknownFilterWithoutSuggestions() {
    assert_syntax_error(
        &env(),
        "{{ 1|foobar }}",
        "Unknown \"foobar\" filter in \"index\" at line 1.",
    );
}

#[test]
fn testUnknownTest() {
    assert_syntax_error(
        &env(),
        "{{ 1 is nul }}",
        "Unknown \"nul\" test. Did you mean \"null\" in \"index\" at line 1",
    );
}

#[test]
fn testUnknownTestWithoutSuggestions() {
    assert_syntax_error(
        &env(),
        "{{ 1 is foobar }}",
        "Unknown \"foobar\" test in \"index\" at line 1.",
    );
}

#[test]
fn testTwoWordTestPrecedence() {
    // a "empty element" test must have precedence over "empty"
    let env = Environment::new(vec![Rc::new(TestExtension {
        tests: vec![TwigCallable::test("empty element", Some("foo"))],
        ..TestExtension::default()
    })]);
    parse(&env, "{{ 1 is empty element }}", "index").unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn testUnaryPrecedenceChange() {
    let env = Environment::new(vec![Rc::new(TestExtension {
        expression_parsers: vec![ExpressionParser::unary(NodeClass::NotUnary, "!", 50)],
        ..TestExtension::default()
    })]);
    parse(&env, "{{ !false ? \"OK\" : \"KO\" }}", "index").unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn testLiteralExpressionParserGetOperatorTokensReturnsEmptyArray() {
    let env = env();
    let parsers = env.expression_parsers();
    let parser = parsers.get(
        parsers
            .by_class(ParserKind::Literal.php_class())
            .expect("LiteralExpressionParser"),
    );

    assert_eq!(parser.operator_tokens(), Vec::<&str>::new());
    assert_eq!(parser.name(), "literal");
}

#[test]
fn testExpressionParserGetOperatorTokensDefaultBehavior() {
    let env = env();
    let parsers = env.expression_parsers();
    for id in parsers.ids() {
        let parser = parsers.get(id);
        if matches!(parser.kind(), ParserKind::Literal) {
            continue;
        }
        let mut expected = vec![parser.name()];
        expected.extend(parser.aliases().iter().map(String::as_str));
        assert_eq!(
            expected,
            parser.operator_tokens(),
            "getOperatorTokens() for {} should return name + aliases.",
            parser.kind().php_class()
        );
    }
}

#[test]
fn testLiteralIsNotRegisteredAsOperator() {
    // Ensure "literal" is not in the operator registry
    let env = env();
    let parsers = env.expression_parsers();
    assert!(
        parsers
            .by_name(ExpressionParserType::Prefix, b"literal")
            .is_none()
    );
    assert!(
        parsers
            .by_name(ExpressionParserType::Infix, b"literal")
            .is_none()
    );
}
