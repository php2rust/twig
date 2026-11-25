//! Port of twig/twig 3.30 for a build-time compiler: the template
//! language (lexer, parser, node tree) and, in later stages, the runtime
//! the compiled Rust templates call (PLAN.md, "M4: Twig").
//!
//! Twig source copyright (c) 2009-present by the Twig Team; distributed
//! under the BSD-3-Clause licence (LICENSE-Twig).
//!
//! Upstream: none

#![forbid(unsafe_code)]

pub mod error;
pub mod lexer;
pub mod source;
pub mod token;
pub mod token_stream;

pub use error::{Error, ErrorKind};
pub use lexer::{Lexer, LexerOptions};
pub use source::Source;
pub use token::{Token, TokenType, TokenValue};
pub use token_stream::TokenStream;
