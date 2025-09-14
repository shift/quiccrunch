# Context: Complete RPC Abstraction Layer

**Objective:** Implement the missing functionality in the RPC abstraction layer, including asynchronous streaming and procedural macro code generation.

**Analysis of `quiccrunch-core/src/rpc.rs`:**
- The `RpcStreamReceiver`'s `poll_next` method is a placeholder that always returns `Poll::Pending`, making asynchronous streaming non-functional.

**Analysis of `quiccrunch-macros/src/rpc_service.rs`:**
- The `#[rpc_service]` macro only generates placeholder structs and does not generate the necessary method implementations for serialization, deserialization, and network calls.

**Plan:**
1.  **Implement Asynchronous Streaming:**
    - In `quiccrunch-core/src/rpc.rs`, implement the `poll_next` method in `RpcStreamReceiver` to correctly poll the underlying stream and return data.
2.  **Complete Procedural Macros:**
    - In `quiccrunch-macros/src/rpc_service.rs`, expand the `#[rpc_service]` macro to generate fully functional client and server stubs. This includes:
        - Serialization/deserialization of Cap'n Proto messages.
        - Client-side logic to send requests and receive responses.
        - Server-side logic to dispatch calls to the correct handler methods.
3.  **Testing:** Create integration tests that use the `#[rpc_service]` macro to define and test a complete RPC service with streaming.
