//! Port of Twig\Parser.php: turns a token stream into a `ModuleNode`.
//!
//! The node visitors that PHP runs at the end of `parse()` come with the
//! emitter; [`Parser::parse`] returns the tree as the parser built it.
//! `Parser::$expressionRefs` is kept on the nodes (see
//! [`Node::parsed_by`]).
//!
//! Upstream: twig/twig src/Parser.php

use std::{collections::HashMap, rc::Rc, sync::Arc};

use symfony_deprecation_contracts::trigger_deprecation;

use crate::{
    callable::TwigCallable,
    environment::Environment,
    error::{Error, ErrorKind},
    expression_parser::{ExpressionParserType, ParserId, infix, prefix},
    node::{Key, Node, NodeClass, statement},
    node_visitor::{self, NodeVisitor},
    source::Source,
    token::{Token, TokenType},
    token_stream::{TokenStream, to_line},
};

/// A tag parser's end test: the tag (for the error message) and the
/// `decide...End()` method.
#[derive(Clone, Copy)]
pub struct EndTest<'a> {
    pub tag: &'a str,
    pub test: fn(&Token) -> bool,
}

impl<'a> EndTest<'a> {
    pub fn new(tag: &'a str, test: fn(&Token) -> bool) -> Self {
        Self { tag, test }
    }
}

/// An entry of `$importedSymbols`.
#[derive(Debug, Clone)]
pub struct ImportedSymbol {
    pub name: Option<Vec<u8>>,
    pub node: Option<Node>,
}

type Scope = HashMap<(String, Vec<u8>), ImportedSymbol>;

/// The properties `parse()` saves and restores around a nested parse.
#[derive(Default)]
struct State {
    parent: Option<Node>,
    blocks: Vec<(Vec<u8>, Node)>,
    block_stack: Vec<Vec<u8>>,
    documentation_targets: Vec<Option<Node>>,
    macros: Vec<(Vec<u8>, Node)>,
    imported_symbols: Vec<Scope>,
    traits: Vec<Node>,
    embedded_templates: Vec<Node>,
    ignore_unknown_twig_callables: bool,
}

/// `Parser`.
pub struct Parser<'env> {
    env: &'env Environment,
    stack: Vec<State>,
    state: State,
    stream: TokenStream,
    last_embed_index: i64,
    /// `$visitors` when set in place of the environment's.
    visitors: Option<Vec<Box<dyn NodeVisitor>>>,
}

fn line_of(token: &Token) -> i64 {
    to_line(token.line())
}

impl<'env> Parser<'env> {
    pub fn new(env: &'env Environment) -> Self {
        Self {
            env,
            stack: Vec::new(),
            state: State::default(),
            stream: TokenStream::new(
                vec![Token::new(TokenType::Eof, "", 1)],
                Arc::new(Source::new("", "")),
            ),
            last_embed_index: 0,
            visitors: None,
        }
    }

    /// Sets the node visitors in place of the environment's (upstream's
    /// tests set the private `$visitors` by reflection; an empty list
    /// parses without them).
    pub fn set_node_visitors(&mut self, visitors: Vec<Box<dyn NodeVisitor>>) {
        self.visitors = Some(visitors);
    }

    /// `getEnvironment()`.
    pub fn environment(&self) -> &'env Environment {
        self.env
    }

    /// `parse($stream)`: a template.
    pub fn parse(&mut self, stream: TokenStream) -> Result<Node, Error> {
        let previous = std::mem::replace(&mut self.stream, stream);
        let result = self.parse_current(None, false);
        self.stream = previous;
        result
    }

    /// `parse($stream, $test, $dropNeedle)` on the stream being parsed (an
    /// embedded template).
    pub fn parse_embedded(&mut self, test: EndTest<'_>, drop_needle: bool) -> Result<Node, Error> {
        self.parse_current(Some(test), drop_needle)
    }

    fn parse_current(
        &mut self,
        test: Option<EndTest<'_>>,
        drop_needle: bool,
    ) -> Result<Node, Error> {
        if self.stack.is_empty() {
            self.last_embed_index = 0;
        }
        let ignore = self.state.ignore_unknown_twig_callables;
        let saved = std::mem::replace(
            &mut self.state,
            State {
                imported_symbols: vec![Scope::new()],
                ignore_unknown_twig_callables: ignore,
                ..State::default()
            },
        );
        self.stack.push(saved);

        let body = match self.subparse(test, drop_needle) {
            Ok(body) => body,
            Err(mut e) => {
                if e.kind() == ErrorKind::Syntax {
                    if e.source_context().is_none() {
                        e.set_source_context(Some(self.stream().source_context().clone()));
                    }
                    if e.template_line() == 0 {
                        e.set_template_line(line_of(self.current_token()));
                    }
                }
                return Err(e);
            }
        };

        let body = if self.state.parent.is_some() {
            cleanup_body_for_child_templates(body)
        } else {
            body
        };

        let state = &self.state;
        let blocks = if state.blocks.is_empty() {
            Node::empty(0)
        } else {
            Node::nodes(
                state
                    .blocks
                    .iter()
                    .map(|(k, n)| (Key::from_bytes(k), n.clone()))
                    .collect(),
                0,
            )
        };
        let macros = statement::macros(
            state
                .macros
                .iter()
                .map(|(k, n)| (Key::from_bytes(k), n.clone()))
                .collect(),
        );
        let traits = if state.traits.is_empty() {
            Node::empty(0)
        } else {
            Node::list(state.traits.clone(), 0)
        };
        let embedded = if state.embedded_templates.is_empty() {
            Node::empty(0)
        } else {
            Node::list(state.embedded_templates.clone(), 0)
        };
        let node = statement::module(
            statement::body(vec![body], 0),
            state.parent.clone(),
            blocks,
            macros,
            traits,
            embedded,
            self.stream().source_context(),
        );

        let node = match self.visitors.as_mut() {
            Some(visitors) => node_visitor::traverse(self.env, visitors, node)?,
            None => self.env.traverse(node)?,
        };

        if let Some(previous) = self.stack.pop() {
            self.state = previous;
        }
        Ok(node)
    }

    /// `shouldIgnoreUnknownTwigCallables()`.
    pub fn should_ignore_unknown_twig_callables(&self) -> bool {
        self.state.ignore_unknown_twig_callables
    }

    /// `subparseIgnoreUnknownTwigCallables()`.
    pub fn subparse_ignore_unknown_twig_callables(
        &mut self,
        test: Option<EndTest<'_>>,
        drop_needle: bool,
    ) -> Result<(), Error> {
        let previous = self.state.ignore_unknown_twig_callables;
        self.state.ignore_unknown_twig_callables = true;
        let result = self.subparse(test, drop_needle);
        self.state.ignore_unknown_twig_callables = previous;
        result.map(|_| ())
    }

    /// `subparse($test, $dropNeedle)`.
    pub fn subparse(
        &mut self,
        test: Option<EndTest<'_>>,
        drop_needle: bool,
    ) -> Result<Node, Error> {
        let lineno = line_of(self.current_token());
        let mut rv: Vec<Node> = Vec::new();
        let finish = |mut rv: Vec<Node>| {
            if rv.len() == 1 {
                rv.remove(0)
            } else {
                Node::list(rv, lineno)
            }
        };
        while !self.stream().is_eof() {
            let current = self.current_token().clone();
            match current.token_type() {
                TokenType::Text => {
                    let token = self.stream_mut().next()?;
                    rv.push(statement::text(&token.value_bytes(), line_of(&token)));
                }
                TokenType::VarStart => {
                    let token = self.stream_mut().next()?;
                    let expr = self.parse_expression(0)?;
                    self.stream_mut().expect(TokenType::VarEnd)?;
                    let node = statement::print(expr, line_of(&token));
                    node.add_documentation(&[&token]);
                    rv.push(node);
                }
                TokenType::BlockStart => {
                    let start_token = self.stream_mut().next()?;
                    let token = self.current_token().clone();
                    if !token.test(TokenType::Name) {
                        return Err(Error::syntax(
                            "A block must start with a tag name.",
                            line_of(&token),
                            Some(self.stream().source_context().clone()),
                        ));
                    }
                    if let Some(test) = &test
                        && (test.test)(&token)
                    {
                        if drop_needle {
                            self.stream_mut().next()?;
                        }
                        return Ok(finish(rv));
                    }
                    let tag = token.value_bytes();
                    let Some(subparser) = self.env.token_parser(&tag) else {
                        let source = Some(self.stream().source_context().clone());
                        let mut e;
                        if let Some(test) = &test {
                            let mut message = b"Unexpected \"".to_vec();
                            message.extend_from_slice(&tag);
                            message.extend_from_slice(b"\" tag");
                            e = Error::syntax(message, line_of(&token), source);
                            e.append_message(
                                format!(
                                    " (expecting closing tag for the \"{}\" tag defined near line {lineno}).",
                                    test.tag
                                )
                                .as_bytes(),
                            );
                        } else {
                            let mut message = b"Unknown \"".to_vec();
                            message.extend_from_slice(&tag);
                            message.extend_from_slice(b"\" tag.");
                            e = Error::syntax(message, line_of(&token), source);
                            let tags = self.env.token_parser_tags();
                            let items: Vec<&[u8]> = tags.iter().map(Vec::as_slice).collect();
                            e.add_suggestions(&tag, &items);
                        }
                        return Err(e);
                    };
                    self.stream_mut().next()?;
                    let index = self.state.documentation_targets.len();
                    self.state.documentation_targets.push(None);
                    let parsed = subparser.parse(self, &token);
                    let target = self
                        .state
                        .documentation_targets
                        .get(index)
                        .cloned()
                        .flatten();
                    self.state.documentation_targets.pop();
                    let node = parsed?;
                    node.set_tag(subparser.tag())?;
                    node.add_documentation(&[&start_token]);
                    if let Some(target) = target
                        && !node.ptr_eq(&target)
                    {
                        node.move_documentation(&target);
                    }
                    rv.push(node);
                }
                _ => {
                    return Err(Error::syntax(
                        "The lexer or the parser ended up in an unsupported state.",
                        line_of(&current),
                        Some(self.stream().source_context().clone()),
                    ));
                }
            }
        }
        Ok(finish(rv))
    }

    /// `peekBlockStack()`.
    pub fn peek_block_stack(&self) -> Option<Vec<u8>> {
        self.state.block_stack.last().cloned()
    }

    /// `popBlockStack()`.
    pub fn pop_block_stack(&mut self) {
        self.state.block_stack.pop();
    }

    /// `pushBlockStack()`.
    pub fn push_block_stack(&mut self, name: &[u8]) {
        self.state.block_stack.push(name.to_vec());
    }

    /// `setBlock()`.
    pub fn set_block(&mut self, name: &[u8], value: &Node) -> Result<(), Error> {
        if let Some((_, existing)) = self.state.blocks.iter().find(|(n, _)| n == name) {
            let mut message = b"The block '".to_vec();
            message.extend_from_slice(name);
            message.extend_from_slice(
                format!("' has already been defined line {}.", existing.line()).as_bytes(),
            );
            return Err(Error::syntax(
                message,
                line_of(self.current_token()),
                existing.source_context(),
            ));
        }
        let body = statement::body(vec![value.clone()], value.line());
        self.state.blocks.push((name.to_vec(), body));
        Ok(())
    }

    /// `setDocumentationTarget()`.
    pub fn set_documentation_target(&mut self, node: &Node) -> Result<(), Error> {
        let Some(slot) = self.state.documentation_targets.last_mut() else {
            return Err(Error::logic(
                "A documentation target can only be set while parsing a tag.",
            ));
        };
        if slot.is_some() {
            return Err(Error::logic(
                "The documentation target for a tag can only be set once.",
            ));
        }
        *slot = Some(node.clone());
        Ok(())
    }

    /// `setMacro()`.
    pub fn set_macro(&mut self, name: &[u8], node: Node) {
        if let Some(slot) = self.state.macros.iter_mut().find(|(n, _)| n == name) {
            let mut message = b"Defining the macro \"".to_vec();
            message.extend_from_slice(name);
            message.extend_from_slice(
                format!(
                    "\" more than once in \"{}\" is deprecated and will throw a SyntaxError in Twig 4.0 (previous definition at line {}, new definition at line {}). The last definition is used in Twig 3.",
                    self.stream.source_context().name(),
                    slot.1.line(),
                    node.line()
                )
                .as_bytes(),
            );
            trigger_deprecation(b"twig/twig", b"3.29", &message);
            slot.1 = node;
        } else {
            self.state.macros.push((name.to_vec(), node));
        }
    }

    /// `addTrait()`.
    pub fn add_trait(&mut self, trait_node: Node) {
        self.state.traits.push(trait_node);
    }

    /// `embedTemplate()`.
    pub fn embed_template(&mut self, template: Node) {
        self.last_embed_index += 1;
        template.set_attr("index", self.last_embed_index);
        self.state.embedded_templates.push(template);
    }

    /// `addImportedSymbol()`.
    pub fn add_imported_symbol(
        &mut self,
        symbol_type: &str,
        alias: &[u8],
        name: Option<&[u8]>,
        internal_ref: Option<Node>,
    ) {
        if let Some(scope) = self.state.imported_symbols.first_mut() {
            scope.insert(
                (symbol_type.to_owned(), alias.to_vec()),
                ImportedSymbol {
                    name: name.map(<[u8]>::to_vec),
                    node: internal_ref,
                },
            );
        }
    }

    /// `getImportedSymbol()`.
    pub fn get_imported_symbol(&self, symbol_type: &str, alias: &[u8]) -> Option<ImportedSymbol> {
        let key = (symbol_type.to_owned(), alias.to_vec());
        self.state
            .imported_symbols
            .first()
            .and_then(|s| s.get(&key))
            .or_else(|| self.state.imported_symbols.last().and_then(|s| s.get(&key)))
            .cloned()
    }

    /// `isMainScope()`.
    pub fn is_main_scope(&self) -> bool {
        self.state.imported_symbols.len() == 1
    }

    /// `pushLocalScope()`.
    pub fn push_local_scope(&mut self) {
        self.state.imported_symbols.insert(0, Scope::new());
    }

    /// `popLocalScope()`.
    pub fn pop_local_scope(&mut self) {
        if !self.state.imported_symbols.is_empty() {
            self.state.imported_symbols.remove(0);
        }
    }

    /// `parseExpression($precedence)`.
    pub fn parse_expression(&mut self, precedence: i64) -> Result<Node, Error> {
        let parsers = self.env.expression_parsers();
        let token = self.current_token().clone();
        let prefix_id = if token.test(TokenType::Operator) {
            parsers.by_name(ExpressionParserType::Prefix, &token.value_bytes())
        } else {
            None
        };
        let mut expr = if let Some(id) = prefix_id {
            self.stream_mut().next()?;
            let expr = prefix::parse(parsers.get(id), self, &token)?;
            self.check_precedence_deprecations(id, &expr);
            expr
        } else {
            prefix::parse_literal(self, &token)?
        };
        loop {
            let token = self.current_token().clone();
            if !token.test(TokenType::Operator) {
                break;
            }
            let Some(id) = parsers.by_name(ExpressionParserType::Infix, &token.value_bytes())
            else {
                break;
            };
            if parsers.get(id).precedence() < precedence {
                break;
            }
            self.stream_mut().next()?;
            expr = infix::parse(parsers.get(id), self, expr, &token)?;
            self.check_precedence_deprecations(id, &expr);
        }
        Ok(expr)
    }

    /// `hasInheritance()`.
    pub fn has_inheritance(&self) -> bool {
        self.state.parent.is_some() || !self.state.traits.is_empty()
    }

    /// `setParent($parent, $throwOnMultiple)`.
    pub fn set_parent(&mut self, parent: Node, throw_on_multiple: bool) -> Result<(), Error> {
        if self.state.parent.is_some() {
            if !throw_on_multiple {
                return Ok(());
            }
            return Err(Error::syntax(
                "Multiple extends tags are forbidden.",
                parent.line(),
                parent.source_context(),
            ));
        }
        self.state.parent = Some(parent);
        Ok(())
    }

    /// `getStream()`.
    pub fn stream(&self) -> &TokenStream {
        &self.stream
    }

    /// `getStream()`, to move through it.
    pub fn stream_mut(&mut self) -> &mut TokenStream {
        &mut self.stream
    }

    /// `getCurrentToken()`.
    pub fn current_token(&self) -> &Token {
        self.stream().current()
    }

    fn source(&self) -> Arc<Source> {
        self.stream().source_context().clone()
    }

    /// `$src->getPath() ?: $src->getName()`.
    fn deprecation_file(&self) -> Vec<u8> {
        let source = self.source();
        if php_value::string::truthy(source.path()) {
            source.path().to_vec()
        } else {
            source.name().as_bytes().to_vec()
        }
    }

    fn unknown(&self, kind: &str, name: &[u8], line: i64, known: &[Vec<u8>]) -> Error {
        let mut message = b"Unknown \"".to_vec();
        message.extend_from_slice(name);
        message.extend_from_slice(format!("\" {kind}.").as_bytes());
        let mut e = Error::syntax(message, line, Some(self.source()));
        let items: Vec<&[u8]> = known.iter().map(Vec::as_slice).collect();
        e.add_suggestions(name, &items);
        e
    }

    /// `getFunction()`.
    pub fn get_function(&self, name: &[u8], line: i64) -> Result<Rc<TwigCallable>, Error> {
        let Some(function) = self.env.function(name) else {
            if self.should_ignore_unknown_twig_callables() {
                return Ok(Rc::new(TwigCallable::function(name, Some("{closure}"))));
            }
            return Err(self.unknown("function", name, line, &self.env.function_names()));
        };
        if function.is_deprecated() {
            function.trigger_deprecation(Some(&self.deprecation_file()), Some(line));
        }
        Ok(function)
    }

    /// `getFilter()`.
    pub fn get_filter(&self, name: &[u8], line: i64) -> Result<Rc<TwigCallable>, Error> {
        let Some(filter) = self.env.filter(name) else {
            if self.should_ignore_unknown_twig_callables() {
                return Ok(Rc::new(TwigCallable::filter(name, Some("{closure}"))));
            }
            return Err(self.unknown("filter", name, line, &self.env.filter_names()));
        };
        if filter.is_deprecated() {
            filter.trigger_deprecation(Some(&self.deprecation_file()), Some(line));
        }
        Ok(filter)
    }

    /// `getTest()`: reads the test name (one or two name tokens).
    pub fn get_test(&mut self, line: i64) -> Result<Rc<TwigCallable>, Error> {
        let mut name = self.stream_mut().expect(TokenType::Name)?.value_bytes();
        let test = if self.stream().test(TokenType::Name) {
            name.push(b' ');
            name.extend_from_slice(&self.current_token().value_bytes());
            let test = self.env.test(&name);
            self.stream_mut().next()?;
            test
        } else {
            self.env.test(&name)
        };
        let Some(test) = test else {
            if self.should_ignore_unknown_twig_callables() {
                return Ok(Rc::new(TwigCallable::test(&name, Some("{closure}"))));
            }
            return Err(self.unknown("test", &name, line, &self.env.test_names()));
        };
        if test.is_deprecated() {
            let current = line_of(self.current_token());
            test.trigger_deprecation(Some(&self.deprecation_file()), Some(current));
        }
        Ok(test)
    }

    fn check_precedence_deprecations(&self, parser: ParserId, expr: &Node) {
        expr.set_parsed_by(parser);
        let parsers = self.env.expression_parsers();
        let changes = parsers.precedence_changes();
        let Some((_, affecting)) = changes.iter().find(|(id, _)| *id == parser) else {
            return;
        };
        if expr.has_explicit_parentheses() {
            return;
        }
        let name = self.stream().source_context().name().to_owned();
        let trigger = |ep: ParserId, node: &Node| {
            let ep = parsers.get(ep);
            if let Some(change) = ep.precedence_change() {
                let message = format!(
                    "As the \"{}\" {} operator will change its precedence in the next major version, add explicit parentheses to avoid behavior change in \"{name}\" at line {}.",
                    ep.name(),
                    ep.kind().parser_type().as_str(),
                    node.line()
                );
                trigger_deprecation(
                    change.package().as_bytes(),
                    change.version().as_bytes(),
                    message.as_bytes(),
                );
            }
        };
        if parsers.get(parser).kind().parser_type() == ExpressionParserType::Prefix
            && let Some(node) = expr.node("node")
        {
            for (ep, list) in changes {
                if !list.contains(&parser) {
                    continue;
                }
                if node.parsed_by() == Some(*ep) {
                    trigger(parser, &node);
                }
            }
        }
        for &ep in affecting {
            for node in expr.child_nodes() {
                if node.parsed_by() == Some(ep) && !node.has_explicit_parentheses() {
                    trigger(ep, &node);
                }
            }
        }
    }
}

/// `cleanupBodyForChildTemplates()`.
fn cleanup_body_for_child_templates(body: Node) -> Node {
    if body.is(NodeClass::BlockReferenceNode) {
        return Node::empty(0);
    }
    if body.is(NodeClass::TextNode) && body.is_blank() {
        return Node::empty(0);
    }
    for (key, node) in body.children() {
        if node.is(NodeClass::BlockReferenceNode)
            || (node.is(NodeClass::TextNode) && node.is_blank())
        {
            body.remove_node(key);
        }
    }
    body
}
