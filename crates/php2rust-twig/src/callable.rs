//! Port of Twig\AbstractTwigCallable.php, TwigFilter.php, TwigFunction.php,
//! TwigTest.php and DeprecatedCallableInfo.php: the definitions of the
//! filters, functions and tests a template can call.
//!
//! The PHP callable is kept as a name (`callable`) that the emitter maps to
//! a Rust implementation; the parser only needs to know if there is one.
//! `parser_callable` and `is_safe_callback` are Rust functions.
//!
//! Upstream: twig/twig src/AbstractTwigCallable.php src/TwigFilter.php src/TwigFunction.php src/TwigTest.php src/DeprecatedCallableInfo.php

use std::fmt;

use symfony_deprecation_contracts::trigger_deprecation;

use crate::{
    error::Error,
    node::{Node, NodeClass},
    parser::Parser,
};

/// Which of `TwigFilter`, `TwigFunction` and `TwigTest` a callable is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallableType {
    Filter,
    Function,
    Test,
}

impl CallableType {
    /// `getType()`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Filter => "filter",
            Self::Function => "function",
            Self::Test => "test",
        }
    }

    /// The PHP class of the callable object.
    #[must_use]
    pub fn php_class(self) -> &'static str {
        match self {
            Self::Filter => "Twig\\TwigFilter",
            Self::Function => "Twig\\TwigFunction",
            Self::Test => "Twig\\TwigTest",
        }
    }
}

/// A `parser_callable` option: `($parser, $fakeNode, $args, $line)`.
pub type ParserCallable = fn(&mut Parser, &Node, &Node, i64) -> Result<Node, Error>;

/// An `is_safe_callback` option: the safe strategies for the arguments.
pub type SafeCallback = fn(&Node) -> Option<Vec<Vec<u8>>>;

/// `DeprecatedCallableInfo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeprecatedCallableInfo {
    package: String,
    version: String,
    alt_name: Option<String>,
    alt_package: Option<String>,
    alt_version: Option<String>,
    callable_type: CallableType,
    name: Vec<u8>,
}

impl DeprecatedCallableInfo {
    #[must_use]
    pub fn new(package: &str, version: &str) -> Self {
        Self {
            package: package.to_owned(),
            version: version.to_owned(),
            alt_name: None,
            alt_package: None,
            alt_version: None,
            callable_type: CallableType::Function,
            name: Vec::new(),
        }
    }

    #[must_use]
    pub fn alternative(
        mut self,
        name: Option<&str>,
        package: Option<&str>,
        version: Option<&str>,
    ) -> Self {
        self.alt_name = name.map(str::to_owned);
        self.alt_package = package.map(str::to_owned);
        self.alt_version = version.map(str::to_owned);
        self
    }

    /// `triggerDeprecation()`.
    pub fn trigger_deprecation(&self, file: Option<&[u8]>, line: Option<i64>) {
        let kind = self.callable_type.as_str();
        let mut message =
            format!("Twig {}{} \"", kind[..1].to_uppercase(), &kind[1..]).into_bytes();
        message.extend_from_slice(&self.name);
        message.extend_from_slice(b"\" is deprecated");
        if let Some(alt) = self
            .alt_name
            .as_deref()
            .filter(|s| !s.is_empty() && *s != "0")
        {
            message.extend_from_slice(format!("; use \"{alt}\"").as_bytes());
            if let Some(p) = self
                .alt_package
                .as_deref()
                .filter(|s| !s.is_empty() && *s != "0")
            {
                message.extend_from_slice(format!(" from the \"{p}\" package").as_bytes());
            }
            if let Some(v) = self
                .alt_version
                .as_deref()
                .filter(|s| !s.is_empty() && *s != "0")
            {
                message.extend_from_slice(format!(" (available since version {v})").as_bytes());
            }
            message.extend_from_slice(b" instead");
        }
        if let Some(file) = file.filter(|f| !f.is_empty() && *f != b"0") {
            message.extend_from_slice(b" in ");
            message.extend_from_slice(file);
            if let Some(line) = line.filter(|l| *l != 0) {
                message.extend_from_slice(format!(" at line {line}").as_bytes());
            }
        }
        message.push(b'.');
        trigger_deprecation(self.package.as_bytes(), self.version.as_bytes(), &message);
    }
}

/// A `TwigFilter`, `TwigFunction` or `TwigTest` with its options (PHP's
/// options array, one field per option).
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct TwigCallable {
    callable_type: CallableType,
    name: Vec<u8>,
    dynamic_name: Vec<u8>,
    /// The PHP callable, by name (`Twig\Extension\CoreExtension::default`,
    /// `abs`), or `None` for a callable compiled by its node class.
    callable: Option<String>,
    arguments: Vec<Vec<u8>>,
    pub(crate) needs_environment: bool,
    pub(crate) needs_context: bool,
    pub(crate) needs_charset: bool,
    pub(crate) needs_is_sandboxed: bool,
    pub(crate) is_variadic: bool,
    pub(crate) always_allowed_in_sandbox: bool,
    pub(crate) deprecation_info: Option<DeprecatedCallableInfo>,
    pub(crate) node_class: NodeClass,
    pub(crate) is_safe: Option<Vec<Vec<u8>>>,
    pub(crate) is_safe_callback: Option<SafeCallback>,
    pub(crate) pre_escape: Option<Vec<u8>>,
    pub(crate) preserves_safety: Option<Vec<Vec<u8>>>,
    pub(crate) parser_callable: Option<ParserCallable>,
    pub(crate) one_mandatory_argument: bool,
    rust: Option<std::rc::Rc<RustCallable>>,
}

/// The Rust function that the compiler calls for a callable: a path to a
/// `fn(&twig::runtime::Call, Vec<Value>) -> twig::runtime::R`, and the
/// PHP parameters of the callable (used to bind named arguments).
#[derive(Debug, Clone)]
pub struct RustCallable {
    pub path: String,
    pub parameters: Vec<crate::util::callable_arguments::Parameter>,
}

impl TwigCallable {
    fn new(callable_type: CallableType, name: &[u8], callable: Option<&str>) -> Self {
        Self {
            callable_type,
            name: name.to_vec(),
            dynamic_name: name.to_vec(),
            callable: callable.map(str::to_owned),
            arguments: Vec::new(),
            needs_environment: false,
            needs_context: false,
            needs_charset: false,
            needs_is_sandboxed: false,
            is_variadic: false,
            always_allowed_in_sandbox: false,
            deprecation_info: None,
            node_class: match callable_type {
                CallableType::Filter => NodeClass::FilterExpression,
                CallableType::Function => NodeClass::FunctionExpression,
                CallableType::Test => NodeClass::TestExpression,
            },
            is_safe: None,
            is_safe_callback: None,
            pre_escape: None,
            preserves_safety: None,
            parser_callable: None,
            one_mandatory_argument: false,
            rust: None,
        }
    }

    /// Sets the Rust function that compiled templates call (this port
    /// does not call PHP callables).
    #[must_use]
    pub fn rust(
        mut self,
        path: &str,
        parameters: Vec<crate::util::callable_arguments::Parameter>,
    ) -> Self {
        self.rust = Some(std::rc::Rc::new(RustCallable {
            path: path.to_owned(),
            parameters,
        }));
        self
    }

    /// The Rust function set with [`Self::rust`].
    #[must_use]
    pub fn rust_callable(&self) -> Option<&RustCallable> {
        self.rust.as_deref()
    }

    /// `new TwigFilter($name, $callable)`.
    pub fn filter(name: impl AsRef<[u8]>, callable: Option<&str>) -> Self {
        Self::new(CallableType::Filter, name.as_ref(), callable)
    }

    /// `new TwigFunction($name, $callable)`.
    pub fn function(name: impl AsRef<[u8]>, callable: Option<&str>) -> Self {
        Self::new(CallableType::Function, name.as_ref(), callable)
    }

    /// `new TwigTest($name, $callable)`.
    pub fn test(name: impl AsRef<[u8]>, callable: Option<&str>) -> Self {
        Self::new(CallableType::Test, name.as_ref(), callable)
    }

    // options, as builder methods

    #[must_use]
    pub fn needs_environment(mut self) -> Self {
        self.needs_environment = true;
        self
    }

    #[must_use]
    pub fn needs_context(mut self) -> Self {
        self.needs_context = true;
        self
    }

    #[must_use]
    pub fn needs_charset(mut self) -> Self {
        self.needs_charset = true;
        self
    }

    #[must_use]
    pub fn needs_is_sandboxed(mut self) -> Self {
        self.needs_is_sandboxed = true;
        self
    }

    #[must_use]
    pub fn variadic(mut self) -> Self {
        self.is_variadic = true;
        self
    }

    #[must_use]
    pub fn always_allowed_in_sandbox(mut self) -> Self {
        self.always_allowed_in_sandbox = true;
        self
    }

    #[must_use]
    pub fn deprecated(mut self, mut info: DeprecatedCallableInfo) -> Self {
        info.name.clone_from(&self.name);
        info.callable_type = self.callable_type;
        self.deprecation_info = Some(info);
        self
    }

    #[must_use]
    pub fn node_class(mut self, class: NodeClass) -> Self {
        self.node_class = class;
        self
    }

    #[must_use]
    pub fn safe(mut self, strategies: &[&str]) -> Self {
        self.is_safe = Some(strategies.iter().map(|s| s.as_bytes().to_vec()).collect());
        self
    }

    #[must_use]
    pub fn safe_callback(mut self, callback: SafeCallback) -> Self {
        self.is_safe_callback = Some(callback);
        self
    }

    #[must_use]
    pub fn pre_escape(mut self, strategy: &str) -> Self {
        self.pre_escape = Some(strategy.as_bytes().to_vec());
        self
    }

    #[must_use]
    pub fn preserves_safety(mut self, strategies: &[&str]) -> Self {
        self.preserves_safety = Some(strategies.iter().map(|s| s.as_bytes().to_vec()).collect());
        self
    }

    #[must_use]
    pub fn parser_callable(mut self, callable: ParserCallable) -> Self {
        self.parser_callable = Some(callable);
        self
    }

    #[must_use]
    pub fn one_mandatory_argument(mut self) -> Self {
        self.one_mandatory_argument = true;
        self
    }

    // getters

    /// `getType()`.
    #[must_use]
    pub fn callable_type(&self) -> CallableType {
        self.callable_type
    }

    /// `getName()`.
    #[must_use]
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    /// `getDynamicName()`.
    #[must_use]
    pub fn dynamic_name(&self) -> &[u8] {
        &self.dynamic_name
    }

    /// `getCallable()`, by name.
    #[must_use]
    pub fn callable(&self) -> Option<&str> {
        self.callable.as_deref()
    }

    /// `getArguments()`: the parts a dynamic name matched.
    #[must_use]
    pub fn arguments(&self) -> &[Vec<u8>] {
        &self.arguments
    }

    /// `getNodeClass()`.
    #[must_use]
    pub fn get_node_class(&self) -> NodeClass {
        self.node_class
    }

    /// `needsCharset()` (always false for a test).
    #[must_use]
    pub fn get_needs_charset(&self) -> bool {
        self.callable_type != CallableType::Test && self.needs_charset
    }

    /// `needsEnvironment()` (always false for a test).
    #[must_use]
    pub fn get_needs_environment(&self) -> bool {
        self.callable_type != CallableType::Test && self.needs_environment
    }

    /// `needsContext()` (always false for a test).
    #[must_use]
    pub fn get_needs_context(&self) -> bool {
        self.callable_type != CallableType::Test && self.needs_context
    }

    /// `needsIsSandboxed()`.
    #[must_use]
    pub fn get_needs_is_sandboxed(&self) -> bool {
        self.needs_is_sandboxed
    }

    /// `isVariadic()`.
    #[must_use]
    pub fn is_variadic(&self) -> bool {
        self.is_variadic
    }

    /// `isAlwaysAllowedInSandbox()`.
    #[must_use]
    pub fn is_always_allowed_in_sandbox(&self) -> bool {
        self.always_allowed_in_sandbox
    }

    /// `isDeprecated()`.
    #[must_use]
    pub fn is_deprecated(&self) -> bool {
        self.deprecation_info.is_some()
    }

    /// `triggerDeprecation()`.
    pub fn trigger_deprecation(&self, file: Option<&[u8]>, line: Option<i64>) {
        if let Some(info) = &self.deprecation_info {
            info.trigger_deprecation(file, line);
        }
    }

    /// `getParserCallable()` (functions only).
    #[must_use]
    pub fn get_parser_callable(&self) -> Option<ParserCallable> {
        self.parser_callable
    }

    /// `hasOneMandatoryArgument()` (tests only).
    #[must_use]
    pub fn has_one_mandatory_argument(&self) -> bool {
        self.one_mandatory_argument
    }

    /// `getSafe()` (filters and functions).
    #[must_use]
    pub fn get_safe(&self, arguments: &Node) -> Option<Vec<Vec<u8>>> {
        if let Some(safe) = &self.is_safe {
            return Some(safe.clone());
        }
        if let Some(callback) = self.is_safe_callback {
            return callback(arguments);
        }
        Some(Vec::new())
    }

    /// `getPreservesSafety()` (filters).
    #[must_use]
    pub fn get_preserves_safety(&self) -> Vec<Vec<u8>> {
        self.preserves_safety.clone().unwrap_or_default()
    }

    /// `getPreEscape()` (filters).
    #[must_use]
    pub fn get_pre_escape(&self) -> Option<&[u8]> {
        self.pre_escape.as_deref()
    }

    /// `withDynamicArguments()`.
    #[must_use]
    pub fn with_dynamic_arguments(
        &self,
        name: &[u8],
        dynamic_name: &[u8],
        arguments: Vec<Vec<u8>>,
    ) -> Self {
        let mut new = self.clone();
        new.name = name.to_vec();
        new.dynamic_name = dynamic_name.to_vec();
        new.arguments = arguments;
        new
    }

    /// `getMinimalNumberOfRequiredArguments()`.
    #[must_use]
    pub fn minimal_number_of_required_arguments(&self) -> usize {
        usize::from(self.needs_charset)
            + usize::from(self.needs_environment)
            + usize::from(self.needs_context)
            + usize::from(self.needs_is_sandboxed)
            + self.arguments.len()
            + usize::from(self.callable_type != CallableType::Function)
    }
}

/// `__toString()`: `Twig\TwigFilter(name)`.
impl fmt::Display for TwigCallable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}({})",
            self.callable_type.php_class(),
            String::from_utf8_lossy(&self.name)
        )
    }
}
