//! Port of Twig\TokenParser\ApplyTokenParser.php: `{% apply upper %}...{% endapply %}`.
//!
//! Upstream: twig/twig src/TokenParser/ApplyTokenParser.php

use super::{TokenParser, line_of};
use crate::{
    error::Error,
    expression_parser::infix::parse_filter,
    node::{Attr, Node, expression, statement},
    parser::{EndTest, Parser},
    token::{Token, TokenType},
};

/// `ApplyTokenParser`.
#[derive(Debug, Default)]
pub struct ApplyTokenParser;

fn decide_apply_end(token: &Token) -> bool {
    token.test_name(b"endapply")
}

impl TokenParser for ApplyTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let lineno = line_of(token);
        let reference = expression::local_variable(Attr::Null, lineno)?;
        let mut filter = reference.clone();
        loop {
            filter = parse_filter(parser, filter)?;
            if !parser.stream().test_value(TokenType::Operator, b"|") {
                break;
            }
            parser.stream_mut().next()?;
        }
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        let body = parser.subparse(Some(EndTest::new("apply", decide_apply_end)), true)?;
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        Ok(Node::list(
            vec![
                statement::set(true, reference, body, lineno)?,
                statement::print(filter, lineno),
            ],
            lineno,
        ))
    }

    fn tag(&self) -> &'static str {
        "apply"
    }
}
