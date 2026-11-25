//! Port of Twig\TokenParser\DeprecatedTokenParser.php: `{% deprecated 'message' %}`.
//!
//! Upstream: twig/twig src/TokenParser/DeprecatedTokenParser.php

use super::{TokenParser, error_here, line_of};
use crate::{
    error::Error,
    node::{Node, statement},
    parser::Parser,
    token::{Token, TokenType},
};

/// `DeprecatedTokenParser`.
#[derive(Debug, Default)]
pub struct DeprecatedTokenParser;

impl TokenParser for DeprecatedTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let expr = parser.parse_expression(0)?;
        let node = statement::deprecated(expr, line_of(token));
        while parser.stream().test(TokenType::Name) {
            let k = parser.current_token().value_bytes();
            parser.stream_mut().next()?;
            parser
                .stream_mut()
                .expect_value(TokenType::Operator, b"=")?;
            match k.as_slice() {
                b"package" | b"version" => {
                    let value = parser.parse_expression(0)?;
                    node.set_node(k.as_slice(), value)?;
                }
                _ => {
                    let mut message = b"Unknown \"".to_vec();
                    message.extend_from_slice(&k);
                    message.extend_from_slice(b"\" option.");
                    return Err(error_here(parser, message));
                }
            }
        }
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        Ok(node)
    }

    fn tag(&self) -> &'static str {
        "deprecated"
    }
}
