//! Port of Twig\Environment.php: the options, the extensions, the lexer,
//! the node visitors, `tokenize()`, `parse()` and `compile()`, and at run
//! time the loading of compiled templates, the globals and `render()`.
//!
//! Templates are compiled to Rust ahead of time, so the loader gives
//! compiled templates ([`TemplateLoader`]); there is no cache and no
//! `auto_reload`, and `createTemplate()` cannot compile at run time.
//!
//! Upstream: twig/twig src/Environment.php

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use php_value::{PhpArray, PhpString, Value};

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
    loader::TemplateLoader,
    node::Node,
    node_visitor::{self, NodeVisitor, optimizer::OptimizerNodeVisitor},
    parser::Parser,
    runtime::{core::CoreOptions, escaper::EscaperRuntime},
    source::Source,
    template::{Context, Template, TemplateWrapper},
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

/// A loaded template: its name and its embed index.
type TemplateKey = (Vec<u8>, Option<i64>);

/// The `Environment` constructor options that change parsing.
#[derive(Clone, Debug)]
pub struct EnvironmentOptions {
    /// `autoescape`, default `'html'`.
    pub autoescape: Autoescape,
    /// `optimizations`, default -1 (`OptimizerNodeVisitor::OPTIMIZE_ALL`).
    pub optimizations: i64,
    /// `setLexer(new Lexer($env, $options))`.
    pub lexer: LexerOptions,
    /// `debug`, default false.
    pub debug: bool,
    /// `charset`, default `'UTF-8'`.
    pub charset: Vec<u8>,
    /// `strict_variables`, default false.
    pub strict_variables: bool,
}

impl Default for EnvironmentOptions {
    fn default() -> Self {
        Self {
            autoescape: Autoescape::Strategy(b"html".to_vec()),
            optimizations: -1,
            lexer: LexerOptions::default(),
            debug: false,
            charset: b"UTF-8".to_vec(),
            strict_variables: false,
        }
    }
}

/// A compiled arrow function (`x => ...`): the environment and the
/// arguments.
pub type ArrowFn = dyn Fn(&Environment, &[Value]) -> Result<Value, Error>;

/// `Environment`, for parsing.
pub struct Environment {
    /// Which environment this is (PHP compares the objects): a
    /// `TemplateWrapper` works only with the environment that made it.
    id: usize,
    extension_set: ExtensionSet,
    lexer: Lexer,
    autoescape: Autoescape,
    /// `ExtensionSet::$visitors`: one set for the environment, made at the
    /// first parse. Their state lasts from one parse to the next, as in
    /// PHP (a parse that fails in the middle of a traversal can leave the
    /// optimizer inside a loop).
    visitors: RefCell<Option<Vec<Box<dyn NodeVisitor>>>>,
    debug: bool,
    charset: Vec<u8>,
    strict_variables: bool,
    loader: RefCell<Option<Rc<dyn TemplateLoader>>>,
    loaded: RefCell<HashMap<TemplateKey, Rc<Template>>>,
    globals: RefCell<PhpArray>,
    arrows: RefCell<Vec<Rc<ArrowFn>>>,
    core: RefCell<CoreOptions>,
    escaper: RefCell<EscaperRuntime>,
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
    #[must_use]
    pub fn new(extensions: Vec<Rc<dyn Extension>>) -> Self {
        Self::build(extensions, EnvironmentOptions::default())
    }

    /// As [`Environment::new`], with `setLexer(new Lexer($env, $options))`.
    #[must_use]
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
        static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
        let mut all: Vec<Rc<dyn Extension>> = vec![
            Rc::new(CoreExtension),
            Rc::new(EscaperExtension),
            Rc::new(OptimizerExtension::new(options.optimizations)),
        ];
        all.extend(extensions);
        let extension_set = ExtensionSet::new(all);
        let operators = extension_set.expression_parsers().operator_tokens();
        let lexer = Lexer::with_operators(operators.iter().map(Vec::as_slice), options.lexer);
        let charset = match options.charset.to_ascii_uppercase() {
            c if c == b"UTF8" || c.is_empty() => b"UTF-8".to_vec(),
            _ => options.charset.clone(),
        };
        Self {
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            extension_set,
            lexer,
            autoescape: options.autoescape,
            visitors: RefCell::new(None),
            debug: options.debug,
            escaper: RefCell::new(EscaperRuntime::new(&charset)),
            charset,
            strict_variables: options.strict_variables,
            loader: RefCell::new(None),
            loaded: RefCell::new(HashMap::new()),
            globals: RefCell::new(PhpArray::new()),
            arrows: RefCell::new(Vec::new()),
            core: RefCell::new(CoreOptions::default()),
        }
    }

    /// The environment's identity, for `TemplateWrapper::isOwnedBy()`.
    #[must_use]
    pub fn id(&self) -> usize {
        self.id
    }

    /// `isDebug()`.
    pub fn is_debug(&self) -> bool {
        self.debug
    }

    /// `getCharset()`.
    pub fn charset(&self) -> &[u8] {
        &self.charset
    }

    /// `isStrictVariables()`.
    pub fn is_strict_variables(&self) -> bool {
        self.strict_variables
    }

    /// `hasExtension(SandboxExtension::class)`.
    pub fn has_sandbox(&self) -> bool {
        self.extension_set.sandbox().is_some()
    }

    /// `getExtension(SandboxExtension::class)`.
    pub fn sandbox(&self) -> Option<&crate::extension::sandbox::SandboxExtension> {
        self.extension_set.sandbox()
    }

    /// `setLoader()`: where compiled templates come from.
    pub fn set_loader(&self, loader: Rc<dyn TemplateLoader>) {
        *self.loader.borrow_mut() = Some(loader);
        self.loaded.borrow_mut().clear();
    }

    /// `getLoader()`.
    pub fn loader(&self) -> Option<Rc<dyn TemplateLoader>> {
        self.loader.borrow().clone()
    }

    /// `addGlobal()`.
    pub fn add_global(&self, name: &str, value: Value) {
        self.globals.borrow_mut().insert(name, value);
    }

    /// `getGlobals()`: the extensions' globals, then the ones added.
    pub fn globals(&self) -> PhpArray {
        let mut globals = self.extension_set.globals();
        for (key, value) in self.globals.borrow().iter() {
            globals.insert(key.clone(), value.clone());
        }
        globals
    }

    /// `getExtension(CoreExtension::class)`'s date and number settings.
    pub fn core_options(&self) -> std::cell::Ref<'_, CoreOptions> {
        self.core.borrow()
    }

    /// `getExtension(CoreExtension::class)`'s settings, to change them.
    pub fn core_options_mut(&self) -> std::cell::RefMut<'_, CoreOptions> {
        self.core.borrow_mut()
    }

    /// `getRuntime(EscaperRuntime::class)`.
    pub fn escaper(&self) -> std::cell::Ref<'_, EscaperRuntime> {
        self.escaper.borrow()
    }

    /// `getRuntime(EscaperRuntime::class)`, to configure it.
    pub fn escaper_mut(&self) -> std::cell::RefMut<'_, EscaperRuntime> {
        self.escaper.borrow_mut()
    }

    /// Keeps a compiled arrow function for the rest of the environment's
    /// life and gives it as a `\Closure` value.
    pub fn register_arrow(&self, arrow: Rc<ArrowFn>) -> Value {
        let mut arrows = self.arrows.borrow_mut();
        arrows.push(arrow);
        crate::runtime::core::Arrow::value(arrows.len() - 1)
    }

    /// The arrow function a [`crate::runtime::core::Arrow`] value stands for.
    pub fn arrow(&self, id: usize) -> Option<Rc<ArrowFn>> {
        self.arrows.borrow().get(id).cloned()
    }

    /// `loadTemplate($cls, $name, $index)`: the loaded instance of a
    /// compiled template.
    pub fn load_template(&self, name: &[u8], index: Option<i64>) -> Result<Rc<Template>, Error> {
        let key = (name.to_vec(), index);
        if let Some(template) = self.loaded.borrow().get(&key) {
            return Ok(Rc::clone(template));
        }
        let loader = self.loader().ok_or_else(|| {
            Error::loader(
                format!(
                    "Template \"{}\" is not defined.",
                    String::from_utf8_lossy(name)
                ),
                -1,
                None,
            )
        })?;
        let compiled = loader.compiled(name, index)?;
        let template = Template::new(compiled, self)?;
        self.loaded.borrow_mut().insert(key, Rc::clone(&template));
        Ok(template)
    }

    /// `load($name)`.
    pub fn load(&self, name: &[u8]) -> Result<TemplateWrapper, Error> {
        let template = self.load_template(name, None)?;
        Ok(TemplateWrapper::of(&template))
    }

    /// `createTemplate($template, $name)`: the template compiled from this
    /// string. Templates are compiled ahead of time, so the string must be
    /// in the loader under the name `createTemplate()` gives it (see
    /// [`crate::runtime::string_loader`] and
    /// [`crate::compiler::compile_set_with`]); any other string is a
    /// runtime error.
    pub fn create_template(
        &self,
        template: &[u8],
        name: Option<&[u8]>,
    ) -> Result<TemplateWrapper, Error> {
        let full = crate::runtime::string_loader::template_name(template, name);
        if self
            .loader()
            .is_none_or(|loader| loader.compiled(&full, None).is_err())
        {
            return Err(crate::runtime::string_loader::unknown_string());
        }
        self.load(&full)
    }

    /// `render($name, $context)`.
    pub fn render(&self, name: &[u8], context: Context) -> Result<Vec<u8>, Error> {
        self.load_template(name, None)?.render(self, context)
    }

    /// `resolveTemplate($names)` for an array of names.
    pub fn resolve_template(&self, names: &PhpArray) -> Result<TemplateWrapper, Error> {
        let count = names.len();
        let mut listed = Vec::new();
        for name in names.values() {
            if let Value::Object(o) = name
                && let Some(wrapper) = o.downcast_ref::<TemplateWrapper>()
            {
                return Ok(wrapper.clone());
            }
            let name = crate::runtime::core::string_argument(name)?;
            listed.push(name.clone());
            if count != 1 && !self.loader().is_some_and(|l| l.exists(name.as_bytes())) {
                continue;
            }
            return self.load(name.as_bytes());
        }
        let names: Vec<String> = listed
            .iter()
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        Err(Error::loader(
            format!(
                "Unable to find one of the following templates: \"{}\".",
                names.join("\", \"")
            ),
            -1,
            None,
        ))
    }

    /// `compileSource($source)`: the Rust code of a template (see
    /// [`crate::compiler`]).
    pub fn compile_source(&self, source: Source) -> Result<String, Error> {
        let arc = std::sync::Arc::new(source.clone());
        let compile = || -> Result<String, Error> {
            let module = self.parse_source(source)?;
            crate::compiler::Compiler::new(self).compile(&module)
        };
        compile().map_err(|mut e| {
            e.set_source_context(Some(arc));
            e
        })
    }

    /// The charset as a PHP string.
    pub fn charset_string(&self) -> PhpString {
        PhpString::from(self.charset.clone())
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
