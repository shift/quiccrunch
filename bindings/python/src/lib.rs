use anyhow::Result;
use once_cell::sync::Lazy;
use pyo3::prelude::*;
use pyo3_asyncio::tokio::future_into_py;
use std::sync::Arc;
use tokio::sync::Mutex;

mod client;
mod server;
mod error;

pub use client::*;
pub use server::*;
pub use error::*;

// Global tokio runtime for async operations
static RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime for Python bindings")
});

/// Initialize the QuicCrunch library
#[pyfunction]
fn init() -> PyResult<()> {
    // Initialize tracing
    let _ = tracing_subscriber::fmt::try_init();
    
    // Test runtime
    RUNTIME.block_on(async {
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    });
    
    Ok(())
}

/// Get the QuicCrunch library version
#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Python module definition
#[pymodule]
fn _quiccrunch(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(init, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    
    m.add_class::<QuicClient>()?;
    m.add_class::<QuicServer>()?;
    m.add_class::<QuicCrunchError>()?;
    
    Ok(())
}