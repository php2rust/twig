//! Where compiled templates come from: the runtime counterpart of
//! Twig\Loader\LoaderInterface.php, giving compiled templates instead of
//! sources (templates are compiled to Rust ahead of time).
//!
//! Upstream: twig/twig src/Loader/LoaderInterface.php src/Loader/ArrayLoader.php

use crate::{error::Error, source::Source, template::CompiledTemplate};

/// The compiled templates an environment can load.
pub trait TemplateLoader {
    /// The compiled template of a name (`index`: an `embed` tag's
    /// template); an error as the loader reports it (`LoaderError`).
    fn compiled(
        &self,
        name: &[u8],
        index: Option<i64>,
    ) -> Result<&'static dyn CompiledTemplate, Error>;

    /// `exists($name)`.
    fn exists(&self, name: &[u8]) -> bool {
        self.compiled(name, None).is_ok()
    }

    /// `getSourceContext($name)` (the `source()` function).
    fn source_context(&self, name: &[u8]) -> Result<Source, Error>;
}

/// A fixed set of compiled templates with their sources, as
/// `ArrayLoader` gives templates: generated code lists them.
pub struct CompiledArrayLoader {
    /// Name, index, the compiled template, and the source code.
    pub templates: &'static [(
        &'static [u8],
        Option<i64>,
        &'static dyn CompiledTemplate,
        &'static [u8],
    )],
}

fn not_defined(name: &[u8]) -> Error {
    Error::loader(
        format!(
            "Template \"{}\" is not defined.",
            String::from_utf8_lossy(name)
        ),
        -1,
        None,
    )
}

impl TemplateLoader for CompiledArrayLoader {
    fn compiled(
        &self,
        name: &[u8],
        index: Option<i64>,
    ) -> Result<&'static dyn CompiledTemplate, Error> {
        self.templates
            .iter()
            .find(|(n, i, _, _)| *n == name && *i == index)
            .map(|(_, _, t, _)| *t)
            .ok_or_else(|| not_defined(name))
    }

    fn source_context(&self, name: &[u8]) -> Result<Source, Error> {
        self.templates
            .iter()
            .find(|(n, i, _, _)| *n == name && i.is_none())
            .map(|(n, _, _, code)| {
                Source::new(code.to_vec(), String::from_utf8_lossy(n).into_owned())
            })
            .ok_or_else(|| not_defined(name))
    }
}
