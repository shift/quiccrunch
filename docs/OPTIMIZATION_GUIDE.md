# QuicCrunch Performance Optimization Guide

## Overview

QuicCrunch includes a comprehensive set of performance optimizations designed to maximize throughput, minimize latency, and reduce memory overhead in QUIC-based RPC applications. This guide explains how to enable and tune these optimizations for your specific use case.

## Quick Start

### Enable All Optimizations (Recommended)

```rust
use quiccrunch::{QuicTransport, enable_maximum_performance, TlsConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Enable all optimizations with one call
    let cpu_opts = enable_maximum_performance()?;
    
    // Your existing code works unchanged
    let transport = QuicTransport::new(tls_config)?;
    // ... rest of your application
    
    Ok(())
}
```

### Feature-Based Configuration

```toml
[dependencies.quiccrunch]
version = "0.1"
features = [
    "memory-pools",        # Thread-local buffer pools
    "simd-optimizations",  # SIMD-accelerated operations
    "backend-tquic",       # High-performance tQUIC backend
    "lock-free-channels",  # Lock-free multi-stream concurrency
]
```

## Performance Features

### 1. Memory Pools

**Purpose**: Eliminate allocation overhead by reusing buffers across operations.

**How it Works**:
- Thread-local pools categorized by size (4KB, 64KB, 1MB, huge)
- Automatic buffer reuse with RAII management
- Zero-copy operations where possible

**Configuration**:
```rust
// Enabled automatically with memory-pools feature
use quiccrunch::optimizations::{get_pool_stats, PooledBuffer};

// Monitor pool efficiency
let stats = get_pool_stats();
println!("Pool hit rate: {:.1}%", stats.hit_rate());

// Manual buffer allocation (optional)
let buffer = PooledBuffer::get(65536); // 64KB buffer
```

**Performance Impact**: 60-80% reduction in allocation overhead for typical workloads.

### 2. SIMD Operations

**Purpose**: Accelerate memory operations using CPU vector instructions.

**Supported Architectures**:
- **x86_64**: AVX-512, AVX2, SSE4.1
- **aarch64**: NEON

**Automatic Optimization**:
```rust
use quiccrunch::optimizations::CpuFeatures;

// Runtime feature detection
let features = CpuFeatures::detect();
println!("SIMD support: AVX2={}, AVX-512={}", features.avx2, features.avx512f);

// Operations ≥32 bytes automatically use SIMD when available
// Falls back to standard operations on unsupported hardware
```

**Performance Impact**: 2-4x faster memory operations for large messages (>32 bytes).

### 3. Optimized Streams

**Purpose**: Advanced stream implementation with all optimizations integrated.

```rust
use quiccrunch::{OptimizedQuicStream, optimizations::StreamConfig};

// Use optimized stream for maximum performance
let config = StreamConfig {
    chunk_size: 262_144,        // 256KB chunks
    use_simd: true,             // Enable SIMD operations
    use_zero_copy: true,        // Zero-copy where possible
    enable_batching: true,      // Batch small operations
    batch_size: 16,             // Operations per batch
    ..Default::default()
};

// The optimized stream automatically chooses the best strategy
// based on message size and system capabilities
```

**Performance Impact**: 20-50% improvement in overall stream throughput.

### 4. CPU Optimizations

**Purpose**: System-level optimizations for maximum performance.

```rust
use quiccrunch::optimizations::{CpuOptimizations, enable_high_performance};

let cpu_opts = CpuOptimizations::detect();

// Enable system-level optimizations (requires sudo for some features)
enable_high_performance(&cpu_opts)?;

// Check optimal configurations
println!("Optimal chunk size: {} KB", cpu_opts.optimal_chunk_size() / 1024);
println!("Optimal batch size: {}", cpu_opts.optimal_batch_size());
println!("CPU cores: {}", cpu_opts.cpu_count);
```

**System Optimizations**:
- **Huge pages**: Reduced TLB misses
- **Performance governor**: Maximum CPU frequency
- **CPU scaling disabled**: Consistent performance
- **NUMA awareness**: Optimal memory placement

## Performance Tuning

### Message Size Optimization

QuicCrunch automatically optimizes based on message size:

```rust
// Small messages (< 64KB): Batching enabled for reduced overhead
// Medium messages (64KB - 256KB): Standard chunked processing  
// Large messages (> 256KB): Zero-copy with SIMD acceleration

// You can tune the thresholds:
let config = StreamConfig {
    chunk_size: 512_144,        // Larger chunks for high-bandwidth networks
    batch_size: 32,             // More batching for latency-sensitive apps
    ..Default::default()
};
```

### Network Optimization

```rust
use quiccrunch::{QuicTransport, TransportSettings, CongestionController};

let settings = TransportSettings {
    stream_receive_window: 4 * 1024 * 1024,      // 4MB for high-bandwidth
    connection_receive_window: 32 * 1024 * 1024,  // 32MB for many streams
    congestion_controller: CongestionController::Bbr, // BBR for high-throughput
    max_concurrent_bidi_streams: 10000,           // Support many streams
    ..Default::default()
};

let transport = QuicTransport::with_settings(tls_config, settings)?;
```

### Concurrent Connections

```rust
// QuicCrunch is designed for high concurrency
// Each connection runs independently with optimized resource usage

#[tokio::main(flavor = "multi_thread", worker_threads = 16)] // Match your CPU cores
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cpu_opts = enable_maximum_performance()?;
    
    // Server can handle thousands of concurrent connections
    while let Ok(conn) = incoming.accept().await {
        tokio::spawn(async move {
            // Each connection uses optimized pools and SIMD
            handle_connection(conn).await;
        });
    }
}
```

## Monitoring and Debugging

### Memory Pool Statistics

```rust
use quiccrunch::optimizations::{get_pool_stats, reset_pool_stats};

// Monitor pool efficiency
let stats = get_pool_stats();
println!("Memory Pool Statistics:");
println!("  Total allocations: {}", stats.allocations);
println!("  Pool hits: {} ({:.1}%)", stats.pool_hits, stats.hit_rate());
println!("  Pool misses: {}", stats.pool_misses);

// Reset for clean measurements
reset_pool_stats();
```

### CPU Feature Detection

```rust
use quiccrunch::optimizations::{CpuOptimizations, benchmark_system};

// Comprehensive system benchmark
let benchmark_results = benchmark_system();
benchmark_results.print_summary();

// Detailed CPU info
let cpu_opts = CpuOptimizations::detect();
println!("System Configuration:");
println!("  CPU cores: {}", cpu_opts.cpu_count);
println!("  NUMA nodes: {}", cpu_opts.numa_nodes);
println!("  Cache line size: {} bytes", cpu_opts.cache_line_size);
println!("  Page size: {} KB", cpu_opts.page_size / 1024);
```

### Performance Benchmarking

```bash
# Run comprehensive benchmarks
cargo bench --features="memory-pools,simd-optimizations,backend-tquic"

# Run validation example
cargo run --example performance_validation --features="memory-pools,simd-optimizations,backend-tquic"

# Compare with/without optimizations
cargo bench optimization_comparison
```

## Best Practices

### 1. Feature Selection

**Production (Maximum Performance)**:
```toml
features = ["memory-pools", "simd-optimizations", "backend-tquic", "lock-free-channels"]
```

**Development (Fast Compile)**:
```toml
features = ["backend-quinn"]
default-features = false
```

**Resource-Constrained**:
```toml
features = ["backend-tquic"]  # Just the efficient backend
default-features = false
```

### 2. Message Patterns

```rust
// ✅ Good: Reuse connections and streams when possible
let conn = transport.connect(addr).await?;
for message in messages {
    let mut stream = conn.open_bi().await?;
    stream.send_message(message).await?;
}

// ❌ Avoid: Creating new connections for each message
for message in messages {
    let conn = transport.connect(addr).await?; // Expensive!
    let mut stream = conn.open_bi().await?;
    stream.send_message(message).await?;
}
```

### 3. Batch Operations

```rust
// ✅ Good: Batch small messages
let mut batch = Vec::new();
for small_message in small_messages {
    batch.push(small_message);
    if batch.len() >= 16 {
        send_batch(&mut stream, &batch).await?;
        batch.clear();
    }
}

// Send remaining messages
if !batch.is_empty() {
    send_batch(&mut stream, &batch).await?;
}
```

### 4. Memory Management

```rust
// ✅ Good: Let QuicCrunch manage buffers automatically
stream.send_message(data).await?; // Uses optimized pools internally

// ⚠️ Manual buffer management (advanced use cases only)
let buffer = PooledBuffer::get(size);
// ... use buffer ...
// Automatically returned to pool when dropped
```

## Troubleshooting

### Common Issues

**Low Pool Hit Rate**:
- Message sizes vary widely
- Solution: Use consistent message sizes or multiple size categories

**SIMD Not Activating**:
- Messages too small (< 32 bytes)  
- Unsupported CPU architecture
- Solution: Check `CpuFeatures::detect()` output

**High Memory Usage**:
- Pool sizes too large for your workload
- Solution: Tune `BufferPool::MAX_POOL_SIZE` or disable memory-pools feature

**Compile Errors**:
- Missing optional dependencies
- Solution: Enable required features or use different backend

### Performance Debugging

```rust
// Enable detailed tracing
use tracing::{info, debug};

// Monitor operation timing
let start = Instant::now();
stream.send_message(data).await?;
debug!("Send took: {:?}", start.elapsed());

// Profile memory usage
let stats_before = get_pool_stats();
// ... operations ...
let stats_after = get_pool_stats();
info!("Pool efficiency: {:.1}%", stats_after.hit_rate());
```

## Conclusion

QuicCrunch's optimization system is designed to provide maximum performance with minimal configuration. The default settings work well for most applications, but fine-tuning based on your specific workload can yield additional performance gains.

For questions or performance issues, check the [benchmarks](../benches/) and [examples](../examples/) directories for reference implementations.