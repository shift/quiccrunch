# Task: Complete RPC Abstraction Layer

**Objective:** Implement asynchronous streaming and complete the procedural macro for the RPC framework.

1.  **Implement `poll_next`:**
    - In `quiccrunch-core/src/rpc.rs`:
        - Implement the `poll_next` method in `RpcStreamReceiver` to correctly handle asynchronous stream data.

2.  **Expand `rpc_service` Macro:**
    - In `quiccrunch-macros/src/rpc_service.rs`:
        - Use `syn` and `quote` to parse the service definition.
        - Generate client and server structs with method implementations for each RPC call.
        - Include logic for serialization/deserialization and network transport.

3.  **Verification:**
    - Create a new example in `quiccrunch-macros/examples/` that defines a service with the `#[rpc_service]` macro and tests its functionality.
    - Run `nix develop -c -- cargo test --workspace` to ensure all tests pass.
