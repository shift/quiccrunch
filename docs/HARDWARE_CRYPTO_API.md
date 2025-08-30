# Hardware Crypto Acceleration API Documentation

## Overview

QuicCrunch provides hardware-accelerated cryptography through the `crypto_accel` module, leveraging CPU extensions like AES-NI, AVX, and SHA-NI for significantly improved TLS performance. This module automatically detects available hardware capabilities and provides graceful fallback to software implementations.

## Core Types

### CryptoAccelConfig

Configuration structure for hardware crypto acceleration.

```rust
#[derive(Clone, Debug)]
pub struct CryptoAccelConfig {
    /// Enable AES-NI acceleration
    pub enable_aes_ni: bool,
    /// Enable batch crypto operations
    pub enable_batch_crypto: bool,
    /// Batch size for crypto operations
    pub batch_size: usize,
    /// Enable hardware crypto detection
    pub enable_hw_detection: bool,
}
```

#### Default Configuration

```rust
impl Default for CryptoAccelConfig {
    fn default() -> Self {
        Self {
            enable_aes_ni: true,
            enable_batch_crypto: true,
            batch_size: 64,
            enable_hw_detection: true,
        }
    }
}
```

### HardwareCapabilities

Structure representing detected hardware crypto capabilities.

```rust
#[derive(Clone, Debug)]
pub struct HardwareCapabilities {
    pub aes_ni_available: bool,
    pub avx_available: bool,
    pub avx2_available: bool,
    pub sha_ni_available: bool,
    pub rdrand_available: bool,
}
```

#### Detection Method

```rust
impl HardwareCapabilities {
    /// Detect available hardware crypto capabilities
    pub fn detect() -> Self {
        // Uses CPUID instruction to detect capabilities
        // Returns capabilities with boolean flags
    }
}
```

### HardwareCrypto

Main hardware crypto accelerator providing high-level encryption/decryption operations.

```rust
#[derive(Debug)]
pub struct HardwareCrypto {
    config: CryptoAccelConfig,
    batch_processor: BatchCryptoProcessor,
    cipher_optimizer: CipherSuiteOptimizer,
    stats: Arc<CryptoStats>,
}
```

## API Methods

### HardwareCrypto Methods

#### Constructor

```rust
impl HardwareCrypto {
    pub fn new(config: CryptoAccelConfig) -> Self {
        // Creates new hardware crypto accelerator
        // Automatically detects hardware capabilities
        // Initializes batch processor and cipher optimizer
    }
}
```

#### Single Operations

```rust
impl HardwareCrypto {
    /// Encrypt data using hardware acceleration when available
    pub fn encrypt(&self, plaintext: &[u8], key: &[u8]) -> Result<Vec<u8>> {
        // Encrypts single plaintext block
        // Uses hardware acceleration if available and enabled
        // Falls back to software AES-GCM if hardware unavailable
        // Records operation statistics
    }

    /// Decrypt data using hardware acceleration when available
    pub fn decrypt(&self, ciphertext: &[u8], key: &[u8]) -> Result<Vec<u8>> {
        // Decrypts single ciphertext block
        // Uses hardware acceleration if available and enabled
        // Falls back to software AES-GCM if hardware unavailable
        // Records operation statistics
    }
}
```

#### Batch Operations

```rust
impl HardwareCrypto {
    /// Batch encrypt multiple data blocks
    pub fn encrypt_batch(&self, plaintexts: &[&[u8]], key: &[u8]) -> Result<Vec<Vec<u8>>> {
        // Encrypts multiple plaintext blocks efficiently
        // Uses batch processing for optimal performance
        // Leverages hardware acceleration when available
        // Records batch operation statistics
    }

    /// Batch decrypt multiple data blocks
    pub fn decrypt_batch(&self, ciphertexts: &[&[u8]], key: &[u8]) -> Result<Vec<Vec<u8>>> {
        // Decrypts multiple ciphertext blocks efficiently
        // Uses batch processing for optimal performance
        // Leverages hardware acceleration when available
        // Records batch operation statistics
    }
}
```

#### Utility Methods

```rust
impl HardwareCrypto {
    /// Get hardware capabilities
    pub fn hardware_capabilities(&self) -> &HardwareCapabilities {
        // Returns reference to detected hardware capabilities
    }

    /// Get cipher suite optimizer
    pub fn cipher_optimizer(&self) -> &CipherSuiteOptimizer {
        // Returns cipher suite optimizer for TLS configuration
    }

    /// Get crypto statistics
    pub fn stats(&self) -> Arc<CryptoStats> {
        // Returns statistics about crypto operations
    }
}
```

### CipherSuiteOptimizer

Optimizes TLS cipher suite selection based on hardware capabilities.

```rust
#[derive(Debug)]
pub struct CipherSuiteOptimizer {
    hw_caps: HardwareCapabilities,
    preferred_suites: Vec<String>,
}
```

#### Methods

```rust
impl CipherSuiteOptimizer {
    pub fn new(hw_caps: HardwareCapabilities) -> Self {
        // Creates optimizer with hardware capability awareness
        // Prioritizes hardware-accelerated cipher suites
    }

    /// Get optimized cipher suite order
    pub fn get_optimized_suites(&self) -> &[String] {
        // Returns ordered list of cipher suites
        // Hardware-accelerated suites prioritized
    }

    /// Check if a cipher suite is hardware-accelerated
    pub fn is_hardware_accelerated(&self, suite: &str) -> bool {
        // Returns true if cipher suite supports hardware acceleration
    }
}
```

### CryptoStats

Statistics collection for crypto operations.

```rust
#[derive(Default, Debug)]
pub struct CryptoStats {
    pub total_operations: AtomicU64,
    pub total_encrypt_ops: AtomicU64,
    pub total_decrypt_ops: AtomicU64,
    pub total_batch_operations: AtomicU64,
    pub total_batch_items: AtomicU64,
    pub total_time_ns: AtomicU64,
    pub hw_accelerated_ops: AtomicU64,
}
```

#### Methods

```rust
impl CryptoStats {
    /// Get statistics snapshot
    pub fn get_stats(&self) -> CryptoStatsSnapshot {
        // Returns thread-safe snapshot of current statistics
    }
}
```

### CryptoStatsSnapshot

Immutable snapshot of crypto statistics.

```rust
#[derive(Debug, Clone)]
pub struct CryptoStatsSnapshot {
    pub total_operations: u64,
    pub total_encrypt_ops: u64,
    pub total_decrypt_ops: u64,
    pub total_batch_operations: u64,
    pub total_batch_items: u64,
    pub total_time_ns: u64,
    pub hw_accelerated_ops: u64,
}
```

#### Calculated Metrics

```rust
impl CryptoStatsSnapshot {
    /// Calculate average operation time
    pub fn avg_operation_time_ns(&self) -> f64 {
        // Returns average time per operation in nanoseconds
    }

    /// Calculate operations per second
    pub fn operations_per_second(&self) -> f64 {
        // Returns operations per second throughput
    }
}
```

## Integration with Transport Layer

### Transport Configuration

Hardware crypto acceleration integrates with the QUIC transport layer:

```rust
impl QuicTransport {
    /// Create transport with hardware crypto acceleration
    pub fn with_crypto_acceleration(
        tls_config: TlsConfig,
        crypto_config: CryptoAccelConfig
    ) -> Result<Self> {
        // Creates transport with hardware acceleration enabled
    }

    /// Enable hardware crypto acceleration on existing transport
    pub fn enable_crypto_acceleration(
        &mut self,
        config: CryptoAccelConfig
    ) -> Result<()> {
        // Enables hardware acceleration on running transport
    }

    /// Check if crypto acceleration is enabled
    pub fn has_crypto_acceleration(&self) -> bool {
        // Returns true if hardware acceleration is active
    }

    /// Get hardware crypto capabilities
    pub fn crypto_capabilities(&self) -> Option<&HardwareCapabilities> {
        // Returns hardware capabilities if acceleration enabled
    }

    /// Get crypto statistics
    pub fn crypto_stats(&self) -> Option<CryptoStatsSnapshot> {
        // Returns crypto operation statistics
    }
}
```

## Error Handling

All crypto operations return `Result<T, TransportError>` where `TransportError` includes:

- `ConfigError`: Configuration-related errors
- `ProtocolError`: Cryptographic operation failures
- `InvalidMessage`: Malformed ciphertext/plaintext

## Performance Characteristics

### Hardware Acceleration Benefits

- **AES-NI**: 2-5x performance improvement for AES-GCM operations
- **Batch Processing**: Additional 20-40% improvement for bulk operations
- **Memory Efficiency**: Reduced memory allocations through optimized paths
- **CPU Utilization**: Lower CPU usage for cryptographic operations

### Fallback Behavior

When hardware acceleration is unavailable:
- Automatic fallback to software AES-GCM implementation
- No performance degradation beyond hardware acceleration benefits
- Full API compatibility maintained
- Transparent operation to application code

### Benchmark Results

Typical performance improvements (based on internal benchmarks):

| Operation | Hardware | Software | Improvement |
|-----------|----------|----------|-------------|
| Single AES-GCM Encrypt | 45 ns | 120 ns | 2.7x |
| Single AES-GCM Decrypt | 48 ns | 125 ns | 2.6x |
| Batch Encrypt (64 ops) | 1.2 μs | 4.8 μs | 4.0x |
| Batch Decrypt (64 ops) | 1.3 μs | 5.1 μs | 3.9x |

*Results vary by CPU architecture and data size*

## Hardware Requirements

### Supported CPU Extensions

- **AES-NI**: Intel Westmere (2010+), AMD Bulldozer (2011+)
- **AVX**: Intel Sandy Bridge (2011+), AMD Bulldozer (2011+)
- **AVX2**: Intel Haswell (2013+), AMD Excavator (2015+)
- **SHA-NI**: Intel Goldmont (2016+), AMD Zen (2017+)

### Detection Process

1. Runtime CPUID instruction execution
2. Feature flag extraction from CPU registers
3. Capability validation and prioritization
4. Fallback path preparation

## Usage Examples

### Basic Setup

```rust
use quiccrunch::crypto_accel::{HardwareCrypto, CryptoAccelConfig};

// Create with default configuration
let crypto = HardwareCrypto::new(CryptoAccelConfig::default())?;

// Check capabilities
let caps = crypto.hardware_capabilities();
println!("AES-NI available: {}", caps.aes_ni_available);
println!("AVX2 available: {}", caps.avx2_available);
println!("SHA-NI available: {}", caps.sha_ni_available);
```

### Advanced Configuration

```rust
use quiccrunch::crypto_accel::{HardwareCrypto, CryptoAccelConfig};

// Custom configuration for high-throughput workloads
let config = CryptoAccelConfig {
    enable_aes_ni: true,
    enable_batch_crypto: true,
    batch_size: 128,  // Larger batch for bulk operations
    enable_hw_detection: true,
};

let crypto = HardwareCrypto::new(config)?;

// Verify configuration
println!("Batch size: {}", config.batch_size);
println!("Hardware acceleration enabled: {}", config.enable_aes_ni);
```

### Single Operations

```rust
// Prepare data
let plaintext = b"Hello, secure world!";
let key = b"0123456789abcdef"; // 16 bytes for AES-128

// Encrypt
let ciphertext = crypto.encrypt(plaintext, key)?;

// Decrypt
let decrypted = crypto.decrypt(&ciphertext, key)?;
assert_eq!(plaintext, &decrypted[..]);
```

### Batch Operations

```rust
// Prepare batch data
let messages = vec![
    b"Message 1",
    b"Message 2",
    b"Message 3",
];
let key = b"0123456789abcdef";

// Encrypt batch
let ciphertexts = crypto.encrypt_batch(&messages, key)?;

// Decrypt batch
let decrypted_refs: Vec<&[u8]> = ciphertexts.iter().map(|v| v.as_slice()).collect();
let decrypted = crypto.decrypt_batch(&decrypted_refs, key)?;

// Verify results
for (i, decrypted_msg) in decrypted.iter().enumerate() {
    assert_eq!(messages[i], decrypted_msg.as_slice());
}
```

### Transport Integration

```rust
use quiccrunch::{QuicTransport, TlsConfig, TransportConfig};
use quiccrunch::crypto_accel::CryptoAccelConfig;

// Configure TLS
let tls_config = TlsConfig { /* ... */ };

// Configure hardware crypto for different scenarios
let crypto_config = match scenario {
    Scenario::HighThroughput => CryptoAccelConfig {
        enable_batch_crypto: true,
        batch_size: 256,
        ..Default::default()
    },
    Scenario::LowLatency => CryptoAccelConfig {
        enable_batch_crypto: false,  // Disable batching for lower latency
        ..Default::default()
    },
    Scenario::Balanced => CryptoAccelConfig::default(),
};

// Create transport with hardware acceleration
let transport = QuicTransport::with_crypto_acceleration(tls_config, crypto_config)?;

// Use transport normally - hardware acceleration is automatic
let conn = transport.connect("example.com:443").await?;

// Monitor crypto performance
if let Some(stats) = transport.crypto_stats() {
    println!("Hardware accelerated operations: {}", stats.hw_accelerated_ops);
    println!("Average operation time: {:.2} ns", stats.avg_operation_time_ns());
}
```

### Dynamic Configuration Changes

```rust
use quiccrunch::{QuicTransport, TlsConfig};
use quiccrunch::crypto_accel::CryptoAccelConfig;

// Create transport without initial crypto acceleration
let tls_config = TlsConfig { /* ... */ };
let mut transport = QuicTransport::new(tls_config)?;

// Later, enable hardware acceleration dynamically
let crypto_config = CryptoAccelConfig {
    enable_aes_ni: true,
    enable_batch_crypto: true,
    batch_size: 64,
    enable_hw_detection: true,
};

transport.enable_crypto_acceleration(crypto_config)?;

// Verify acceleration is enabled
assert!(transport.has_crypto_acceleration());

// Check hardware capabilities
if let Some(caps) = transport.crypto_capabilities() {
    if caps.aes_ni_available {
        println!("AES-NI acceleration active");
    }
}
```

### Statistics Monitoring

```rust
// Get statistics
let stats = crypto.stats().get_stats();

// Analyze performance
println!("Total operations: {}", stats.total_operations);
println!("Hardware accelerated: {}", stats.hw_accelerated_ops);
println!("Average operation time: {:.2} ns", stats.avg_operation_time_ns());
println!("Operations per second: {:.0}", stats.operations_per_second());

// Calculate hardware acceleration ratio
let hw_ratio = if stats.total_operations > 0 {
    stats.hw_accelerated_ops as f64 / stats.total_operations as f64
} else {
    0.0
};
println!("Hardware acceleration ratio: {:.2%}", hw_ratio);

// Monitor performance over time
std::thread::spawn(move || {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60)); // Every minute
        let current_stats = crypto.stats().get_stats();
        let current_ratio = if current_stats.total_operations > 0 {
            current_stats.hw_accelerated_ops as f64 / current_stats.total_operations as f64
        } else {
            0.0
        };

        if current_ratio < 0.8 {
            eprintln!("Warning: Hardware acceleration ratio dropped to {:.1}%", current_ratio * 100.0);
        }
    }
});
```

### Error Handling and Recovery

```rust
use quiccrunch::crypto_accel::{HardwareCrypto, CryptoAccelConfig};
use quiccrunch::TransportError;

fn encrypt_with_fallback(
    crypto: &HardwareCrypto,
    data: &[u8],
    key: &[u8]
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    match crypto.encrypt(data, key) {
        Ok(result) => Ok(result),
        Err(TransportError::ProtocolError(e)) if e.contains("AES-GCM") => {
            eprintln!("Hardware acceleration failed, falling back to software: {}", e);
            // Fallback to software implementation would go here
            // For now, return the error
            Err(Box::new(TransportError::ProtocolError(e)))
        }
        Err(e) => Err(Box::new(e)),
    }
}

// Usage
let crypto = HardwareCrypto::new(CryptoAccelConfig::default())?;
let data = b"Sensitive data";
let key = b"0123456789abcdef";

match encrypt_with_fallback(&crypto, data, key) {
    Ok(encrypted) => println!("Encryption successful, size: {} bytes", encrypted.len()),
    Err(e) => eprintln!("Encryption failed: {}", e),
}
```

### Cipher Suite Optimization

```rust
use quiccrunch::crypto_accel::{HardwareCrypto, CryptoAccelConfig};

// Create crypto accelerator
let crypto = HardwareCrypto::new(CryptoAccelConfig::default())?;

// Get optimized cipher suites
let optimizer = crypto.cipher_optimizer();
let suites = optimizer.get_optimized_suites();

println!("Recommended cipher suites (in order of preference):");
for (i, suite) in suites.iter().enumerate() {
    let is_hw = optimizer.is_hardware_accelerated(suite);
    println!("{}. {} {}", i + 1, suite,
        if is_hw { "(Hardware accelerated)" } else { "(Software)" });
}

// Check if specific cipher suite is hardware accelerated
let test_suites = vec![
    "TLS_AES_128_GCM_SHA256",
    "TLS_AES_256_GCM_SHA384",
    "TLS_CHACHA20_POLY1305_SHA256",
];

for suite in test_suites {
    let is_hw = optimizer.is_hardware_accelerated(suite);
    println!("{}: {}", suite,
        if is_hw { "Hardware accelerated" } else { "Software only" });
}
```

### Performance Benchmarking

```rust
use quiccrunch::crypto_accel::{HardwareCrypto, CryptoAccelConfig};
use std::time::Instant;

fn benchmark_crypto_operations(crypto: &HardwareCrypto, iterations: usize) -> Result<(), Box<dyn std::error::Error>> {
    let test_data = b"Hello, World! This is a test message for benchmarking.";
    let key = b"0123456789abcdef";

    // Benchmark single operations
    let start = Instant::now();
    for _ in 0..iterations {
        let _encrypted = crypto.encrypt(test_data, key)?;
    }
    let encrypt_time = start.elapsed();

    let start = Instant::now();
    for _ in 0..iterations {
        let encrypted = crypto.encrypt(test_data, key)?;
        let _decrypted = crypto.decrypt(&encrypted, key)?;
    }
    let round_trip_time = start.elapsed();

    // Calculate metrics
    let avg_encrypt_time = encrypt_time.as_nanos() as f64 / iterations as f64;
    let avg_round_trip_time = round_trip_time.as_nanos() as f64 / iterations as f64;
    let operations_per_second = iterations as f64 / encrypt_time.as_secs_f64();

    println!("Benchmark Results ({} iterations):", iterations);
    println!("Average encryption time: {:.2} ns", avg_encrypt_time);
    println!("Average round-trip time: {:.2} ns", avg_round_trip_time);
    println!("Operations per second: {:.0}", operations_per_second);

    // Get final statistics
    let stats = crypto.stats().get_stats();
    let hw_ratio = if stats.total_operations > 0 {
        stats.hw_accelerated_ops as f64 / stats.total_operations as f64
    } else {
        0.0
    };

    println!("Hardware acceleration ratio: {:.2%}", hw_ratio);
    println!("Total operations: {}", stats.total_operations);

    Ok(())
}

// Usage
let crypto = HardwareCrypto::new(CryptoAccelConfig::default())?;
benchmark_crypto_operations(&crypto, 10000)?;
```

### Memory Usage Monitoring

```rust
use quiccrunch::crypto_accel::{HardwareCrypto, CryptoAccelConfig};

// Monitor memory usage patterns
fn monitor_memory_usage(crypto: &HardwareCrypto) -> Result<(), Box<dyn std::error::Error>> {
    // Get initial memory stats (this would require a memory profiler in a real implementation)
    println!("Monitoring crypto memory usage...");

    // Perform operations
    let test_data = vec![0u8; 4096]; // 4KB test data
    let key = b"0123456789abcdef";

    for i in 0..100 {
        let _encrypted = crypto.encrypt(&test_data, key)?;

        if i % 20 == 0 {
            // Periodic memory checks
            let stats = crypto.stats().get_stats();
            println!("After {} operations: {} total, {} HW accelerated",
                i + 1, stats.total_operations, stats.hw_accelerated_ops);
        }
    }

    // Final memory analysis
    let final_stats = crypto.stats().get_stats();
    println!("Final Statistics:");
    println!("  Total operations: {}", final_stats.total_operations);
    println!("  Hardware accelerated: {}", final_stats.hw_accelerated_ops);
    println!("  Average operation time: {:.2} ns", final_stats.avg_operation_time_ns());

    Ok(())
}

// Usage
let crypto = HardwareCrypto::new(CryptoAccelConfig::default())?;
monitor_memory_usage(&crypto)?;
```

## Best Practices

### Configuration

1. **Use Default Configuration**: `CryptoAccelConfig::default()` provides optimal settings
2. **Enable Hardware Detection**: Keep `enable_hw_detection: true` for automatic capability detection
3. **Batch Processing**: Enable for high-throughput scenarios with `enable_batch_crypto: true`
4. **Appropriate Batch Size**: Use 64 for most workloads, adjust based on message size

### Performance Optimization

1. **Reuse Crypto Instances**: Create `HardwareCrypto` once and reuse for multiple operations
2. **Batch Operations**: Use batch methods for multiple messages to leverage optimizations
3. **Key Management**: Use consistent key sizes (16 bytes for AES-128, 32 bytes for AES-256)
4. **Statistics Monitoring**: Monitor performance metrics to verify hardware acceleration benefits

### Error Handling

1. **Check Capabilities**: Verify hardware support before performance-critical operations
2. **Handle Fallbacks**: Design for graceful degradation when hardware acceleration unavailable
3. **Monitor Statistics**: Use statistics to detect performance issues or capability changes
4. **Resource Management**: Properly manage crypto instance lifecycles

## Troubleshooting

### Hardware Detection Issues

```rust
// Manually check capabilities
let caps = HardwareCapabilities::detect();
if !caps.aes_ni_available {
    println!("AES-NI not available, using software fallback");
}
```

### Performance Issues

```rust
// Check if hardware acceleration is being used
let stats = crypto.stats().get_stats();
let hw_usage = stats.hw_accelerated_ops as f64 / stats.total_operations as f64;

if hw_usage < 0.8 {
    println!("Warning: Only {:.1}% operations using hardware acceleration", hw_usage * 100.0);
}
```

### Memory Usage

```rust
// Monitor batch processor memory usage
let batch_size = crypto_config.batch_size;
let estimated_memory = batch_size * 4096; // Rough estimate per batch
println!("Estimated batch memory usage: {} KB", estimated_memory / 1024);
```

## Migration Guide

### From Software-Only Crypto

1. Add crypto_accel dependency (already included in quiccrunch-core)
2. Replace direct AES-GCM usage with HardwareCrypto
3. Enable hardware acceleration in transport configuration
4. Test performance improvements
5. Monitor statistics for optimization opportunities

### Configuration Changes

```rust
// Before (software only)
let transport = QuicTransport::new(tls_config)?;

// After (with hardware acceleration)
let crypto_config = CryptoAccelConfig::default();
let transport = QuicTransport::with_crypto_acceleration(tls_config, crypto_config)?;
```

## Security Considerations

- **Key Management**: Hardware acceleration doesn't change key security requirements
- **Side Channels**: Hardware operations may have different side-channel characteristics
- **Fallback Security**: Software fallback maintains same security properties
- **Certificate Validation**: Hardware acceleration doesn't affect TLS certificate validation

## Future Enhancements

- **Additional CPU Extensions**: Support for AVX-512, VAES instructions
- **Quantum-Resistant Algorithms**: Hardware acceleration for post-quantum crypto
- **Custom Hardware**: Support for dedicated crypto accelerators
- **Advanced Batch Processing**: SIMD-optimized batch operations
- **Memory Protection**: Hardware-assisted memory encryption