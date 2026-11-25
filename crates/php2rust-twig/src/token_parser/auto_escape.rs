//! Port of Twig\TokenParser\AutoEscapeTokenParser.php: `{% autoescape 'js' %}`.
//!
//! Upstream: twig/twig src/TokenParser/AutoEscapeTokenParser.php

use super::{TokenParser, error_here, line_of};
use crate::{
    error::Error,
    node::{Attr, Node, NodeClass, statement},
    parser::{EndTest, Parser},
    token::{Token, TokenType},
};

/// `AutoEscapeTokenParser`.
#[derive(Debug, Default)]
pub struct AutoEscapeTokenParser;

fn decide_block_end(token: &Token) -> bool {
    token.test_name(b"endautoescape")
}

impl TokenParser for AutoEscapeTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let lineno = line_of(token);
        let value = if parser.stream().test(TokenType::BlockEnd) {
            Attr::from("html")
        } else {
            let expr = parser.parse_expression(0)?;
            if !expr.is(NodeClass::ConstantExpression) {
                return Err(error_here(
                    parser,
                    "An escaping strategy must be a string or false.",
                ));
            }
            expr.get_attr("value")?
        };
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        let body = parser.subparse(Some(EndTest::new("autoescape", decide_block_end)), true)?;
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        Ok(statement::auto_escape(value, body, lineno))
    }

    fn tag(&self) -> &'static str {
        "autoescape"
    }
}
