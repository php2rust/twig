//! Port of Twig\TokenParser\UseTokenParser.php: `{% use 'blocks.twig' with a as b %}`.
//!
//! Upstream: twig/twig src/TokenParser/UseTokenParser.php

use super::{TokenParser, error_here, line_of};
use crate::{
    error::Error,
    node::{Attr, Key, Node, NodeClass, expression},
    parser::Parser,
    token::{Token, TokenType},
};

/// `UseTokenParser`.
#[derive(Debug, Default)]
pub struct UseTokenParser;

impl TokenParser for UseTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let template = parser.parse_expression(0)?;
        if !template.is(NodeClass::ConstantExpression) {
            return Err(error_here(
                parser,
                "The template references in a \"use\" statement must be a string.",
            ));
        }
        let mut targets: Vec<(Key, Node)> = Vec::new();
        if parser
            .stream_mut()
            .next_if_value(TokenType::Name, b"with")?
            .is_some()
        {
            loop {
                let name = parser.stream_mut().expect(TokenType::Name)?.value_bytes();
                let mut alias = name.clone();
                if parser
                    .stream_mut()
                    .next_if_value(TokenType::Name, b"as")?
                    .is_some()
                {
                    alias = parser.stream_mut().expect(TokenType::Name)?.value_bytes();
                }
                let key = Key::from_bytes(&name);
                let value = expression::constant(Attr::Str(alias), -1);
                if let Some(slot) = targets.iter_mut().find(|(k, _)| *k == key) {
                    slot.1 = value;
                } else {
                    targets.push((key, value));
                }
                if parser
                    .stream_mut()
                    .next_if_value(TokenType::Punctuation, b",")?
                    .is_none()
                {
                    break;
                }
            }
        }
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        parser.add_trait(Node::nodes(
            vec![
                (Key::Str(b"template".to_vec()), template),
                (Key::Str(b"targets".to_vec()), Node::nodes(targets, 0)),
            ],
            0,
        ));
        Ok(Node::new(
            NodeClass::ConfigNode,
            Vec::new(),
            Vec::new(),
            line_of(token),
        ))
    }

    fn tag(&self) -> &'static str {
        "use"
    }
}
