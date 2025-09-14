# Context: Fix Mock tQUIC Transport Layer

**Objective:** Replace the mocked tQUIC transport layer in `quiccrunch-core/src/transport/tquic.rs` with a functional implementation that integrates the actual `tquic` library.

**Analysis of `quiccrunch-core/src/transport/tquic.rs`:**
- The current implementation uses a `_MockPacketHandler` to simulate network I/O.
- RPC calls are simulated by echoing data back within spawned Tokio tasks.
- The code contains explicit comments confirming its non-functional state, such as "Simplified tQUIC context without direct tQUIC integration for now."
- The existing implementation does not use the `tquic` crate for any actual network operations.

**Plan:**
1.  **Remove Mock Implementation:** Delete the `_MockPacketHandler` and all associated simulation logic from `tquic.rs`.
2.  **Integrate `tquic`:** Modify the `TquicTransport` struct and its methods to use the `tquic` library's API for creating and managing QUIC connections, streams, and packet handling.
3.  **Implement `Transport` Trait:** Ensure the updated `TquicTransport` correctly implements the `Transport` trait defined in `quiccrunch-core/src/transport/mod.rs`.
4.  **Testing:** After implementation, run the project's test suite to validate the changes and ensure no regressions were introduced.
