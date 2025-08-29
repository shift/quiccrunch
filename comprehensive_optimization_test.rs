/// Comprehensive tQUIC Performance Test with All Optimization Thresholds
/// Tests message sizes from 1KB to 2MB to cover all optimization boundaries

use quiccrunch_core::*;
use std::time::Instant;
use bytes::Bytes;

async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔬 Comprehensive tQUIC Backend Performance Test");
    println!("===============================================");
    println!("Testing optimization thresholds with message sizes 1KB to 2MB\n");

    // Test sizes that hit all optimization boundaries
    let test_sizes = vec![
        // Small sizes - Standard allocation (0-8KB)
        1024,        // 1KB
        4096,        // 4KB
        8192,        // 8KB - memory pool threshold
        
        // Medium sizes - Pool only (8KB-64KB)
        16384,       // 16KB
        32768,       // 32KB
        65536,       // 64KB - SIMD threshold / Full optimization boundary
        
        // Large sizes - Pool + SIMD considerations (64KB+)
        131072,      // 128KB - SSE4.1 SIMD threshold
        262144,      // 256KB - AVX2 SIMD threshold  
        524288,      // 512KB
        1048576,     // 1MB - AVX-512 SIMD threshold
        1572864,     // 1.5MB
        2097152,     // 2MB
    ];

    println!("📊 Testing Buffer Allocation Performance:");
    println!("Size       | Standard (ops/sec) | Pooled (ops/sec) | Improvement | Strategy");
    println!("-----------|-------------------|-----------------|-------------|----------");

    for &size in &test_sizes {
        let (standard_ops, pooled_ops, strategy) = benchmark_allocation(size).await;
        let improvement = if standard_ops > 0.0 {
            ((pooled_ops - standard_ops) / standard_ops) * 100.0
        } else {
            0.0
        };
        
        println!("{:>10} | {:>17.0} | {:>15.0} | {:>10.1}% | {:?}", 
                 format_size(size), standard_ops, pooled_ops, improvement, strategy);
    }

    println!("\n🔄 Testing SIMD Copy Performance:");
    println!("Size       | Standard (MB/s) | SIMD (MB/s) | Improvement | SIMD Used");
    println!("-----------|-----------------|-------------|-------------|----------");

    for &size in &test_sizes {
        let (standard_mbps, simd_mbps, simd_used) = benchmark_copy_performance(size).await;
        let improvement = if standard_mbps > 0.0 {
            ((simd_mbps - standard_mbps) / standard_mbps) * 100.0
        } else {
            0.0
        };
        
        println!("{:>10} | {:>15.1} | {:>11.1} | {:>10.1}% | {}", 
                 format_size(size), standard_mbps, simd_mbps, improvement,
                 if simd_used { "Yes" } else { "No" });
    }

    println!("\n📈 Optimization Strategy Analysis:");
    for &size in &test_sizes {
        let strategy = adaptive::OptimizationStrategy::for_size(size);
        let aligned_strategy = {
            let aligned_data = vec![0u8; size];
            adaptive::OptimizationStrategy::for_buffer(&aligned_data)
        };
        
        println!("{:>10}: Size-based={:?}, Alignment-based={:?}", 
                 format_size(size), strategy, aligned_strategy);
    }

    // Test with actual tQUIC streams if available
    println!("\n🚀 Stream Configuration Recommendations:");
    for &size in &test_sizes {
        let config = recommend_stream_config(size);
        println!("{:>10}: chunk_size={}, use_simd={}, use_zero_copy={}, enable_batching={}", 
                 format_size(size), 
                 format_size(config.chunk_size),
                 config.use_simd,
                 config.use_zero_copy,
                 config.enable_batching);
    }

    Ok(())
}

async fn benchmark_allocation(size: usize) -> (f64, f64, adaptive::OptimizationStrategy) {
    const ITERATIONS: usize = 10000;
    
    // Benchmark standard allocation
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let _buffer = vec![0u8; size];
        std::hint::black_box(_buffer);
    }
    let standard_time = start.elapsed();
    let standard_ops = ITERATIONS as f64 / standard_time.as_secs_f64();

    // Benchmark pooled allocation
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let _buffer = memory::PooledBuffer::get(size);
        std::hint::black_box(_buffer);
    }
    let pooled_time = start.elapsed();
    let pooled_ops = ITERATIONS as f64 / pooled_time.as_secs_f64();

    let strategy = adaptive::OptimizationStrategy::for_size(size);
    
    (standard_ops, pooled_ops, strategy)
}

async fn benchmark_copy_performance(size: usize) -> (f64, f64, bool) {
    const ITERATIONS: usize = 1000;
    
    let src = vec![42u8; size];
    let mut dst = vec![0u8; size];
    
    // Benchmark standard copy
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        dst.copy_from_slice(&src);
        std::hint::black_box(&dst);
    }
    let standard_time = start.elapsed();
    let standard_mbps = (size * ITERATIONS) as f64 / standard_time.as_secs_f64() / (1024.0 * 1024.0);

    // Benchmark SIMD copy (if available)
    let start = Instant::now();
    let mut simd_used = false;
    for _ in 0..ITERATIONS {
        // Check if SIMD would be used based on size and alignment thresholds
        if size >= 65536 && src.as_ptr() as usize % 64 == 0 && dst.as_ptr() as usize % 64 == 0 {
            simd_used = true;
            #[cfg(feature = "simd-optimizations")]
            simd::fast_copy(&src, &mut dst);
            #[cfg(not(feature = "simd-optimizations"))]
            dst.copy_from_slice(&src);
        } else {
            dst.copy_from_slice(&src);
        }
        std::hint::black_box(&dst);
    }
    let simd_time = start.elapsed();
    let simd_mbps = (size * ITERATIONS) as f64 / simd_time.as_secs_f64() / (1024.0 * 1024.0);

    (standard_mbps, simd_mbps, simd_used)
}

fn recommend_stream_config(size: usize) -> StreamConfig {
    use stream_optimized::StreamConfig;
    
    // Base configuration for different message sizes
    match size {
        0..=8192 => StreamConfig::low_latency(),      // Small messages
        8193..=65536 => StreamConfig::balanced(),     // Medium messages  
        _ => StreamConfig::high_throughput(),         // Large messages
    }
}

fn format_size(size: usize) -> String {
    if size >= 1024 * 1024 {
        format!("{}MB", size / (1024 * 1024))
    } else if size >= 1024 {
        format!("{}KB", size / 1024)
    } else {
        format!("{}B", size)
    }
}

#[tokio::main]
async fn main_wrapper() {
    if let Err(e) = main().await {
        eprintln!("Test failed: {}", e);
    }
}