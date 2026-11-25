//! Port of Twig\Template.php and Twig\TemplateWrapper.php: a loaded
//! template, its blocks, parents and traits, and the public wrapper.
//!
//! PHP generates one class per template that extends `Template`; here the
//! compiler emits a type that implements [`CompiledTemplate`] (the
//! methods the PHP subclass overrides), and [`Template`] holds the state
//! of the instance (`$parent`, `$blocks`, `$traits`, `$macros`, ...) and
//! the methods of the base class. Output goes to a byte buffer instead of
//! a generator (`yield`). PHP's `handleException()` guesses the line of an
//! error from the backtrace; compiled functions give the line of the last
//! debug info they passed instead ([`Error::guess_line`]).
//!
//! Not ported: `use_yield` (output is always collected), `loadTemplate()`
//! (deprecated), the sandbox hooks (`ensureSecurityChecked()`,
//! `checkTraitsAllowed()`: the sandbox is not ported yet).
//!
//! Upstream: twig/twig src/Template.php src/TemplateWrapper.php

use std::{
    cell::RefCell,
    collections::HashMap,
    fmt,
    rc::{Rc, Weak},
    sync::Arc,
};

use php_value::{PhpArray, PhpObject, PhpString, Value};

use crate::{
    environment::Environment, error::Error, macro_namespace::MacroNamespace, source::Source,
    twig_macro::TwigMacro,
};

/// A template context: PHP's `array $context`.
pub type Context = PhpArray;

/// `Template::ANY_CALL`.
pub const ANY_CALL: &str = "any";
/// `Template::ARRAY_CALL`.
pub const ARRAY_CALL: &str = "array";
/// `Template::METHOD_CALL`.
pub const METHOD_CALL: &str = "method";

/// A block: `[$template, 'block_<name>']`; `name` is the `<name>` part of
/// the method, which stays the same when a trait renames the block.
#[derive(Clone)]
pub struct BlockRef {
    pub template: Rc<Template>,
    pub name: Vec<u8>,
}

/// An array of blocks, `name => [$template, $method]`, in PHP's order.
#[derive(Clone, Default)]
pub struct Blocks(Vec<(Vec<u8>, BlockRef)>);

impl Blocks {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `isset($blocks[$name])`.
    #[must_use]
    pub fn contains(&self, name: &[u8]) -> bool {
        self.get(name).is_some()
    }

    /// `$blocks[$name]`.
    #[must_use]
    pub fn get(&self, name: &[u8]) -> Option<&BlockRef> {
        self.0.iter().find(|(n, _)| n == name).map(|(_, b)| b)
    }

    /// `$blocks[$name] = $block`: a new name goes last, a known one keeps
    /// its place.
    pub fn insert(&mut self, name: impl Into<Vec<u8>>, block: BlockRef) {
        let name = name.into();
        if let Some(slot) = self.0.iter_mut().find(|(n, _)| *n == name) {
            slot.1 = block;
        } else {
            self.0.push((name, block));
        }
    }

    /// `unset($blocks[$name])`.
    pub fn remove(&mut self, name: &[u8]) -> Option<BlockRef> {
        let i = self.0.iter().position(|(n, _)| n == name)?;
        Some(self.0.remove(i).1)
    }

    /// `array_merge($this, $other)`.
    #[must_use]
    pub fn merge(&self, other: &Self) -> Self {
        let mut merged = self.clone();
        for (name, block) in &other.0 {
            merged.insert(name.clone(), block.clone());
        }
        merged
    }

    /// `array_keys($blocks)`.
    #[must_use]
    pub fn names(&self) -> Vec<Vec<u8>> {
        self.0.iter().map(|(n, _)| n.clone()).collect()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The macro namespaces of a template, `$this->macros` and the `$macros`
/// local variable: name => `MacroNamespace`.
pub type Macros = HashMap<Vec<u8>, Rc<MacroNamespace>>;

/// What the PHP subclass a template compiles to defines; implemented by
/// generated code.
pub trait CompiledTemplate: Sync {
    /// `getTemplateName()`.
    fn template_name(&self) -> &'static [u8];

    /// `getSourceContext()`.
    fn source_context(&self) -> Source;

    /// `isTraitable()`.
    fn is_traitable(&self) -> bool {
        true
    }

    /// `getDefaultEscapeStrategy()`; `None` for `false`.
    fn default_escape_strategy(&self) -> Option<&'static [u8]> {
        None
    }

    /// The constructor: `$this->parent`, `$this->traits` and
    /// `$this->blocks`.
    fn construct(&self, this: &Rc<Template>, env: &Environment) -> Result<(), Error>;

    /// `doGetParent()`; `None` for `false`.
    fn do_get_parent(
        &self,
        this: &Rc<Template>,
        env: &Environment,
        context: &Context,
    ) -> Result<Option<Rc<Template>>, Error> {
        let _ = (this, env, context);
        Ok(None)
    }

    /// `doDisplay()`.
    fn do_display(
        &self,
        this: &Rc<Template>,
        env: &Environment,
        context: Context,
        blocks: &Blocks,
        out: &mut Vec<u8>,
    ) -> Result<(), Error>;

    /// `$this->block_<name>($context, $blocks)`.
    fn call_block(
        &self,
        name: &[u8],
        this: &Rc<Template>,
        env: &Environment,
        context: Context,
        blocks: &Blocks,
        out: &mut Vec<u8>,
    ) -> Result<(), Error>;

    /// `loadDeclaredMacros()`.
    fn load_declared_macros(&self) -> Vec<TwigMacro> {
        Vec::new()
    }
}

/// An instance of a compiled template (`Twig\Template`).
pub struct Template {
    compiled: &'static dyn CompiledTemplate,
    source: Arc<Source>,
    this: Weak<Template>,
    /// `$this->parent`: `None` is null (not resolved yet), `Some(None)`
    /// is `false`.
    #[allow(clippy::option_option)]
    parent: RefCell<Option<Option<Rc<Template>>>>,
    parents: RefCell<Vec<(Vec<u8>, Rc<Template>)>>,
    blocks: RefCell<Blocks>,
    traits: RefCell<Blocks>,
    trait_aliases: RefCell<HashMap<Vec<u8>, Vec<u8>>>,
    macros: RefCell<Macros>,
    macro_namespace: RefCell<Option<Rc<MacroNamespace>>>,
}

impl fmt::Debug for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Template")
            .field("name", &String::from_utf8_lossy(self.name()))
            .finish_non_exhaustive()
    }
}

impl Template {
    /// `new $cls($env)`.
    pub fn new(
        compiled: &'static dyn CompiledTemplate,
        env: &Environment,
    ) -> Result<Rc<Self>, Error> {
        let template = Rc::new_cyclic(|this| Self {
            compiled,
            source: Arc::new(compiled.source_context()),
            this: this.clone(),
            parent: RefCell::new(None),
            parents: RefCell::new(Vec::new()),
            blocks: RefCell::new(Blocks::new()),
            traits: RefCell::new(Blocks::new()),
            trait_aliases: RefCell::new(HashMap::new()),
            macros: RefCell::new(Macros::new()),
            macro_namespace: RefCell::new(None),
        });
        compiled.construct(&template, env)?;
        Ok(template)
    }

    fn rc(&self) -> Rc<Self> {
        // `self` is always inside the Rc built by `new`.
        self.this
            .upgrade()
            .unwrap_or_else(|| unreachable!("a template lives in an Rc"))
    }

    /// `getTemplateName()`.
    pub fn name(&self) -> &'static [u8] {
        self.compiled.template_name()
    }

    /// `getSourceContext()` (the `$this->source` property).
    pub fn source(&self) -> &Arc<Source> {
        &self.source
    }

    /// `getDefaultEscapeStrategy()`.
    pub fn default_escape_strategy(&self) -> Option<&'static [u8]> {
        self.compiled.default_escape_strategy()
    }

    /// `isTraitable()`.
    pub fn is_traitable(&self) -> bool {
        self.compiled.is_traitable()
    }

    /// `hasFixedParent()`.
    pub fn has_fixed_parent(&self) -> bool {
        self.parent.borrow().is_some()
    }

    /// `getBlocks()`.
    pub fn blocks(&self) -> Blocks {
        self.blocks.borrow().clone()
    }

    /// `$this->blocks = ...` (constructor).
    pub fn set_blocks(&self, blocks: Blocks) {
        *self.blocks.borrow_mut() = blocks;
    }

    /// `$this->traits = ...` (constructor).
    pub fn set_traits(&self, traits: Blocks) {
        *self.traits.borrow_mut() = traits;
    }

    /// `$this->traitAliases[$alias] = $name` (constructor).
    pub fn set_trait_alias(&self, alias: Vec<u8>, name: Vec<u8>) {
        self.trait_aliases.borrow_mut().insert(alias, name);
    }

    /// `$this->parent = $parent` (`false` is `None`).
    pub fn set_parent(&self, parent: Option<Rc<Self>>) {
        *self.parent.borrow_mut() = Some(parent);
    }

    /// `$this->parent` when it is resolved and not `false`.
    pub fn fixed_parent(&self) -> Option<Rc<Self>> {
        self.parent.borrow().clone().flatten()
    }

    /// A block of this template: `[$this, 'block_<name>']`.
    pub fn block_ref(&self, name: &[u8]) -> BlockRef {
        BlockRef {
            template: self.rc(),
            name: name.to_vec(),
        }
    }

    /// `$this->macros` (copied into `$macros` at the start of each method).
    pub fn macros(&self) -> Macros {
        self.macros.borrow().clone()
    }

    /// `$this->macros[$name] = $namespace`.
    pub fn set_macro(&self, name: &[u8], namespace: Rc<MacroNamespace>) {
        self.macros.borrow_mut().insert(name.to_vec(), namespace);
    }

    /// `getParent()`.
    pub fn get_parent(
        &self,
        env: &Environment,
        context: &Context,
    ) -> Result<Option<Rc<Self>>, Error> {
        if let Some(parent) = self.parent.borrow().clone() {
            return Ok(parent);
        }
        let parent = self
            .compiled
            .do_get_parent(&self.rc(), env, context)
            .map_err(|e| self.handle_exception(e, -1))?;
        let Some(parent) = parent else {
            return Ok(None);
        };
        let name = parent.name().to_vec();
        let mut parents = self.parents.borrow_mut();
        if let Some(slot) = parents.iter_mut().find(|(n, _)| *n == name) {
            slot.1 = Rc::clone(&parent);
        } else {
            parents.push((name, Rc::clone(&parent)));
        }
        Ok(Some(parent))
    }

    /// `handleException()` for errors that leave a method without a line
    /// to guess from.
    fn handle_exception(&self, error: Error, line: i64) -> Error {
        error.guess_line(line, &self.source)
    }

    /// `load()`: a template name, a `TemplateWrapper` or an array of
    /// names (`resolveTemplate()`), with the line to report on a loader
    /// error (-1: guess).
    pub fn load(
        &self,
        env: &Environment,
        template: &Value,
        line: i64,
        index: Option<i64>,
    ) -> Result<Rc<Self>, Error> {
        let loaded = match template {
            Value::Array(names) => env
                .resolve_template(names)
                .and_then(|w| w.unwrap_template(env)),
            Value::Object(o) if o.downcast_ref::<TemplateWrapper>().is_some() => o
                .downcast_ref::<TemplateWrapper>()
                .map_or_else(|| unreachable!(), |w| w.unwrap_template(env)),
            other => {
                let name = crate::runtime::core::string_argument(other)?;
                env.load_template(name.as_bytes(), index)
            }
        };
        loaded.map_err(|mut e| {
            if e.source_context().is_none() {
                e.set_source_context(Some(Arc::clone(&self.source)));
            }
            if e.template_line() > 0 {
                return e;
            }
            if line != -1 {
                e.set_template_line(line);
            }
            e
        })
    }

    /// `hasBlock()`.
    pub fn has_block(
        &self,
        env: &Environment,
        name: &[u8],
        context: &Context,
        blocks: &Blocks,
    ) -> Result<bool, Error> {
        if let Some(block) = blocks.get(name) {
            let _ = block;
            return Ok(true);
        }
        if self.blocks.borrow().contains(name) {
            return Ok(true);
        }
        if let Some(parent) = self.get_parent(env, context)? {
            return parent.has_block(env, name, context, &Blocks::new());
        }
        Ok(false)
    }

    /// `getBlockNames()`.
    pub fn block_names(
        &self,
        env: &Environment,
        context: &Context,
        blocks: &Blocks,
    ) -> Result<Vec<Vec<u8>>, Error> {
        let mut names = blocks.names();
        names.extend(self.blocks.borrow().names());
        if let Some(parent) = self.get_parent(env, context)? {
            names.extend(parent.block_names(env, context, &Blocks::new())?);
        }
        let mut unique: Vec<Vec<u8>> = Vec::new();
        for name in names {
            if !unique.contains(&name) {
                unique.push(name);
            }
        }
        Ok(unique)
    }

    /// `render()`.
    pub fn render(&self, env: &Environment, context: Context) -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        self.display(env, context, &Blocks::new(), &mut out)?;
        Ok(out)
    }

    /// `yield()` / `display()`.
    pub fn display(
        &self,
        env: &Environment,
        mut context: Context,
        blocks: &Blocks,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        context.union(&env.globals());
        let blocks = self.blocks.borrow().merge(blocks);
        let this = self.rc();
        self.compiled
            .do_display(&this, env, context, &blocks, out)
            .map_err(|e| self.handle_exception(e, -1))
    }

    /// `yieldBlock()` / `displayBlock()`.
    #[allow(clippy::too_many_arguments)] // the PHP parameters, and the output
    pub fn display_block(
        &self,
        env: &Environment,
        name: &[u8],
        context: Context,
        blocks: &Blocks,
        use_blocks: bool,
        template_context: Option<&Self>,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let own = self.blocks.borrow().get(name).cloned();
        let (block, blocks) = if use_blocks && let Some(block) = blocks.get(name) {
            (Some(block.clone()), blocks.clone())
        } else if let Some(block) = own {
            // expose this template's own blocks so nested block() calls resolve against them
            (Some(block), self.blocks.borrow().merge(blocks))
        } else {
            (None, blocks.clone())
        };
        if let Some(block) = block {
            let template = &block.template;
            return template
                .compiled
                .call_block(&block.name, template, env, context, &blocks, out)
                .map_err(|e| template.handle_exception(e, -1));
        }
        if let Some(parent) = self.get_parent(env, &context)? {
            let merged = self.blocks.borrow().merge(&blocks);
            return parent.display_block(
                env,
                name,
                context,
                &merged,
                false,
                Some(template_context.unwrap_or(self)),
                out,
            );
        }
        if let Some(block) = blocks.get(name) {
            return Err(Error::runtime(
                format!(
                    "Block \"{}\" should not call parent() in \"{}\" as the block does not exist in the parent template \"{}\".",
                    String::from_utf8_lossy(name),
                    String::from_utf8_lossy(block.template.name()),
                    String::from_utf8_lossy(self.name())
                ),
                -1,
                Some(Arc::clone(&block.template.source)),
            ));
        }
        Err(Error::runtime(
            format!(
                "Block \"{}\" on template \"{}\" does not exist.",
                String::from_utf8_lossy(name),
                String::from_utf8_lossy(self.name())
            ),
            -1,
            Some(Arc::clone(&template_context.unwrap_or(self).source)),
        ))
    }

    /// `renderBlock()`.
    pub fn render_block(
        &self,
        env: &Environment,
        name: &[u8],
        context: Context,
        blocks: &Blocks,
        use_blocks: bool,
    ) -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        self.display_block(env, name, context, blocks, use_blocks, None, &mut out)?;
        Ok(out)
    }

    /// `yieldParentBlock()` / `displayParentBlock()`.
    pub fn display_parent_block(
        &self,
        env: &Environment,
        name: &[u8],
        context: Context,
        blocks: &Blocks,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let trait_block = self.traits.borrow().get(name).cloned();
        if let Some(block) = trait_block {
            let alias = self
                .trait_aliases
                .borrow()
                .get(name)
                .cloned()
                .unwrap_or_else(|| name.to_vec());
            return block
                .template
                .display_block(env, &alias, context, blocks, false, None, out);
        }
        if let Some(parent) = self.get_parent(env, &context)? {
            return parent.display_block(env, name, context, blocks, false, None, out);
        }
        Err(Error::runtime(
            format!(
                "The template has no parent and no traits defining the \"{}\" block.",
                String::from_utf8_lossy(name)
            ),
            -1,
            Some(Arc::clone(&self.source)),
        ))
    }

    /// `renderParentBlock()`.
    pub fn render_parent_block(
        &self,
        env: &Environment,
        name: &[u8],
        context: Context,
        blocks: &Blocks,
    ) -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        self.display_parent_block(env, name, context, blocks, &mut out)?;
        Ok(out)
    }

    /// `getMacroNamespace()`.
    pub fn macro_namespace(&self) -> Rc<MacroNamespace> {
        if let Some(namespace) = self.macro_namespace.borrow().clone() {
            return namespace;
        }
        let namespace = Rc::new(MacroNamespace::new(
            self.rc(),
            self.compiled.load_declared_macros(),
        ));
        *self.macro_namespace.borrow_mut() = Some(Rc::clone(&namespace));
        namespace
    }

    /// `throwUninitializedMacroNamespace()`.
    pub fn uninitialized_macro_namespace(&self, line: i64) -> Error {
        Error::runtime(
            format!(
                "Macros imported in the body of template \"{}\" are not available because the body was not rendered; move the \"import\" or \"from\" tag inside the block or the macro that uses it.",
                String::from_utf8_lossy(self.name())
            ),
            line,
            Some(Arc::clone(&self.source)),
        )
    }
}

/// `Twig\TemplateWrapper`, as a PHP value: the template it wraps, found
/// again through the environment's loaded templates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateWrapper {
    name: Vec<u8>,
    index: Option<i64>,
}

impl TemplateWrapper {
    pub const CLASS: &'static str = "Twig\\TemplateWrapper";

    pub fn new(name: impl Into<Vec<u8>>, index: Option<i64>) -> Self {
        Self {
            name: name.into(),
            index,
        }
    }

    /// The wrapper of a template.
    pub fn of(template: &Template) -> Self {
        Self::new(template.name(), None)
    }

    /// The wrapper as a value.
    #[must_use]
    pub fn value(self) -> Value {
        Value::Object(Arc::new(self))
    }

    /// `getTemplateName()`.
    #[must_use]
    pub fn name(&self) -> &[u8] {
        &self.name
    }

    /// `unwrap($env)`.
    pub fn unwrap_template(&self, env: &Environment) -> Result<Rc<Template>, Error> {
        env.load_template(&self.name, self.index)
    }

    /// `render($context)`.
    pub fn render(&self, env: &Environment, context: Context) -> Result<Vec<u8>, Error> {
        self.unwrap_template(env)?.render(env, context)
    }

    /// `renderBlock($name, $context)`.
    pub fn render_block(
        &self,
        env: &Environment,
        name: &[u8],
        mut context: Context,
    ) -> Result<Vec<u8>, Error> {
        context.union(&env.globals());
        self.unwrap_template(env)?
            .render_block(env, name, context, &Blocks::new(), true)
    }

    /// `hasBlock($name, $context)`.
    pub fn has_block(
        &self,
        env: &Environment,
        name: &[u8],
        mut context: Context,
    ) -> Result<bool, Error> {
        context.union(&env.globals());
        self.unwrap_template(env)?
            .has_block(env, name, &context, &Blocks::new())
    }

    /// `getBlockNames($context)`.
    pub fn block_names(
        &self,
        env: &Environment,
        mut context: Context,
    ) -> Result<Vec<Vec<u8>>, Error> {
        context.union(&env.globals());
        self.unwrap_template(env)?
            .block_names(env, &context, &Blocks::new())
    }
}

impl PhpObject for TemplateWrapper {
    fn class(&self) -> &str {
        Self::CLASS
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn methods(&self) -> Vec<String> {
        [
            "__construct",
            "stream",
            "streamBlock",
            "render",
            "display",
            "hasBlock",
            "getBlockNames",
            "renderBlock",
            "displayBlock",
            "getSourceContext",
            "getTemplateName",
            "getDefaultEscapeStrategy",
            "isOwnedBy",
            "unwrap",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    fn call_method(
        &self,
        name: &str,
        _arguments: &[Value],
    ) -> Option<Result<Value, php_value::Thrown>> {
        match name.to_ascii_lowercase().as_str() {
            "gettemplatename" => Some(Ok(Value::String(PhpString::from(self.name.clone())))),
            _ => None,
        }
    }
}
