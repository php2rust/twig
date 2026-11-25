//! Port of Twig\TokenParser\BlockTokenParser.php: `{% block name %}...{% endblock %}`.
//!
//! Upstream: twig/twig src/TokenParser/BlockTokenParser.php

use super::{TokenParser, error_here, line_of};
use crate::{
    error::Error,
    node::{Node, statement},
    parser::{EndTest, Parser},
    token::{Token, TokenType},
};

/// `BlockTokenParser`.
#[derive(Debug, Default)]
pub struct BlockTokenParser;

fn decide_block_end(token: &Token) -> bool {
    token.test_name(b"endblock")
}

impl TokenParser for BlockTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let lineno = line_of(token);
        let name = parser.stream_mut().expect(TokenType::Name)?.value_bytes();
        let block = statement::block(&name, Node::empty(0), lineno);
        parser.set_block(&name, &block)?;
        parser.set_documentation_target(&block)?;
        parser.push_local_scope();
        parser.push_block_stack(&name);
        let body = if parser.stream_mut().next_if(TokenType::BlockEnd)?.is_some() {
            let body = parser.subparse(Some(EndTest::new("block", decide_block_end)), true)?;
            if let Some(end) = parser.stream_mut().next_if(TokenType::Name)? {
                let value = end.value_bytes();
                if value != name {
                    let mut message = b"Expected endblock for block \"".to_vec();
                    message.extend_from_slice(&name);
                    message.extend_from_slice(b"\" (but \"");
                    message.extend_from_slice(&value);
                    message.extend_from_slice(b"\" given).");
                    return Err(error_here(parser, message));
                }
            }
            body
        } else {
            let expr = parser.parse_expression(0)?;
            Node::list(vec![statement::print(expr, lineno)], 0)
        };
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        block.set_node("body", body)?;
        parser.pop_block_stack();
        parser.pop_local_scope();
        Ok(statement::block_reference(&name, lineno))
    }

    fn tag(&self) -> &'static str {
        "block"
    }
}
