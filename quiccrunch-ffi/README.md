# QuicCrunch FFI

C Foreign Function Interface (FFI) bindings for the QuicCrunch high-performance QUIC library.

## Overview

This crate provides a C-compatible API that enables language bindings for Python, Node.js, Go, Java, C#, and other languages to use QuicCrunch's high-performance QUIC transport and Cap'n Proto serialization.

## Features

- **Complete C API** - Full access to QuicCrunch client and server functionality
- **Memory Safe** - Careful memory management with clear ownership semantics  
- **Async Bridge** - Handles Rust async/await with C-style callbacks
- **Error Handling** - Comprehensive error codes for robust error handling
- **Zero Dependencies** - Self-contained library with minimal external dependencies

## Building

### As a Dynamic Library

```bash
cargo build --release
# Produces: target/release/libquiccrunch_ffi.so (Linux)
#           target/release/libquiccrunch_ffi.dylib (macOS) 
#           target/release/quiccrunch_ffi.dll (Windows)
```

### Generate C Headers

```bash
cargo build --features generate-headers
# Produces: quiccrunch.h
```

## Usage Example

```c
#include "quiccrunch.h"
#include <stdio.h>

int main() {
    // Initialize the library
    if (quiccrunch_init() != QUIC_ERROR_SUCCESS) {
        printf("Failed to initialize QuicCrunch\n");
        return 1;
    }
    
    // Create and initialize a client
    QuicTransportHandle* client = quiccrunch_client_new();
    if (!client) {
        printf("Failed to create client\n");
        return 1;
    }
    
    if (quiccrunch_client_init(client) != QUIC_ERROR_SUCCESS) {
        printf("Failed to initialize client\n");
        quiccrunch_client_free(client);
        return 1;
    }
    
    // Connect to server
    uint32_t connection_id;
    QuicError result = quiccrunch_client_connect(
        client, 
        "127.0.0.1:50051", 
        &connection_id
    );
    
    if (result == QUIC_ERROR_SUCCESS) {
        printf("Connected! Connection ID: %u\n", connection_id);
        
        // Send a message
        QuicMessage msg = {
            .data = (uint8_t*)"Hello, QuicCrunch!",
            .len = 18
        };
        
        result = quiccrunch_client_send_message(client, connection_id, &msg);
        if (result == QUIC_ERROR_SUCCESS) {
            printf("Message sent successfully\n");
        }
        
        // Disconnect
        quiccrunch_client_disconnect(client, connection_id);
    }
    
    // Cleanup
    quiccrunch_client_free(client);
    quiccrunch_shutdown();
    
    return 0;
}
```

## API Reference

### Initialization

- `quiccrunch_init()` - Initialize the library (call first)
- `quiccrunch_shutdown()` - Shutdown the library (call last)
- `quiccrunch_version()` - Get library version string

### Client API

- `quiccrunch_client_new()` - Create a new client
- `quiccrunch_client_free()` - Free a client handle
- `quiccrunch_client_init()` - Initialize client transport
- `quiccrunch_client_connect()` - Connect to server (blocking)
- `quiccrunch_client_connect_async()` - Connect to server (async with callback)
- `quiccrunch_client_disconnect()` - Disconnect from server
- `quiccrunch_client_send_message()` - Send a message
- `quiccrunch_client_connection_count()` - Get number of active connections

### Server API

- `quiccrunch_server_new()` - Create a new server
- `quiccrunch_server_free()` - Free a server handle
- `quiccrunch_server_init()` - Initialize server transport
- `quiccrunch_server_listen()` - Start listening on address
- `quiccrunch_server_serve()` - Run server loop (blocking)
- `quiccrunch_server_set_message_handler()` - Set message handler callback
- `quiccrunch_server_connection_count()` - Get number of active connections

### Memory Management

- `quiccrunch_string_free()` - Free strings returned by FFI functions

## Error Handling

All functions return `QuicError` enum values:

- `QUIC_ERROR_SUCCESS` - Operation completed successfully
- `QUIC_ERROR_INVALID_ARGUMENT` - Invalid argument passed
- `QUIC_ERROR_CONNECTION_FAILED` - Connection failed
- `QUIC_ERROR_TIMEOUT_ERROR` - Operation timed out
- `QUIC_ERROR_INVALID_HANDLE` - Invalid handle/pointer
- And more...

## Memory Safety

- All `*_new()` functions return owned pointers that must be freed with `*_free()`
- String parameters are borrowed and don't need to be freed by caller
- String return values are owned and must be freed with `quiccrunch_string_free()`
- Null pointer checks are performed on all handle parameters

## Thread Safety

The FFI layer is thread-safe when used correctly:

- Each handle should only be used from one thread at a time
- Multiple handles can be used concurrently from different threads
- Callbacks may be called from background threads

## Performance

The FFI layer is designed for minimal overhead:

- Zero-copy operations where possible
- Direct forwarding to core library functions
- Efficient async runtime integration
- No unnecessary allocations or data copying

## Language Bindings

This FFI layer is designed to support bindings for:

- **Python** - via PyO3/ctypes
- **Node.js** - via NAPI-RS/ffi-napi
- **Go** - via CGO
- **Java** - via JNI
- **C#** - via P/Invoke
- **C/C++** - direct usage

## Testing

```bash
# Run FFI tests
cargo test

# Test with different features
cargo test --features generate-headers
```

## Integration

### CMake Integration

```cmake
find_library(QUICCRUNCH_FFI quiccrunch_ffi PATHS /usr/local/lib)
target_link_libraries(your_target ${QUICCRUNCH_FFI})
```

### Pkg-config Integration

```bash
pkg-config --cflags --libs quiccrunch-ffi
```

## Examples

See the `examples/` directory for complete usage examples in C and integration examples for various language bindings.

## Performance Considerations

- Use async functions where possible to avoid blocking
- Reuse connections when making multiple requests  
- Configure appropriate timeouts for your use case
- Monitor connection counts to detect leaks

## Troubleshooting

### Library Not Found
Ensure the dynamic library is in your system's library path (`LD_LIBRARY_PATH` on Linux, `DYLD_LIBRARY_PATH` on macOS).

### Crashes
- Always call `quiccrunch_init()` before using other functions
- Check return values for error conditions
- Ensure handles are not used after being freed
- Don't pass null pointers except where documented as allowed

### Performance Issues
- Use connection pooling for high-frequency operations
- Consider async APIs for better concurrency
- Monitor memory usage with `connection_count()` functions