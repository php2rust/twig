//! Port of Twig\TokenParser\FlushTokenParser.php: `{% flush %}`.
//!
//! Upstream: twig/twig src/TokenParser/FlushTokenParser.php

use super::{TokenParser, line_of};
use crate::{
    error::Error,
    node::{Node, statement},
    parser::Parser,
    token::{Token, TokenType},
};

/// `FlushTokenParser`.
#[derive(Debug, Default)]
pub struct FlushTokenParser;

impl TokenParser for FlushTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        Ok(statement::flush(line_of(token)))
    }

    fn tag(&self) -> &'static str {
        "flush"
    }
}
