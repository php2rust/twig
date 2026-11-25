//! Port of the parse-time part of Twig\Environment.php: the options that
//! change parsing (`autoescape`, `optimizations`), the extensions, the
//! lexer, the node visitors, `tokenize()` and `parse()`.
//!
//! Loading, caching, compiling and rendering come with the emitter and
//! the runtime.
//!
//! Upstream: twig/twig src/Environment.php

use std::{cell::RefCell, rc::Rc};

use crate::{
    callable::TwigCallable,
    error::Error,
    expression_parser::ExpressionParsers,
    extension::{
        Extension, ExtensionSet, core::CoreExtension, escaper::EscaperExtension,
        optimizer::OptimizerExtension,
    },
    file_extension_escaping_strategy,
    lexer::{Lexer, LexerOptions},
    node::Node,
    node_visitor::{self, NodeVisitor, optimizer::OptimizerNodeVisitor},
    parser::Parser,
    source::Source,
    token_parser::TokenParser,
    token_stream::TokenStream,
};

/// `Environment::VERSION`.
pub const VERSION: &str = "3.30.0";

/// The `autoescape` option: `EscaperExtension`'s default strategy.
#[derive(Clone, Debug)]
pub enum Autoescape {
    /// `false`: no auto-escaping.
    Off,
    /// A strategy name (`'html'`, `'js'`, ...).
    Strategy(Vec<u8>),
    /// `'name'`: `FileExtensionEscapingStrategy::guess()`.
    Name,
    /// A callable: the strategy for a template name, `None` for `false`.
    Callback(fn(&[u8]) -> Option<Vec<u8>>),
}

/// The `Environment` constructor options that change parsing.
#[derive(Clone, Debug)]
pub struct EnvironmentOptions {
    /// `autoescape`, default `'html'`.
    pub autoescape: Autoescape,
    /// `optimizations`, default -1 (`OptimizerNodeVisitor::OPTIMIZE_ALL`).
    pub optimizations: i64,
    /// `setLexer(new Lexer($env, $options))`.
    pub lexer: LexerOptions,
}

impl Default for EnvironmentOptions {
    fn default() -> Self {
        Self {
            autoescape: Autoescape::Strategy(b"html".to_vec()),
            optimizations: -1,
            lexer: LexerOptions::default(),
        }
    }
}

/// `Environment`, for parsing.
pub struct Environment {
    extension_set: ExtensionSet,
    lexer: Lexer,
    autoescape: Autoescape,
    /// `ExtensionSet::$visitors`: one set for the environment, made at the
    /// first parse. Their state lasts from one parse to the next, as in
    /// PHP (a parse that fails in the middle of a traversal can leave the
    /// optimizer inside a loop).
    visitors: RefCell<Option<Vec<Box<dyn NodeVisitor>>>>,
}

impl Default for Environment {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Environment {
    /// `new Environment($loader)`: the core, escaper and optimizer
    /// extensions, then these ones (`addExtension()`), with the default
    /// options.
    pub fn new(extensions: Vec<Rc<dyn Extension>>) -> Self {
        Self::build(extensions, EnvironmentOptions::default())
    }

    /// As [`Environment::new`], with `setLexer(new Lexer($env, $options))`.
    pub fn with_lexer_options(extensions: Vec<Rc<dyn Extension>>, options: LexerOptions) -> Self {
        Self::build(
            extensions,
            EnvironmentOptions {
                lexer: options,
                ..EnvironmentOptions::default()
            },
        )
    }

    /// `new Environment($loader, $options)`; an invalid `optimizations`
    /// mode is upstream's `InvalidArgumentException`.
    pub fn with_options(
        extensions: Vec<Rc<dyn Extension>>,
        options: EnvironmentOptions,
    ) -> Result<Self, Error> {
        OptimizerNodeVisitor::check_mode(options.optimizations)?;
        Ok(Self::build(extensions, options))
    }

    fn build(extensions: Vec<Rc<dyn Extension>>, options: EnvironmentOptions) -> Self {
        let mut all: Vec<Rc<dyn Extension>> = vec![
            Rc::new(CoreExtension),
            Rc::new(EscaperExtension),
            Rc::new(OptimizerExtension::new(options.optimizations)),
        ];
        all.extend(extensions);
        let extension_set = ExtensionSet::new(all);
        let operators = extension_set.expression_parsers().operator_tokens();
        let lexer = Lexer::with_operators(operators.iter().map(Vec::as_slice), options.lexer);
        Self {
            extension_set,
            lexer,
            autoescape: options.autoescape,
            visitors: RefCell::new(None),
        }
    }

    /// `getExtension(EscaperExtension::class)->getDefaultStrategy($name)`:
    /// the strategy, or `None` for `false`.
    pub fn default_escaping_strategy(&self, name: &str) -> Option<Vec<u8>> {
        match &self.autoescape {
            Autoescape::Off => None,
            Autoescape::Strategy(s) => Some(s.clone()),
            Autoescape::Name => {
                file_extension_escaping_strategy::guess(name.as_bytes()).map(<[u8]>::to_vec)
            }
            Autoescape::Callback(f) => f(name.as_bytes()),
        }
    }

    /// Runs the environment's node visitors (`getNodeVisitors()`) on a
    /// parsed module, as `Parser::parse()` does.
    pub fn traverse(&self, node: Node) -> Result<Node, Error> {
        // A parse started by a visitor would find the set in use: it gets
        // a set of its own.
        let mut visitors = self
            .visitors
            .borrow_mut()
            .take()
            .unwrap_or_else(|| self.extension_set.node_visitors());
        let result = node_visitor::traverse(self, &mut visitors, node);
        *self.visitors.borrow_mut() = Some(visitors);
        result
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
