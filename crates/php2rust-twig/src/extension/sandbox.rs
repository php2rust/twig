//! Port of Twig\Extension\SandboxExtension.php: the `sandbox` tag, the
//! sandbox node visitor and the [`SecurityChecker`] compiled templates
//! call.
//!
//! The checker's methods (`checkSecurity()`, `checkMethodAllowed()`,
//! `ensureToStringAllowed()`, ...) are reached through
//! [`SandboxExtension::checker`]; the extension forwards only the ones
//! that have no deprecation of their own.
//!
//! Upstream: twig/twig src/Extension/SandboxExtension.php

use std::rc::Rc;

use symfony_deprecation_contracts::trigger_deprecation;

use super::Extension;
use crate::{
    node_visitor::{NodeVisitor, sandbox::SandboxNodeVisitor},
    sandbox::{SecurityChecker, SecurityPolicyInterface, SourcePolicyInterface},
    source::Source,
    token_parser::{TokenParser, sandbox::SandboxTokenParser},
};

/// `SandboxExtension`.
#[derive(Debug)]
pub struct SandboxExtension {
    checker: SecurityChecker,
}

impl SandboxExtension {
    pub const CLASS: &'static str = "Twig\\Extension\\SandboxExtension";

    /// `new SandboxExtension($policy, $sandboxed, $sourcePolicy)`; a source
    /// policy is deprecated.
    pub fn new(
        policy: Rc<dyn SecurityPolicyInterface>,
        sandboxed: bool,
        source_policy: Option<Rc<dyn SourcePolicyInterface>>,
    ) -> Self {
        if source_policy.is_some() {
            trigger_deprecation(
                b"twig/twig",
                b"3.27.0",
                b"The \"Twig\\Sandbox\\SourcePolicyInterface\" interface is deprecated with no replacement, do not pass an instance to \"Twig\\Extension\\SandboxExtension\".",
            );
        }
        Self {
            checker: SecurityChecker::new(policy, sandboxed, source_policy),
        }
    }

    /// `getChecker()`.
    #[must_use]
    pub fn checker(&self) -> &SecurityChecker {
        &self.checker
    }

    fn deprecated(method: &str) {
        trigger_deprecation(
            b"twig/twig",
            b"3.29",
            format!(
                "The \"Twig\\Extension\\SandboxExtension::{method}()\" method is deprecated, use \"Twig\\Sandbox\\Sandbox\" to render untrusted templates instead."
            )
            .as_bytes(),
        );
    }

    /// `enableSandbox()` (deprecated).
    pub fn enable_sandbox(&self) {
        Self::deprecated("enableSandbox");
        self.checker.set_sandboxed(true);
    }

    /// `disableSandbox()` (deprecated).
    pub fn disable_sandbox(&self) {
        Self::deprecated("disableSandbox");
        self.checker.set_sandboxed(false);
    }

    /// `isSandboxed($source)`.
    #[must_use]
    pub fn is_sandboxed(&self, source: Option<&Source>) -> bool {
        self.checker.is_sandboxed(source)
    }

    /// `isSandboxedGlobally()` (deprecated).
    #[must_use]
    pub fn is_sandboxed_globally(&self) -> bool {
        Self::deprecated("isSandboxedGlobally");
        self.checker.is_sandboxed_globally()
    }

    /// `setSecurityPolicy()`.
    pub fn set_security_policy(&self, policy: Rc<dyn SecurityPolicyInterface>) {
        self.checker.set_security_policy(policy);
    }

    /// `getSecurityPolicy()`.
    #[must_use]
    pub fn security_policy(&self) -> Rc<dyn SecurityPolicyInterface> {
        self.checker.security_policy()
    }
}

impl Extension for SandboxExtension {
    fn name(&self) -> &str {
        Self::CLASS
    }

    fn token_parsers(&self) -> Vec<Rc<dyn TokenParser>> {
        vec![Rc::new(SandboxTokenParser)]
    }

    fn node_visitors(&self) -> Vec<Box<dyn NodeVisitor>> {
        vec![Box::new(SandboxNodeVisitor::default())]
    }

    fn sandbox(&self) -> Option<&SandboxExtension> {
        Some(self)
    }
}
