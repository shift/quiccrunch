// Copyright (c) 2025 QuicCrunch Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # QuicCrunch FFI
//!
//! C-compatible Foreign Function Interface (FFI) for the QuicCrunch library.
//! This provides a C API that can be used to create language bindings for Python,
//! Node.js, Go, Java, C#, and other languages.
//!
//! ## Safety
//! 
//! All functions in this FFI are marked as `unsafe` externally since they deal with
//! raw pointers and C-style memory management. However, the implementations are safe
//! when used correctly according to the documented contracts.
//!
//! ## Memory Management
//!
//! - All `*_new()` functions return owned pointers that must be freed with `*_free()`
//! - String parameters are borrowed and do not need to be freed by the caller
//! - String return values are owned and must be freed with `quiccrunch_string_free()`
//!
//! ## Async Operations
//!
//! Since C doesn't have async/await, all async operations are handled via callbacks.
//! The FFI layer manages a tokio runtime internally to bridge async Rust code with
//! synchronous C callbacks.

pub mod error;
pub mod client;
pub mod server;
pub mod types;
pub mod runtime;

pub use error::*;
pub use client::*;
pub use server::*;
pub use types::*;
pub use runtime::*;

use std::os::raw::{c_char, c_void};
use std::ffi::{CStr, CString};
use once_cell::sync::Lazy;

/// Global tokio runtime for handling async operations
static RUNTIME: Lazy<tokio::runtime::Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime for FFI")
});

/// Initialize the QuicCrunch FFI library.
/// This must be called before using any other FFI functions.
/// 
/// Returns 0 on success, non-zero on error.
#[no_mangle]
pub extern "C" fn quiccrunch_init() -> QuicError {
    // Initialize tracing if not already done
    let _ = tracing_subscriber::fmt::try_init();
    
    // Test that the runtime is working
    RUNTIME.block_on(async {
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    });
    
    QuicError::Success
}

/// Clean up and shutdown the QuicCrunch FFI library.
/// This should be called when done using the library.
#[no_mangle]
pub extern "C" fn quiccrunch_shutdown() -> QuicError {
    // Note: We can't actually shutdown the runtime since it's a Lazy static
    // and tokio::runtime::Runtime doesn't have a proper shutdown method
    // that we can call from FFI safely. The runtime will be cleaned up
    // when the process exits.
    QuicError::Success
}

/// Free a string that was allocated by QuicCrunch FFI functions.
/// 
/// # Safety
/// The string pointer must have been returned by a QuicCrunch FFI function
/// and must not be used after calling this function.
#[no_mangle]
pub unsafe extern "C" fn quiccrunch_string_free(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}

/// Get the version of the QuicCrunch library.
/// Returns a null-terminated string that must be freed with quiccrunch_string_free().
#[no_mangle]
pub extern "C" fn quiccrunch_version() -> *mut c_char {
    let version = env!("CARGO_PKG_VERSION");
    match CString::new(version) {
        Ok(c_str) => c_str.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}