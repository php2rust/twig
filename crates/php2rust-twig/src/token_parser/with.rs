//! Port of Twig\TokenParser\WithTokenParser.php: `{% with vars only %}...{% endwith %}`.
//!
//! Upstream: twig/twig src/TokenParser/WithTokenParser.php

use super::{TokenParser, line_of};
use crate::{
    error::Error,
    node::{Node, statement},
    parser::{EndTest, Parser},
    token::{Token, TokenType},
};

/// `WithTokenParser`.
#[derive(Debug, Default)]
pub struct WithTokenParser;

fn decide_with_end(token: &Token) -> bool {
    token.test_name(b"endwith")
}

impl TokenParser for WithTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let mut variables = None;
        let mut only = false;
        if !parser.stream().test(TokenType::BlockEnd) {
            variables = Some(parser.parse_expression(0)?);
            only = parser
                .stream_mut()
                .next_if_value(TokenType::Name, b"only")?
                .is_some();
        }
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        let body = parser.subparse(Some(EndTest::new("with", decide_with_end)), true)?;
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        Ok(statement::with(body, variables, only, line_of(token)))
    }

    fn tag(&self) -> &'static str {
        "with"
    }
}
