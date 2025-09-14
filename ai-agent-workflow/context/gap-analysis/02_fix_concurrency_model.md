# Context: Fix Flawed Concurrency Model

**Objective:** Replace the `tokio::sync::Mutex` in `quiccrunch-core/src/lockfree.rs` with a concurrent, sharded hash map to fix the performance bottleneck in stream management.

**Analysis of `quiccrunch-core/src/lockfree.rs`:**
- The `ConnectionHandle` struct uses `Arc<Mutex<HashMap<u64, StreamHandle>>>` to manage streams.
- A comment in the code, `// PERFORMANCE CRITICAL: This Mutex negates "lock-free" benefits!`, explicitly identifies this as a major bottleneck.
- This design serializes all stream creation and management, preventing true concurrency.

**Plan:**
1.  **Introduce `dashmap`:** Add the `dashmap` crate as a dependency in `quiccrunch-core/Cargo.toml`.
2.  **Replace `Mutex<HashMap>`:** In `quiccrunch-core/src/lockfree.rs`, replace `Arc<Mutex<HashMap<u64, StreamHandle>>>` with `Arc<DashMap<u64, StreamHandle>>`.
3.  **Update Stream Management Logic:** Modify the methods within `ConnectionHandle` (e.g., `new_stream`, `get_stream`, `remove_stream`) to use `dashmap`'s API. This involves replacing `lock().await` with direct, thread-safe calls.
4.  **Testing:** Run the project's test suite to ensure the new implementation is correct and performant.
