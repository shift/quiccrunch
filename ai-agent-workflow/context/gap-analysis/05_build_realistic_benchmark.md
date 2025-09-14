# Context: Build Realistic Performance Benchmark

**Objective:** Update the gRPC comparison benchmark to be a true end-to-end test that provides meaningful performance metrics.

**Analysis of `quiccrunch-core/examples/grpc_comparison.rs`:**
- The current benchmark is a simple echo server that does not exercise the full RPC stack.
- It does not measure serialization/deserialization overhead, which is a key performance factor.
- The results are misleading as they only measure network round-trip time for raw bytes.

**Plan:**
1.  **Create a Representative Schema:**
    - Define a Cap'n Proto schema that is representative of a real-world RPC call, with a mix of data types.
2.  **Implement a Full RPC Service:**
    - Use the (now functional) `#[rpc_service]` macro to define a service based on the new schema.
3.  **Update Benchmark Logic:**
    - In `quiccrunch-core/examples/grpc_comparison.rs`, modify the client and server to use the generated RPC service.
    - The client should create a Cap'n Proto message, send it via the RPC call, and process the response.
    - The server should deserialize the message, perform some minimal logic, and return a Cap'n Proto response.
4.  **Measure and Compare:**
    - Run the updated benchmark against a comparable gRPC implementation to get realistic performance numbers.
