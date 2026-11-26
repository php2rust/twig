//! Port of Twig\Sandbox\SecurityPolicy.php: allow-lists of tags, filters,
//! functions, tests, methods and properties.
//!
//! The allow-lists can change after the policy is given to a sandbox (the
//! setters take `&self`), as in PHP. Methods and properties are listed by
//! class: an object is allowed when it is an instance of the class
//! (`instanceof`, its own class, a parent or an interface it reports).
//!
//! Upstream: twig/twig src/Sandbox/SecurityPolicy.php

use std::cell::{Cell, RefCell};

use php_value::PhpObject;
use symfony_deprecation_contracts::trigger_deprecation;

use super::{SecurityPolicyInterface, instance_of};
use crate::{
    error::{Error, ErrorKind},
    markup::Markup,
};

/// `SecurityPolicy`.
#[derive(Debug, Default)]
pub struct SecurityPolicy {
    allowed_tags: RefCell<Vec<Vec<u8>>>,
    allowed_filters: RefCell<Vec<Vec<u8>>>,
    /// class => lower-case method names.
    allowed_methods: RefCell<Vec<(String, Vec<Vec<u8>>)>>,
    /// class => property names.
    allowed_properties: RefCell<Vec<(String, Vec<Vec<u8>>)>>,
    allowed_functions: RefCell<Vec<Vec<u8>>>,
    allowed_tests: RefCell<Vec<Vec<u8>>>,
    strict: Cell<bool>,
}

fn names(list: &[&str]) -> Vec<Vec<u8>> {
    list.iter().map(|s| s.as_bytes().to_vec()).collect()
}

fn members(list: &[(&str, &[&str])], lower: bool) -> Vec<(String, Vec<Vec<u8>>)> {
    list.iter()
        .map(|(class, members)| {
            let members = members
                .iter()
                .map(|m| {
                    if lower {
                        m.to_ascii_lowercase().into_bytes()
                    } else {
                        m.as_bytes().to_vec()
                    }
                })
                .collect();
            ((*class).to_owned(), members)
        })
        .collect()
}

impl SecurityPolicy {
    /// `new SecurityPolicy($allowedTags, $allowedFilters, $allowedMethods,
    /// $allowedProperties, $allowedFunctions, $allowedTests)`.
    #[must_use]
    pub fn new(
        tags: &[&str],
        filters: &[&str],
        methods: &[(&str, &[&str])],
        properties: &[(&str, &[&str])],
        functions: &[&str],
        tests: &[&str],
    ) -> Self {
        let policy = Self::default();
        policy.set_allowed_tags(tags);
        policy.set_allowed_filters(filters);
        policy.set_allowed_methods(methods);
        policy.set_allowed_properties(properties);
        policy.set_allowed_functions(functions);
        policy.set_allowed_tests(tests);
        policy
    }

    /// `setAllowedTags()`.
    pub fn set_allowed_tags(&self, tags: &[&str]) {
        *self.allowed_tags.borrow_mut() = names(tags);
    }

    /// `setAllowedFilters()`.
    pub fn set_allowed_filters(&self, filters: &[&str]) {
        *self.allowed_filters.borrow_mut() = names(filters);
    }

    /// `setAllowedMethods()`: method names are matched without case.
    pub fn set_allowed_methods(&self, methods: &[(&str, &[&str])]) {
        *self.allowed_methods.borrow_mut() = members(methods, true);
    }

    /// `setAllowedProperties()`.
    pub fn set_allowed_properties(&self, properties: &[(&str, &[&str])]) {
        *self.allowed_properties.borrow_mut() = members(properties, false);
    }

    /// `setAllowedFunctions()`.
    pub fn set_allowed_functions(&self, functions: &[&str]) {
        *self.allowed_functions.borrow_mut() = names(functions);
    }

    /// `setAllowedTests()`.
    pub fn set_allowed_tests(&self, tests: &[&str]) {
        *self.allowed_tests.borrow_mut() = names(tests);
    }

    /// `setStrict()`: in strict mode the `extends` and `use` tags, the
    /// `parent`, `block` and `attribute` functions and all tests must be
    /// in the allow-lists too (the Twig 4.0 behaviour).
    pub fn set_strict(&self, strict: bool) {
        self.strict.set(strict);
    }

    /// `isStrict()`.
    #[must_use]
    pub fn is_strict(&self) -> bool {
        self.strict.get()
    }
}

fn always_allowed(what: &str, name: &str, version: &str) {
    trigger_deprecation(
        b"twig/twig",
        version.as_bytes(),
        format!(
            "The \"{name}\" {what} is always allowed in sandboxes, but won't be in 4.0, please enable it explicitly in your sandbox policy if needed (or enable strict mode on the security policy to opt-in to the 4.0 behavior now)."
        )
        .as_bytes(),
    );
}

fn text(name: &[u8]) -> String {
    String::from_utf8_lossy(name).into_owned()
}

impl SecurityPolicyInterface for SecurityPolicy {
    fn check_security(
        &self,
        tags: &[Vec<u8>],
        filters: &[Vec<u8>],
        functions: &[Vec<u8>],
        tests: &[Vec<u8>],
    ) -> Result<(), Error> {
        let strict = self.strict.get();
        for tag in tags {
            if !self.allowed_tags.borrow().contains(tag) {
                match tag.as_slice() {
                    b"extends" | b"use" if !strict => always_allowed("tag", &text(tag), "3.12"),
                    _ => {
                        return Err(Error::security_not_allowed(
                            ErrorKind::SecurityNotAllowedTag,
                            format!("Tag \"{}\" is not allowed.", text(tag)),
                            tag,
                        ));
                    }
                }
            }
        }
        for filter in filters {
            if !self.allowed_filters.borrow().contains(filter) {
                return Err(Error::security_not_allowed(
                    ErrorKind::SecurityNotAllowedFilter,
                    format!("Filter \"{}\" is not allowed.", text(filter)),
                    filter,
                ));
            }
        }
        for function in functions {
            if !self.allowed_functions.borrow().contains(function) {
                match function.as_slice() {
                    b"parent" | b"block" | b"attribute" if !strict => {
                        always_allowed("function", &text(function), "3.27");
                    }
                    _ => {
                        return Err(Error::security_not_allowed(
                            ErrorKind::SecurityNotAllowedFunction,
                            format!("Function \"{}\" is not allowed.", text(function)),
                            function,
                        ));
                    }
                }
            }
        }
        for test in tests {
            if !self.allowed_tests.borrow().contains(test) {
                if strict {
                    return Err(Error::security_not_allowed(
                        ErrorKind::SecurityNotAllowedTest,
                        format!("Test \"{}\" is not allowed.", text(test)),
                        test,
                    ));
                }
                always_allowed("test", &text(test), "3.28");
            }
        }
        Ok(())
    }

    fn check_method_allowed(&self, object: &dyn PhpObject, method: &[u8]) -> Result<(), Error> {
        if instance_of(object, "Twig\\Template") || instance_of(object, Markup::CLASS) {
            return Ok(());
        }
        let method = method.to_ascii_lowercase();
        let allowed = self
            .allowed_methods
            .borrow()
            .iter()
            .any(|(class, methods)| instance_of(object, class) && methods.contains(&method));
        if allowed {
            return Ok(());
        }
        let class = object.class();
        Err(Error::security_not_allowed_member(
            ErrorKind::SecurityNotAllowedMethod,
            format!(
                "Calling \"{}\" method on a \"{class}\" object is not allowed.",
                text(&method)
            ),
            class,
            &method,
        ))
    }

    fn check_property_allowed(&self, object: &dyn PhpObject, property: &[u8]) -> Result<(), Error> {
        let allowed = self
            .allowed_properties
            .borrow()
            .iter()
            .any(|(class, properties)| {
                instance_of(object, class) && properties.iter().any(|p| p == property)
            });
        if allowed {
            return Ok(());
        }
        let class = object.class();
        Err(Error::security_not_allowed_member(
            ErrorKind::SecurityNotAllowedProperty,
            format!(
                "Calling \"{}\" property on a \"{class}\" object is not allowed.",
                text(property)
            ),
            class,
            property,
        ))
    }

    fn as_security_policy(&self) -> Option<&SecurityPolicy> {
        Some(self)
    }
}
