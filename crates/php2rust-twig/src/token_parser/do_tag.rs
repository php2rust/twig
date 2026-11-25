//! Port of Twig\TokenParser\DoTokenParser.php: `{% do expr %}`.
//!
//! Upstream: twig/twig src/TokenParser/DoTokenParser.php

use super::{TokenParser, line_of};
use crate::{
    error::Error,
    node::{Node, statement},
    parser::Parser,
    token::{Token, TokenType},
};

/// `DoTokenParser`.
#[derive(Debug, Default)]
pub struct DoTokenParser;

impl TokenParser for DoTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let expr = parser.parse_expression(0)?;
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        Ok(statement::do_node(expr, line_of(token)))
    }

    fn tag(&self) -> &'static str {
        "do"
    }
}
