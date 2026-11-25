//! Port of the parse-time part of Twig\Environment.php: the extensions,
//! the lexer, `tokenize()` and `parse()`.
//!
//! Loading, caching, compiling and rendering come with the emitter and
//! the runtime.
//!
//! Upstream: twig/twig src/Environment.php

use std::rc::Rc;

use crate::{
    callable::TwigCallable,
    error::Error,
    expression_parser::ExpressionParsers,
    extension::{Extension, ExtensionSet, core::CoreExtension, escaper::EscaperExtension},
    lexer::{Lexer, LexerOptions},
    node::Node,
    parser::Parser,
    source::Source,
    token_parser::TokenParser,
    token_stream::TokenStream,
};

/// `Environment::VERSION`.
pub const VERSION: &str = "3.30.0";

/// `Environment`, for parsing.
pub struct Environment {
    extension_set: ExtensionSet,
    lexer: Lexer,
}

impl Default for Environment {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Environment {
    /// `new Environment($loader)`: the core and escaper extensions, then
    /// these ones (`addExtension()`), with the default lexer options.
    pub fn new(extensions: Vec<Rc<dyn Extension>>) -> Self {
        Self::with_lexer_options(extensions, LexerOptions::default())
    }

    /// As [`Environment::new`], with `setLexer(new Lexer($env, $options))`.
    pub fn with_lexer_options(extensions: Vec<Rc<dyn Extension>>, options: LexerOptions) -> Self {
        let mut all: Vec<Rc<dyn Extension>> =
            vec![Rc::new(CoreExtension), Rc::new(EscaperExtension)];
        all.extend(extensions);
        let extension_set = ExtensionSet::new(all);
        let operators = extension_set.expression_parsers().operator_tokens();
        let lexer = Lexer::with_operators(operators.iter().map(Vec::as_slice), options);
        Self {
            extension_set,
            lexer,
        }
    }

    /// `getExtensionSet()`.
    pub fn extension_set(&self) -> &ExtensionSet {
        &self.extension_set
    }

    /// `getLexer()`.
    pub fn lexer(&self) -> &Lexer {
        &self.lexer
    }

    /// `tokenize($source)`.
    pub fn tokenize(&self, source: Source) -> Result<TokenStream, Error> {
        self.lexer.tokenize(source)
    }

    /// `parse($stream)`.
    pub fn parse(&self, stream: TokenStream) -> Result<Node, Error> {
        Parser::new(self).parse(stream)
    }

    /// `parse(tokenize($source))`.
    pub fn parse_source(&self, source: Source) -> Result<Node, Error> {
        self.parse(self.tokenize(source)?)
    }

    /// `getTokenParser()`.
    pub fn token_parser(&self, tag: &[u8]) -> Option<Rc<dyn TokenParser>> {
        self.extension_set.token_parser(tag)
    }

    /// `array_keys(getTokenParsers())`.
    pub fn token_parser_tags(&self) -> Vec<Vec<u8>> {
        self.extension_set.token_parser_tags()
    }

    /// `getExpressionParsers()`.
    pub fn expression_parsers(&self) -> &ExpressionParsers {
        self.extension_set.expression_parsers()
    }

    /// `getFilter()`.
    pub fn filter(&self, name: &[u8]) -> Option<Rc<TwigCallable>> {
        self.extension_set.filter(name)
    }

    /// `getFunction()`.
    pub fn function(&self, name: &[u8]) -> Option<Rc<TwigCallable>> {
        self.extension_set.function(name)
    }

    /// `getTest()`.
    pub fn test(&self, name: &[u8]) -> Option<Rc<TwigCallable>> {
        self.extension_set.test(name)
    }

    /// `array_keys(getFilters())`.
    pub fn filter_names(&self) -> Vec<Vec<u8>> {
        self.extension_set.filters.names()
    }

    /// `array_keys(getFunctions())`.
    pub fn function_names(&self) -> Vec<Vec<u8>> {
        self.extension_set.functions.names()
    }

    /// `array_keys(getTests())`.
    pub fn test_names(&self) -> Vec<Vec<u8>> {
        self.extension_set.tests.names()
    }
}
