//! Port of Twig\TokenParser\SetTokenParser.php: `{% set a, b = 1, 2 %}` and `{% set a %}...{% endset %}`.
//!
//! Upstream: twig/twig src/TokenParser/SetTokenParser.php

use super::{TokenParser, error_here, line_of, parse_assignment_expression};
use crate::{
    error::Error,
    node::{Node, statement},
    parser::{EndTest, Parser},
    token::{Token, TokenType},
};

/// `SetTokenParser`.
#[derive(Debug, Default)]
pub struct SetTokenParser;

fn decide_block_end(token: &Token) -> bool {
    token.test_name(b"endset")
}

impl TokenParser for SetTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let lineno = line_of(token);
        let names = parse_assignment_expression(parser)?;
        let mut capture = false;
        let values = if parser
            .stream_mut()
            .next_if_value(TokenType::Operator, b"=")?
            .is_some()
        {
            let values = parse_multitarget_expression(parser)?;
            parser.stream_mut().expect(TokenType::BlockEnd)?;
            if names.count() != values.count() {
                return Err(error_here(
                    parser,
                    "When using set, you must have the same number of variables and assignments.",
                ));
            }
            values
        } else {
            capture = true;
            if names.count() > 1 {
                return Err(error_here(
                    parser,
                    "When using set with a block, you cannot have a multi-target.",
                ));
            }
            parser.stream_mut().expect(TokenType::BlockEnd)?;
            let values = parser.subparse(Some(EndTest::new("set", decide_block_end)), true)?;
            parser.stream_mut().expect(TokenType::BlockEnd)?;
            values
        };
        statement::set(capture, names, values, lineno)
    }

    fn tag(&self) -> &'static str {
        "set"
    }
}

/// `parseMultitargetExpression()`.
fn parse_multitarget_expression(parser: &mut Parser<'_>) -> Result<Node, Error> {
    let mut targets = Vec::new();
    loop {
        targets.push(parser.parse_expression(0)?);
        if parser
            .stream_mut()
            .next_if_value(TokenType::Punctuation, b",")?
            .is_none()
        {
            break;
        }
    }
    Ok(Node::list(targets, 0))
}
