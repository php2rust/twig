//! Port of Twig\Lexer.php.
//!
//! Upstream builds PCRE patterns from the options and matches them with
//! `preg_match()`. Each pattern here is a hand-written matcher with the
//! same semantics: `\s` is PCRE2's ASCII set (space, `\t`, `\n`, `\v`,
//! `\f`, `\r`), alternatives are tried in pattern order, an anchored
//! pattern (`/A`) matches only at the cursor, and an unanchored one finds
//! the leftmost match. The template is bytes, as in PHP (names accept
//! the bytes `\x7f`-`\xff`).
//!
//! The operator list comes from the environment's expression parsers
//! (`ExpressionParsers::getOperatorTokensFor()`); [`Lexer::new`] takes the
//! core extension's list, [`Lexer::with_operators`] any other.
//!
//! The `whitespace_line_chars` option is the set of bytes itself, not
//! the regex character-class text upstream inserts into its patterns.
//!
//! Upstream: twig/twig src/Lexer.php

use std::sync::Arc;

use php_value::numeric::{Number, numeric_value};
use symfony_deprecation_contracts::trigger_deprecation;

use crate::{
    error::Error,
    source::{Source, normalize_newlines},
    token::{Token, TokenType, TokenValue},
    token_stream::{TokenStream, to_line},
};

/// The operator tokens of `CoreExtension::getExpressionParsers()`, in
/// registration order (names, then aliases).
pub const CORE_OPERATOR_TOKENS: &[&str] = &[
    // unary operators
    "not",
    "...",
    "-",
    "+", // binary operators
    "?:",
    "? :",
    "??",
    "or",
    "xor",
    "and",
    "b-or",
    "b-xor",
    "b-and",
    "==",
    "!=",
    "<=>",
    "<",
    ">",
    ">=",
    "<=",
    "not in",
    "in",
    "matches",
    "starts with",
    "ends with",
    "has some",
    "has every",
    "===",
    "!==",
    "..",
    "+",
    "-",
    "~",
    "*",
    "/",
    "//",
    "%",
    "**", // ternary
    "?",  // assignment
    "=",  // Twig callables
    "is",
    "is not",
    "|",
    "(", // get attribute operators
    ".",
    "?.",
    "[", // grouping
    "(", // arrow function
    "=>",
];

/// The lexer options (`tag_comment`, `tag_block`, ...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexerOptions {
    pub tag_comment: (Vec<u8>, Vec<u8>),
    pub tag_block: (Vec<u8>, Vec<u8>),
    pub tag_variable: (Vec<u8>, Vec<u8>),
    pub whitespace_trim: Vec<u8>,
    pub whitespace_line_trim: Vec<u8>,
    /// The bytes `~` trims after a closing tag (upstream: `' \t\0\x0B'`,
    /// a character class).
    pub whitespace_line_chars: Vec<u8>,
    pub interpolation: (Vec<u8>, Vec<u8>),
}

impl Default for LexerOptions {
    fn default() -> Self {
        Self {
            tag_comment: (b"{#".to_vec(), b"#}".to_vec()),
            tag_block: (b"{%".to_vec(), b"%}".to_vec()),
            tag_variable: (b"{{".to_vec(), b"}}".to_vec()),
            whitespace_trim: b"-".to_vec(),
            whitespace_line_trim: b"~".to_vec(),
            whitespace_line_chars: b" \t\0\x0B".to_vec(),
            interpolation: (b"#{".to_vec(), b"}".to_vec()),
        }
    }
}

/// The lexer. It keeps no state between [`Lexer::tokenize`] calls.
#[derive(Debug, Clone)]
pub struct Lexer {
    options: LexerOptions,
    /// The operators, longest first (`arsort()` by length, stable).
    operators: Vec<Vec<u8>>,
}

const OPENING_BRACKETS: &[u8] = b"{([";
const CLOSING_BRACKETS: &[u8] = b"})]";
const PUNCTUATION: &[u8] = b"()[]{}?:.,|";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Data,
    Block,
    Var,
    String,
    Interpolation,
}

/// A match of `lex_tokens_start`: where it starts, its length, the tag
/// alternative (group 1) and the whitespace modifier (group 2).
#[derive(Debug, Clone)]
struct TagStart {
    start: usize,
    len: usize,
    tag: Vec<u8>,
    modifier: Option<Vec<u8>>,
}

impl Lexer {
    /// `new Lexer($env, $options)` for an environment with the core
    /// extension only.
    pub fn new(options: LexerOptions) -> Self {
        Self::with_operators(CORE_OPERATOR_TOKENS.iter().map(|s| s.as_bytes()), options)
    }

    /// A lexer for these operator tokens (the environment's expression
    /// parsers' `getOperatorTokens()`, in registration order).
    pub fn with_operators<'a>(
        operators: impl IntoIterator<Item = &'a [u8]>,
        options: LexerOptions,
    ) -> Self {
        // array_combine(): a repeated token keeps its first position
        let mut unique: Vec<Vec<u8>> = Vec::new();
        for op in operators {
            if !unique.iter().any(|u| u == op) {
                unique.push(op.to_vec());
            }
        }
        // arsort() by length is stable since PHP 8.0
        unique.sort_by_key(|op| std::cmp::Reverse(op.len()));
        Self {
            options,
            operators: unique,
        }
    }

    pub fn options(&self) -> &LexerOptions {
        &self.options
    }

    /// `tokenize()`.
    pub fn tokenize(&self, source: Source) -> Result<TokenStream, Error> {
        let source = Arc::new(source);
        let mut run = Run {
            lexer: self,
            o: &self.options,
            code: source.code(),
            source: &source,
            cursor: 0,
            lineno: 1,
            tokens: Vec::new(),
            state: State::Data,
            states: Vec::new(),
            brackets: Vec::new(),
            position: None,
            positions: Vec::new(),
            current_var_block_line: 0,
            documentation: Vec::new(),
        };
        run.positions = run.tag_starts();
        let end = run.code.len();
        while run.cursor < end {
            match run.state {
                State::Data => run.lex_data()?,
                State::Block => run.lex_block()?,
                State::Var => run.lex_var()?,
                State::String => run.lex_string()?,
                State::Interpolation => run.lex_interpolation()?,
            }
        }
        run.push_token(TokenType::Eof, Vec::new(), None, None);
        if let Some((expect, lineno)) = run.brackets.pop() {
            return Err(run.unclosed(&expect, lineno));
        }
        let tokens = std::mem::take(&mut run.tokens);
        Ok(TokenStream::new(tokens, source.clone()))
    }
}

/// The state of one `tokenize()` call.
struct Run<'a> {
    lexer: &'a Lexer,
    o: &'a LexerOptions,
    code: &'a [u8],
    source: &'a Arc<Source>,
    cursor: usize,
    lineno: usize,
    tokens: Vec<Token>,
    state: State,
    states: Vec<State>,
    brackets: Vec<(Vec<u8>, usize)>,
    position: Option<usize>,
    positions: Vec<TagStart>,
    current_var_block_line: usize,
    documentation: Vec<Vec<u8>>,
}

/// PCRE2 `\s` without `/u`.
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// The bytes `trim()`, `ltrim()` and `rtrim()` strip by default.
fn is_trim_char(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\0' | 0x0b)
}

/// The bytes `rtrim($text, " \t\0\x0B")` strips.
fn is_line_trim_char(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\0' | 0x0b)
}

fn rtrim_by(text: &[u8], strip: impl Fn(u8) -> bool) -> &[u8] {
    let end = text.iter().rposition(|&b| !strip(b)).map_or(0, |i| i + 1);
    &text[..end]
}

fn trim(text: &[u8]) -> &[u8] {
    let start = text
        .iter()
        .position(|&b| !is_trim_char(b))
        .unwrap_or(text.len());
    rtrim_by(&text[start..], is_trim_char)
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b >= 0x7f
}

fn is_name_char(b: u8) -> bool {
    is_name_start(b) || b.is_ascii_digit()
}

/// The length of the run of bytes from `at` that pass `f`.
fn span(code: &[u8], at: usize, f: impl Fn(u8) -> bool) -> usize {
    code.get(at..)
        .map_or(0, |rest| rest.iter().take_while(|&&b| f(b)).count())
}

/// The length of `literal` when the code has it at `at`.
fn lit(code: &[u8], at: usize, literal: &[u8]) -> Option<usize> {
    code.get(at..)
        .filter(|rest| rest.starts_with(literal))
        .map(|_| literal.len())
}

/// `(?:\r\n?|\n)?` at `at`.
fn opt_newline(code: &[u8], at: usize) -> usize {
    match code.get(at) {
        Some(b'\r') if code.get(at + 1) == Some(&b'\n') => 2,
        Some(b'\r' | b'\n') => 1,
        _ => 0,
    }
}

/// `\s*\n?` at `at` (`\s*` already takes every newline).
fn spaces(code: &[u8], at: usize) -> usize {
    span(code, at, is_space)
}

/// The leftmost match of `at_pos` from `from`: its start and result.
fn search<T>(code: &[u8], from: usize, at_pos: impl Fn(usize) -> Option<T>) -> Option<(usize, T)> {
    (from..code.len()).find_map(|i| at_pos(i).map(|m| (i, m)))
}

/// `[0-9]+(_[0-9]+)*` at `at`.
fn lnum(code: &[u8], at: usize) -> Option<usize> {
    let mut len = span(code, at, |b| b.is_ascii_digit());
    if len == 0 {
        return None;
    }
    while code.get(at + len) == Some(&b'_') {
        let digits = span(code, at + len + 1, |b| b.is_ascii_digit());
        if digits == 0 {
            break;
        }
        len += 1 + digits;
    }
    Some(len)
}

/// `REGEX_NUMBER` at `at`: `LNUM (\.LNUM)? ([eE][+-]?LNUM)?`.
fn number(code: &[u8], at: usize) -> Option<usize> {
    let mut len = lnum(code, at)?;
    if code.get(at + len) == Some(&b'.')
        && let Some(frac) = lnum(code, at + len + 1)
    {
        len += 1 + frac;
    }
    if matches!(code.get(at + len), Some(b'e' | b'E')) {
        let mut sign = 1;
        if matches!(code.get(at + len + 1), Some(b'+' | b'-')) {
            sign = 2;
        }
        if let Some(exp) = lnum(code, at + len + sign) {
            len += sign + exp;
        }
    }
    Some(len)
}

/// `REGEX_STRING` at `at`: a double-quoted string without `#` and
/// unescaped `"` or `\`, or a single-quoted one; escapes `\.` allowed.
fn quoted_string(code: &[u8], at: usize) -> Option<usize> {
    let quote = *code.get(at)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut i = at + 1;
    loop {
        match *code.get(i)? {
            b'\\' => {
                code.get(i + 1)?;
                i += 2;
            }
            b if b == quote => return Some(i + 1 - at),
            b'#' if quote == b'"' => return None,
            _ => i += 1,
        }
    }
}

/// `REGEX_DQ_STRING_PART` at `at`: bytes other than `"` and `\`, escapes
/// `\.`, and `#` not followed by `{`. May be empty.
fn dq_string_part(code: &[u8], at: usize) -> usize {
    let mut i = at;
    loop {
        match code.get(i) {
            Some(b'"') | None => break,
            Some(b'\\') => {
                if code.get(i + 1).is_none() {
                    break;
                }
                i += 2;
            }
            Some(b'#') => {
                if code.get(i + 1) == Some(&b'{') {
                    break;
                }
                i += 1;
            }
            Some(_) => i += 1,
        }
    }
    i - at
}

impl Run<'_> {
    fn syntax(&self, message: impl Into<Vec<u8>>, lineno: usize) -> Error {
        Error::syntax(message, to_line(lineno), Some(self.source.clone()))
    }

    fn unclosed(&self, expect: &[u8], lineno: usize) -> Error {
        let mut message = b"Unclosed \"".to_vec();
        message.extend_from_slice(expect);
        message.extend_from_slice(b"\".");
        self.syntax(message, lineno)
    }

    fn unexpected_character(&self) -> Error {
        let mut message = b"Unexpected character \"".to_vec();
        message.push(self.code[self.cursor]);
        message.extend_from_slice(b"\".");
        self.syntax(message, self.lineno)
            .with_column(Some(self.source.column(self.cursor)))
    }

    /// The closing alternatives shared by the tag patterns:
    /// `<trim><close>\s*`, `<line_trim><close>[line chars]*`, or
    /// `<close>` followed by `tail` (a newline or nothing).
    fn closing(&self, at: usize, close: &[u8], tail: fn(&[u8], usize) -> usize) -> Option<usize> {
        let o = self.o;
        let mut ws_close = o.whitespace_trim.clone();
        ws_close.extend_from_slice(close);
        if let Some(len) = lit(self.code, at, &ws_close) {
            // `\s*\n?` and `\s*` match the same
            return Some(len + spaces(self.code, at + len));
        }
        let mut line_close = o.whitespace_line_trim.clone();
        line_close.extend_from_slice(close);
        if let Some(len) = lit(self.code, at, &line_close) {
            return Some(
                len + span(self.code, at + len, |b| {
                    o.whitespace_line_chars.contains(&b)
                }),
            );
        }
        lit(self.code, at, close).map(|len| len + tail(self.code, at + len))
    }

    /// `lex_tokens_start`, every match (`preg_match_all()`).
    fn tag_starts(&self) -> Vec<TagStart> {
        let o = self.o;
        let mut doc_open = o.tag_comment.0.clone();
        doc_open.push(b'#');
        // the empty comment ("{##}") is not a documentation comment
        let doc_lookahead: Option<&[u8]> = o.tag_comment.1.strip_prefix(b"#");
        let alternatives: [&[u8]; 4] = [
            &o.tag_variable.0,
            &o.tag_block.0,
            &doc_open,
            &o.tag_comment.0,
        ];
        let mut found = Vec::new();
        let mut i = 0;
        while i < self.code.len() {
            let mut hit = None;
            for (n, alt) in alternatives.iter().enumerate() {
                let Some(len) = lit(self.code, i, alt) else {
                    continue;
                };
                if n == 2 {
                    let blocked = match doc_lookahead {
                        None => false,
                        // (?!) never matches
                        Some([]) => true,
                        Some(rest) => lit(self.code, i + len, rest).is_some(),
                    };
                    if blocked {
                        continue;
                    }
                }
                hit = Some(((*alt).to_vec(), len));
                break;
            }
            let Some((tag, len)) = hit else {
                i += 1;
                continue;
            };
            let modifier = [&o.whitespace_trim, &o.whitespace_line_trim]
                .into_iter()
                .find(|m| lit(self.code, i + len, m).is_some())
                .cloned();
            let full = len + modifier.as_ref().map_or(0, Vec::len);
            found.push(TagStart {
                start: i,
                len: full,
                tag,
                modifier,
            });
            // an empty match cannot happen: every tag has bytes
            i += full.max(1);
        }
        found
    }

    fn lex_data(&mut self) -> Result<(), Error> {
        let last = self.positions.len().checked_sub(1);
        // if no matches are left we return the rest of the template as simple text token
        if self.position == last {
            let text = &self.code[self.cursor..];
            self.push_token(TokenType::Text, normalize_newlines(text), None, None);
            self.move_cursor(self.code.len() - self.cursor);
            return Ok(());
        }
        // Find the first token after the current cursor
        let mut index = self.position.map_or(0, |p| p + 1);
        self.position = Some(index);
        while self.positions[index].start < self.cursor {
            if Some(index) == last {
                return Ok(());
            }
            index += 1;
            self.position = Some(index);
        }
        let position = self.positions[index].clone();

        // push the template text first
        let text_content = &self.code[self.cursor..position.start];
        let mut text = text_content;
        match &position.modifier {
            Some(m) if *m == self.o.whitespace_trim => text = rtrim_by(text, is_trim_char),
            Some(m) if *m == self.o.whitespace_line_trim => {
                text = rtrim_by(text, is_line_trim_char);
            }
            _ => {}
        }
        let text = normalize_newlines(text);
        self.push_token(TokenType::Text, text, None, None);
        self.move_cursor(text_content.len());

        let mut doc_open = self.o.tag_comment.0.clone();
        doc_open.push(b'#');
        if position.tag == doc_open {
            self.move_cursor(position.len);
            self.lex_comment(true)?;
        } else if position.tag == self.o.tag_comment.0 {
            self.move_cursor(position.len);
            self.lex_comment(false)?;
        } else if position.tag == self.o.tag_block.0 {
            let lineno = self.lineno;
            let cursor = self.cursor;
            self.move_cursor(position.len);
            if let Some(len) = self.block_raw() {
                // raw data?
                self.documentation.clear();
                self.move_cursor(len);
                self.lex_raw_data()?;
            } else if let Some((len, line)) = self.block_line() {
                // {% line \d+ %}
                self.documentation.clear();
                self.move_cursor(len);
                self.lineno = line;
            } else {
                self.push_token(
                    TokenType::BlockStart,
                    Vec::new(),
                    Some(cursor),
                    Some(lineno),
                );
                self.push_state(State::Block);
                self.current_var_block_line = self.lineno;
            }
        } else if position.tag == self.o.tag_variable.0 {
            let lineno = self.lineno;
            let cursor = self.cursor;
            self.move_cursor(position.len);
            self.push_token(TokenType::VarStart, Vec::new(), Some(cursor), Some(lineno));
            self.push_state(State::Var);
            self.current_var_block_line = self.lineno;
        }
        Ok(())
    }

    /// `lex_block_raw`: `\s*verbatim\s*` and a closing `%}`.
    fn block_raw(&self) -> Option<usize> {
        let at = self.cursor;
        let mut len = spaces(self.code, at);
        len += lit(self.code, at + len, b"verbatim")?;
        len += spaces(self.code, at + len);
        let close = self.closing(at + len, &self.o.tag_block.1, |_, _| 0)?;
        Some(len + close)
    }

    /// `lex_block_line`: `\s*line\s+(\d+)\s*%}`; the length and the line.
    fn block_line(&self) -> Option<(usize, usize)> {
        let at = self.cursor;
        let mut len = spaces(self.code, at);
        len += lit(self.code, at + len, b"line")?;
        let ws = spaces(self.code, at + len);
        if ws == 0 {
            return None;
        }
        len += ws;
        let digits = span(self.code, at + len, |b| b.is_ascii_digit());
        if digits == 0 {
            return None;
        }
        let number = &self.code[at + len..at + len + digits];
        len += digits;
        len += spaces(self.code, at + len);
        len += lit(self.code, at + len, &self.o.tag_block.1)?;
        // (int) "<digits>" saturates at PHP_INT_MAX
        let max = usize::try_from(i64::MAX).unwrap_or(usize::MAX);
        let line = number.iter().fold(0usize, |n, &d| {
            n.saturating_mul(10)
                .saturating_add(usize::from(d - b'0'))
                .min(max)
        });
        Some((len, line))
    }

    fn lex_block(&mut self) -> Result<(), Error> {
        if self.brackets.is_empty()
            && let Some(len) = self.closing_tag(&self.o.tag_block.1.clone(), true)
        {
            self.push_closing_token(TokenType::BlockEnd, len);
            self.pop_state();
            Ok(())
        } else {
            self.lex_expression()
        }
    }

    fn lex_var(&mut self) -> Result<(), Error> {
        if self.brackets.is_empty()
            && let Some(len) = self.closing_tag(&self.o.tag_variable.1.clone(), false)
        {
            self.push_closing_token(TokenType::VarEnd, len);
            self.pop_state();
            Ok(())
        } else {
            self.lex_expression()
        }
    }

    /// `lex_block` (`block` true) and `lex_var`: `\s*` and a closing tag.
    fn closing_tag(&self, close: &[u8], block: bool) -> Option<usize> {
        let at = self.cursor;
        let ws = spaces(self.code, at);
        let tail: fn(&[u8], usize) -> usize = if block { opt_newline } else { |_, _| 0 };
        self.closing(at + ws, close, tail).map(|len| ws + len)
    }

    fn lex_expression(&mut self) -> Result<(), Error> {
        // whitespace
        let ws = spaces(self.code, self.cursor);
        if ws > 0 {
            self.move_cursor(ws);
            if self.cursor >= self.code.len() {
                let what = if self.state == State::Block {
                    "block"
                } else {
                    "variable"
                };
                return Err(
                    self.syntax(format!("Unclosed \"{what}\"."), self.current_var_block_line)
                );
            }
        }

        if let Some(len) = self.operator() {
            // operators
            let matched = &self.code[self.cursor..self.cursor + len];
            let operator = collapse_spaces(matched);
            if operator.len() == 1 && OPENING_BRACKETS.contains(&operator[0]) {
                self.check_brackets(operator[0])?;
            }
            self.push_token(TokenType::Operator, operator, None, None);
            self.move_cursor(len);
        } else if self
            .code
            .get(self.cursor)
            .copied()
            .is_some_and(is_name_start)
        {
            // names
            let len = span(self.code, self.cursor, is_name_char);
            let name = self.code[self.cursor..self.cursor + len].to_vec();
            self.push_token(TokenType::Name, name, None, None);
            self.move_cursor(len);
        } else if let Some(len) = number(self.code, self.cursor) {
            // numbers
            let digits: Vec<u8> = self.code[self.cursor..self.cursor + len]
                .iter()
                .copied()
                .filter(|&b| b != b'_')
                .collect();
            let value = numeric_value(&digits).unwrap_or(Number::Int(0));
            self.push_number(value);
            self.move_cursor(len);
        } else if PUNCTUATION.contains(&self.code[self.cursor]) {
            // punctuation
            let c = self.code[self.cursor];
            self.check_brackets(c)?;
            self.push_token(TokenType::Punctuation, vec![c], None, None);
            self.move_cursor(1);
        } else if let Some(len) = quoted_string(self.code, self.cursor) {
            // strings
            let quote = self.code[self.cursor];
            let inner = normalize_newlines(&self.code[self.cursor + 1..self.cursor + len - 1]);
            let value = self.stripcslashes(&inner, quote);
            self.push_token(TokenType::String, value, None, None);
            self.move_cursor(len);
        } else if self.code[self.cursor] == b'"' {
            // opening double quoted string
            self.brackets.push((b"\"".to_vec(), self.lineno));
            self.push_state(State::String);
            self.move_cursor(1);
        } else if self.code[self.cursor] == b'#' {
            // inline comment
            let len = span(self.code, self.cursor, |b| b != b'\r' && b != b'\n');
            let comment = &self.code[self.cursor..self.cursor + len];
            if let Some(doc) = comment.strip_prefix(b"##") {
                self.add_documentation(doc);
            }
            self.move_cursor(len);
        } else {
            // unlexable
            return Err(self.unexpected_character());
        }
        Ok(())
    }

    /// The operator regex at the cursor: the first operator (longest
    /// first) that matches. An operator ending with a letter must be
    /// followed by whitespace, `(`, `)`, `[` or `{`; one starting with a
    /// letter must not follow `.` or `|` (with or without one whitespace
    /// byte between); a space inside one matches any whitespace run.
    fn operator(&self) -> Option<usize> {
        let at = self.cursor;
        let code = self.code;
        self.lexer.operators.iter().find_map(|op| {
            let (&first, &last) = (op.first()?, op.last()?);
            if first.is_ascii_alphabetic() && at >= 2 {
                let (a, b) = (code[at - 2], code[at - 1]);
                let dot_or_pipe = |c: u8| c == b'.' || c == b'|';
                if (dot_or_pipe(a) && is_space(b)) || (a != b'\n' && dot_or_pipe(b)) {
                    return None;
                }
            }
            let mut len = 0;
            let mut i = 0;
            while i < op.len() {
                if is_space(op[i]) {
                    let run = op[i..].iter().take_while(|&&b| is_space(b)).count();
                    let ws = spaces(code, at + len);
                    if ws == 0 {
                        return None;
                    }
                    i += run;
                    len += ws;
                } else if code.get(at + len) == Some(&op[i]) {
                    i += 1;
                    len += 1;
                } else {
                    return None;
                }
            }
            if last.is_ascii_alphabetic() {
                let next = *code.get(at + len)?;
                if !(is_space(next) || matches!(next, b'(' | b')' | b'[' | b'{')) {
                    return None;
                }
            }
            Some(len)
        })
    }

    fn stripcslashes(&self, s: &[u8], quote_type: u8) -> Vec<u8> {
        let mut result = Vec::with_capacity(s.len());
        let length = s.len();
        let mut i = 0;
        while i < length {
            let Some(pos) = s[i..].iter().position(|&b| b == b'\\').map(|p| p + i) else {
                result.extend_from_slice(&s[i..]);
                break;
            };
            result.extend_from_slice(&s[i..pos]);
            i = pos + 1;
            if i >= length {
                result.push(b'\\');
                break;
            }
            let next = s[i];
            match next {
                b'f' => result.push(0x0c),
                b'n' => result.push(b'\n'),
                b'r' => result.push(b'\r'),
                b't' => result.push(b'\t'),
                b'v' => result.push(0x0b),
                b'\\' => result.push(b'\\'),
                b'\'' | b'"' => {
                    if next != quote_type {
                        self.escape_deprecation(next, i);
                    }
                    result.push(next);
                }
                b'#' if s.get(i + 1) == Some(&b'{') => {
                    result.extend_from_slice(b"#{");
                    i += 1;
                }
                b'x' if s.get(i + 1).is_some_and(u8::is_ascii_hexdigit) => {
                    i += 1;
                    let mut hex = u32::from(hex_value(s[i]));
                    if s.get(i + 1).is_some_and(u8::is_ascii_hexdigit) {
                        i += 1;
                        hex = hex * 16 + u32::from(hex_value(s[i]));
                    }
                    result.push(u8::try_from(hex).unwrap_or(0));
                }
                b'0'..=b'7' => {
                    let mut octal = u32::from(next - b'0');
                    let mut digits = 1;
                    while digits < 3 && matches!(s.get(i + 1), Some(b'0'..=b'7')) {
                        i += 1;
                        octal = octal * 8 + u32::from(s[i] - b'0');
                        digits += 1;
                    }
                    result.push(u8::try_from(octal % 256).unwrap_or(0));
                }
                _ => {
                    self.escape_deprecation(next, i);
                    result.push(next);
                }
            }
            i += 1;
        }
        result
    }

    fn escape_deprecation(&self, character: u8, i: usize) {
        let mut message = b"Character \"".to_vec();
        message.push(character);
        message.extend_from_slice(
            format!(
                "\" should not be escaped; the \"\\\" character is ignored in Twig 3 but will not be in Twig 4. Please remove the extra \"\\\" character at position {} in \"{}\" at line {}.",
                i + 1,
                self.source.name(),
                self.lineno
            )
            .as_bytes(),
        );
        trigger_deprecation(b"twig/twig", b"3.12", &message);
    }

    fn lex_raw_data(&mut self) -> Result<(), Error> {
        let o = self.o;
        let found = search(self.code, self.cursor, |at| {
            let mut len = lit(self.code, at, &o.tag_block.0)?;
            let modifier = [&o.whitespace_trim, &o.whitespace_line_trim]
                .into_iter()
                .find(|m| lit(self.code, at + len, m).is_some())
                .cloned();
            len += modifier.as_ref().map_or(0, Vec::len);
            len += spaces(self.code, at + len);
            len += lit(self.code, at + len, b"endverbatim")?;
            len += spaces(self.code, at + len);
            len += self.closing(at + len, &o.tag_block.1, |_, _| 0)?;
            Some((len, modifier))
        });
        let Some((start, (len, modifier))) = found else {
            return Err(self.syntax(
                "Unexpected end of file: Unclosed \"verbatim\" block.",
                self.lineno,
            ));
        };

        let offset = self.cursor;
        let text_content = &self.code[self.cursor..start];
        let mut text = text_content;
        self.move_cursor(text_content.len() + len);
        match modifier {
            Some(m) if m == o.whitespace_trim => text = rtrim_by(text, is_trim_char),
            Some(_) => text = rtrim_by(text, is_line_trim_char),
            None => {}
        }
        let text = normalize_newlines(text);
        self.push_token(TokenType::Text, text, Some(offset), None);
        Ok(())
    }

    fn lex_comment(&mut self, is_documentation: bool) -> Result<(), Error> {
        let o = self.o;
        let close = &o.tag_comment.1;
        let mut doc_close = b"#".to_vec();
        doc_close.extend_from_slice(close);
        let found = search(self.code, self.cursor, |at| {
            if is_documentation && let Some(len) = self.closing_doc(at, &doc_close) {
                return Some(len);
            }
            self.closing(at, close, opt_newline)
        });
        let Some((start, len)) = found else {
            return Err(self.syntax("Unclosed comment.", self.lineno));
        };
        let comment = &self.code[self.cursor..start];
        if is_documentation {
            // support a symmetric "##}" closing marker
            let documentation = comment.strip_suffix(b"#").unwrap_or(comment).to_vec();
            self.add_documentation(&documentation);
        }
        self.move_cursor(comment.len() + len);
        Ok(())
    }

    /// The first two alternatives of `lex_documentation_comment`:
    /// `-##}\s*\n?` and `~##}[line chars]*`.
    fn closing_doc(&self, at: usize, doc_close: &[u8]) -> Option<usize> {
        let o = self.o;
        let mut ws_close = o.whitespace_trim.clone();
        ws_close.extend_from_slice(doc_close);
        if let Some(len) = lit(self.code, at, &ws_close) {
            return Some(len + spaces(self.code, at + len));
        }
        let mut line_close = o.whitespace_line_trim.clone();
        line_close.extend_from_slice(doc_close);
        lit(self.code, at, &line_close).map(|len| {
            len + span(self.code, at + len, |b| {
                o.whitespace_line_chars.contains(&b)
            })
        })
    }

    fn lex_string(&mut self) -> Result<(), Error> {
        let o = self.o;
        if let Some(len) = lit(self.code, self.cursor, &o.interpolation.0) {
            // interpolation_start: `#{\s*`
            let len = len + spaces(self.code, self.cursor + len);
            self.brackets.push((o.interpolation.0.clone(), self.lineno));
            self.push_token(TokenType::InterpolationStart, Vec::new(), None, None);
            self.move_cursor(len);
            self.push_state(State::Interpolation);
            return Ok(());
        }
        let len = dq_string_part(self.code, self.cursor);
        if len > 0 {
            let part = normalize_newlines(&self.code[self.cursor..self.cursor + len]);
            let value = self.stripcslashes(&part, b'"');
            self.push_token(TokenType::String, value, None, None);
            self.move_cursor(len);
        } else if self.code[self.cursor] == b'"' {
            let (expect, lineno) = self.brackets.pop().unwrap_or_default();
            if self.code[self.cursor] != b'"' {
                return Err(self.unclosed(&expect, lineno));
            }
            self.pop_state();
            self.move_cursor(1);
        } else {
            // unlexable
            return Err(self.unexpected_character());
        }
        Ok(())
    }

    fn lex_interpolation(&mut self) -> Result<(), Error> {
        let o = self.o;
        let in_interpolation = self
            .brackets
            .last()
            .is_some_and(|(b, _)| *b == o.interpolation.0);
        if in_interpolation {
            // interpolation_end: `\s*}`
            let ws = spaces(self.code, self.cursor);
            if let Some(len) = lit(self.code, self.cursor + ws, &o.interpolation.1) {
                self.brackets.pop();
                self.push_closing_token(TokenType::InterpolationEnd, ws + len);
                self.pop_state();
                return Ok(());
            }
        }
        self.lex_expression()
    }

    fn push_number(&mut self, value: Number) {
        self.push_value(TokenType::Number, TokenValue::Number(value), None, None);
    }

    fn push_token(
        &mut self,
        ty: TokenType,
        value: Vec<u8>,
        offset: Option<usize>,
        lineno: Option<usize>,
    ) {
        // do not push empty text tokens
        if ty == TokenType::Text && value.is_empty() {
            return;
        }
        self.push_value(ty, TokenValue::Bytes(value), offset, lineno);
    }

    fn push_value(
        &mut self,
        ty: TokenType,
        value: TokenValue,
        offset: Option<usize>,
        lineno: Option<usize>,
    ) {
        let mut documentation = None;
        let blank_text =
            ty == TokenType::Text && value.as_bytes().is_some_and(|v| trim(v).is_empty());
        if !self.documentation.is_empty() && !blank_text {
            documentation = Some(self.documentation.join(&b'\n'));
            self.documentation.clear();
        }
        self.tokens.push(Token::with_value(
            ty,
            value,
            lineno.unwrap_or(self.lineno),
            Some(offset.unwrap_or(self.cursor)),
            documentation,
        ));
    }

    fn add_documentation(&mut self, documentation: &[u8]) {
        let normalized = normalize_newlines(documentation);
        let documentation = trim(&normalized);
        if !documentation.is_empty() {
            self.documentation.push(documentation.to_vec());
        }
    }

    /// `moveCursor()` over the next `len` bytes: `\r\n` and `\r` count as
    /// one newline each within the chunk.
    fn move_cursor(&mut self, len: usize) {
        let text = &self.code[self.cursor..self.cursor + len];
        let mut newlines = 0;
        let mut i = 0;
        while i < text.len() {
            match text[i] {
                b'\r' => {
                    newlines += 1;
                    if text.get(i + 1) == Some(&b'\n') {
                        i += 1;
                    }
                }
                b'\n' => newlines += 1,
                _ => {}
            }
            i += 1;
        }
        self.cursor += len;
        self.lineno = self.lineno.saturating_add(newlines);
    }

    /// `pushClosingToken()`: the token points at the marker, after the
    /// leading whitespace the closing pattern also takes.
    fn push_closing_token(&mut self, ty: TokenType, len: usize) {
        let matched = &self.code[self.cursor..self.cursor + len];
        let leading = matched.iter().take_while(|&&b| is_trim_char(b)).count();
        if leading > 0 {
            self.move_cursor(leading);
        }
        self.push_token(ty, Vec::new(), None, None);
        self.move_cursor(len - leading);
    }

    fn push_state(&mut self, state: State) {
        self.states.push(self.state);
        self.state = state;
    }

    fn pop_state(&mut self) {
        // upstream throws a LogicException on an empty stack, which the
        // state machine never reaches
        if let Some(state) = self.states.pop() {
            self.state = state;
        }
    }

    fn check_brackets(&mut self, code: u8) -> Result<(), Error> {
        if OPENING_BRACKETS.contains(&code) {
            // opening bracket
            self.brackets.push((vec![code], self.lineno));
        } else if CLOSING_BRACKETS.contains(&code) {
            // closing bracket
            let Some((expect, lineno)) = self.brackets.pop() else {
                return Err(self
                    .syntax(format!("Unexpected \"{}\".", char::from(code)), self.lineno)
                    .with_column(Some(self.source.column(self.cursor))));
            };
            // str_replace($openingBrackets, $closingBrackets, $expect)
            let closing: Vec<u8> = expect
                .iter()
                .map(|b| {
                    OPENING_BRACKETS
                        .iter()
                        .position(|o| o == b)
                        .map_or(*b, |i| CLOSING_BRACKETS[i])
                })
                .collect();
            if closing != [code] {
                return Err(self.unclosed(&expect, lineno));
            }
        }
        Ok(())
    }
}

fn hex_value(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => b - b'A' + 10,
    }
}

/// `preg_replace('/\s+/', ' ', $match)`.
fn collapse_spaces(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut in_space = false;
    for &b in text {
        if is_space(b) {
            if !in_space {
                out.push(b' ');
            }
            in_space = true;
        } else {
            out.push(b);
            in_space = false;
        }
    }
    out
}
