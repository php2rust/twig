//! Port of Twig\Extension\StringLoaderExtension.php: the
//! `template_from_string` function.
//!
//! Templates are compiled ahead of time, so the function loads only the
//! strings known when the templates are compiled
//! ([`crate::compiler::compile_set`] compiles them):
//! see [`crate::runtime::string_loader`].
//!
//! Upstream: twig/twig src/Extension/StringLoaderExtension.php

use super::Extension;
use crate::callable::TwigCallable;

/// `StringLoaderExtension`.
#[derive(Debug, Clone, Copy, Default)]
pub struct StringLoaderExtension;

impl Extension for StringLoaderExtension {
    fn name(&self) -> &'static str {
        "Twig\\Extension\\StringLoaderExtension"
    }

    fn functions(&self) -> Vec<TwigCallable> {
        vec![
            TwigCallable::function(
                "template_from_string",
                Some(crate::runtime::string_loader::CALLABLE),
            )
            .needs_environment(),
        ]
    }
}
