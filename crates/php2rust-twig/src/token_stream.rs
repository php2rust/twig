//! Port of Twig\TokenStream.php.
//!
//! The stream always holds a source (the null-source deprecation path of
//! the constructor is not ported). Methods that return a token return a
//! copy, as PHP returns the object.
//!
//! Upstream: twig/twig src/TokenStream.php

use std::{fmt, sync::Arc};

use crate::{
    error::Error,
    source::Source,
    token::{Token, TokenType},
};

/// A token stream: the tokens of one template and a cursor.
#[derive(Debug, Clone)]
pub struct TokenStream {
    tokens: Vec<Token>,
    current: usize,
    source: Arc<Source>,
}

impl TokenStream {
    /// `new TokenStream($tokens, $source)`. The last token should be
    /// `EOF_TYPE`, as the lexer makes it.
    pub fn new(tokens: Vec<Token>, source: Arc<Source>) -> Self {
        Self {
            tokens,
            current: 0,
            source,
        }
    }

    /// `injectTokens()`: inserts tokens at the cursor.
    pub fn inject_tokens(&mut self, tokens: Vec<Token>) {
        let tail = self.tokens.split_off(self.current.min(self.tokens.len()));
        self.tokens.extend(tokens);
        self.tokens.extend(tail);
    }

    /// `next()`: moves to the next token and returns the old one.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Token, Error> {
        self.current += 1;
        if self.current >= self.tokens.len() {
            let line = self.tokens.get(self.current - 1).map_or(0, Token::line);
            return Err(Error::syntax(
                "Unexpected end of template.",
                to_line(line),
                Some(self.source.clone()),
            ));
        }
        Ok(self.tokens[self.current - 1].clone())
    }

    /// `nextIf($type)`.
    pub fn next_if(&mut self, ty: TokenType) -> Result<Option<Token>, Error> {
        if self.current().test(ty) {
            self.next().map(Some)
        } else {
            Ok(None)
        }
    }

    /// `nextIf($type, $value)`.
    pub fn next_if_value(&mut self, ty: TokenType, value: &[u8]) -> Result<Option<Token>, Error> {
        if self.current().test_value(ty, value) {
            self.next().map(Some)
        } else {
            Ok(None)
        }
    }

    /// `nextIf($type, [$value, ...])`.
    pub fn next_if_values(
        &mut self,
        ty: TokenType,
        values: &[&[u8]],
    ) -> Result<Option<Token>, Error> {
        if self.current().test_values(ty, values) {
            self.next().map(Some)
        } else {
            Ok(None)
        }
    }

    /// `expect($type)`.
    pub fn expect(&mut self, ty: TokenType) -> Result<Token, Error> {
        self.expect_with(ty, None, None)
    }

    /// `expect($type, $value)`.
    pub fn expect_value(&mut self, ty: TokenType, value: &[u8]) -> Result<Token, Error> {
        self.expect_with(ty, Some(value), None)
    }

    /// `expect($type, $value, $message)`.
    pub fn expect_with(
        &mut self,
        ty: TokenType,
        value: Option<&[u8]>,
        message: Option<&[u8]>,
    ) -> Result<Token, Error> {
        let token = self.current().clone();
        let matches = match value {
            Some(v) => token.test_value(ty, v),
            None => token.test(ty),
        };
        if !matches {
            let mut text = Vec::new();
            if let Some(m) = message.filter(|m| php_value::string::truthy(m)) {
                text.extend_from_slice(m);
                text.extend_from_slice(b". ");
            }
            text.extend_from_slice(b"Unexpected token \"");
            text.extend_from_slice(token.to_english().as_bytes());
            text.push(b'"');
            if token.value().is_truthy() {
                text.extend_from_slice(b" of value \"");
                text.extend_from_slice(&token.value_bytes());
                text.push(b'"');
            }
            text.extend_from_slice(b" (\"");
            text.extend_from_slice(ty.to_english().as_bytes());
            text.extend_from_slice(b"\" expected");
            if let Some(v) = value.filter(|v| php_value::string::truthy(v)) {
                text.extend_from_slice(b" with value \"");
                text.extend_from_slice(v);
                text.push(b'"');
            }
            text.extend_from_slice(b").");
            let column = token.offset().map(|o| self.source.column(o));
            return Err(
                Error::syntax(text, to_line(token.line()), Some(self.source.clone()))
                    .with_column(column),
            );
        }
        self.next()?;
        Ok(token)
    }

    /// `look($number)`: the token `number` places after the current one.
    pub fn look(&self, number: usize) -> Result<&Token, Error> {
        let index = self.current + number;
        self.tokens.get(index).ok_or_else(|| {
            let line = index
                .checked_sub(1)
                .and_then(|i| self.tokens.get(i))
                .map_or(0, Token::line);
            Error::syntax(
                "Unexpected end of template.",
                to_line(line),
                Some(self.source.clone()),
            )
        })
    }

    /// `test($type)` on the current token.
    pub fn test(&self, ty: TokenType) -> bool {
        self.current().test(ty)
    }

    /// `test($type, $value)` on the current token.
    pub fn test_value(&self, ty: TokenType, value: &[u8]) -> bool {
        self.current().test_value(ty, value)
    }

    /// `test($type, [$value, ...])` on the current token.
    pub fn test_values(&self, ty: TokenType, values: &[&[u8]]) -> bool {
        self.current().test_values(ty, values)
    }

    /// `isEOF()`.
    pub fn is_eof(&self) -> bool {
        self.current().test(TokenType::Eof)
    }

    /// `getCurrent()`.
    ///
    /// # Panics
    ///
    /// When the stream has no tokens, which the lexer never produces.
    pub fn current(&self) -> &Token {
        &self.tokens[self.current.min(self.tokens.len().saturating_sub(1))]
    }

    /// `getSourceContext()`.
    pub fn source_context(&self) -> &Arc<Source> {
        &self.source
    }
}

/// A token line as an error line.
pub(crate) fn to_line(line: usize) -> i64 {
    i64::try_from(line).unwrap_or(i64::MAX)
}

/// `__toString()`: the tokens, one per line.
impl fmt::Display for TokenStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, token) in self.tokens.iter().enumerate() {
            if i > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{token}")?;
        }
        Ok(())
    }
}
