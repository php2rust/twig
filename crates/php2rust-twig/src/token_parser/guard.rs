//! Port of Twig\TokenParser\GuardTokenParser.php: `{% guard function name %}...{% else %}...{% endguard %}`.
//!
//! Upstream: twig/twig src/TokenParser/GuardTokenParser.php

use super::TokenParser;
use crate::{
    error::Error,
    node::Node,
    parser::{EndTest, Parser},
    token::{Token, TokenType},
    token_stream::to_line,
};

/// `GuardTokenParser`.
#[derive(Debug, Default)]
pub struct GuardTokenParser;

fn decide_guard_fork(token: &Token) -> bool {
    token.test_names(&[b"else", b"endguard"])
}

fn decide_guard_end(token: &Token) -> bool {
    token.test_names(&[b"endguard"])
}

impl TokenParser for GuardTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, _token: &Token) -> Result<Node, Error> {
        let type_token = parser.stream_mut().expect(TokenType::Name)?;
        let kind = type_token.value_bytes();
        if !matches!(kind.as_slice(), b"function" | b"filter" | b"test") {
            let mut message = b"Supported guard types are function, filter and test, \"".to_vec();
            message.extend_from_slice(&kind);
            message.extend_from_slice(b"\" given.");
            return Err(Error::syntax(
                message,
                to_line(type_token.line()),
                Some(parser.stream().source_context().clone()),
            ));
        }
        let mut name = parser.stream_mut().expect(TokenType::Name)?.value_bytes();
        if kind == b"test" && parser.stream().test(TokenType::Name) {
            name.push(b' ');
            name.extend_from_slice(&parser.current_token().value_bytes());
            parser.stream_mut().next()?;
        }
        let env = parser.environment();
        let exists = match kind.as_slice() {
            b"function" => env.function(&name).is_some(),
            b"filter" => env.filter(&name).is_some(),
            _ => env.test(&name).is_some(),
        };
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        let fork = EndTest::new("guard", decide_guard_fork);
        let body = if exists {
            parser.subparse(Some(fork), false)?
        } else {
            parser.subparse_ignore_unknown_twig_callables(Some(fork), false)?;
            Node::empty(0)
        };
        let mut else_node = Node::empty(0);
        if parser.stream_mut().next()?.value_bytes() == b"else" {
            parser.stream_mut().expect(TokenType::BlockEnd)?;
            else_node = parser.subparse(Some(EndTest::new("guard", decide_guard_end)), true)?;
        }
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        Ok(Node::list(vec![if exists { body } else { else_node }], 0))
    }

    fn tag(&self) -> &'static str {
        "guard"
    }
}
