//! Port of Twig\Sandbox\Sandbox.php and SandboxInterface.php: an
//! environment that renders untrusted templates, always sandboxed, with a
//! strict policy.
//!
//! PHP adds the sandbox extension to an environment that the caller made;
//! a Rust environment takes its extensions when it is made, so
//! [`Sandbox::new`] makes it from the extensions and options. The
//! environment is [`Sandbox::environment`] (set its loader there). PHP's
//! `display()` and `stream()` write to the output; here
//! [`Sandbox::render`] returns it. `createTemplate()` is not ported
//! (templates are compiled ahead of time).
//!
//! Upstream: twig/twig src/Sandbox/Sandbox.php src/Sandbox/SandboxInterface.php

use std::rc::Rc;

use super::SecurityPolicyInterface;
use crate::{
    environment::{Environment, EnvironmentOptions},
    error::Error,
    extension::{Extension, sandbox::SandboxExtension},
    template::Context,
};

/// `Sandbox`.
pub struct Sandbox {
    env: Environment,
}

impl Sandbox {
    /// `new Sandbox($env, $policy)`: the environment of these extensions
    /// and options, with a sandbox extension that is always on. A
    /// [`super::SecurityPolicy`] that is not strict is refused, and so are
    /// extensions that already hold a sandbox extension.
    pub fn new(
        extensions: Vec<Rc<dyn Extension>>,
        options: EnvironmentOptions,
        policy: Rc<dyn SecurityPolicyInterface>,
    ) -> Result<Self, Error> {
        if policy.as_security_policy().is_some_and(|p| !p.is_strict()) {
            return Err(Error::logic(
                "The sandbox requires a strict security policy, call \"setStrict(true)\" on a dedicated policy for this sandbox.",
            ));
        }
        if extensions.iter().any(|e| e.sandbox().is_some()) {
            return Err(Error::logic(format!(
                "The environment passed to \"Twig\\Sandbox\\Sandbox\" must be dedicated to it: pass a freshly built environment that has no \"{}\" registered and has not been used yet.",
                SandboxExtension::CLASS
            )));
        }
        let mut extensions = extensions;
        extensions.push(Rc::new(SandboxExtension::new(policy, true, None)));
        Ok(Self {
            env: Environment::with_options(extensions, options)?,
        })
    }

    /// The sandbox's environment.
    #[must_use]
    pub fn environment(&self) -> &Environment {
        &self.env
    }

    /// `render($name, $context)`.
    pub fn render(&self, name: &[u8], context: Context) -> Result<Vec<u8>, Error> {
        self.env.render(name, context)
    }

    /// `renderBlock($name, $block, $context)`.
    pub fn render_block(
        &self,
        name: &[u8],
        block: &[u8],
        context: Context,
    ) -> Result<Vec<u8>, Error> {
        self.env.load(name)?.render_block(&self.env, block, context)
    }
}
