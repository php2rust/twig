//! Ports of `src/Sandbox`: the security policy, the checker that the
//! compiled templates call, and the `Sandbox` facade.
//!
//! The `SecurityError` classes are kinds of [`crate::error::Error`]
//! (`ErrorKind::Security*`).
//!
//! Upstream: twig/twig src/Sandbox/SecurityPolicyInterface.php src/Sandbox/SourcePolicyInterface.php

#[allow(
    clippy::module_inception,
    reason = "the module of src/Sandbox/Sandbox.php"
)]
pub mod sandbox;
pub mod security_checker;
pub mod security_policy;

use php_value::PhpObject;

pub use sandbox::Sandbox;
pub use security_checker::SecurityChecker;
pub use security_policy::SecurityPolicy;

use crate::{error::Error, source::Source};

/// `SecurityPolicyInterface`.
pub trait SecurityPolicyInterface {
    /// `checkSecurity($tags, $filters, $functions, $tests)`.
    fn check_security(
        &self,
        tags: &[Vec<u8>],
        filters: &[Vec<u8>],
        functions: &[Vec<u8>],
        tests: &[Vec<u8>],
    ) -> Result<(), Error>;

    /// `checkMethodAllowed($obj, $method)`.
    fn check_method_allowed(&self, object: &dyn PhpObject, method: &[u8]) -> Result<(), Error>;

    /// `checkPropertyAllowed($obj, $property)`.
    fn check_property_allowed(&self, object: &dyn PhpObject, property: &[u8]) -> Result<(), Error>;

    /// The policy as a [`SecurityPolicy`] (`$policy instanceof
    /// SecurityPolicy`).
    fn as_security_policy(&self) -> Option<&SecurityPolicy> {
        None
    }
}

/// `SourcePolicyInterface` (deprecated since Twig 3.27): turns the
/// sandbox on for some templates.
pub trait SourcePolicyInterface {
    /// `enableSandbox($source)`.
    fn enable_sandbox(&self, source: &Source) -> bool;
}

/// `$obj instanceof $class`.
pub(crate) fn instance_of(object: &dyn PhpObject, class: &str) -> bool {
    let class = class.trim_start_matches('\\');
    object.class().eq_ignore_ascii_case(class)
        || object.instance_of(class)
        || (class.eq_ignore_ascii_case("Stringable") && object.is_stringable())
        || (class.eq_ignore_ascii_case("Traversable") && object.traverse().is_some())
}
