//! Port of Twig\Sandbox\SecurityChecker.php: the sandbox state and the
//! checks that compiled templates run.
//!
//! Not ported: the deprecation for a policy whose `checkSecurity()` takes
//! no `$tests` argument (a Rust policy always takes it), and
//! `ReflectionReference` (a Rust array holds no PHP references, so an
//! array cannot contain itself).
//!
//! Upstream: twig/twig src/Sandbox/SecurityChecker.php

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

use php_value::{PhpArray, PhpObject, Value};

use super::{SecurityPolicyInterface, SourcePolicyInterface, instance_of};
use crate::{error::Error, runtime::core::iterator_to_array, source::Source};

/// `SecurityChecker`.
pub struct SecurityChecker {
    policy: RefCell<Rc<dyn SecurityPolicyInterface>>,
    sandboxed_globally: bool,
    source_policy: Option<Rc<dyn SourcePolicyInterface>>,
    sandboxed: Cell<bool>,
}

impl std::fmt::Debug for SecurityChecker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecurityChecker")
            .field("sandboxed_globally", &self.sandboxed_globally)
            .field("sandboxed", &self.sandboxed.get())
            .finish_non_exhaustive()
    }
}

/// Objects already checked, by address (`SplObjectStorage`).
type Seen = Vec<*const ()>;

fn address(object: &Arc<dyn PhpObject>) -> *const () {
    Arc::as_ptr(object).cast::<()>()
}

impl SecurityChecker {
    /// `new SecurityChecker($policy, $sandboxedGlobally, $sourcePolicy)`.
    pub fn new(
        policy: Rc<dyn SecurityPolicyInterface>,
        sandboxed_globally: bool,
        source_policy: Option<Rc<dyn SourcePolicyInterface>>,
    ) -> Self {
        Self {
            policy: RefCell::new(policy),
            sandboxed_globally,
            source_policy,
            sandboxed: Cell::new(false),
        }
    }

    /// `setSandboxed()`: the legacy sandbox state.
    pub fn set_sandboxed(&self, sandboxed: bool) {
        self.sandboxed.set(sandboxed);
    }

    /// `isSandboxed($source)`.
    #[must_use]
    pub fn is_sandboxed(&self, source: Option<&Source>) -> bool {
        self.sandboxed_globally || self.sandboxed.get() || self.is_source_sandboxed(source)
    }

    /// `isSandboxedGlobally()`.
    #[must_use]
    pub fn is_sandboxed_globally(&self) -> bool {
        self.sandboxed_globally
    }

    fn is_source_sandboxed(&self, source: Option<&Source>) -> bool {
        match (source, &self.source_policy) {
            (Some(source), Some(policy)) => policy.enable_sandbox(source),
            _ => false,
        }
    }

    /// `setSecurityPolicy()`.
    pub fn set_security_policy(&self, policy: Rc<dyn SecurityPolicyInterface>) {
        *self.policy.borrow_mut() = policy;
    }

    /// `getSecurityPolicy()`.
    #[must_use]
    pub fn security_policy(&self) -> Rc<dyn SecurityPolicyInterface> {
        Rc::clone(&self.policy.borrow())
    }

    /// `checkSecurity($tags, $filters, $functions, $tests, $source)`.
    pub fn check_security(
        &self,
        tags: &[Vec<u8>],
        filters: &[Vec<u8>],
        functions: &[Vec<u8>],
        tests: &[Vec<u8>],
        source: Option<&Arc<Source>>,
    ) -> Result<(), Error> {
        if !self.is_sandboxed(source.map(AsRef::as_ref)) {
            return Ok(());
        }
        self.security_policy()
            .check_security(tags, filters, functions, tests)
            .map_err(|mut e| {
                if e.kind().is_security_error() {
                    e.set_source_context(source.cloned());
                }
                e
            })
    }

    /// `checkMethodAllowed($obj, $method, $lineno, $source)`.
    pub fn check_method_allowed(
        &self,
        object: &dyn PhpObject,
        method: &[u8],
        lineno: i64,
        source: Option<&Arc<Source>>,
    ) -> Result<(), Error> {
        if !self.is_sandboxed(source.map(AsRef::as_ref)) {
            return Ok(());
        }
        self.security_policy()
            .check_method_allowed(object, method)
            .map_err(|e| {
                located(
                    e,
                    crate::error::ErrorKind::SecurityNotAllowedMethod,
                    lineno,
                    source,
                )
            })
    }

    /// `checkPropertyAllowed($obj, $property, $lineno, $source)`.
    pub fn check_property_allowed(
        &self,
        object: &dyn PhpObject,
        property: &[u8],
        lineno: i64,
        source: Option<&Arc<Source>>,
    ) -> Result<(), Error> {
        if !self.is_sandboxed(source.map(AsRef::as_ref)) {
            return Ok(());
        }
        self.security_policy()
            .check_property_allowed(object, property)
            .map_err(|e| {
                located(
                    e,
                    crate::error::ErrorKind::SecurityNotAllowedProperty,
                    lineno,
                    source,
                )
            })
    }

    /// `ensureToStringAllowed($obj, $lineno, $source)`: checks
    /// `__toString()` on a `Stringable` object and on what an array or a
    /// `Traversable` holds; an `Iterator` is read into an array, which is
    /// returned in its place (unless the iterator is `Stringable`).
    pub fn ensure_to_string_allowed(
        &self,
        value: Value,
        lineno: i64,
        source: Option<&Arc<Source>>,
    ) -> Result<Value, Error> {
        self.do_ensure_to_string_allowed(value, lineno, source, &mut Vec::new())
    }

    /// `ensureSpreadAllowed($obj, $lineno, $source)`: a spread operand
    /// read into an array, with the policy run on every element.
    pub fn ensure_spread_allowed(
        &self,
        value: Value,
        lineno: i64,
        source: Option<&Arc<Source>>,
    ) -> Result<Value, Error> {
        let mut seen = Vec::new();
        let array = match &value {
            Value::Object(o) if o.traverse().is_some() => {
                seen.push(address(o));
                iterator_to_array(&value, true)?
            }
            Value::Array(a) => a.clone(),
            _ => return Ok(value),
        };
        self.ensure_to_string_allowed_for_array(&array, lineno, source, &mut seen)?;
        Ok(Value::Array(array))
    }

    fn do_ensure_to_string_allowed(
        &self,
        value: Value,
        lineno: i64,
        source: Option<&Arc<Source>>,
        seen: &mut Seen,
    ) -> Result<Value, Error> {
        if let Value::Array(array) = &value {
            self.ensure_to_string_allowed_for_array(array, lineno, source, seen)?;
            return Ok(value);
        }
        if !self.is_sandboxed(source.map(AsRef::as_ref)) {
            return Ok(value);
        }
        let Value::Object(object) = &value else {
            return Ok(value);
        };
        if instance_of(&**object, "Stringable") {
            self.security_policy()
                .check_method_allowed(&**object, b"__toString")
                .map_err(|e| {
                    located(
                        e,
                        crate::error::ErrorKind::SecurityNotAllowedMethod,
                        lineno,
                        source,
                    )
                })?;
        }
        if object.traverse().is_none() {
            return Ok(value);
        }
        if seen.contains(&address(object)) {
            return Ok(value);
        }
        seen.push(address(object));
        if instance_of(&**object, "IteratorAggregate") {
            for (_, element) in object.traverse().unwrap_or(Ok(Vec::new()))? {
                self.do_ensure_to_string_allowed(element, lineno, source, seen)?;
            }
            return Ok(value);
        }
        let array = iterator_to_array(&value, true)?;
        self.ensure_to_string_allowed_for_array(&array, lineno, source, seen)?;
        if instance_of(&**object, "Stringable") {
            return Ok(value);
        }
        Ok(Value::Array(array))
    }

    fn ensure_to_string_allowed_for_array(
        &self,
        array: &PhpArray,
        lineno: i64,
        source: Option<&Arc<Source>>,
        seen: &mut Seen,
    ) -> Result<(), Error> {
        for (_, element) in array.iter() {
            if !element.is_truthy() {
                continue;
            }
            match element {
                Value::Array(inner) => {
                    self.ensure_to_string_allowed_for_array(inner, lineno, source, seen)?;
                }
                _ => {
                    self.do_ensure_to_string_allowed(element.clone(), lineno, source, seen)?;
                }
            }
        }
        Ok(())
    }
}

/// `catch (SecurityNotAllowed<X>Error $e) { $e->setSourceContext($source);
/// $e->setTemplateLine($lineno); }`.
fn located(
    mut e: Error,
    kind: crate::error::ErrorKind,
    lineno: i64,
    source: Option<&Arc<Source>>,
) -> Error {
    if e.kind() == kind {
        e.set_source_context(source.cloned());
        e.set_template_line(lineno);
    }
    e
}
