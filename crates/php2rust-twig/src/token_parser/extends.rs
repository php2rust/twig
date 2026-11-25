//! Port of Twig\TokenParser\ExtendsTokenParser.php: `{% extends 'base.twig' %}`.
//!
//! Upstream: twig/twig src/TokenParser/ExtendsTokenParser.php

use super::{TokenParser, line_of};
use crate::{
    error::Error,
    node::{Node, NodeClass},
    parser::Parser,
    token::{Token, TokenType},
};

/// `ExtendsTokenParser`.
#[derive(Debug, Default)]
pub struct ExtendsTokenParser;

impl TokenParser for ExtendsTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let parent = parser.parse_expression(0)?;
        parser.set_parent(parent, false)?;
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        Ok(Node::new(
            NodeClass::ConfigNode,
            Vec::new(),
            Vec::new(),
            line_of(token),
        ))
    }

    fn tag(&self) -> &'static str {
        "extends"
    }
}
