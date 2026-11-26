//! Port of twig/twig 3.30 for a build-time compiler: the template
//! language (lexer, parser, node tree) and, in later stages, the runtime
//! the compiled Rust templates call (PLAN.md, "M4: Twig").
//!
//! Twig source copyright (c) 2009-present by the Twig Team; distributed
//! under the BSD-3-Clause licence (LICENSE-Twig).
//!
//! Upstream: none

#![forbid(unsafe_code)]

pub mod callable;
pub mod compiler;
pub mod environment;
pub mod error;
pub mod expression_parser;
pub mod extension;
pub mod file_extension_escaping_strategy;
pub mod lexer;
pub mod loader;
pub mod macro_namespace;
pub mod markup;
pub mod node;
pub mod node_visitor;
pub mod parser;
pub mod runtime;
pub mod sandbox;
pub mod source;
pub mod template;
pub mod token;
pub mod token_parser;
pub mod token_stream;
pub mod twig_macro;
pub mod util;

pub use callable::TwigCallable;
pub use environment::{Autoescape, Environment, EnvironmentOptions};
pub use error::{Error, ErrorKind};
pub use lexer::{Lexer, LexerOptions};
pub use node::{Attr, Key, Node, NodeClass};
pub use parser::Parser;
pub use source::Source;
pub use token::{Token, TokenType, TokenValue};
pub use token_stream::TokenStream;
