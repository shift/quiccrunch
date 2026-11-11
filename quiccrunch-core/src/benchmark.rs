//! Benchmark suite module.
//!
//! The historical benchmark harness lived outside the workspace. This module
//! keeps the `benchmark-suite` feature compiling; add suite code here rather
//! than re-enabling the legacy standalone benchmarks.

/// Version identifier for the benchmark suite surface.
pub const SUITE_VERSION: &str = env!("CARGO_PKG_VERSION");
