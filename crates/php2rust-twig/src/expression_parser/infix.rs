//! Port of the infix expression parsers' `parse()` methods and of
//! `ArgumentsTrait`.
//!
//! Upstream: twig/twig src/ExpressionParser/Infix/ArgumentsTrait.php src/ExpressionParser/Infix/ArrowExpressionParser.php src/ExpressionParser/Infix/AssignmentExpressionParser.php src/ExpressionParser/Infix/BinaryOperatorExpressionParser.php src/ExpressionParser/Infix/ConditionalTernaryExpressionParser.php src/ExpressionParser/Infix/DotExpressionParser.php src/ExpressionParser/Infix/FilterExpressionParser.php src/ExpressionParser/Infix/FunctionExpressionParser.php src/ExpressionParser/Infix/IsExpressionParser.php src/ExpressionParser/Infix/IsNotExpressionParser.php src/ExpressionParser/Infix/SquareBracketExpressionParser.php

use symfony_deprecation_contracts::trigger_deprecation;

use super::{ExpressionParser, ParserKind};
use crate::{
    error::Error,
    lexer::name_match_len,
    node::{
        Attr, Key, Node, NodeClass,
        expression::{self, ANY_CALL, ARRAY_CALL, METHOD_CALL, MacroName},
    },
    parser::Parser,
    token::{Token, TokenType, TokenValue},
    token_stream::to_line,
};

fn line_of(token: &Token) -> i64 {
    to_line(token.line())
}

/// `InfixExpressionParserInterface::parse($parser, $left, $token)`.
pub fn parse(
    ep: &ExpressionParser,
    parser: &mut Parser<'_>,
    left: Node,
    token: &Token,
) -> Result<Node, Error> {
    match ep.kind() {
        ParserKind::Binary { class } => {
            let right = parser.parse_expression(ep.right_precedence())?;
            expression::binary(class, left, right, line_of(token))
        }
        ParserKind::Assignment => parse_assignment(ep, parser, left, token),
        ParserKind::ConditionalTernary => {
            let then = parser.parse_expression(ep.precedence())?;
            let otherwise = if parser
                .stream_mut()
                .next_if_value(TokenType::Punctuation, b":")?
                .is_some()
            {
                parser.parse_expression(ep.precedence())?
            } else {
                expression::constant("", line_of(token))
            };
            Ok(expression::conditional_ternary(
                left,
                then,
                otherwise,
                line_of(token),
            ))
        }
        ParserKind::Is => parse_is(ep, parser, left, token),
        ParserKind::IsNot => {
            let test = parse_is(ep, parser, left, token)?;
            Ok(expression::unary(NodeClass::NotUnary, test, line_of(token)))
        }
        ParserKind::Filter => parse_filter(parser, left),
        ParserKind::Function => parse_function(parser, &left, token),
        ParserKind::Dot => parse_dot(parser, left, token),
        ParserKind::SquareBracket => parse_square_bracket(parser, left, token),
        ParserKind::Arrow => {
            let expr = parser.parse_expression(0)?;
            expression::arrow_function(expr, left, line_of(token))
        }
        ParserKind::Unary { .. } | ParserKind::Grouping | ParserKind::Literal => Err(Error::logic(
            format!("\"{ep}\" is not an infix expression parser."),
        )),
    }
}

/// `AssignmentExpressionParser::parse()`.
fn parse_assignment(
    ep: &ExpressionParser,
    parser: &mut Parser<'_>,
    left: Node,
    token: &Token,
) -> Result<Node, Error> {
    if !left.is(NodeClass::ContextVariable) && !left.is(NodeClass::ArrayExpression) {
        return Err(Error::syntax(
            format!(
                "Cannot assign to \"{}\", only variables can be assigned.",
                left.class().php_class()
            ),
            line_of(token),
            Some(parser.stream().source_context().clone()),
        ));
    }
    let right = parser.parse_expression(ep.right_precedence())?;
    if left.is(NodeClass::ArrayExpression) {
        let pairs = left.key_value_pairs();
        let is_sequence = left.is_sequence();
        if pairs.is_empty()
            || (is_sequence && pairs.iter().all(|(_, v)| v.is(NodeClass::EmptyExpression)))
        {
            return Err(Error::syntax(
                "Cannot destructure to an empty list of variables.",
                line_of(token),
                Some(parser.stream().source_context().clone()),
            ));
        }
        for (i, (_, value)) in pairs.iter().enumerate() {
            if value.is(NodeClass::ContextVariable) && !value.is(NodeClass::AssignContextVariable) {
                let name = value.attr_bytes("name").unwrap_or_default();
                let assign = expression::assign_context_variable(&name, value.line())?;
                left.set_node(Key::from(2 * i + 1), assign)?;
            }
        }
        let class = if is_sequence {
            NodeClass::SequenceDestructuringSetBinary
        } else {
            NodeClass::ObjectDestructuringSetBinary
        };
        return expression::binary(class, left, right, line_of(token));
    }
    expression::binary(NodeClass::SetBinary, left, right, line_of(token))
}

/// `IsExpressionParser::parse()`.
fn parse_is(
    ep: &ExpressionParser,
    parser: &mut Parser<'_>,
    expr: Node,
    token: &Token,
) -> Result<Node, Error> {
    let test = parser.get_test(line_of(token))?;
    let mut arguments = None;
    if parser.stream().test_value(TokenType::Operator, b"(") {
        arguments = Some(parse_named_arguments(parser, true)?);
    } else if test.has_one_mandatory_argument() {
        let argument = parser.parse_expression(ep.precedence())?;
        arguments = Some(Node::nodes(vec![(Key::Int(0), argument)], 0));
    }
    let mut expr = expr;
    if test.name() == b"defined" {
        if expr.is(NodeClass::NameExpression) {
            let name = expr.attr_bytes("name").unwrap_or_default();
            if let Some(alias) = parser.get_imported_symbol("function", &name) {
                let var = alias
                    .node
                    .as_ref()
                    .ok_or_else(|| Error::logic("An imported macro has no internal reference."))?
                    .get_node("var")?;
                let line = expr.line();
                let reference = expression::macro_reference(
                    var,
                    MacroName::Str(alias.name.unwrap_or_default()),
                    expression::array(Vec::new(), line),
                    line,
                );
                reference.set_has_call_parentheses(false);
                expr = reference;
            }
        }
        if expr.is(NodeClass::MacroReferenceExpression) && expr.has_call_parentheses() {
            let message = format!(
                "Using parentheses when testing a macro with the \"defined\" test is deprecated and will throw a SyntaxError in Twig 4.0; remove the parentheses after the macro name in \"{}\" at line {}.",
                parser.stream().source_context().name(),
                expr.line()
            );
            trigger_deprecation(b"twig/twig", b"3.29", message.as_bytes());
        }
    }
    let line = line_of(parser.current_token());
    expression::test(test.get_node_class(), expr, test, arguments, line)
}

/// `FilterExpressionParser::parse()`.
pub(crate) fn parse_filter(parser: &mut Parser<'_>, expr: Node) -> Result<Node, Error> {
    let token = parser.stream_mut().expect(TokenType::Name)?;
    let line = line_of(&token);
    let arguments = if parser.stream().test_value(TokenType::Operator, b"(") {
        parse_named_arguments(parser, true)?
    } else {
        Node::empty(0)
    };
    let filter = parser.get_filter(&token.value_bytes(), line)?;
    expression::filter(filter.get_node_class(), expr, filter, arguments, line)
}

/// `FunctionExpressionParser::parse()`.
fn parse_function(parser: &mut Parser<'_>, expr: &Node, token: &Token) -> Result<Node, Error> {
    let line = line_of(token);
    if !expr.is(NodeClass::NameExpression) {
        return Err(Error::syntax(
            "Function name must be an identifier.",
            line,
            Some(parser.stream().source_context().clone()),
        ));
    }
    let name = expr.attr_bytes("name").unwrap_or_default();
    if let Some(alias) = parser.get_imported_symbol("function", &name) {
        let arguments = parse_callable_arguments(parser, line, false, true)?;
        let var = alias
            .node
            .as_ref()
            .ok_or_else(|| Error::logic("An imported macro has no internal reference."))?
            .get_node("var")?;
        let node = expression::macro_reference(
            var,
            MacroName::Str(alias.name.unwrap_or_default()),
            arguments,
            line,
        );
        node.set_has_call_parentheses(true);
        return Ok(node);
    }
    let args = parse_named_arguments(parser, false)?;
    let function = parser.get_function(&name, line)?;
    if let Some(parser_callable) = function.get_parser_callable() {
        let fake_node = Node::empty(line);
        fake_node.set_source_context(parser.stream().source_context());
        let node = parser_callable(parser, &fake_node, &args, line)?;
        node.set_attr("sandboxed_function_name", Attr::Str(name));
        node.set_attr("sandboxed_function", Attr::Callable(function));
        return Ok(node);
    }
    Ok(expression::function(
        function.get_node_class(),
        function,
        args,
        line,
    ))
}

/// `DotExpressionParser::parse()`.
fn parse_dot(parser: &mut Parser<'_>, expr: Node, token: &Token) -> Result<Node, Error> {
    let null_safe = token.value_bytes() == b"?.";
    let mut token = parser.current_token().clone();
    let lineno = line_of(&token);
    let mut arguments = expression::array(Vec::new(), lineno);
    let mut call_type = ANY_CALL;
    let attribute = if parser
        .stream_mut()
        .next_if_value(TokenType::Operator, b"(")?
        .is_some()
    {
        let attribute = parser.parse_expression(0)?;
        parser
            .stream_mut()
            .expect_value(TokenType::Punctuation, b")")?;
        attribute
    } else {
        token = parser.stream_mut().next()?;
        let value = token.value_bytes();
        if token.test(TokenType::Name)
            || token.test(TokenType::Number)
            || (token.test(TokenType::Operator) && name_match_len(&value) > 0)
        {
            constant_of(&token)
        } else {
            let mut message = b"Expected name or number, got value \"".to_vec();
            message.extend_from_slice(&value);
            message.extend_from_slice(b"\" of type \"");
            message.extend_from_slice(token.to_english().as_bytes());
            message.extend_from_slice(b"\".");
            return Err(Error::syntax(
                message,
                line_of(&token),
                Some(parser.stream().source_context().clone()),
            ));
        }
    };
    let is_macro_target = expr.is(NodeClass::NameExpression) && {
        let name = expr.attr_bytes("name").unwrap_or_default();
        parser.get_imported_symbol("template", &name).is_some() || name == b"_self"
    };
    if parser.stream().test_value(TokenType::Operator, b"(") {
        call_type = METHOD_CALL;
        arguments = parse_callable_arguments(parser, line_of(&token), true, is_macro_target)?;
    }
    if is_macro_target {
        let name = if attribute.is(NodeClass::ConstantExpression) {
            MacroName::Str(
                attribute
                    .attr("value")
                    .and_then(|v| v.to_php_string())
                    .unwrap_or_default(),
            )
        } else {
            MacroName::Expr(attribute)
        };
        let var_name = expr.attr("name").unwrap_or(Attr::Null);
        let node = expression::macro_reference(
            expression::macro_variable(var_name, expr.line())?,
            name,
            arguments,
            expr.line(),
        );
        node.set_has_call_parentheses(call_type == METHOD_CALL);
        return Ok(node);
    }
    Ok(expression::get_attr(
        expr,
        attribute,
        Some(arguments),
        call_type,
        lineno,
        null_safe,
    ))
}

/// `new ConstantExpression($token->getValue(), $token->getLine())`.
pub(crate) fn constant_of(token: &Token) -> Node {
    expression::constant(token_attr(token.value()), line_of(token))
}

/// A token value as a node attribute value.
pub(crate) fn token_attr(value: &TokenValue) -> Attr {
    match value {
        TokenValue::Bytes(b) => Attr::Str(b.clone()),
        TokenValue::Number(php_value::numeric::Number::Int(i)) => Attr::Int(*i),
        TokenValue::Number(php_value::numeric::Number::Float(f)) => Attr::Float(*f),
    }
}

/// `SquareBracketExpressionParser::parse()`.
fn parse_square_bracket(parser: &mut Parser<'_>, expr: Node, token: &Token) -> Result<Node, Error> {
    let lineno = line_of(token);
    let mut slice = false;
    let attribute = if parser.stream().test_value(TokenType::Punctuation, b":") {
        slice = true;
        expression::constant(0i64, lineno)
    } else {
        parser.parse_expression(0)?
    };
    if parser
        .stream_mut()
        .next_if_value(TokenType::Punctuation, b":")?
        .is_some()
    {
        slice = true;
    }
    if slice {
        let length = if parser.stream().test_value(TokenType::Punctuation, b"]") {
            expression::constant(Attr::Null, lineno)
        } else {
            parser.parse_expression(0)?
        };
        let filter = parser.get_filter(b"slice", lineno)?;
        let arguments = Node::list(vec![attribute, length], 0);
        let node = expression::filter(filter.get_node_class(), expr, filter, arguments, lineno)?;
        parser
            .stream_mut()
            .expect_value(TokenType::Punctuation, b"]")?;
        return Ok(node);
    }
    parser
        .stream_mut()
        .expect_value(TokenType::Punctuation, b"]")?;
    Ok(expression::get_attr(
        expr,
        attribute,
        Some(expression::array(Vec::new(), lineno)),
        ARRAY_CALL,
        lineno,
        false,
    ))
}

/// `ArgumentsTrait::parseCallableArguments()`.
pub(crate) fn parse_callable_arguments(
    parser: &mut Parser<'_>,
    line: i64,
    parse_open_parenthesis: bool,
    preserve_names: bool,
) -> Result<Node, Error> {
    let arguments = expression::array(Vec::new(), line);
    for (key, node) in parse_named_arguments(parser, parse_open_parenthesis)?.children() {
        let key_node = match key {
            Key::Int(i) => expression::local_variable(Attr::Int(i), line)?,
            Key::Str(s) if !preserve_names => expression::local_variable(Attr::Str(s), line)?,
            Key::Str(s) => expression::constant(Attr::Str(s), line),
        };
        arguments.add_element(node, Some(key_node));
    }
    Ok(arguments)
}

/// `ArgumentsTrait::parseNamedArguments()`.
pub(crate) fn parse_named_arguments(
    parser: &mut Parser<'_>,
    parse_open_parenthesis: bool,
) -> Result<Node, Error> {
    let mut args: Vec<(Key, Node)> = Vec::new();
    let mut next_index: i64 = 0;
    if parse_open_parenthesis {
        parser.stream_mut().expect_with(
            TokenType::Operator,
            Some(b"("),
            Some(b"A list of arguments must begin with an opening parenthesis"),
        )?;
    }
    let mut has_spread = false;
    while !parser.stream().test_value(TokenType::Punctuation, b")") {
        if !args.is_empty() {
            parser.stream_mut().expect_with(
                TokenType::Punctuation,
                Some(b","),
                Some(b"Arguments must be separated by a comma"),
            )?;
            if parser.stream().test_value(TokenType::Punctuation, b")") {
                break;
            }
        }
        let mut value = parser.parse_expression(0)?;
        if value.is(NodeClass::SpreadUnary) {
            has_spread = true;
        } else if has_spread {
            return Err(Error::syntax(
                "Normal arguments must be placed before argument unpacking.",
                line_of(parser.current_token()),
                Some(parser.stream().source_context().clone()),
            ));
        }
        let mut name = None;
        if value.is(NodeClass::SetBinary) {
            name = Some(value.get_node("left")?.get_attr("name")?);
            value = value.get_node("right")?;
        } else {
            let token = match parser
                .stream_mut()
                .next_if_value(TokenType::Operator, b"=")?
            {
                Some(t) => Some(t),
                None => parser
                    .stream_mut()
                    .next_if_value(TokenType::Punctuation, b":")?,
            };
            if let Some(token) = token {
                if !value.is(NodeClass::ContextVariable) {
                    return Err(Error::syntax(
                        format!(
                            "A parameter name must be a string, \"{}\" given.",
                            value.class().php_class()
                        ),
                        line_of(&token),
                        Some(parser.stream().source_context().clone()),
                    ));
                }
                name = Some(value.get_attr("name")?);
                value = parser.parse_expression(0)?;
            }
        }
        match name {
            None => {
                args.push((Key::Int(next_index), value));
                next_index = next_index.saturating_add(1);
            }
            Some(name) => {
                let key = Key::from_bytes(&name.to_php_string().unwrap_or_default());
                if let Key::Int(i) = key
                    && i >= next_index
                {
                    next_index = i.saturating_add(1);
                }
                if let Some(slot) = args.iter_mut().find(|(k, _)| *k == key) {
                    slot.1 = value;
                } else {
                    args.push((key, value));
                }
            }
        }
    }
    parser.stream_mut().expect_with(
        TokenType::Punctuation,
        Some(b")"),
        Some(b"A list of arguments must be closed by a parenthesis"),
    )?;
    Ok(Node::nodes(args, 0))
}
