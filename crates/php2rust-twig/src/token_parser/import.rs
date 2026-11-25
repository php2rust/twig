//! Port of Twig\TokenParser\ImportTokenParser.php: `{% import 'forms.twig' as forms %}`.
//!
//! Upstream: twig/twig src/TokenParser/ImportTokenParser.php

use super::{TokenParser, line_of};
use crate::{
    error::Error,
    node::{Attr, Node, expression, statement},
    parser::Parser,
    token::{Token, TokenType},
};

/// `ImportTokenParser`.
#[derive(Debug, Default)]
pub struct ImportTokenParser;

impl TokenParser for ImportTokenParser {
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error> {
        let line = line_of(token);
        let macro_expr = parser.parse_expression(0)?;
        parser.stream_mut().expect_value(TokenType::Name, b"as")?;
        let name = parser.stream_mut().expect(TokenType::Name)?.value_bytes();
        let var = expression::assign_macro_variable(
            expression::macro_variable(Attr::Str(name.clone()), line)?,
            parser.is_main_scope(),
        );
        parser.stream_mut().expect(TokenType::BlockEnd)?;
        parser.add_imported_symbol("template", &name, None, None);
        Ok(statement::import(macro_expr, var, line))
    }

    fn tag(&self) -> &'static str {
        "import"
    }
}
