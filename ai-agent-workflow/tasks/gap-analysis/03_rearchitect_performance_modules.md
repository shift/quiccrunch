# Task: Re-architect High-Performance Modules

**Objective:** Fix the `io_uring` and `simd` modules.

1.  **`io_uring` Implementation:**
    - In `quiccrunch-core/src/io_uring.rs`:
        - Remove the `submit_and_wait(1)` call.
        - Implement a non-blocking loop to submit I/O requests and process completion queue events.

2.  **`simd` Validation:**
    - Create a new benchmark in `quiccrunch-core/benches/` to test the `fast_copy` function with various buffer sizes (e.g., 1KB, 4KB, 16KB).
    - Analyze the benchmark results to determine if the SIMD optimization is effective for realistic RPC message sizes.
    - If not effective, remove or disable the feature.

3.  **Verification:**
    - Run `nix develop -c -- cargo test --package quiccrunch-core`.
    - Run the new benchmark to confirm performance characteristics.
