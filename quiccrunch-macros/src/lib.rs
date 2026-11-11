//! # QuicCrunch Procedural Macros
//!
//! This crate provides procedural macros for the QuicCrunch RPC framework,
//! enabling ergonomic code generation for RPC services and clients.
//!
//! ## Available Macros
//!
//! - `#[rpc_service]` - Derive macro for generating RPC service implementations
//! - `#[rpc_client]` - Derive macro for generating RPC client stubs
//! - `rpc_service!` - Declarative macro for service definitions (legacy)
//!
//! ## Example Usage
//!
//! ```no_run
//! use quiccrunch_macros::rpc_service;
//! use quiccrunch_core::rpc::{RpcContext, RpcResult};
//!
//! #[rpc_service]
//! trait Calculator {
//!     async fn add(&self, ctx: RpcContext, a: i32, b: i32) -> RpcResult<i32>;
//!     async fn subtract(&self, ctx: RpcContext, a: i32, b: i32) -> RpcResult<i32>;
//! }
//!
//! struct CalculatorImpl;
//!
//! impl Calculator for CalculatorImpl {
//!     async fn add(&self, _ctx: RpcContext, a: i32, b: i32) -> RpcResult<i32> {
//!         Ok(a + b)
//!     }
//!     
//!     async fn subtract(&self, _ctx: RpcContext, a: i32, b: i32) -> RpcResult<i32> {
//!         Ok(a - b)
//!     }
//! }
//! ```

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, punctuated::Punctuated, DeriveInput, ItemTrait, Meta, Token};

mod rpc_client;
mod rpc_service;
mod utils;

/// Derive macro for generating RPC service implementations.
///
/// This macro generates the necessary boilerplate code for implementing
/// an RPC service with the QuicCrunch framework, including:
///
/// - Cap'n Proto serialization/deserialization
/// - Method dispatch routing
/// - Error handling
/// - Streaming support
///
/// ## Attributes
///
/// - `#[rpc_service(name = "ServiceName")]` - Override the service name
/// - `#[rpc_method(streaming = "server")]` - Mark method as server streaming
/// - `#[rpc_method(streaming = "client")]` - Mark method as client streaming  
/// - `#[rpc_method(streaming = "bidirectional")]` - Mark method as bidirectional streaming
///
/// ## Example
///
/// ```no_run
/// use quiccrunch_macros::rpc_service;
/// use quiccrunch_core::rpc::{RpcContext, RpcResult, RpcStreamSink};
///
/// #[rpc_service(name = "Calculator")]
/// trait Calculator {
///     async fn add(&self, ctx: RpcContext, a: i32, b: i32) -> RpcResult<i32>;
///     
///     // Note: streaming attributes will be implemented in future versions
///     async fn fibonacci(&self, ctx: RpcContext, n: u32) -> RpcResult<RpcStreamSink<u64>>;
/// }
/// ```
#[proc_macro_attribute]
pub fn rpc_service(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let input = parse_macro_input!(input as ItemTrait);

    match rpc_service::expand_rpc_service(args, input) {
        Ok(expanded) => expanded.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Derive macro for generating RPC client stubs.
///
/// This macro generates client-side stub code for calling RPC services,
/// including connection management, method calls, and response handling.
///
/// ## Example
///
/// ```ignore
/// use quiccrunch_macros::rpc_client;
/// use quiccrunch_core::rpc::{RpcClient, RpcResult};
///
/// #[rpc_client(service = "Calculator")]
/// struct CalculatorClient;
///
/// // The macro generates:
/// // - A `client: RpcClient` field
/// // - RPC method implementations based on the service definition
/// ```
#[proc_macro_attribute]
pub fn rpc_client(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated::<Meta, Token![,]>::parse_terminated);
    let input = parse_macro_input!(input as DeriveInput);

    match rpc_client::expand_rpc_client(args, input) {
        Ok(expanded) => expanded.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Helper macro for backwards compatibility with the legacy macro-based approach.
///
/// This re-exports the existing declarative macro from the main quiccrunch crate
/// while we transition to the procedural macro approach.
#[proc_macro]
pub fn rpc_service_legacy(input: TokenStream) -> TokenStream {
    // For now, just pass through to show the approach
    // In practice, this would parse and generate the same as the old declarative macro
    let _input = proc_macro2::TokenStream::from(input);
    quote! {
        compile_error!("Legacy rpc_service! macro - use #[rpc_service] attribute instead");
    }
    .into()
}
