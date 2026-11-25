//! Port of the definitions in Twig\Extension\CoreExtension.php: the core
//! tags, filters, functions, tests and operators, and the parser callables
//! of `parent()`, `block()` and `attribute()`.
//!
//! The filters' and functions' PHP implementations are named by their
//! callables here; their Rust ports come with the runtime.
//!
//! Upstream: twig/twig src/Extension/CoreExtension.php

use std::rc::Rc;

use super::Extension;
use crate::{
    callable::{DeprecatedCallableInfo, TwigCallable},
    error::Error,
    expression_parser::{Associativity, ExpressionParser, ParserKind, PrecedenceChange},
    node::{
        Node, NodeClass,
        expression::{self, ANY_CALL},
    },
    parser::Parser,
    token_parser::{self, TokenParser},
    util::callable_arguments::{Parameter, extract_arguments},
};

/// `CoreExtension`.
#[derive(Debug, Default)]
pub struct CoreExtension;

const CORE: &str = "Twig\\Extension\\CoreExtension";

fn core(method: &str) -> String {
    format!("{CORE}::{method}")
}

fn filter(name: &str, method: &str) -> TwigCallable {
    TwigCallable::filter(name, Some(&core(method)))
}

fn function(name: &str, method: &str) -> TwigCallable {
    TwigCallable::function(name, Some(&core(method)))
}

impl Extension for CoreExtension {
    fn name(&self) -> &str {
        CORE
    }

    fn node_visitors(&self) -> Vec<Box<dyn crate::node_visitor::NodeVisitor>> {
        vec![Box::new(
            crate::node_visitor::correctness::CorrectnessNodeVisitor::default(),
        )]
    }

    fn token_parsers(&self) -> Vec<Rc<dyn TokenParser>> {
        token_parser::core_token_parsers()
    }

    fn filters(&self) -> Vec<TwigCallable> {
        vec![
            // formatting filters
            filter("date", "formatDate"),
            filter("date_modify", "modifyDate"),
            filter("format", "sprintf"),
            filter("replace", "replace"),
            filter("number_format", "formatNumber"),
            TwigCallable::filter("abs", Some("abs")),
            filter("round", "round"),
            // encoding
            filter("url_encode", "urlencode"),
            TwigCallable::filter("json_encode", Some("json_encode")),
            filter("convert_encoding", "convertEncoding"),
            // string filters
            filter("title", "titleCase").needs_charset(),
            filter("capitalize", "capitalize").needs_charset(),
            filter("upper", "upper").needs_charset(),
            filter("lower", "lower").needs_charset(),
            filter("striptags", "striptags"),
            filter("trim", "trim"),
            filter("nl2br", "nl2br").pre_escape("html").safe(&["html"]),
            filter("spaceless", "spaceless")
                .pre_escape("html")
                .safe(&["html"])
                .deprecated(DeprecatedCallableInfo::new("twig/twig", "3.12")),
            // array helpers
            filter("join", "join"),
            filter("split", "split").needs_charset(),
            filter("sort", "sort")
                .needs_environment()
                .needs_is_sandboxed(),
            filter("merge", "merge"),
            filter("batch", "batch"),
            filter("column", "column")
                .needs_environment()
                .needs_is_sandboxed(),
            filter("filter", "filter")
                .needs_environment()
                .needs_is_sandboxed(),
            filter("map", "map")
                .needs_environment()
                .needs_is_sandboxed(),
            filter("reduce", "reduce")
                .needs_environment()
                .needs_is_sandboxed(),
            filter("find", "find")
                .needs_environment()
                .needs_is_sandboxed(),
            // string/array filters
            filter("reverse", "reverse").needs_charset(),
            filter("shuffle", "shuffle").needs_charset(),
            filter("length", "length").needs_charset(),
            filter("slice", "slice").needs_charset(),
            filter("first", "first").needs_charset(),
            filter("last", "last").needs_charset(),
            // iteration and runtime
            filter("default", "default").node_class(NodeClass::DefaultFilter),
            filter("keys", "keys"),
            filter("invoke", "invoke"),
        ]
    }

    fn functions(&self) -> Vec<TwigCallable> {
        vec![
            TwigCallable::function("parent", None).parser_callable(parse_parent_function),
            TwigCallable::function("block", None).parser_callable(parse_block_function),
            TwigCallable::function("attribute", None).parser_callable(parse_attribute_function),
            TwigCallable::function("max", Some("max")),
            TwigCallable::function("min", Some("min")),
            TwigCallable::function("range", Some("range")),
            function("constant", "constant"),
            function("cycle", "cycle"),
            function("random", "random").needs_charset(),
            function("date", "convertDate"),
            function("include", "include")
                .needs_environment()
                .needs_context()
                .safe(&["all"]),
            function("include_only", "includeOnly")
                .needs_environment()
                .safe(&["all"]),
            function("source", "source")
                .needs_environment()
                .safe(&["all"]),
            function("enum_cases", "enumCases").node_class(NodeClass::EnumCasesFunction),
            function("enum", "enum").node_class(NodeClass::EnumFunction),
        ]
    }

    fn tests(&self) -> Vec<TwigCallable> {
        let test = |name: &str, class: NodeClass| {
            TwigCallable::test(name, None)
                .node_class(class)
                .always_allowed_in_sandbox()
        };
        vec![
            test("even", NodeClass::EvenTest),
            test("odd", NodeClass::OddTest),
            test("defined", NodeClass::DefinedTest),
            test("same as", NodeClass::SameasTest).one_mandatory_argument(),
            test("none", NodeClass::NullTest),
            test("null", NodeClass::NullTest),
            test("divisible by", NodeClass::DivisiblebyTest).one_mandatory_argument(),
            TwigCallable::test("constant", None).node_class(NodeClass::ConstantTest),
            TwigCallable::test("empty", Some(&core("testEmpty"))).always_allowed_in_sandbox(),
            TwigCallable::test("iterable", Some("is_iterable")).always_allowed_in_sandbox(),
            TwigCallable::test("sequence", Some(&core("testSequence"))).always_allowed_in_sandbox(),
            TwigCallable::test("mapping", Some(&core("testMapping"))).always_allowed_in_sandbox(),
            test("true", NodeClass::TrueTest),
        ]
    }

    fn expression_parsers(&self) -> Vec<ExpressionParser> {
        use Associativity::{Left, Right};
        use NodeClass as C;
        let change = |version: &str, precedence: i64| {
            Some(PrecedenceChange::new("twig/twig", version, precedence))
        };
        let unary = |class, name, precedence| ExpressionParser::unary(class, name, precedence);
        let binary =
            |class, name, precedence| ExpressionParser::binary(class, name, precedence, Left);
        vec![
            // unary operators
            unary(C::NotUnary, "not", 50).with_precedence_change(change("3.15", 70)),
            unary(C::SpreadUnary, "...", 512)
                .with_description("Spread operator")
                .with_operand_precedence(0),
            unary(C::NegUnary, "-", 500),
            unary(C::PosUnary, "+", 500),
            // binary operators
            ExpressionParser::binary(C::ElvisBinary, "?:", 5, Right)
                .with_description("Elvis operator (a ?: b)")
                .with_aliases(&["? :"]),
            ExpressionParser::binary(C::NullCoalesceBinary, "??", 300, Right)
                .with_precedence_change(change("3.15", 5))
                .with_description("Null coalescing operator (a ?? b)"),
            binary(C::OrBinary, "or", 10),
            binary(C::XorBinary, "xor", 12),
            binary(C::AndBinary, "and", 15),
            binary(C::BitwiseOrBinary, "b-or", 16),
            binary(C::BitwiseXorBinary, "b-xor", 17),
            binary(C::BitwiseAndBinary, "b-and", 18),
            binary(C::EqualBinary, "==", 20),
            binary(C::NotEqualBinary, "!=", 20),
            binary(C::SpaceshipBinary, "<=>", 20),
            binary(C::LessBinary, "<", 20),
            binary(C::GreaterBinary, ">", 20),
            binary(C::GreaterEqualBinary, ">=", 20),
            binary(C::LessEqualBinary, "<=", 20),
            binary(C::NotInBinary, "not in", 20),
            binary(C::InBinary, "in", 20),
            binary(C::MatchesBinary, "matches", 20),
            binary(C::StartsWithBinary, "starts with", 20),
            binary(C::EndsWithBinary, "ends with", 20),
            binary(C::HasSomeBinary, "has some", 20),
            binary(C::HasEveryBinary, "has every", 20),
            binary(C::SameAsBinary, "===", 20),
            binary(C::NotSameAsBinary, "!==", 20),
            binary(C::RangeBinary, "..", 25),
            binary(C::AddBinary, "+", 30),
            binary(C::SubBinary, "-", 30),
            binary(C::ConcatBinary, "~", 40).with_precedence_change(change("3.15", 27)),
            binary(C::MulBinary, "*", 60),
            binary(C::DivBinary, "/", 60),
            binary(C::FloorDivBinary, "//", 60).with_description("Floor division"),
            binary(C::ModBinary, "%", 60),
            ExpressionParser::binary(C::PowerBinary, "**", 200, Right)
                .with_description("Exponentiation operator"),
            // ternary operator
            ExpressionParser::builtin(ParserKind::ConditionalTernary),
            // assignment operator
            ExpressionParser::builtin(ParserKind::Assignment),
            // Twig callables
            ExpressionParser::builtin(ParserKind::Is),
            ExpressionParser::builtin(ParserKind::IsNot),
            ExpressionParser::builtin(ParserKind::Filter),
            ExpressionParser::builtin(ParserKind::Function),
            // get attribute operators
            ExpressionParser::builtin(ParserKind::Dot),
            ExpressionParser::builtin(ParserKind::SquareBracket),
            // group expression
            ExpressionParser::builtin(ParserKind::Grouping),
            // arrow function
            ExpressionParser::builtin(ParserKind::Arrow),
            // all literals
            ExpressionParser::builtin(ParserKind::Literal),
        ]
    }
}

/// `CoreExtension::parseParentFunction()`.
pub fn parse_parent_function(
    parser: &mut Parser<'_>,
    _fake_node: &Node,
    _args: &Node,
    line: i64,
) -> Result<Node, Error> {
    let Some(block_name) = parser.peek_block_stack() else {
        return Err(Error::syntax(
            "Calling the \"parent\" function outside of a block is forbidden.",
            line,
            Some(parser.stream().source_context().clone()),
        ));
    };
    if !parser.has_inheritance() {
        return Err(Error::syntax(
            "Calling the \"parent\" function on a template that does not call \"extends\" or \"use\" is forbidden.",
            line,
            Some(parser.stream().source_context().clone()),
        ));
    }
    Ok(expression::parent(&block_name, line))
}

/// `CoreExtension::parseBlockFunction()`.
pub fn parse_block_function(
    _parser: &mut Parser<'_>,
    fake_node: &Node,
    args: &Node,
    line: i64,
) -> Result<Node, Error> {
    let fake = TwigCallable::function("block", Some("{closure}"));
    let params = [
        Parameter::required("name"),
        Parameter::optional_null("template"),
    ];
    let mut args = extract_arguments(fake_node, &fake, &params, args)?.into_iter();
    let name = args
        .next()
        .ok_or_else(|| Error::logic("Undefined array key 0"))?;
    Ok(expression::block_reference(name, args.next(), line))
}

/// `CoreExtension::parseAttributeFunction()`.
pub fn parse_attribute_function(
    _parser: &mut Parser<'_>,
    fake_node: &Node,
    args: &Node,
    line: i64,
) -> Result<Node, Error> {
    let fake = TwigCallable::function("attribute", Some("{closure}"));
    let params = [
        Parameter::required("variable"),
        Parameter::required("attribute"),
        Parameter::optional_null("arguments"),
    ];
    let mut args = extract_arguments(fake_node, &fake, &params, args)?.into_iter();
    let (Some(variable), Some(attribute)) = (args.next(), args.next()) else {
        return Err(Error::logic("Undefined array key 1"));
    };
    Ok(expression::get_attr(
        variable,
        attribute,
        args.next(),
        ANY_CALL,
        line,
        false,
    ))
}

/// `Rc` of the core definitions, for tests.
#[must_use]
pub fn core_extension() -> Rc<dyn Extension> {
    Rc::new(CoreExtension)
}
