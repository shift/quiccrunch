use quiccrunch_core::crypto_accel::{HardwareCrypto, CryptoAccelConfig};

fn main() {
    println!("=== Hardware Crypto Acceleration Performance Test ===\n");

    // Test hardware capabilities
    let caps = quiccrunch_core::crypto_accel::HardwareCapabilities::detect();
    println!("Hardware Capabilities:");
    println!("  AES-NI: {}", caps.aes_ni_available);
    println!("  AVX: {}", caps.avx_available);
    println!("  AVX2: {}", caps.avx2_available);
    println!("  SHA-NI: {}", caps.sha_ni_available);
    println!("  RDRAND: {}", caps.rdrand_available);
    println!();

    // Create crypto accelerator
    let config = CryptoAccelConfig::default();
    let crypto = HardwareCrypto::new(config);

    println!("Crypto Configuration:");
    println!("  Enable AES-NI: {}", config.enable_aes_ni);
    println!("  Enable batch crypto: {}", config.enable_batch_crypto);
    println!("  Batch size: {}", config.batch_size);
    println!("  Enable HW detection: {}", config.enable_hw_detection);
    println!();

    // Test data
    let test_sizes = vec![64, 256, 1024, 4096, 16384];
    let key = b"0123456789abcdef0123456789abcdef"; // 32 bytes

    println!("Performance Test Results:");
    println!("Size (bytes)\tEncrypt (μs)\tDecrypt (μs)\tThroughput (MB/s)");
    println!("------------------------------------------------------------");

    for &size in &test_sizes {
        let data = vec![0x42u8; size]; // Fill with some data

        // Time encryption
        let start = std::time::Instant::now();
        let encrypted = crypto.encrypt(&data, key).unwrap();
        let encrypt_time = start.elapsed();

        // Time decryption
        let start = std::time::Instant::now();
        let decrypted = crypto.decrypt(&encrypted, key).unwrap();
        let decrypt_time = start.elapsed();

        // Calculate throughput
        let total_bytes = data.len() + encrypted.len();
        let total_time = encrypt_time + decrypt_time;
        let throughput_mbs = (total_bytes as f64 / 1_000_000.0) / total_time.as_secs_f64();

        println!("{}\t\t{:.2}\t\t{:.2}\t\t{:.2}",
                size,
                encrypt_time.as_micros(),
                decrypt_time.as_micros(),
                throughput_mbs);

        // Verify correctness
        assert_eq!(data, decrypted);
    }

    // Get final stats
    let stats = crypto.stats().get_stats();
    println!("\nFinal Statistics:");
    println!("  Total operations: {}", stats.total_operations);
    println!("  Hardware accelerated: {}", stats.hw_accelerated_ops);
    println!("  Average operation time: {:.2} ns", stats.avg_operation_time_ns());
    println!("  Operations per second: {:.0}", stats.operations_per_second());

    // Test cipher suite optimization
    let optimizer = crypto.cipher_optimizer();
    let suites = optimizer.get_optimized_suites();
    println!("\nOptimized Cipher Suites:");
    for suite in suites {
        let is_hw = optimizer.is_hardware_accelerated(suite);
        println!("  {} {}", suite, if is_hw { "(HW accelerated)" } else { "(software)" });
    }

    println!("\n✅ Hardware crypto acceleration test completed successfully!");
}