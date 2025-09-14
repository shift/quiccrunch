# Task: Fix Concurrency Model with DashMap

**Objective:** Replace the `Mutex<HashMap>` with `DashMap` in `quiccrunch-core/src/lockfree.rs`.

1.  **Add Dependency:**
    - Add `dashmap` to `quiccrunch-core/Cargo.toml`.

2.  **Implementation:**
    - In `quiccrunch-core/src/lockfree.rs`:
        - Import `dashmap::DashMap`.
        - Change the type of the `streams` field in `ConnectionHandle` to `Arc<DashMap<u64, StreamHandle>>`.
        - Update the stream management methods to use the `DashMap` API (e.g., `insert`, `get`, `remove`).

3.  **Verification:**
    - Run `nix develop -c -- cargo test --package quiccrunch-core` to ensure all tests pass.
