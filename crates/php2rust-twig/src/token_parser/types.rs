//! Port of Twig\TokenParser\TypesTokenParser.php: `{% types {name: 'string', age?: 'int'} %}`.
//!
//! Upstream: twig/twig src/TokenParser/TypesTokenParser.php

use super::{TokenParser, line_of};
use crate::{
    error::Error,
    node::{
        Attr, Key, Node, NodeClass,
        statement::{self, TypeEntry},
    },
    parser::Parser,
    token::{Token, TokenType},
};

/// `TypesTokenParser`.
#[derive(Debug, Default)]
pub struct TypesTokenParser;

impl TokenParser for TypesTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let (types, nodes) = parse_simple_mapping_expression(parser)?;
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        let node = statement::types(types, line_of(token));
        for (name, ty) in nodes {
            node.set_node(Key::from_bytes(&name), ty)?;
        }
        Ok(node)
    }

    fn tag(&self) -> &'static str {
        "types"
    }
}

fn at_end(parser: &Parser<'_>) -> bool {
    parser.stream().test_value(TokenType::Punctuation, b"}")
        || parser.stream().test(TokenType::BlockEnd)
}

/// `parseSimpleMappingExpression()`: the entries and their nodes.
#[allow(clippy::type_complexity)]
fn parse_simple_mapping_expression(
    parser: &mut Parser<'_>,
) -> Result<(Vec<TypeEntry>, Vec<(Vec<u8>, Node)>), Error> {
    let enclosed = parser
        .stream_mut()
        .next_if_value(TokenType::Punctuation, b"{")?
        .is_some();
    let mut types: Vec<TypeEntry> = Vec::new();
    let mut nodes: Vec<(Vec<u8>, Node)> = Vec::new();
    let mut first = true;
    while !at_end(parser) {
        if !first {
            parser.stream_mut().expect_with(
                TokenType::Punctuation,
                Some(b","),
                Some(b"A type string must be followed by a comma"),
            )?;
            if at_end(parser) {
                break;
            }
        }
        first = false;
        let name_token = parser.stream_mut().expect(TokenType::Name)?;
        let optional = if parser
            .stream_mut()
            .next_if_value(TokenType::Operator, b"?:")?
            .is_some()
        {
            true
        } else {
            let optional = parser
                .stream_mut()
                .next_if_value(TokenType::Operator, b"?")?
                .is_some();
            parser.stream_mut().expect_with(
                TokenType::Punctuation,
                Some(b":"),
                Some(b"A type name must be followed by a colon (:)"),
            )?;
            optional
        };
        let value_token = parser.stream_mut().expect(TokenType::String)?;
        let name = name_token.value_bytes();
        let type_name = value_token.value_bytes();
        let entry = TypeEntry {
            name: name.clone(),
            type_name: type_name.clone(),
            optional,
        };
        if let Some(slot) = types.iter_mut().find(|t| t.name == name) {
            *slot = entry;
        } else {
            types.push(entry);
        }
        let node = Node::new(
            NodeClass::TypeNode,
            Vec::new(),
            vec![
                ("name", Attr::Str(name.clone())),
                ("type", Attr::Str(type_name)),
                ("optional", Attr::Bool(optional)),
            ],
            line_of(&name_token),
        );
        node.add_documentation(&[&name_token]);
        if let Some(slot) = nodes.iter_mut().find(|(n, _)| *n == name) {
            slot.1 = node;
        } else {
            nodes.push((name, node));
        }
    }
    if enclosed {
        parser.stream_mut().expect_with(
            TokenType::Punctuation,
            Some(b"}"),
            Some(b"An opened mapping is not properly closed"),
        )?;
    }
    Ok((types, nodes))
}
