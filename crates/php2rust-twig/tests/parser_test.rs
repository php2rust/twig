//! ParserTest.php at twig/twig 8c737079b726af72ff8ef3c595be9f6a810ea1ef.
//! Names and assertions are PHPUnit's; data-provider rows are written out
//! as cases of one test. `expectExceptionMessage` checks that the message
//! contains the text. `expectDeprecation` and `collectDeprecations()` use
//! `symfony_deprecation_contracts::collect`.
//!
//! Not here (pending, they need the node visitors: `CorrectnessNodeVisitor`
//! reports macro calls without parentheses, `OptimizerNodeVisitor` turns
//! constant prints into text): testMacroTargetsWithoutParenthesesAreDeprecated,
//! testAMacroCallWithoutParenthesesIsReportedOncePerCallSite,
//! testEachMacroCallWithoutParenthesesIsReportedOnItsOwnLine,
//! testDistinctMacroCallsWithoutParenthesesOnTheSameLineAreBothReported,
//! testADynamicMacroCallWithoutParenthesesIsReportedWithoutAName,
//! testAMacroCallWithParenthesesDoesNotSilenceOneWithoutOnTheSameLine,
//! testDocumentationIsAvailableToNodeVisitors,
//! testDocumentationIsRemovedFromUnsupportedOptimizedNodes.
//!
//! Upstream: twig/twig tests/ParserTest.php
#![allow(non_snake_case, clippy::unwrap_used, clippy::expect_used)]

use std::{rc::Rc, sync::Arc};

use twig::{
    Environment, Source,
    error::{Error, ErrorKind},
    extension::Extension,
    node::{Attr, Key, Node, NodeClass, expression, statement},
    parser::Parser,
    token::{Token, TokenType},
    token_parser::TokenParser,
    token_stream::TokenStream,
};

fn env() -> Environment {
    Environment::new(Vec::new())
}

fn env_with(parser: impl TokenParser + 'static) -> Environment {
    struct Ext(Rc<dyn TokenParser>);
    impl Extension for Ext {
        fn name(&self) -> &'static str {
            "TestExtension"
        }
        fn token_parsers(&self) -> Vec<Rc<dyn TokenParser>> {
            vec![self.0.clone()]
        }
    }
    Environment::new(vec![Rc::new(Ext(Rc::new(parser)))])
}

fn parse(env: &Environment, template: &str) -> Node {
    env.parse_source(Source::new(template, "index"))
        .unwrap_or_else(|e| panic!("{e}"))
}

fn deprecations_of(env: &Environment, template: &str) -> (Node, Vec<String>) {
    let (result, deprecations) =
        symfony_deprecation_contracts::collect(|| env.parse_source(Source::new(template, "index")));
    let module = result.unwrap_or_else(|e| panic!("{e}"));
    let messages = deprecations
        .into_iter()
        .map(|d| String::from_utf8_lossy(&d.message).into_owned())
        .collect();
    (module, messages)
}

fn assert_syntax_error(result: Result<Node, Error>, text: &str) {
    match result {
        Ok(node) => panic!("expected a SyntaxError, got {node}"),
        Err(e) => {
            assert_eq!(e.kind(), ErrorKind::Syntax);
            let message = String::from_utf8_lossy(e.message());
            assert!(
                message.contains(text),
                "{message:?} does not contain {text:?}"
            );
        }
    }
}

fn token(ty: TokenType, value: &str) -> Token {
    Token::new(ty, value, 1)
}

fn stream(tokens: Vec<Token>) -> TokenStream {
    TokenStream::new(tokens, Arc::new(Source::new("", "")))
}

fn body(module: &Node) -> Node {
    module.get_node("body").unwrap().get_node(0_i64).unwrap()
}

fn child(node: &Node, name: impl Into<Key>) -> Node {
    node.get_node(name).unwrap()
}

fn doc(node: &Node) -> Option<String> {
    node.documentation()
        .map(|d| String::from_utf8_lossy(&d).into_owned())
}

/// `collectExpressions()`: the number of macro references and of
/// attribute expressions under the node.
fn collect_expressions(node: &Node) -> (usize, usize) {
    let mut counts = (
        usize::from(node.is(NodeClass::MacroReferenceExpression)),
        usize::from(node.is(NodeClass::GetAttrExpression)),
    );
    for child in node.child_nodes() {
        let (m, a) = collect_expressions(&child);
        counts.0 += m;
        counts.1 += a;
    }
    counts
}

/// `getParser()`.
fn get_parser(env: &Environment) -> Parser<'_> {
    let mut parser = Parser::new(env);
    parser
        .set_parent(expression::constant("base.html", 1), true)
        .unwrap();
    parser
}

#[test]
fn testUnknownTag() {
    let env = env();
    let mut parser = Parser::new(&env);
    let result = parser.parse(stream(vec![
        token(TokenType::BlockStart, ""),
        token(TokenType::Name, "foo"),
        token(TokenType::BlockEnd, ""),
        token(TokenType::Eof, ""),
    ]));
    assert_syntax_error(
        result,
        "Unknown \"foo\" tag. Did you mean \"for\" at line 1?",
    );
}

#[test]
fn testUnknownTagWithoutSuggestions() {
    let env = env();
    let mut parser = Parser::new(&env);
    let result = parser.parse(stream(vec![
        token(TokenType::BlockStart, ""),
        token(TokenType::Name, "foobar"),
        token(TokenType::BlockEnd, ""),
        token(TokenType::Eof, ""),
    ]));
    assert_syntax_error(result, "Unknown \"foobar\" tag at line 1.");
}

/// `TestTokenParser`.
struct ReentrantTokenParser;

impl TokenParser for ReentrantTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, _token: &Token) -> Result<Node, Error> {
        // simulate the parsing of another template right in the middle of the parsing of the current template
        parser.parse(stream(vec![
            token(TokenType::BlockStart, ""),
            token(TokenType::Name, "extends"),
            token(TokenType::String, "base"),
            token(TokenType::BlockEnd, ""),
            token(TokenType::Eof, ""),
        ]))?;
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        Ok(Node::empty(1))
    }

    fn tag(&self) -> &'static str {
        "test"
    }
}

#[test]
fn testParseIsReentrant() {
    let env = env_with(ReentrantTokenParser);
    let mut parser = Parser::new(&env);
    parser
        .parse(stream(vec![
            token(TokenType::BlockStart, ""),
            token(TokenType::Name, "test"),
            token(TokenType::BlockEnd, ""),
            token(TokenType::VarStart, ""),
            token(TokenType::Name, "foo"),
            token(TokenType::VarEnd, ""),
            token(TokenType::Eof, ""),
        ]))
        .unwrap();
    assert!(!parser.has_inheritance());
}

#[test]
fn testGetVarName() {
    let env = env();
    parse(
        &env,
        "{% from _self import foo %}\n\n{% macro foo() %}\n    {{ foo }}\n{% endmacro %}",
    );
}

fn macro_target_expressions() -> Vec<String> {
    let mut out = Vec::new();
    for target in ["_self", "macros"] {
        out.push(format!("{target}.foo()"));
        out.push(format!("({target}.foo())"));
        out.push(format!("{target}.(name)()"));
        out.push(format!("({target}.(name)())"));
    }
    out
}

fn macro_target_expressions_without_parentheses() -> Vec<(String, Option<&'static str>)> {
    let mut out = Vec::new();
    for target in ["_self", "macros"] {
        out.push((format!("{target}.foo"), Some("foo")));
        out.push((format!("({target}.foo)"), Some("foo")));
        out.push((format!("{target}.(name)"), None));
        out.push((format!("({target}.(name))"), None));
    }
    out
}

#[test]
fn testMacroTargetsOnlyCompileAsMacroReferences() {
    let env = env();
    for expression in macro_target_expressions() {
        let module = parse(
            &env,
            &format!("{{% import _self as macros %}}{{{{ {expression} }}}}"),
        );
        assert_eq!(collect_expressions(&module), (1, 0), "{expression}");
    }
}

#[test]
fn testMacroTargetsWithoutParenthesesAreAllowedInDefinedTest() {
    let env = env();
    for (expression, _) in macro_target_expressions_without_parentheses() {
        let (module, deprecations) = deprecations_of(
            &env,
            &format!(
                "{{% import _self as macros %}}{{{{ {expression} is defined }}}}{{{{ {expression} is not defined }}}}"
            ),
        );
        assert_eq!(deprecations, Vec::<String>::new(), "{expression}");
        assert_eq!(collect_expressions(&module), (2, 0), "{expression}");
    }
}

#[test]
fn testMacroTargetsWithParenthesesAreDeprecatedInDefinedTest() {
    let env = env();
    for expression in macro_target_expressions() {
        let (module, deprecations) = deprecations_of(
            &env,
            &format!("{{% import _self as macros %}}{{{{ {expression} is defined }}}}"),
        );
        assert!(
            deprecations.iter().any(|d| d == "Since twig/twig 3.29: Using parentheses when testing a macro with the \"defined\" test is deprecated and will throw a SyntaxError in Twig 4.0; remove the parentheses after the macro name in \"index\" at line 1."),
            "{expression}: {deprecations:?}"
        );
        assert_eq!(collect_expressions(&module), (1, 0), "{expression}");
    }
}

#[test]
fn testImplicitMacroArgumentDefaultValues() {
    let env = env();
    let stream = env
        .tokenize(Source::new(
            "{% macro marco (po, lo = true) %}{% endmacro %}",
            "index",
        ))
        .unwrap();
    let module = get_parser(&env).parse(stream).unwrap();
    let arguments = child(&child(&child(&module, "macros"), "marco"), "arguments");

    assert!(child(&arguments, 1_i64).has_attr("is_implicit"));
    assert!(matches!(
        child(&arguments, 1_i64).get_attr("value").unwrap(),
        Attr::Null
    ));

    assert!(!child(&arguments, 3_i64).has_attr("is_implicit"));
    assert!(matches!(
        child(&arguments, 3_i64).get_attr("value").unwrap(),
        Attr::Bool(true)
    ));
}

#[test]
fn testMacroDeclarationIsRepresentedInTheTemplateBody() {
    let module = parse(&env(), "{% macro input() %}{% endmacro %}");
    let declaration = child(&child(&module, "body"), "0");

    assert!(declaration.is(NodeClass::MacroDeclarationNode));
    assert_eq!(declaration.attr_bytes("name").unwrap(), b"input");
    assert_eq!(declaration.tag().as_deref(), Some("macro"));
    assert_eq!(child(&module, "macros").count(), 1);
    assert!(!declaration.ptr_eq(&child(&child(&module, "macros"), "input")));
}

#[test]
fn testEmbeddedTemplatesHaveSequentialIndices() {
    let env = env();
    let stream = env
        .tokenize(Source::new(
            "{% embed \"first\" %}{% endembed %}{% embed \"second\" %}{% endembed %}",
            "index",
        ))
        .unwrap();
    let module = get_parser(&env).parse(stream).unwrap();
    let Attr::Node(embeds) = module.get_attr("embedded_templates").unwrap() else {
        panic!("embedded_templates is not a node");
    };

    assert_eq!(
        child(&embeds, 0_i64).attr("index").unwrap().as_int(),
        Some(1)
    );
    assert_eq!(
        child(&embeds, 1_i64).attr("index").unwrap().as_int(),
        Some(2)
    );
}

#[test]
fn testBodyForChildTemplates() {
    let module = parse(
        &env(),
        "{% extends \"base\" %}\n\n{% block header %}\n    header\n{% endblock %}\n\n{% set foo = 'bar' %}\n\n{% block footer %}\n    footer\n{% endblock %}\n",
    );
    let body = body(&module);
    assert_eq!(body.count(), 2);
    assert_eq!(child(&body, "0").tag().as_deref(), Some("extends"));
    assert_eq!(child(&body, "4").tag().as_deref(), Some("set"));
}

/// `ParentSettingTokenParser`.
struct ParentSettingTokenParser;

impl TokenParser for ParentSettingTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let line = i64::try_from(token.line()).unwrap_or(-1);
        parser.set_parent(expression::constant("base", line), false)?;
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        // returns a blank text node so the whole child body is a single removable node
        Ok(statement::text(b"   ", line))
    }

    fn tag(&self) -> &'static str {
        "set_parent"
    }
}

#[test]
fn testCleanupBodyForChildTemplatesWithASingleNodeBody() {
    let module = parse(&env_with(ParentSettingTokenParser), "{% set_parent %}");
    assert!(body(&module).is(NodeClass::EmptyNode));
}

/// `UnregisteredBlockReferenceTokenParser`.
struct UnregisteredBlockReferenceTokenParser;

impl TokenParser for UnregisteredBlockReferenceTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let line = i64::try_from(token.line()).unwrap_or(-1);
        parser.set_parent(expression::constant("base", line), false)?;
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        Ok(statement::block_reference(b"missing", line))
    }

    fn tag(&self) -> &'static str {
        "unregistered_block_reference"
    }
}

#[test]
fn testCleanupBodyAcceptsAnUnregisteredBlockReference() {
    let module = parse(
        &env_with(UnregisteredBlockReferenceTokenParser),
        "{% unregistered_block_reference %}",
    );
    assert!(body(&module).is(NodeClass::EmptyNode));
}

#[test]
fn testDocumentationIsAttachedToNodes() {
    let module = parse(
        &env(),
        "{## Printed expression #}\n{{ answer }}\n{## Conditional tag #}\n{% if enabled %}Enabled{% endif %}\n{## Do tag #}\n{% do max() %}\n{## Text node #}\nText",
    );
    let body = body(&module);

    assert!(child(&body, "0").is(NodeClass::PrintNode));
    assert_eq!(
        doc(&child(&body, "0")).as_deref(),
        Some("Printed expression")
    );
    assert_eq!(doc(&child(&child(&body, "0"), "expr")), None);
    assert!(child(&body, "2").is(NodeClass::IfNode));
    assert_eq!(doc(&child(&body, "2")).as_deref(), Some("Conditional tag"));
    assert!(child(&body, "3").is(NodeClass::DoNode));
    assert_eq!(doc(&child(&body, "3")).as_deref(), Some("Do tag"));
    assert!(child(&body, "4").is(NodeClass::TextNode));
    assert_eq!(doc(&child(&body, "4")), None);
}

#[test]
fn testEmptyCommentIsNotLexedAsDocumentation() {
    let module = parse(
        &env(),
        "{##}{{ answer }}\n{% block a %}x{##}{% endblock %}{% block b %}y{% endblock %}",
    );
    let body = body(&module);

    assert!(child(&body, "0").is(NodeClass::PrintNode));
    assert_eq!(doc(&child(&body, "0")), None);
}

#[test]
fn testInlineDocumentationBeforeATagNameIsIgnored() {
    let module = parse(&env(), "{% ## Not tag documentation\ndo max() %}");
    assert_eq!(doc(&body(&module)), None);
}

/// `DocumentationTargetTokenParser`.
struct DocumentationTargetTokenParser;

impl TokenParser for DocumentationTargetTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let line = i64::try_from(token.line()).unwrap_or(-1);
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        let target = Node::empty(line);
        parser.set_documentation_target(&target)?;
        Ok(Node::nodes(vec![(Key::from("target"), target)], line))
    }

    fn tag(&self) -> &'static str {
        "documentation_target"
    }
}

#[test]
fn testCustomTokenParserCanSetDocumentationTarget() {
    let module = parse(
        &env_with(DocumentationTargetTokenParser),
        "{## Target documentation #}{% documentation_target %}",
    );
    let node = body(&module);

    assert_eq!(doc(&node), None);
    assert_eq!(
        doc(&child(&node, "target")).as_deref(),
        Some("Target documentation")
    );
}

#[test]
fn testDocumentationIsAttachedToAssignmentTargets() {
    let module = parse(
        &env(),
        "{% set ## First name\n    first_name, ## Last name\n    last_name = user.first_name, user.last_name\n%}\n{% for ## Product identifier\n    product_id, ## Product\n    product in products\n%}{% endfor %}",
    );
    let body = body(&module);
    let names = child(&child(&body, "0"), "names");

    assert_eq!(doc(&child(&names, "0")).as_deref(), Some("First name"));
    assert_eq!(doc(&child(&names, "1")).as_deref(), Some("Last name"));

    let for_node = child(&body, "1");
    assert!(for_node.is(NodeClass::ForNode));
    assert_eq!(
        doc(&child(&for_node, "key_target")).as_deref(),
        Some("Product identifier")
    );
    assert_eq!(
        doc(&child(&for_node, "value_target")).as_deref(),
        Some("Product")
    );
}

#[test]
fn testDocumentationIsAttachedToMacroNode() {
    let env = env();
    let module = parse(
        &env,
        "{## Input macro #}\n{% macro input() %}{% endmacro %}",
    );
    assert_eq!(
        doc(&child(&child(&module, "macros"), "input")).as_deref(),
        Some("Input macro")
    );

    let module = parse(
        &env,
        "{## Input macro #}\n{% macro input(## Name argument\nname, ## Value argument\nvalue) %}{% endmacro %}",
    );
    let declaration = body(&module);
    let macro_node = child(&child(&module, "macros"), "input");
    let arguments = child(&macro_node, "arguments");

    assert_eq!(doc(&declaration), None);
    assert_eq!(doc(&macro_node).as_deref(), Some("Input macro"));
    assert_eq!(
        doc(&child(&arguments, "0")).as_deref(),
        Some("Name argument")
    );
    assert_eq!(child(&arguments, "0").line(), 3);
    assert_eq!(
        doc(&child(&arguments, "2")).as_deref(),
        Some("Value argument")
    );
    assert_eq!(child(&arguments, "2").line(), 4);
}

#[test]
fn testDuplicateMacroKeepsOnlyTheLastDocumentation() {
    let (module, deprecations) = deprecations_of(
        &env(),
        "{## First #}{% macro input() %}{% endmacro %}{## Second #}{% macro input() %}{% endmacro %}",
    );
    assert!(deprecations.contains(&"Since twig/twig 3.29: Defining the macro \"input\" more than once in \"index\" is deprecated and will throw a SyntaxError in Twig 4.0 (previous definition at line 1, new definition at line 1). The last definition is used in Twig 3.".to_owned()), "{deprecations:?}");

    assert_eq!(
        doc(&child(&child(&module, "macros"), "input")).as_deref(),
        Some("Second")
    );
    for declaration in body(&module).child_nodes() {
        assert_eq!(doc(&declaration), None);
    }
}

#[test]
fn testDuplicateMacroDeprecationsReferToPreviousDefinition() {
    let (_, deprecations) = deprecations_of(
        &env(),
        "{% macro input() %}{% endmacro %}\n{% macro input() %}{% endmacro %}\n{% macro input() %}{% endmacro %}",
    );
    assert!(deprecations.contains(&"Since twig/twig 3.29: Defining the macro \"input\" more than once in \"index\" is deprecated and will throw a SyntaxError in Twig 4.0 (previous definition at line 1, new definition at line 2). The last definition is used in Twig 3.".to_owned()), "{deprecations:?}");
    assert!(deprecations.contains(&"Since twig/twig 3.29: Defining the macro \"input\" more than once in \"index\" is deprecated and will throw a SyntaxError in Twig 4.0 (previous definition at line 2, new definition at line 3). The last definition is used in Twig 3.".to_owned()), "{deprecations:?}");
}

#[test]
fn testBodyForParentTemplates() {
    let module = parse(
        &env(),
        "{% block header %}\n    header\n{% endblock %}\n\n{% set foo = 'bar' %}\n\n{% block footer %}\n    footer\n{% endblock %}\n",
    );
    let body = body(&module);
    assert_eq!(body.count(), 5);
    assert_eq!(child(&body, "0").tag().as_deref(), Some("block"));
    assert!(child(&body, "1").is(NodeClass::TextNode));
    assert_eq!(child(&body, "2").tag().as_deref(), Some("set"));
    assert!(child(&body, "3").is(NodeClass::TextNode));
    assert_eq!(child(&body, "4").tag().as_deref(), Some("block"));
}
