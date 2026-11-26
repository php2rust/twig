//! Twig's integration fixtures, compiled to Rust at build time: the
//! extension of tests/IntegrationTest.php and the Rust functions its
//! callables call. The fixtures run in tests/integration.rs.
//!
//! Upstream: twig/twig tests/IntegrationTest.php

pub mod callables;
pub mod classes;
pub mod extension;
pub mod legacy_sandbox;
pub mod legacy_sandbox_extension;
pub mod sandbox;
pub mod sandbox_extension;
