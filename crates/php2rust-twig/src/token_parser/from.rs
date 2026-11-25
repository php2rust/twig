//! Port of Twig\TokenParser\FromTokenParser.php: `{% from 'forms.twig' import input as field %}`.
//!
//! Upstream: twig/twig src/TokenParser/FromTokenParser.php

use super::{TokenParser, line_of};
use crate::{
    error::Error,
    node::{Attr, Node, expression, statement},
    parser::Parser,
    token::{Token, TokenType},
};

/// `FromTokenParser`.
#[derive(Debug, Default)]
pub struct FromTokenParser;

impl TokenParser for FromTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let line = line_of(token);
        let macro_expr = parser.parse_expression(0)?;
        parser
            .stream_mut()
            .expect_value(TokenType::Name, b"import")?;
        let mut targets: Vec<(Vec<u8>, Node)> = Vec::new();
        loop {
            let name = parser.stream_mut().expect(TokenType::Name)?.value_bytes();
            let alias = if parser
                .stream_mut()
                .next_if_value(TokenType::Name, b"as")?
                .is_some()
            {
                let alias = parser.stream_mut().expect(TokenType::Name)?.value_bytes();
                expression::assign_context_variable(&alias, line)?
            } else {
                expression::assign_context_variable(&name, line)?
            };
            if let Some(slot) = targets.iter_mut().find(|(n, _)| *n == name) {
                slot.1 = alias;
            } else {
                targets.push((name, alias));
            }
            if parser
                .stream_mut()
                .next_if_value(TokenType::Punctuation, b",")?
                .is_none()
            {
                break;
            }
        }
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        let internal_ref = expression::assign_macro_variable(
            expression::macro_variable(Attr::Null, line)?,
            parser.is_main_scope(),
        );
        let node = statement::import(macro_expr, internal_ref.clone(), line);
        for (name, alias) in targets {
            let alias_name = alias.attr_bytes("name").unwrap_or_default();
            parser.add_imported_symbol(
                "function",
                &alias_name,
                Some(&name),
                Some(internal_ref.clone()),
            );
        }
        Ok(node)
    }

    fn tag(&self) -> &'static str {
        "from"
    }
}
