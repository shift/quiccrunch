# Task: Implement Real tQUIC Transport

**Objective:** Replace the mocked tQUIC transport layer with a functional implementation using the `tquic` library.

1.  **Read and Understand:**
    - Read `quiccrunch-core/src/transport/tquic.rs`.
    - Read `quiccrunch-core/src/transport/mod.rs` to understand the `Transport` trait.

2.  **Implementation:**
    - In `quiccrunch-core/src/transport/tquic.rs`:
        - Remove the `_MockPacketHandler` and associated simulation logic.
        - Modify `TquicTransport` to use `tquic::TquicClient` and `tquic::TquicServer`.
        - Implement the `connect` and `listen` methods using the actual `tquic` API.
        - Ensure all methods of the `Transport` trait are correctly implemented.

3.  **Verification:**
    - Run `nix develop -c -- cargo test --package quiccrunch-core` to ensure all tests pass.
