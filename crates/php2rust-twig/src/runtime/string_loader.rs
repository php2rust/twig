//! Port of the runtime of Twig\Extension\StringLoaderExtension.php
//! (`templateFromString()`) and of `Environment::createTemplate()`'s
//! template names.
//!
//! PHP compiles the string when the function runs. Here templates are
//! compiled ahead of time: [`crate::compiler::compile_set`] compiles each
//! string passed to `template_from_string()` as a literal, or set to a
//! variable with `{% set name = '...' %}` and then passed, under the name
//! `createTemplate()` gives it. At run time the function loads the
//! template of that name; a string not compiled that way is a runtime
//! error (the crate README lists this difference).
//!
//! Upstream: twig/twig src/Extension/StringLoaderExtension.php src/Environment.php

use php_value::Value;

use super::{Call, R};
use crate::error::Error;

/// The PHP callable of the function.
pub const CALLABLE: &str = "Twig\\Extension\\StringLoaderExtension::templateFromString";

/// `createTemplate()`'s name for a string template: `__string_template__`
/// and the string's `xxh128` hash, or `$name (string template <hash>)`.
#[must_use]
pub fn template_name(template: &[u8], name: Option<&[u8]>) -> Vec<u8> {
    let hash = format!("{:032x}", twox_hash::XxHash3_128::oneshot(template));
    match name {
        Some(name) => {
            let mut out = name.to_vec();
            out.extend_from_slice(format!(" (string template {hash})").as_bytes());
            out
        }
        None => format!("__string_template__{hash}").into_bytes(),
    }
}

/// `templateFromString($env, $template, $name)`: the template compiled
/// from this string, as a `TemplateWrapper`.
#[allow(
    clippy::needless_pass_by_value,
    reason = "the runtime functions share one signature"
)]
pub fn template_from_string(call: &Call<'_>, args: Vec<Value>) -> R {
    let template = match args.first() {
        Some(Value::String(s)) => s.as_bytes().to_vec(),
        Some(value @ Value::Object(_)) => value
            .to_php_string_or_stringable()
            .map(|s| s.as_bytes().to_vec())
            .ok_or_else(|| argument_error(value))?,
        Some(value @ (Value::Int(_) | Value::Float(_) | Value::Bool(_))) => super::bytes(value)?,
        other => return Err(argument_error(other.unwrap_or(&Value::Null))),
    };
    let name = match args.get(1) {
        None | Some(Value::Null) => None,
        Some(value) => Some(super::bytes(value)?),
    };
    let full = template_name(&template, name.as_deref());
    let known = call
        .env
        .loader()
        .is_some_and(|loader| loader.compiled(&full, None).is_ok());
    if !known {
        return Err(Error::runtime(
            "The \"template_from_string\" function can only load a string that was known when the templates were compiled (a literal, or a variable set to a literal); this string was not.",
            -1,
            None,
        ));
    }
    Ok(Value::Object(std::sync::Arc::new(call.env.load(&full)?)))
}

fn argument_error(value: &Value) -> Error {
    Error::exception(
        "TypeError",
        format!(
            "Twig\\Extension\\StringLoaderExtension::templateFromString(): Argument #2 ($template) must be of type Stringable|string, {} given",
            value.debug_type()
        ),
    )
}

/// A string template that did not compile: loading it gives the error, as
/// `createTemplate()` throws it when the function runs.
pub struct FailedTemplate {
    pub name: &'static [u8],
    pub code: &'static [u8],
    pub kind: crate::error::ErrorKind,
    pub message: &'static [u8],
    pub line: i64,
    pub column: Option<usize>,
}

impl crate::template::CompiledTemplate for FailedTemplate {
    fn template_name(&self) -> &'static [u8] {
        self.name
    }

    fn source_context(&self) -> crate::source::Source {
        crate::source::Source::new(self.code, String::from_utf8_lossy(self.name).into_owned())
    }

    fn construct(
        &self,
        _this: &std::rc::Rc<crate::template::Template>,
        _env: &crate::Environment,
    ) -> Result<(), Error> {
        Err(Error::new(
            self.kind,
            self.message,
            self.line,
            Some(std::sync::Arc::new(self.source_context())),
        )
        .with_column(self.column))
    }

    fn do_display(
        &self,
        _this: &std::rc::Rc<crate::template::Template>,
        _env: &crate::Environment,
        _context: crate::template::Context,
        _blocks: &crate::template::Blocks,
        _out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        Ok(())
    }

    fn call_block(
        &self,
        _name: &[u8],
        _this: &std::rc::Rc<crate::template::Template>,
        _env: &crate::Environment,
        _context: crate::template::Context,
        _blocks: &crate::template::Blocks,
        _out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        Ok(())
    }
}
