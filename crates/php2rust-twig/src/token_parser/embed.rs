//! Port of Twig\TokenParser\EmbedTokenParser.php: `{% embed 'base.twig' %}...{% endembed %}`.
//!
//! Upstream: twig/twig src/TokenParser/EmbedTokenParser.php

use super::{TokenParser, include::parse_arguments, line_of};
use crate::{
    error::Error,
    node::{Node, NodeClass, statement},
    parser::{EndTest, Parser},
    token::{Token, TokenType},
};

/// `EmbedTokenParser`.
#[derive(Debug, Default)]
pub struct EmbedTokenParser;

fn decide_block_end(token: &Token) -> bool {
    token.test_name(b"endembed")
}

impl TokenParser for EmbedTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let line = token.line();
        let parent = parser.parse_expression(0)?;
        let (variables, only, ignore_missing) = parse_arguments(parser)?;
        let mut fake_parent = true;
        let parent_token = if parent.is(NodeClass::ConstantExpression) {
            fake_parent = false;
            let value = parent
                .get_attr("value")?
                .to_php_string()
                .unwrap_or_default();
            Token::new(TokenType::String, value, line)
        } else if parent.is(NodeClass::ContextVariable) {
            fake_parent = false;
            Token::new(
                TokenType::Name,
                parent.attr_bytes("name").unwrap_or_default(),
                line,
            )
        } else {
            Token::new(TokenType::String, "__parent__", line)
        };
        parser.stream_mut().inject_tokens(vec![
            Token::new(TokenType::BlockStart, "", line),
            Token::new(TokenType::Name, "extends", line),
            parent_token,
            Token::new(TokenType::BlockEnd, "", line),
        ]);
        let module = parser.parse_embedded(EndTest::new("embed", decide_block_end), true)?;
        if fake_parent {
            module.set_node("parent", parent)?;
        }
        parser.embed_template(module.clone());
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        let name = module.template_name().unwrap_or_default();
        let index = module
            .attr("index")
            .and_then(|a| a.as_int())
            .unwrap_or_default();
        Ok(statement::embed(
            name.as_bytes(),
            index,
            variables,
            only,
            ignore_missing,
            line_of(token),
        ))
    }

    fn tag(&self) -> &'static str {
        "embed"
    }
}
