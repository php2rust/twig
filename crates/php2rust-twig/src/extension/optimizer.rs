//! Port of Twig\Extension\OptimizerExtension.php: the optimizer visitor
//! with the environment's `optimizations` mode.
//!
//! Upstream: twig/twig src/Extension/OptimizerExtension.php

use super::Extension;
use crate::node_visitor::{NodeVisitor, optimizer::OptimizerNodeVisitor};

/// `OptimizerExtension`.
#[derive(Debug)]
pub struct OptimizerExtension {
    optimizers: i64,
}

impl OptimizerExtension {
    /// `new OptimizerExtension($optimizers)`, for a mode that
    /// `OptimizerNodeVisitor::check_mode()` accepted.
    pub fn new(optimizers: i64) -> Self {
        Self { optimizers }
    }
}

impl Extension for OptimizerExtension {
    fn name(&self) -> &'static str {
        "Twig\\Extension\\OptimizerExtension"
    }

    fn node_visitors(&self) -> Vec<Box<dyn NodeVisitor>> {
        vec![Box::new(OptimizerNodeVisitor::new(self.optimizers))]
    }
}
