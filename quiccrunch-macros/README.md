# QuicCrunch Procedural Macros

This crate provides procedural macros for the QuicCrunch RPC framework, enabling ergonomic code generation for RPC services and clients.

## Features

- `#[rpc_service]` - Automatic RPC service implementation generation
- `#[rpc_client]` - Client stub generation
- Streaming RPC support
- Cap'n Proto integration
- Type-safe code generation

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
quiccrunch-macros = "0.1.0"
```

Then use the macros in your code:

```rust
use quiccrunch_macros::rpc_service;
use quiccrunch::rpc::{RpcContext, RpcResult};

#[rpc_service]
trait Calculator {
    async fn add(&self, ctx: RpcContext, a: i32, b: i32) -> RpcResult<i32>;
}
```