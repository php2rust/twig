//! Port of the prefix expression parsers' `parse()` methods.
//!
//! Upstream: twig/twig src/ExpressionParser/Prefix/GroupingExpressionParser.php src/ExpressionParser/Prefix/LiteralExpressionParser.php src/ExpressionParser/Prefix/UnaryOperatorExpressionParser.php

use super::{ExpressionParser, ParserKind, infix::constant_of};
use crate::{
    error::Error,
    lexer::name_match_len,
    node::{
        Attr, Node, NodeClass,
        expression::{self},
    },
    parser::Parser,
    token::{Token, TokenType},
    token_stream::to_line,
};

fn line_of(token: &Token) -> i64 {
    to_line(token.line())
}

/// `PrefixExpressionParserInterface::parse($parser, $token)`.
pub fn parse(ep: &ExpressionParser, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
    match ep.kind() {
        ParserKind::Unary {
            class,
            operand_precedence,
        } => {
            let operand = parser.parse_expression(operand_precedence.unwrap_or(ep.precedence()))?;
            Ok(expression::unary(class, operand, line_of(token)))
        }
        ParserKind::Grouping => parse_grouping(ep, parser, token),
        ParserKind::Literal => parse_literal(parser, token),
        _ => Err(Error::logic(format!(
            "\"{ep}\" is not a prefix expression parser."
        ))),
    }
}

/// `GroupingExpressionParser::parse()`.
fn parse_grouping(
    ep: &ExpressionParser,
    parser: &mut Parser<'_>,
    token: &Token,
) -> Result<Node, Error> {
    let expr = parser.parse_expression(ep.precedence())?;
    if parser
        .stream_mut()
        .next_if_value(TokenType::Punctuation, b")")?
        .is_some()
    {
        if !parser.stream().test_value(TokenType::Operator, b"=>") {
            return Ok(expr.set_explicit_parentheses());
        }
        return Ok(expression::list(
            vec![to_assign_context_variable(&expr)?],
            line_of(token),
        ));
    }
    if !parser.stream().test_value(TokenType::Punctuation, b",") {
        parser.stream_mut().expect_with(
            TokenType::Punctuation,
            Some(b")"),
            Some(b"An opened parenthesis is not properly closed"),
        )?;
    }
    let mut names = vec![expr];
    let mut token = token.clone();
    loop {
        if parser
            .stream_mut()
            .next_if_value(TokenType::Punctuation, b")")?
            .is_some()
        {
            break;
        }
        parser
            .stream_mut()
            .expect_value(TokenType::Punctuation, b",")?;
        token = parser.stream_mut().expect(TokenType::Name)?;
        names.push(expression::context_variable(
            &token.value_bytes(),
            line_of(&token),
        ));
    }
    if !parser.stream().test_value(TokenType::Operator, b"=>") {
        return Err(Error::syntax(
            "A list of variables must be followed by an arrow.",
            line_of(parser.current_token()),
            Some(parser.stream().source_context().clone()),
        ));
    }
    let items = names
        .iter()
        .map(to_assign_context_variable)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(expression::list(items, line_of(&token)))
}

/// `GroupingExpressionParser::toAssignContextVariable()`.
fn to_assign_context_variable(expr: &Node) -> Result<Node, Error> {
    if !expr.is(NodeClass::ContextVariable) {
        return Err(Error::syntax(
            "A list must only contain variables.",
            expr.line(),
            expr.source_context(),
        ));
    }
    if expr.is(NodeClass::AssignContextVariable) {
        return Ok(expr.clone());
    }
    expression::assign_context_variable(&expr.attr_bytes("name").unwrap_or_default(), expr.line())
}

/// `LiteralExpressionParser::parse()`.
pub fn parse_literal(parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
    let line = line_of(token);
    match token.token_type() {
        TokenType::Name => {
            parser.stream_mut().next()?;
            let value = token.value_bytes();
            return Ok(match value.as_slice() {
                b"true" | b"TRUE" => expression::constant(true, line),
                b"false" | b"FALSE" => expression::constant(false, line),
                b"none" | b"NONE" | b"null" | b"NULL" => expression::constant(Attr::Null, line),
                _ => expression::context_variable(&value, line),
            });
        }
        TokenType::Number => {
            parser.stream_mut().next()?;
            return Ok(constant_of(token));
        }
        TokenType::String | TokenType::InterpolationStart => {
            return parse_string_expression(parser);
        }
        TokenType::Punctuation if token.value_bytes() == b"{" => {
            return parse_mapping_expression(parser);
        }
        TokenType::Punctuation | TokenType::Operator => {
            // the punctuation case falls through to the operator case
            let value = token.value_bytes();
            if value == b"[" {
                return parse_sequence_expression(parser);
            }
            if !value.is_empty() && name_match_len(&value) == value.len() {
                parser.stream_mut().next()?;
                return Ok(expression::context_variable(&value, line));
            }
        }
        _ => {}
    }
    let mut message = b"Unexpected token \"".to_vec();
    message.extend_from_slice(token.to_english().as_bytes());
    message.extend_from_slice(b"\" of value \"");
    message.extend_from_slice(&token.value_bytes());
    message.extend_from_slice(b"\".");
    Err(Error::syntax(
        message,
        line,
        Some(parser.stream().source_context().clone()),
    ))
}

/// `LiteralExpressionParser::parseStringExpression()`.
fn parse_string_expression(parser: &mut Parser<'_>) -> Result<Node, Error> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut next_can_be_string = true;
    loop {
        if next_can_be_string && let Some(token) = parser.stream_mut().next_if(TokenType::String)? {
            nodes.push(constant_of(&token));
            next_can_be_string = false;
            continue;
        }
        if parser
            .stream_mut()
            .next_if(TokenType::InterpolationStart)?
            .is_some()
        {
            nodes.push(parser.parse_expression(0)?);
            parser.stream_mut().expect(TokenType::InterpolationEnd)?;
            next_can_be_string = true;
        } else {
            break;
        }
    }
    let mut nodes = nodes.into_iter();
    let Some(mut expr) = nodes.next() else {
        return Err(Error::logic("A string expression has no parts."));
    };
    for node in nodes {
        let line = node.line();
        expr = expression::binary(NodeClass::ConcatBinary, expr, node, line)?;
    }
    Ok(expr)
}

/// `LiteralExpressionParser::parseSequenceExpression()`.
fn parse_sequence_expression(parser: &mut Parser<'_>) -> Result<Node, Error> {
    parser.stream_mut().expect_with(
        TokenType::Operator,
        Some(b"["),
        Some(b"A sequence element was expected"),
    )?;
    let node = expression::array(Vec::new(), line_of(parser.current_token()));
    let mut first = true;
    while !parser.stream().test_value(TokenType::Punctuation, b"]") {
        if !first {
            parser.stream_mut().expect_with(
                TokenType::Punctuation,
                Some(b","),
                Some(b"A sequence element must be followed by a comma"),
            )?;
            if parser.stream().test_value(TokenType::Punctuation, b"]") {
                break;
            }
        }
        first = false;
        if parser.stream().test_value(TokenType::Punctuation, b",") {
            let line = line_of(parser.current_token());
            node.add_element(expression::empty_expression(line), None);
        } else {
            let value = parser.parse_expression(0)?;
            node.add_element(value, None);
        }
    }
    parser.stream_mut().expect_with(
        TokenType::Punctuation,
        Some(b"]"),
        Some(b"An opened sequence is not properly closed"),
    )?;
    Ok(node)
}

/// `LiteralExpressionParser::parseMappingExpression()`.
fn parse_mapping_expression(parser: &mut Parser<'_>) -> Result<Node, Error> {
    parser.stream_mut().expect_with(
        TokenType::Punctuation,
        Some(b"{"),
        Some(b"A mapping element was expected"),
    )?;
    let node = expression::array(Vec::new(), line_of(parser.current_token()));
    let mut first = true;
    while !parser.stream().test_value(TokenType::Punctuation, b"}") {
        if !first {
            parser.stream_mut().expect_with(
                TokenType::Punctuation,
                Some(b","),
                Some(b"A mapping value must be followed by a comma"),
            )?;
            if parser.stream().test_value(TokenType::Punctuation, b"}") {
                break;
            }
        }
        first = false;
        if parser.stream().test_value(TokenType::Operator, b"...") {
            let value = parser.parse_expression(0)?;
            node.add_element(value, None);
            continue;
        }
        let key = if let Some(token) = parser.stream_mut().next_if(TokenType::Name)? {
            let key = constant_of(&token);
            if parser
                .stream()
                .test_values(TokenType::Punctuation, &[b",", b"}"])
            {
                let name = key
                    .attr("value")
                    .and_then(|v| v.to_php_string())
                    .unwrap_or_default();
                let value = expression::context_variable(&name, key.line());
                node.add_element(value, Some(key));
                continue;
            }
            key
        } else if let Some(token) = match parser.stream_mut().next_if(TokenType::String)? {
            Some(t) => Some(t),
            None => parser.stream_mut().next_if(TokenType::Number)?,
        } {
            constant_of(&token)
        } else if parser.stream().test_value(TokenType::Operator, b"(") {
            parser.parse_expression(0)?
        } else {
            let current = parser.current_token().clone();
            let mut message = b"A mapping key must be a quoted string, a number, a name, or an expression enclosed in parentheses (unexpected token \"".to_vec();
            message.extend_from_slice(current.to_english().as_bytes());
            message.extend_from_slice(b"\" of value \"");
            message.extend_from_slice(&current.value_bytes());
            message.extend_from_slice(b"\".");
            return Err(Error::syntax(
                message,
                line_of(&current),
                Some(parser.stream().source_context().clone()),
            ));
        };
        parser.stream_mut().expect_with(
            TokenType::Punctuation,
            Some(b":"),
            Some(b"A mapping key must be followed by a colon (:)"),
        )?;
        let value = parser.parse_expression(0)?;
        node.add_element(value, Some(key));
    }
    parser.stream_mut().expect_with(
        TokenType::Punctuation,
        Some(b"}"),
        Some(b"An opened mapping is not properly closed"),
    )?;
    Ok(node)
}
