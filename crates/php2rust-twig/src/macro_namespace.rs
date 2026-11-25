//! Port of Twig\MacroNamespace.php: the macros of a template, as `import`
//! and `from` make them available, with the lookup through the parents.
//!
//! Upstream: twig/twig src/MacroNamespace.php

use std::{fmt, rc::Rc};

use php_value::{PhpArray, Value};
use symfony_deprecation_contracts::trigger_deprecation;

use crate::{
    environment::Environment,
    error::Error,
    source::Source,
    template::{Context, Template},
    twig_macro::TwigMacro,
};

/// A macro name as given, and as declared.
type NamePair = (Vec<u8>, Vec<u8>);

/// `Twig\MacroNamespace`.
pub struct MacroNamespace {
    template: Rc<Template>,
    macros: Vec<TwigMacro>,
}

impl fmt::Debug for MacroNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacroNamespace")
            .field("template", &self.template)
            .finish_non_exhaustive()
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

impl MacroNamespace {
    pub fn new(template: Rc<Template>, macros: Vec<TwigMacro>) -> Self {
        Self { template, macros }
    }

    /// `has($name, $context)`.
    pub fn has(&self, env: &Environment, name: &[u8], context: &Context) -> Result<bool, Error> {
        if let Some((declared, template_name)) = self.find_declared_name(env, name, context)? {
            if declared != name {
                trigger_deprecation(
                    b"twig/twig",
                    b"3.29",
                    format!(
                        "Testing whether the macro \"{}\" (defined in template \"{}\") is defined as \"{}\" is deprecated; macro names will be case-sensitive in Twig 4.0 and this test will return false.",
                        text(&declared),
                        text(&template_name),
                        text(name)
                    )
                    .as_bytes(),
                );
            }
            return Ok(true);
        }
        let Some(bare) = name.strip_prefix(b"macro_") else {
            return Ok(false);
        };
        if self.find_declared_name(env, bare, context)?.is_none() {
            return Ok(false);
        }
        trigger_deprecation(
            b"twig/twig",
            b"3.29",
            format!(
                "Testing whether the macro \"{}\" is defined via the \"macro_\"-prefixed name \"{}\" is deprecated; pass the bare macro name to \"Twig\\Node\\Expression\\MacroReferenceExpression\" instead.",
                text(bare),
                text(name)
            )
            .as_bytes(),
        );
        Ok(true)
    }

    /// `call($name, $arguments, $context, $line, $source)`.
    pub fn call(
        &self,
        env: &Environment,
        name: &[u8],
        arguments: &PhpArray,
        context: &Context,
        line: i64,
        source: &Source,
    ) -> Result<Value, Error> {
        if let Some(found) = self.get_declared(name) {
            return found.call_legacy(&self.template, env, arguments, source, line);
        }
        if let Some((template, found)) = self.resolve(env, name, context)? {
            return found.call_legacy(&template, env, arguments, source, line);
        }
        if let Some(bare) = name.strip_prefix(b"macro_")
            && let Some((template, found)) = self.resolve(env, bare, context)?
        {
            trigger_deprecation(
                b"twig/twig",
                b"3.29",
                format!(
                    "Calling the macro \"{}\" via the \"macro_\"-prefixed name \"{}\" is deprecated; pass the bare macro name to \"Twig\\Node\\Expression\\MacroReferenceExpression\" instead.",
                    text(bare),
                    text(name)
                )
                .as_bytes(),
            );
            return found.call_legacy(&template, env, arguments, source, line);
        }
        Err(Error::runtime(
            format!(
                "Macro \"{}\" is not defined in template \"{}\".",
                text(name),
                text(self.template.name())
            ),
            line,
            Some(std::sync::Arc::new(source.clone())),
        ))
    }

    fn find_declared_name(
        &self,
        env: &Environment,
        name: &[u8],
        context: &Context,
    ) -> Result<Option<NamePair>, Error> {
        let mut namespace: Option<Rc<Self>> = None;
        loop {
            let current = namespace.as_deref().unwrap_or(self);
            if current.macros.iter().any(|m| m.name() == name) {
                return Ok(Some((name.to_vec(), current.template.name().to_vec())));
            }
            if let Some(m) = current
                .macros
                .iter()
                .find(|m| m.name().eq_ignore_ascii_case(name))
            {
                return Ok(Some((m.name().to_vec(), current.template.name().to_vec())));
            }
            match current.parent(env, context)? {
                Some(parent) => namespace = Some(parent),
                None => return Ok(None),
            }
        }
    }

    fn get_declared(&self, name: &[u8]) -> Option<&TwigMacro> {
        if let Some(m) = self.macros.iter().find(|m| m.name() == name) {
            return Some(m);
        }
        let m = self
            .macros
            .iter()
            .find(|m| m.name().eq_ignore_ascii_case(name))?;
        trigger_deprecation(
            b"twig/twig",
            b"3.29",
            format!(
                "Calling the macro \"{}\" (defined in template \"{}\") as \"{}\" is deprecated; macro names will be case-sensitive in Twig 4.0.",
                text(m.name()),
                text(self.template.name()),
                text(name)
            )
            .as_bytes(),
        );
        Some(m)
    }

    fn resolve(
        &self,
        env: &Environment,
        name: &[u8],
        context: &Context,
    ) -> Result<Option<(Rc<Template>, TwigMacro)>, Error> {
        let mut namespace: Option<Rc<Self>> = None;
        loop {
            let current = namespace.as_deref().unwrap_or(self);
            if let Some(m) = current.get_declared(name) {
                return Ok(Some((Rc::clone(&current.template), m.clone())));
            }
            match current.parent(env, context)? {
                Some(parent) => namespace = Some(parent),
                None => return Ok(None),
            }
        }
    }

    fn parent(&self, env: &Environment, context: &Context) -> Result<Option<Rc<Self>>, Error> {
        Ok(self
            .template
            .get_parent(env, context)?
            .map(|parent| parent.macro_namespace()))
    }
}
