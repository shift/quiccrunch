# Language Bindings for QuicCrunch

This document outlines the plan for creating language bindings for the QuicCrunch core library, enabling developers to use QuicCrunch's high-performance QUIC transport and Cap'n Proto serialization from multiple programming languages.

## Current State

QuicCrunch is currently implemented as a pure Rust library without C FFI exports. The library features:

- Async/await based APIs using tokio
- Complex Rust ownership patterns (Arc, Mutex, etc.)
- Zero-copy operations with custom buffer management
- Performance optimizations (SIMD, io_uring, memory pools)
- Cap'n Proto schema-based serialization

## Architecture Overview

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Python/JS     │    │      C/C++      │    │   Java/Go/.NET  │
│   Bindings      │    │    Bindings     │    │    Bindings     │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
                    ┌─────────────────┐
                    │   C FFI Layer   │
                    │  (Async Bridge) │
                    └─────────────────┘
                                 │
                    ┌─────────────────┐
                    │  QuicCrunch     │
                    │  Core Library   │
                    │    (Rust)       │
                    └─────────────────┘
```

## Language Support Tiers

### Tier 1: High Feasibility

#### Python
- **Tooling**: PyO3 + maturin for Rust-Python integration
- **Cap'n Proto**: python-capnp library
- **Async Support**: asyncio integration with tokio
- **Target Users**: Data scientists, web developers, automation scripts

**Implementation Approach**:
```python
import asyncio
from quiccrunch import QuicClient, QuicServer

async def main():
    client = QuicClient()
    await client.connect("127.0.0.1:50051")
    response = await client.call("UserService", "get_user", {"id": 123})
```

#### Node.js/JavaScript
- **Tooling**: NAPI-RS for native Node.js modules
- **Cap'n Proto**: capnp-js library
- **Async Support**: Native Promise integration
- **Target Users**: Web developers, microservices, real-time applications

**Implementation Approach**:
```javascript
const { QuicClient } = require('quiccrunch');

const client = new QuicClient();
await client.connect('127.0.0.1:50051');
const response = await client.call('UserService', 'getUser', { id: 123 });
```

#### C/C++
- **Tooling**: cbindgen + manual C wrapper
- **Cap'n Proto**: Native C++ implementation
- **Async Support**: Callback or polling-based APIs
- **Target Users**: System programmers, embedded systems, legacy integration

**Implementation Approach**:
```c
QuicClient* client = quiccrunch_client_new();
quiccrunch_connect(client, "127.0.0.1:50051", connect_callback);
quiccrunch_call(client, "UserService", "get_user", params, response_callback);
```

### Tier 2: Medium Feasibility

#### Go
- **Tooling**: CGO + C FFI layer
- **Cap'n Proto**: zombiezen/go-capnproto2
- **Challenges**: Goroutine↔tokio async bridging
- **Target Users**: Cloud infrastructure, microservices

#### Java
- **Tooling**: JNI + C FFI layer
- **Cap'n Proto**: Java implementation available
- **Challenges**: JVM memory management, async interop
- **Target Users**: Enterprise applications, Android development

#### C#/.NET
- **Tooling**: P/Invoke + C FFI layer
- **Cap'n Proto**: capnproto-dotnet
- **Challenges**: .NET Task↔Rust Future bridging
- **Target Users**: Windows applications, enterprise services

### Tier 3: Specialized Use Cases

#### WebAssembly
- **Approach**: Direct Rust→WASM compilation
- **Limitations**: Network access restrictions in browsers
- **Use Cases**: Server-side WASM runtimes (Wasmtime, WASI)

## Implementation Plan

### Phase 1: Foundation (C FFI Layer)

1. **Create C-compatible API surface**:
   ```toml
   # Add to quiccrunch-core/Cargo.toml
   [lib]
   crate-type = ["cdylib", "rlib"]
   ```

2. **Implement core C exports**:
   ```rust
   // src/ffi.rs
   #[no_mangle]
   pub extern "C" fn quiccrunch_client_new() -> *mut QuicClient
   
   #[no_mangle]
   pub extern "C" fn quiccrunch_connect(
       client: *mut QuicClient, 
       addr: *const c_char,
       callback: extern "C" fn(*mut c_void, i32)
   ) -> i32
   ```

3. **Async Runtime Bridge**:
   - Embed tokio runtime in C layer
   - Provide callback-based APIs for async operations
   - Handle cross-thread communication safely

4. **Error Handling**:
   ```rust
   #[repr(C)]
   pub enum QuicError {
       Success = 0,
       ConnectionFailed = 1,
       TimeoutError = 2,
       SerializationError = 3,
   }
   ```

### Phase 2: High-Priority Bindings

#### Python Implementation
- Use PyO3 for Rust-Python integration
- Leverage existing tokio-asyncio bridges
- Provide both sync and async APIs

**File Structure**:
```
bindings/python/
├── Cargo.toml
├── pyproject.toml
├── src/
│   ├── lib.rs
│   ├── client.rs
│   └── server.rs
├── python/
│   └── quiccrunch/
│       ├── __init__.py
│       └── types.py
└── tests/
```

#### Node.js Implementation
- Use NAPI-RS for native module creation
- Integrate with Node.js event loop
- Provide TypeScript definitions

**File Structure**:
```
bindings/nodejs/
├── Cargo.toml
├── package.json
├── src/
│   ├── lib.rs
│   ├── client.rs
│   └── server.rs
├── index.d.ts
└── tests/
```

### Phase 3: Additional Language Support

Implement Go, Java, and C# bindings using the established C FFI layer.

## Cap'n Proto Schema Integration

### Advantages
- **Language Agnostic**: Existing `.capnp` files work across all languages
- **Zero-Copy**: Serialization benefits carry over to bindings
- **Type Safety**: Generated code provides compile-time guarantees

### Schema Files to Expose
- `schema/rpc.capnp` - Core RPC message types
- `schema/frame.capnp` - Transport frame definitions

### Code Generation Pipeline
```bash
# Generate bindings for each language
capnp compile -opyramid schema/rpc.capnp     # Python
capnp compile -oc++    schema/rpc.capnp     # C++
capnp compile -ogo     schema/rpc.capnp     # Go
capnp compile -ojava   schema/rpc.capnp     # Java
```

## Performance Considerations

### Optimization Preservation
- **SIMD Operations**: Expose through C FFI for languages that support it
- **Memory Pools**: Provide pool configuration from binding languages
- **io_uring**: Available on supported platforms (Linux)
- **Zero-Copy**: Maintain where possible through shared memory

### Benchmarking Strategy
- Establish performance baselines for Rust implementation
- Measure binding overhead for each language
- Target <10% performance loss for high-level bindings
- Target <5% performance loss for C/C++ bindings

## Documentation Strategy

### Per-Language Documentation
- Language-specific quick start guides
- API reference documentation
- Performance tuning guides
- Migration guides from existing solutions

### Example Applications
- Chat service implementation in each language
- Benchmarking tools
- Interoperability demos (Rust server ↔ Python client)

## Testing Strategy

### Cross-Language Integration Tests
- Rust server ↔ Python client
- Node.js server ↔ Java client
- Protocol compatibility verification

### Performance Regression Tests
- Automated benchmarking in CI
- Performance alerts for binding overhead
- Memory leak detection

## Maintenance Considerations

### API Stability
- Semantic versioning across all bindings
- Deprecation policies for breaking changes
- Long-term support (LTS) versioning

### Build Infrastructure
- Automated binding generation
- Cross-platform CI/CD pipelines
- Package publishing automation

## Getting Started

### For Contributors
1. Start with C FFI layer implementation
2. Choose a target language from Tier 1
3. Follow the established patterns for async bridging
4. Add comprehensive tests and documentation

### For Users
Once available, bindings will be distributed through:
- **Python**: PyPI packages
- **Node.js**: npm packages  
- **C/C++**: System package managers + source
- **Go**: Go modules
- **Java**: Maven Central
- **C#**: NuGet packages

## Next Steps

### Immediate Actions
1. **Add C FFI capability** to `quiccrunch-core/Cargo.toml`
2. **Create FFI module** in `quiccrunch-core/src/ffi.rs`
3. **Set up bindings workspace** with initial Python implementation
4. **Establish CI pipeline** for cross-language testing

### Timeline
- **Q1 2025**: C FFI layer and Python bindings
- **Q2 2025**: Node.js and C++ bindings
- **Q3 2025**: Go and Java bindings
- **Q4 2025**: C# bindings and WebAssembly support

## Technical Challenges & Solutions

### Memory Management
- **Challenge**: Rust ownership model vs manual memory management
- **Solution**: Reference counting with explicit cleanup functions

### Async Interop
- **Challenge**: Bridging Rust futures with language-specific async models
- **Solution**: Runtime bridges (tokio-asyncio, tokio-nodejs event loop)

### Error Propagation
- **Challenge**: Rust's Result<T, E> doesn't map directly to other languages
- **Solution**: Error code enums + detailed error message strings

### Schema Evolution
- **Challenge**: Maintaining compatibility as Cap'n Proto schemas evolve
- **Solution**: Versioned schema compilation + compatibility testing

This plan leverages QuicCrunch's Cap'n Proto foundation and performance optimizations to create a comprehensive multi-language ecosystem while maintaining the library's high-performance characteristics.