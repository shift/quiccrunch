# Task: Build Realistic Performance Benchmark

**Objective:** Update `grpc_comparison.rs` to be a true end-to-end benchmark.

1.  **Define Schema:**
    - Create a new `.capnp` file in `quiccrunch-core/proto/` with a representative schema (e.g., a `UserData` struct with a few fields).

2.  **Update Benchmark:**
    - In `quiccrunch-core/examples/grpc_comparison.rs`:
        - Use the `#[rpc_service]` macro to define a service using the new schema.
        - Implement the server logic to handle the RPC call.
        - Update the client to serialize a Cap'n Proto message, make the RPC call, and deserialize the response.

3.  **Verification:**
    - Run the benchmark and analyze the output to ensure it is measuring end-to-end performance.
    - Compare the results with a similar gRPC benchmark.
