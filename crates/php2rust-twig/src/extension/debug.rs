//! Port of Twig\Extension\DebugExtension.php: the `dump` function.
//!
//! Upstream: twig/twig src/Extension/DebugExtension.php

use super::Extension;
use crate::callable::TwigCallable;

/// `DebugExtension`.
#[derive(Debug, Clone, Copy)]
pub struct DebugExtension {
    html_safe: bool,
}

impl DebugExtension {
    /// `new DebugExtension()`. `html_safe` is PHP's check for the CLI
    /// SAPI (or Xdebug's HTML `var_dump()`): there, `dump`'s output is
    /// marked safe for HTML.
    #[must_use]
    pub fn new(html_safe: bool) -> Self {
        Self { html_safe }
    }
}

impl Extension for DebugExtension {
    fn name(&self) -> &'static str {
        "Twig\\Extension\\DebugExtension"
    }

    fn functions(&self) -> Vec<TwigCallable> {
        let dump = TwigCallable::function("dump", Some("Twig\\Extension\\DebugExtension::dump"))
            .needs_context()
            .needs_environment()
            .variadic();
        vec![if self.html_safe {
            dump.safe(&["html"])
        } else {
            dump
        }]
    }
}
