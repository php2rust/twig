//! Port of Twig\TokenParser\MacroTokenParser.php: `{% macro name(a, b = 1, ...rest) %}...{% endmacro %}`.
//!
//! Upstream: twig/twig src/TokenParser/MacroTokenParser.php

use super::{TokenParser, error_here, line_of};
use crate::{
    error::Error,
    node::{
        Attr, Node, NodeClass,
        expression::{self, RESERVED_NAME_PREFIX},
        statement,
    },
    parser::{EndTest, Parser},
    token::{Token, TokenType},
    token_stream::to_line,
};

/// `MacroTokenParser`.
#[derive(Debug, Default)]
pub struct MacroTokenParser;

fn decide_block_end(token: &Token) -> bool {
    token.test_name(b"endmacro")
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

impl TokenParser for MacroTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let lineno = line_of(token);
        let name = parser.stream_mut().expect(TokenType::Name)?.value_bytes();
        let (arguments, variadic_name) = parse_definition(parser, &name)?;
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        parser.push_local_scope();
        let body = parser.subparse(Some(EndTest::new("macro", decide_block_end)), true)?;
        if let Some(end) = parser.stream_mut().next_if(TokenType::Name)? {
            let value = end.value_bytes();
            if value != name {
                return Err(error_here(
                    parser,
                    format!(
                        "Expected endmacro for macro \"{}\" (but \"{}\" given).",
                        lossy(&name),
                        lossy(&value)
                    ),
                ));
            }
        }
        parser.pop_local_scope();
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        let macro_node = statement::macro_node(
            &name,
            statement::body(vec![body], 0),
            arguments,
            lineno,
            variadic_name.as_deref(),
        )?;
        parser.set_macro(&name, macro_node.clone());
        parser.set_documentation_target(&macro_node)?;
        Ok(statement::macro_declaration(&name, lineno))
    }

    fn tag(&self) -> &'static str {
        "macro"
    }
}

/// `parseDefinition()`: the arguments and the variadic name.
fn parse_definition(
    parser: &mut Parser<'_>,
    macro_name: &[u8],
) -> Result<(Node, Option<Vec<u8>>), Error> {
    let arguments = expression::array(Vec::new(), line_of(parser.current_token()));
    let mut variadic_name = None;
    parser.stream_mut().expect_with(
        TokenType::Operator,
        Some(b"("),
        Some(b"A list of arguments must begin with an opening parenthesis"),
    )?;
    while !parser.stream().test_value(TokenType::Punctuation, b")") {
        if arguments.count() > 0 {
            parser.stream_mut().expect_with(
                TokenType::Punctuation,
                Some(b","),
                Some(b"Arguments must be separated by a comma"),
            )?;
            if parser.stream().test_value(TokenType::Punctuation, b")") {
                break;
            }
        }
        if parser
            .stream_mut()
            .next_if_value(TokenType::Operator, b"...")?
            .is_some()
        {
            let token = parser.stream_mut().expect_with(
                TokenType::Name,
                None,
                Some(b"A variadic argument must be a name"),
            )?;
            let local =
                expression::local_variable(Attr::Str(token.value_bytes()), line_of(&token))?;
            let mut name = local
                .attr("name")
                .and_then(|a| a.to_php_string())
                .unwrap_or_default();
            if let Some(stripped) = name.strip_prefix(RESERVED_NAME_PREFIX) {
                name = stripped.to_vec();
            }
            let source = Some(parser.stream().source_context().clone());
            if parser.stream().test_value(TokenType::Operator, b"=") {
                return Err(Error::syntax(
                    format!(
                        "The variadic argument \"{}\" in macro \"{}\" cannot have a default value.",
                        lossy(&name),
                        lossy(macro_name)
                    ),
                    to_line(token.line()),
                    source,
                ));
            }
            if parser
                .stream_mut()
                .next_if_value(TokenType::Punctuation, b",")?
                .is_some()
                && !parser.stream().test_value(TokenType::Punctuation, b")")
            {
                return Err(Error::syntax(
                    format!(
                        "The variadic argument \"{}\" in macro \"{}\" must be the last one.",
                        lossy(&name),
                        lossy(macro_name)
                    ),
                    to_line(token.line()),
                    source,
                ));
            }
            variadic_name = Some(name);
            break;
        }
        let token = parser.stream_mut().expect_with(
            TokenType::Name,
            None,
            Some(b"An argument must be a name"),
        )?;
        let name = expression::local_variable(Attr::Str(token.value_bytes()), line_of(&token))?;
        name.add_documentation(&[&token]);
        let (default, error_line) = if let Some(eq) = parser
            .stream_mut()
            .next_if_value(TokenType::Operator, b"=")?
        {
            (parser.parse_expression(0)?, line_of(&eq))
        } else {
            let default = expression::constant(Attr::Null, line_of(parser.current_token()));
            default.set_attr("is_implicit", true);
            (default, line_of(&token))
        };
        if !check_constant_expression(&default) {
            return Err(Error::syntax(
                "A default value for an argument must be a constant (a boolean, a string, a number, a sequence, or a mapping).",
                error_line,
                Some(parser.stream().source_context().clone()),
            ));
        }
        arguments.add_element(default, Some(name));
    }
    parser.stream_mut().expect_with(
        TokenType::Punctuation,
        Some(b")"),
        Some(b"A list of arguments must be closed by a parenthesis"),
    )?;
    Ok((arguments, variadic_name))
}

/// `checkConstantExpression()`.
fn check_constant_expression(node: &Node) -> bool {
    if !(node.is(NodeClass::ConstantExpression)
        || node.is(NodeClass::ArrayExpression)
        || node.is(NodeClass::NegUnary)
        || node.is(NodeClass::PosUnary))
    {
        return false;
    }
    node.child_nodes().iter().all(check_constant_expression)
}
