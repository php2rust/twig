//! The sandbox calls of compiled templates: what PHP's generated
//! `checkSecurity()`, `checkTraitsAllowed()` and `CheckToStringNode`
//! code does with `$this->sandbox`.
//!
//! Upstream: twig/twig src/Node/CheckSecurityNode.php src/Node/CheckSecurityCallNode.php src/Node/CheckToStringNode.php

use std::sync::Arc;

use php_value::Value;

use super::R;
use crate::{
    environment::Environment,
    error::{Error, ErrorKind},
    extension::sandbox::SandboxExtension,
    sandbox::SecurityChecker,
    source::Source,
};

/// `$env->getExtension(SandboxExtension::class)->getChecker()`.
pub fn checker(env: &Environment) -> R<&SecurityChecker> {
    env.sandbox().map(SandboxExtension::checker).ok_or_else(|| {
        Error::runtime(
            format!(
                "The \"{}\" extension is not enabled.",
                SandboxExtension::CLASS
            ),
            -1,
            None,
        )
    })
}

/// `$env->hasExtension(SandboxExtension::class) &&
/// $env->getExtension(SandboxExtension::class)->getChecker()->isSandboxed($source)`.
#[must_use]
pub fn is_sandboxed(env: &Environment, source: &Arc<Source>) -> bool {
    env.sandbox()
        .is_some_and(|s| s.checker().is_sandboxed(Some(source)))
}

/// A used name and the line of its first use.
pub type Used<'a> = &'a [(&'a [u8], i64)];

fn names(used: Used<'_>) -> Vec<Vec<u8>> {
    used.iter().map(|(n, _)| n.to_vec()).collect()
}

/// The line PHP's `static $tags` (or filters, ...) gives a name: lines
/// that are 0 are filtered out (`array_filter()`).
fn line_of(used: Used<'_>, name: Option<&[u8]>) -> Option<i64> {
    let name = name?;
    used.iter()
        .find(|(n, line)| *n == name && *line != 0)
        .map(|(_, line)| *line)
}

/// The generated `ensureSecurityChecked()` and `checkSecurity()`: when the
/// template is sandboxed, the policy checks what it uses; an error gets
/// the line of the first use.
pub fn ensure_security_checked(
    env: &Environment,
    source: &Arc<Source>,
    tags: Used<'_>,
    filters: Used<'_>,
    functions: Used<'_>,
    tests: Used<'_>,
) -> R<()> {
    let sandbox = checker(env)?;
    if !sandbox.is_sandboxed(Some(source)) {
        return Ok(());
    }
    sandbox
        .check_security(
            &names(tags),
            &names(filters),
            &names(functions),
            &names(tests),
            Some(source),
        )
        .map_err(|mut e| {
            let used = match e.kind() {
                ErrorKind::SecurityNotAllowedTag => tags,
                ErrorKind::SecurityNotAllowedFilter => filters,
                ErrorKind::SecurityNotAllowedFunction => functions,
                ErrorKind::SecurityNotAllowedTest => tests,
                _ => return e,
            };
            if let Some(line) = line_of(used, e.security_name()) {
                e.set_template_line(line);
            }
            e
        })
}

/// The generated `checkTraitsAllowed()` of a template with `use`: the
/// `use` tag alone is checked before the traits are loaded.
pub fn check_traits_allowed(env: &Environment, source: &Arc<Source>, line: i64) -> R<()> {
    let sandbox = checker(env)?;
    if !sandbox.is_sandboxed(Some(source)) {
        return Ok(());
    }
    sandbox
        .check_security(&[b"use".to_vec()], &[], &[], &[], Some(source))
        .map_err(|mut e| {
            if e.kind() == ErrorKind::SecurityNotAllowedTag {
                e.set_template_line(line);
            }
            e
        })
}

/// `CheckToStringNode`: `$this->sandbox->ensureToStringAllowed()`.
pub fn ensure_to_string_allowed(
    env: &Environment,
    value: Value,
    line: i64,
    source: &Arc<Source>,
) -> R {
    checker(env)?.ensure_to_string_allowed(value, line, Some(source))
}

/// `CheckToStringNode` of a spread operand:
/// `$this->sandbox->ensureSpreadAllowed()`.
pub fn ensure_spread_allowed(
    env: &Environment,
    value: Value,
    line: i64,
    source: &Arc<Source>,
) -> R {
    checker(env)?.ensure_spread_allowed(value, line, Some(source))
}
