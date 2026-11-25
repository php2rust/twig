//! Port of Twig\Extension\ExtensionInterface.php and Twig\ExtensionSet.php:
//! the extensions an environment holds and the registry of their filters,
//! functions, tests, tag parsers, expression parsers and node visitors.
//!
//! `getOperators()` (deprecated since 3.21) and the undefined-callable
//! callbacks are not ported. The runtime part of an extension's callables
//! is the Rust function each callable names (see [`TwigCallable`]).
//!
//! Upstream: twig/twig src/Extension/ExtensionInterface.php src/Extension/AbstractExtension.php src/ExtensionSet.php

pub mod core;
pub mod escaper;
pub mod optimizer;

use std::{collections::HashMap, rc::Rc};

use crate::{
    callable::TwigCallable,
    expression_parser::{ExpressionParser, ExpressionParsers},
    node_visitor::NodeVisitor,
    token_parser::TokenParser,
};

/// `ExtensionInterface`: what an extension adds to the template language.
pub trait Extension {
    /// The PHP class name (`Twig\Extension\CoreExtension`).
    fn name(&self) -> &str;

    fn token_parsers(&self) -> Vec<Rc<dyn TokenParser>> {
        Vec::new()
    }

    fn filters(&self) -> Vec<TwigCallable> {
        Vec::new()
    }

    fn functions(&self) -> Vec<TwigCallable> {
        Vec::new()
    }

    fn tests(&self) -> Vec<TwigCallable> {
        Vec::new()
    }

    fn expression_parsers(&self) -> Vec<ExpressionParser> {
        Vec::new()
    }

    /// `getNodeVisitors()`: new visitors (one set for each parser).
    fn node_visitors(&self) -> Vec<Box<dyn NodeVisitor>> {
        Vec::new()
    }

    /// `GlobalsInterface::getGlobals()`.
    fn globals(&self) -> php_value::PhpArray {
        php_value::PhpArray::new()
    }
}

/// Callables by name, in PHP array order: a later definition with the
/// same name replaces the earlier one in its place.
#[derive(Default)]
pub(crate) struct CallableMap {
    items: Vec<Rc<TwigCallable>>,
    by_name: HashMap<Vec<u8>, usize>,
    /// Names with `*`: the pattern parts and the callable.
    dynamic: Vec<(Vec<Vec<u8>>, Rc<TwigCallable>)>,
}

impl CallableMap {
    fn add(&mut self, callable: TwigCallable) {
        let name = callable.name().to_vec();
        let callable = Rc::new(callable);
        if name.contains(&b'*') {
            let parts: Vec<Vec<u8>> = name.split(|&b| b == b'*').map(<[u8]>::to_vec).collect();
            if let Some(slot) = self.dynamic.iter_mut().find(|(p, _)| *p == parts) {
                slot.1 = callable.clone();
            } else {
                self.dynamic.push((parts, callable.clone()));
            }
        }
        if let Some(&i) = self.by_name.get(&name) {
            self.items[i] = callable;
        } else {
            self.by_name.insert(name, self.items.len());
            self.items.push(callable);
        }
    }

    /// `getFilter()` / `getFunction()` / `getTest()`.
    pub(crate) fn get(&self, name: &[u8]) -> Option<Rc<TwigCallable>> {
        if let Some(&i) = self.by_name.get(name) {
            return Some(self.items[i].clone());
        }
        for (parts, callable) in &self.dynamic {
            if let Some(matches) = match_dynamic(parts, name) {
                return Some(Rc::new(callable.with_dynamic_arguments(
                    name,
                    callable.name(),
                    matches,
                )));
            }
        }
        None
    }

    /// The names, in order (`array_keys()`).
    pub(crate) fn names(&self) -> Vec<Vec<u8>> {
        self.items.iter().map(|c| c.name().to_vec()).collect()
    }
}

/// `preg_match('#^a(.*?)b(.*?)c$#', $name)`: the captures, or `None`.
fn match_dynamic(parts: &[Vec<u8>], name: &[u8]) -> Option<Vec<Vec<u8>>> {
    let (first, rest) = parts.split_first()?;
    let tail = name.strip_prefix(first.as_slice())?;
    let mut captures = Vec::new();
    if match_rest(rest, tail, &mut captures) {
        Some(captures)
    } else {
        None
    }
}

/// Lazy `(.*?)` matching, with backtracking, as PCRE does it.
fn match_rest(parts: &[Vec<u8>], text: &[u8], captures: &mut Vec<Vec<u8>>) -> bool {
    let Some((literal, rest)) = parts.split_first() else {
        return text.is_empty();
    };
    for len in 0..=text.len() {
        if text[len..].starts_with(literal) {
            captures.push(text[..len].to_vec());
            if match_rest(rest, &text[len + literal.len()..], captures) {
                return true;
            }
            captures.pop();
        }
    }
    false
}

/// `ExtensionSet`, initialized: everything the extensions define.
pub struct ExtensionSet {
    extensions: Vec<Rc<dyn Extension>>,
    pub(crate) filters: CallableMap,
    pub(crate) functions: CallableMap,
    pub(crate) tests: CallableMap,
    token_parsers: Vec<(String, Rc<dyn TokenParser>)>,
    expression_parsers: ExpressionParsers,
}

impl ExtensionSet {
    /// `initExtensions()` over these extensions, in order.
    #[must_use]
    pub fn new(extensions: Vec<Rc<dyn Extension>>) -> Self {
        let mut set = Self {
            extensions: Vec::new(),
            filters: CallableMap::default(),
            functions: CallableMap::default(),
            tests: CallableMap::default(),
            token_parsers: Vec::new(),
            expression_parsers: ExpressionParsers::default(),
        };
        for extension in extensions {
            for filter in extension.filters() {
                set.filters.add(filter);
            }
            for function in extension.functions() {
                set.functions.add(function);
            }
            for test in extension.tests() {
                set.tests.add(test);
            }
            for parser in extension.token_parsers() {
                let tag = parser.tag().to_owned();
                if let Some(slot) = set.token_parsers.iter_mut().find(|(t, _)| *t == tag) {
                    slot.1 = parser;
                } else {
                    set.token_parsers.push((tag, parser));
                }
            }
            set.expression_parsers.add(extension.expression_parsers());
            set.extensions.push(extension);
        }
        set
    }

    /// `hasExtension()`, by class name.
    pub fn has_extension(&self, name: &str) -> bool {
        self.extensions.iter().any(|e| e.name() == name)
    }

    pub fn filter(&self, name: &[u8]) -> Option<Rc<TwigCallable>> {
        self.filters.get(name)
    }

    pub fn function(&self, name: &[u8]) -> Option<Rc<TwigCallable>> {
        self.functions.get(name)
    }

    pub fn test(&self, name: &[u8]) -> Option<Rc<TwigCallable>> {
        self.tests.get(name)
    }

    /// `getTokenParser()`.
    pub fn token_parser(&self, tag: &[u8]) -> Option<Rc<dyn TokenParser>> {
        self.token_parsers
            .iter()
            .find(|(t, _)| t.as_bytes() == tag)
            .map(|(_, p)| p.clone())
    }

    /// The tags, in order.
    pub fn token_parser_tags(&self) -> Vec<Vec<u8>> {
        self.token_parsers
            .iter()
            .map(|(t, _)| t.as_bytes().to_vec())
            .collect()
    }

    /// `getExpressionParsers()`.
    pub fn expression_parsers(&self) -> &ExpressionParsers {
        &self.expression_parsers
    }

    /// `getGlobals()`: the extensions' globals merged in order.
    pub fn globals(&self) -> php_value::PhpArray {
        let mut globals = php_value::PhpArray::new();
        for extension in &self.extensions {
            for (key, value) in extension.globals().iter() {
                globals.insert(key.clone(), value.clone());
            }
        }
        globals
    }

    /// `getNodeVisitors()`: new visitors, in extension order.
    pub fn node_visitors(&self) -> Vec<Box<dyn NodeVisitor>> {
        self.extensions
            .iter()
            .flat_map(|e| e.node_visitors())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(pattern: &str) -> Vec<Vec<u8>> {
        pattern.split('*').map(|p| p.as_bytes().to_vec()).collect()
    }

    #[test]
    fn dynamic_names_match_lazily() {
        assert_eq!(
            match_dynamic(&parts("foo_*_bar"), b"foo_a_b_bar"),
            Some(vec![b"a_b".to_vec()])
        );
        assert_eq!(
            match_dynamic(&parts("*_*"), b"a_b_c"),
            Some(vec![b"a".to_vec(), b"b_c".to_vec()])
        );
        assert_eq!(match_dynamic(&parts("foo_*"), b"bar_x"), None);
        assert_eq!(match_dynamic(&parts("*"), b""), Some(vec![Vec::new()]));
    }
}
