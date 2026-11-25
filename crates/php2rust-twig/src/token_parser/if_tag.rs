//! Port of Twig\TokenParser\IfTokenParser.php: `{% if %}...{% elseif %}...{% else %}...{% endif %}`.
//!
//! Upstream: twig/twig src/TokenParser/IfTokenParser.php

use super::{TokenParser, error_here, line_of};
use crate::{
    error::Error,
    node::{Node, statement},
    parser::{EndTest, Parser},
    token::{Token, TokenType},
};

/// `IfTokenParser`.
#[derive(Debug, Default)]
pub struct IfTokenParser;

fn decide_if_fork(token: &Token) -> bool {
    token.test_names(&[b"elseif", b"else", b"endif"])
}

fn decide_if_end(token: &Token) -> bool {
    token.test_names(&[b"endif"])
}

impl TokenParser for IfTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let lineno = line_of(token);
        let expr = parser.parse_expression(0)?;
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        let body = parser.subparse(Some(EndTest::new("if", decide_if_fork)), false)?;
        let mut tests = vec![expr, body];
        let mut else_node = None;
        loop {
            match parser.stream_mut().next()?.value_bytes().as_slice() {
                b"else" => {
                    parser.stream_mut().expect(TokenType::BlockEnd)?;
                    else_node =
                        Some(parser.subparse(Some(EndTest::new("if", decide_if_end)), false)?);
                }
                b"elseif" => {
                    let expr = parser.parse_expression(0)?;
                    parser.stream_mut().expect(TokenType::BlockEnd)?;
                    let body = parser.subparse(Some(EndTest::new("if", decide_if_fork)), false)?;
                    tests.push(expr);
                    tests.push(body);
                }
                b"endif" => break,
                _ => {
                    return Err(error_here(
                        parser,
                        format!(
                            "Unexpected end of template. Twig was looking for the following tags \"else\", \"elseif\", or \"endif\" to close the \"if\" block started at line {lineno})."
                        ),
                    ));
                }
            }
        }
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        statement::if_node(Node::list(tests, 0), else_node, lineno)
    }

    fn tag(&self) -> &'static str {
        "if"
    }
}
