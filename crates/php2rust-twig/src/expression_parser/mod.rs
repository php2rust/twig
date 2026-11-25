//! Port of `src/ExpressionParser`: the expression parser descriptions and
//! their registry.
//!
//! PHP has one class per parser; here [`ExpressionParser`] holds the
//! common data (name, precedence, associativity, aliases) and a
//! [`ParserKind`] that selects the `parse()` code in [`infix`] or
//! [`prefix`]. A parser's identity (PHP's object identity) is its index in
//! [`ExpressionParsers`].
//!
//! Upstream: twig/twig src/ExpressionParser/AbstractExpressionParser.php src/ExpressionParser/ExpressionParserInterface.php src/ExpressionParser/ExpressionParserType.php src/ExpressionParser/ExpressionParsers.php src/ExpressionParser/InfixAssociativity.php src/ExpressionParser/PrecedenceChange.php src/ExpressionParser/InfixExpressionParserInterface.php src/ExpressionParser/PrefixExpressionParserInterface.php src/ExpressionParser/ExpressionParserDescriptionInterface.php

pub mod infix;
pub mod prefix;

use std::{cell::OnceCell, fmt, rc::Rc};

use symfony_deprecation_contracts::trigger_deprecation;

use crate::node::NodeClass;

/// `InfixAssociativity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Associativity {
    Left,
    Right,
}

/// `ExpressionParserType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExpressionParserType {
    Prefix,
    Infix,
}

impl ExpressionParserType {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prefix => "prefix",
            Self::Infix => "infix",
        }
    }
}

/// `PrecedenceChange`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrecedenceChange {
    package: String,
    version: String,
    new_precedence: i64,
}

impl PrecedenceChange {
    #[must_use]
    pub fn new(package: &str, version: &str, new_precedence: i64) -> Self {
        Self {
            package: package.to_owned(),
            version: version.to_owned(),
            new_precedence,
        }
    }

    #[must_use]
    pub fn package(&self) -> &str {
        &self.package
    }

    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    #[must_use]
    pub fn new_precedence(&self) -> i64 {
        self.new_precedence
    }
}

/// Which PHP parser class an [`ExpressionParser`] is, with its own data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserKind {
    /// `UnaryOperatorExpressionParser`.
    Unary {
        class: NodeClass,
        operand_precedence: Option<i64>,
    },
    /// `BinaryOperatorExpressionParser`.
    Binary { class: NodeClass },
    /// `AssignmentExpressionParser`.
    Assignment,
    /// `ConditionalTernaryExpressionParser`.
    ConditionalTernary,
    /// `IsExpressionParser`.
    Is,
    /// `IsNotExpressionParser`.
    IsNot,
    /// `FilterExpressionParser`.
    Filter,
    /// `FunctionExpressionParser`.
    Function,
    /// `DotExpressionParser`.
    Dot,
    /// `SquareBracketExpressionParser`.
    SquareBracket,
    /// `GroupingExpressionParser`.
    Grouping,
    /// `ArrowExpressionParser`.
    Arrow,
    /// `LiteralExpressionParser`.
    Literal,
}

impl ParserKind {
    /// The PHP class (short name), as `getByClass()` keys it.
    #[must_use]
    pub fn php_class(self) -> &'static str {
        match self {
            Self::Unary { .. } => "UnaryOperatorExpressionParser",
            Self::Binary { .. } => "BinaryOperatorExpressionParser",
            Self::Assignment => "AssignmentExpressionParser",
            Self::ConditionalTernary => "ConditionalTernaryExpressionParser",
            Self::Is => "IsExpressionParser",
            Self::IsNot => "IsNotExpressionParser",
            Self::Filter => "FilterExpressionParser",
            Self::Function => "FunctionExpressionParser",
            Self::Dot => "DotExpressionParser",
            Self::SquareBracket => "SquareBracketExpressionParser",
            Self::Grouping => "GroupingExpressionParser",
            Self::Arrow => "ArrowExpressionParser",
            Self::Literal => "LiteralExpressionParser",
        }
    }

    /// `ExpressionParserType::getType()`.
    #[must_use]
    pub fn parser_type(self) -> ExpressionParserType {
        match self {
            Self::Unary { .. } | Self::Grouping | Self::Literal => ExpressionParserType::Prefix,
            _ => ExpressionParserType::Infix,
        }
    }
}

/// An expression parser: `ExpressionParserInterface`.
#[derive(Debug, Clone)]
pub struct ExpressionParser {
    kind: ParserKind,
    name: String,
    precedence: i64,
    precedence_change: Option<PrecedenceChange>,
    associativity: Associativity,
    description: String,
    aliases: Vec<String>,
}

impl ExpressionParser {
    /// `new UnaryOperatorExpressionParser($class, $name, $precedence)`.
    #[must_use]
    pub fn unary(class: NodeClass, name: &str, precedence: i64) -> Self {
        Self {
            kind: ParserKind::Unary {
                class,
                operand_precedence: None,
            },
            name: name.to_owned(),
            precedence,
            precedence_change: None,
            associativity: Associativity::Left,
            description: String::new(),
            aliases: Vec::new(),
        }
    }

    /// `new BinaryOperatorExpressionParser($class, $name, $precedence, $associativity)`.
    #[must_use]
    pub fn binary(
        class: NodeClass,
        name: &str,
        precedence: i64,
        associativity: Associativity,
    ) -> Self {
        Self {
            kind: ParserKind::Binary { class },
            name: name.to_owned(),
            precedence,
            precedence_change: None,
            associativity,
            description: String::new(),
            aliases: Vec::new(),
        }
    }

    /// One of the core parsers with a class of its own.
    #[must_use]
    pub fn builtin(kind: ParserKind) -> Self {
        let (name, precedence, associativity, description, aliases, change): (
            &str,
            i64,
            Associativity,
            &str,
            &[&str],
            Option<PrecedenceChange>,
        ) = match kind {
            ParserKind::Assignment => (
                "=",
                0,
                Associativity::Right,
                "Assignment operator",
                &[],
                None,
            ),
            ParserKind::ConditionalTernary => (
                "?",
                0,
                Associativity::Left,
                "Conditional operator (a ? b : c)",
                &[],
                None,
            ),
            ParserKind::Is => ("is", 100, Associativity::Left, "Twig tests", &[], None),
            ParserKind::IsNot => ("is not", 100, Associativity::Left, "Twig tests", &[], None),
            ParserKind::Filter => (
                "|",
                512,
                Associativity::Left,
                "Twig filter call",
                &[],
                Some(PrecedenceChange::new("twig/twig", "3.21", 300)),
            ),
            ParserKind::Function => (
                "(",
                512,
                Associativity::Left,
                "Twig function call",
                &[],
                None,
            ),
            ParserKind::Dot => (
                ".",
                512,
                Associativity::Left,
                "Get an attribute on a variable",
                &["?."],
                None,
            ),
            ParserKind::SquareBracket => ("[", 512, Associativity::Left, "Array access", &[], None),
            ParserKind::Grouping => (
                "(",
                0,
                Associativity::Left,
                "Explicit group expression (a)",
                &[],
                None,
            ),
            ParserKind::Arrow => (
                "=>",
                250,
                Associativity::Left,
                "Arrow function (x => expr)",
                &[],
                None,
            ),
            ParserKind::Literal => (
                "literal",
                0,
                Associativity::Left,
                "A literal value (boolean, string, number, sequence, mapping, ...)",
                &[],
                None,
            ),
            ParserKind::Unary { .. } | ParserKind::Binary { .. } => {
                ("", 0, Associativity::Left, "", &[], None)
            }
        };
        Self {
            kind,
            name: name.to_owned(),
            precedence,
            precedence_change: change,
            associativity,
            description: description.to_owned(),
            aliases: aliases.iter().map(|&a| a.to_owned()).collect(),
        }
    }

    #[must_use]
    pub fn with_precedence_change(mut self, change: Option<PrecedenceChange>) -> Self {
        self.precedence_change = change;
        self
    }

    #[must_use]
    pub fn with_description(mut self, description: &str) -> Self {
        description.clone_into(&mut self.description);
        self
    }

    #[must_use]
    pub fn with_aliases(mut self, aliases: &[&str]) -> Self {
        self.aliases = aliases.iter().map(|&a| a.to_owned()).collect();
        self
    }

    /// The `operandPrecedence` of a unary operator parser.
    #[must_use]
    pub fn with_operand_precedence(mut self, precedence: i64) -> Self {
        if let ParserKind::Unary { class, .. } = self.kind {
            self.kind = ParserKind::Unary {
                class,
                operand_precedence: Some(precedence),
            };
        }
        self
    }

    #[must_use]
    pub fn kind(&self) -> ParserKind {
        self.kind
    }

    /// `getName()`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `getPrecedence()`.
    #[must_use]
    pub fn precedence(&self) -> i64 {
        self.precedence
    }

    /// `getPrecedenceChange()`.
    #[must_use]
    pub fn precedence_change(&self) -> Option<&PrecedenceChange> {
        self.precedence_change.as_ref()
    }

    /// `getAssociativity()`.
    #[must_use]
    pub fn associativity(&self) -> Associativity {
        self.associativity
    }

    /// `getDescription()`.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// `getAliases()`.
    #[must_use]
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }

    /// `getOperatorTokens()`: the name and the aliases (none for the
    /// literal parser).
    pub fn operator_tokens(&self) -> Vec<&str> {
        if self.kind == ParserKind::Literal {
            return Vec::new();
        }
        std::iter::once(self.name.as_str())
            .chain(self.aliases.iter().map(String::as_str))
            .collect()
    }

    /// The precedence for the right operand of a binary parser.
    pub(crate) fn right_precedence(&self) -> i64 {
        if self.associativity == Associativity::Left {
            self.precedence + 1
        } else {
            self.precedence
        }
    }
}

/// `__toString()`: `prefix(not)`, `infix(+)`.
impl fmt::Display for ExpressionParser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.kind.parser_type().as_str(), self.name)
    }
}

/// A parser's identity in its registry.
pub type ParserId = usize;

/// `ExpressionParsers`: the registry, by operator token and by class.
#[derive(Debug, Default)]
pub struct ExpressionParsers {
    parsers: Vec<Rc<ExpressionParser>>,
    /// The token maps of the two interfaces, in first-use order; each in
    /// PHP array order.
    by_name: Vec<(ExpressionParserType, Vec<(String, ParserId)>)>,
    by_class: Vec<(&'static str, ParserId)>,
    precedence_changes: OnceCell<Vec<(ParserId, Vec<ParserId>)>>,
}

impl ExpressionParsers {
    /// `add()`.
    pub fn add(&mut self, parsers: Vec<ExpressionParser>) {
        for parser in parsers {
            if parser.precedence > 512 || parser.precedence < 0 {
                trigger_deprecation(
                    b"twig/twig",
                    b"3.21",
                    format!(
                        "Precedence for \"{}\" must be between 0 and 512, got {}.",
                        parser.name, parser.precedence
                    )
                    .as_bytes(),
                );
            }
            let id = self.parsers.len();
            let ty = parser.kind.parser_type();
            let class = parser.kind.php_class();
            if let Some(slot) = self.by_class.iter_mut().find(|(c, _)| *c == class) {
                slot.1 = id;
            } else {
                self.by_class.push((class, id));
            }
            let tokens: Vec<String> = parser
                .operator_tokens()
                .iter()
                .map(|&t| t.to_owned())
                .collect();
            if !tokens.is_empty() {
                let index = if let Some(i) = self.by_name.iter().position(|(t, _)| *t == ty) {
                    i
                } else {
                    self.by_name.push((ty, Vec::new()));
                    self.by_name.len() - 1
                };
                let group = &mut self.by_name[index].1;
                for token in tokens {
                    if let Some(slot) = group.iter_mut().find(|(t, _)| *t == token) {
                        slot.1 = id;
                    } else {
                        group.push((token, id));
                    }
                }
            }
            self.parsers.push(Rc::new(parser));
        }
        self.precedence_changes = OnceCell::new();
    }

    /// The parser with this identity.
    pub fn get(&self, id: ParserId) -> &Rc<ExpressionParser> {
        &self.parsers[id]
    }

    /// `getByClass()`, by PHP short class name.
    pub fn by_class(&self, class: &str) -> Option<ParserId> {
        self.by_class
            .iter()
            .find(|(c, _)| *c == class)
            .map(|(_, id)| *id)
    }

    /// `getByName()`.
    pub fn by_name(&self, ty: ExpressionParserType, name: &[u8]) -> Option<ParserId> {
        self.by_name
            .iter()
            .find(|(t, _)| *t == ty)?
            .1
            .iter()
            .find(|(t, _)| t.as_bytes() == name)
            .map(|(_, id)| *id)
    }

    /// `getIterator()`: each parser once, by name first, then by class.
    pub fn ids(&self) -> Vec<ParserId> {
        let mut seen = Vec::new();
        let ids = self
            .by_name
            .iter()
            .flat_map(|(_, group)| group.iter().map(|(_, id)| *id))
            .chain(self.by_class.iter().map(|(_, id)| *id));
        for id in ids {
            if !seen.contains(&id) {
                seen.push(id);
            }
        }
        seen
    }

    /// All operator tokens, in iteration order (the lexer's list).
    pub fn operator_tokens(&self) -> Vec<Vec<u8>> {
        self.ids()
            .into_iter()
            .flat_map(|id| {
                self.parsers[id]
                    .operator_tokens()
                    .into_iter()
                    .map(|t| t.as_bytes().to_vec())
            })
            .collect()
    }

    /// `getPrecedenceChanges()`: for each parser whose precedence lies
    /// strictly between the old and new precedence of a changing parser,
    /// the changing parsers; in the `WeakMap`'s insertion order.
    pub fn precedence_changes(&self) -> &[(ParserId, Vec<ParserId>)] {
        self.precedence_changes.get_or_init(|| {
            let mut changes: Vec<(ParserId, Vec<ParserId>)> = Vec::new();
            let all = self.ids();
            for &ep in &all {
                let parser = &self.parsers[ep];
                let Some(change) = &parser.precedence_change else {
                    continue;
                };
                let min = change.new_precedence.min(parser.precedence);
                let max = change.new_precedence.max(parser.precedence);
                for &e in &all {
                    let p = self.parsers[e].precedence;
                    if p > min && p < max {
                        if let Some(slot) = changes.iter_mut().find(|(id, _)| *id == e) {
                            slot.1.push(ep);
                        } else {
                            changes.push((e, vec![ep]));
                        }
                    }
                }
            }
            changes
        })
    }
}
