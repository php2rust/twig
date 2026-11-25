//! Port of Twig\TokenParser\TokenParserInterface.php and
//! AbstractTokenParser.php, and the core tag parsers in the submodules.
//!
//! Upstream: twig/twig src/TokenParser/TokenParserInterface.php src/TokenParser/AbstractTokenParser.php

pub mod apply;
pub mod auto_escape;
pub mod block;
pub mod deprecated;
pub mod do_tag;
pub mod embed;
pub mod extends;
pub mod flush;
pub mod for_tag;
pub mod from;
pub mod guard;
pub mod if_tag;
pub mod import;
pub mod include;
pub mod macro_tag;
pub mod sandbox;
pub mod set;
pub mod types;
pub mod use_tag;
pub mod with;

use std::rc::Rc;

use crate::{
    error::Error,
    lexer::name_match_len,
    node::{Node, expression},
    parser::Parser,
    token::{Token, TokenType},
    token_stream::to_line,
};

/// `TokenParserInterface`: parses one tag. `setParser()` is the `parser`
/// argument of [`TokenParser::parse`].
pub trait TokenParser {
    /// `parse($token)`: the tag name token is consumed.
    fn parse(&self, parser: &mut Parser<'_>, token: &Token) -> Result<Node, Error>;

    /// `getTag()`.
    fn tag(&self) -> &str;

    /// `isAlwaysAllowedInSandbox()`.
    fn is_always_allowed_in_sandbox(&self) -> bool {
        false
    }
}

/// The core tags of `CoreExtension::getTokenParsers()`, in order.
pub fn core_token_parsers() -> Vec<Rc<dyn TokenParser>> {
    vec![
        Rc::new(apply::ApplyTokenParser),
        Rc::new(for_tag::ForTokenParser),
        Rc::new(if_tag::IfTokenParser),
        Rc::new(extends::ExtendsTokenParser),
        Rc::new(include::IncludeTokenParser),
        Rc::new(block::BlockTokenParser),
        Rc::new(use_tag::UseTokenParser),
        Rc::new(macro_tag::MacroTokenParser),
        Rc::new(import::ImportTokenParser),
        Rc::new(from::FromTokenParser),
        Rc::new(set::SetTokenParser),
        Rc::new(types::TypesTokenParser),
        Rc::new(flush::FlushTokenParser),
        Rc::new(do_tag::DoTokenParser),
        Rc::new(embed::EmbedTokenParser),
        Rc::new(with::WithTokenParser),
        Rc::new(deprecated::DeprecatedTokenParser),
        Rc::new(guard::GuardTokenParser),
    ]
}

pub(crate) fn line_of(token: &Token) -> i64 {
    to_line(token.line())
}

/// A syntax error at the current token, in the stream's template.
pub(crate) fn error_here(parser: &Parser<'_>, message: impl Into<Vec<u8>>) -> Error {
    Error::syntax(
        message,
        line_of(parser.current_token()),
        Some(parser.stream().source_context().clone()),
    )
}

/// `AbstractTokenParser::parseAssignmentExpression()`.
pub(crate) fn parse_assignment_expression(parser: &mut Parser<'_>) -> Result<Node, Error> {
    let mut targets = Vec::new();
    loop {
        let token = parser.current_token().clone();
        if parser.stream().test(TokenType::Operator) && name_match_len(&token.value_bytes()) > 0 {
            // in this context, string operators are variable names
            parser.stream_mut().next()?;
        } else {
            parser.stream_mut().expect_with(
                TokenType::Name,
                None,
                Some(b"Only variables can be assigned to"),
            )?;
        }
        let target = expression::assign_context_variable(&token.value_bytes(), line_of(&token))?;
        target.add_documentation(&[&token]);
        targets.push(target);
        if parser
            .stream_mut()
            .next_if_value(TokenType::Punctuation, b",")?
            .is_none()
        {
            break;
        }
    }
    Ok(Node::list(targets, 0))
}
