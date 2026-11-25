//! Port of Twig\TokenParser\IncludeTokenParser.php: `{% include 'a.twig' ignore missing with vars only %}`.
//!
//! Upstream: twig/twig src/TokenParser/IncludeTokenParser.php

use super::{TokenParser, line_of};
use crate::{
    error::Error,
    node::{Node, statement},
    parser::Parser,
    token::{Token, TokenType},
};

/// `IncludeTokenParser`.
#[derive(Debug, Default)]
pub struct IncludeTokenParser;

/// `parseArguments()`: the variables, `only` and `ignore missing`.
pub(crate) fn parse_arguments(
    parser: &mut Parser<'_>,
) -> Result<(Option<Node>, bool, bool), Error> {
    let mut ignore_missing = false;
    if parser
        .stream_mut()
        .next_if_value(TokenType::Name, b"ignore")?
        .is_some()
    {
        parser
            .stream_mut()
            .expect_value(TokenType::Name, b"missing")?;
        ignore_missing = true;
    }
    let mut variables = None;
    if parser
        .stream_mut()
        .next_if_value(TokenType::Name, b"with")?
        .is_some()
    {
        variables = Some(parser.parse_expression(0)?);
    }
    let mut only = false;
    if parser
        .stream_mut()
        .next_if_value(TokenType::Name, b"only")?
        .is_some()
    {
        only = true;
    }
    parser.stream_mut().expect(TokenType::BlockEnd)?;
    Ok((variables, only, ignore_missing))
}

impl TokenParser for IncludeTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let expr = parser.parse_expression(0)?;
        let (variables, only, ignore_missing) = parse_arguments(parser)?;
        Ok(statement::include(
            expr,
            variables,
            only,
            ignore_missing,
            line_of(token),
        ))
    }

    fn tag(&self) -> &'static str {
        "include"
    }
}
