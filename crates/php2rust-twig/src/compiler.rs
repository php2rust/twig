//! Port of Twig\Compiler.php (stub).
//!
//! Upstream: twig/twig src/Compiler.php

use crate::{environment::Environment, error::Error, node::Node};

pub struct Compiler<'a> {
    env: &'a Environment,
}

impl<'a> Compiler<'a> {
    pub fn new(env: &'a Environment) -> Self {
        Self { env }
    }

    pub fn compile(&mut self, node: &Node) -> Result<String, Error> {
        let _ = (self.env, node);
        Ok(String::new())
    }
}
