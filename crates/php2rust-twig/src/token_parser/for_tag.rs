//! Port of Twig\TokenParser\ForTokenParser.php: `{% for k, v in seq %}...{% else %}...{% endfor %}`.
//!
//! Upstream: twig/twig src/TokenParser/ForTokenParser.php

use super::{TokenParser, line_of, parse_assignment_expression};
use crate::{
    error::Error,
    node::{Node, expression, statement},
    parser::{EndTest, Parser},
    token::{Token, TokenType},
};

/// `ForTokenParser`.
#[derive(Debug, Default)]
pub struct ForTokenParser;

fn decide_for_fork(token: &Token) -> bool {
    token.test_names(&[b"else", b"endfor"])
}

fn decide_for_end(token: &Token) -> bool {
    token.test_name(b"endfor")
}

impl TokenParser for ForTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let lineno = line_of(token);
        let targets = parse_assignment_expression(parser)?;
        parser
            .stream_mut()
            .expect_value(TokenType::Operator, b"in")?;
        let seq = parser.parse_expression(0)?;
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        let body = parser.subparse(Some(EndTest::new("for", decide_for_fork)), false)?;
        let else_node = if parser.stream_mut().next()?.value_bytes() == b"else" {
            let else_lineno = line_of(parser.current_token());
            parser.stream_mut().expect(TokenType::BlockEnd)?;
            let body = parser.subparse(Some(EndTest::new("for", decide_for_end)), true)?;
            Some(statement::for_else(body, else_lineno))
        } else {
            None
        };
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        let (key_target, value_target) = if targets.count() > 1 {
            (targets.get_node(0i64)?, targets.get_node(1i64)?)
        } else {
            (
                expression::assign_context_variable(b"_key", lineno)?,
                targets.get_node(0i64)?,
            )
        };
        Ok(statement::for_node(
            key_target,
            value_target,
            seq,
            body,
            else_node,
            lineno,
        ))
    }

    fn tag(&self) -> &'static str {
        "for"
    }
}
