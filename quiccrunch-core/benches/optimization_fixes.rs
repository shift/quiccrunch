/// Optimization Benchmark - Validates that performance fixes are working
/// 
/// This benchmark specifically tests the issues identified in the performance analysis:
/// 1. Memory pool cache misses (should be >0% hit rate)
/// 2. SIMD overhead (should be positive improvement for large buffers)  
/// 3. Adaptive strategy selection (should choose correct strategy for size)

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use quiccrunch_core::memory::{PooledBuffer, get_pool_stats, reset_pool_stats};
use quiccrunch_core::simd::{fast_copy, CpuFeatures};
use quiccrunch_core::adaptive::{OptimizationStrategy, allocate_optimized, copy_optimized};

/// Test memory pool hit rates for different usage patterns
pub fn bench_memory_pool_hit_rates(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_pool_hit_rates");
    
    // Test scenario: Allocate, use, drop, then allocate again (should hit cache)
    group.bench_function("pool_reuse_pattern", |b| {
        b.iter(|| {
            reset_pool_stats();
            
            // Create and immediately drop buffers in pattern that should allow reuse
            for size in [1024, 4096, 16384, 65536] {
                {
                    let buffer1 = PooledBuffer::get(size);
                    drop(buffer1);
                }
                {
                    let buffer2 = PooledBuffer::get(size);  // Should reuse buffer1
                    drop(buffer2);
                }
            }
            
            let stats = get_pool_stats();
            black_box(stats.hit_rate())
        });
    });
    
    // Test high-frequency allocation pattern
    group.bench_function("high_frequency_allocation", |b| {
        b.iter(|| {
            reset_pool_stats();
            
            // Simulate high-frequency allocation/deallocation
            let mut buffers = Vec::new();
            for _ in 0..100 {
                buffers.push(PooledBuffer::get(black_box(32768)));
            }
            // Drop all at once
            buffers.clear();
            
            // Now allocate again - should hit cache
            for _ in 0..100 {
                let buffer = PooledBuffer::get(black_box(32768));
                black_box(buffer);
            }
            
            get_pool_stats().hit_rate()
        });
    });
    
    group.finish();
}

/// Test SIMD performance to ensure overhead is fixed
pub fn bench_simd_performance_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("simd_performance_validation");
    
    // Test different buffer sizes where SIMD should/shouldn't be used
    let sizes = [
        (1024, "1KB"),      // Should use standard (SIMD overhead)
        (32768, "32KB"),    // Should use standard (SIMD overhead)  
        (131072, "128KB"),  // Should use SIMD (large enough)
        (524288, "512KB"),  // Should definitely use SIMD
    ];
    
    for (size, name) in sizes.iter() {
        let src = vec![0x42u8; *size];
        let mut dst_simd = vec![0u8; *size];
        let mut dst_standard = vec![0u8; *size];
        
        group.bench_with_input(
            BenchmarkId::new("simd_copy", name),
            size,
            |b, &_| {
                b.iter(|| {
                    fast_copy(black_box(&src), black_box(&mut dst_simd));
                });
            },
        );
        
        group.bench_with_input(
            BenchmarkId::new("standard_copy", name),
            size,
            |b, &_| {
                b.iter(|| {
                    dst_standard.copy_from_slice(black_box(&src));
                });
            },
        );
    }
    
    group.finish();
}

/// Test adaptive optimization strategy selection
pub fn bench_adaptive_strategy_selection(c: &mut Criterion) {
    let mut group = c.benchmark_group("adaptive_strategy_selection");
    
    // Test that strategies are selected correctly based on size
    group.bench_function("strategy_selection", |b| {
        b.iter(|| {
            let small_strategy = OptimizationStrategy::for_size(black_box(1024));
            let medium_strategy = OptimizationStrategy::for_size(black_box(32768));
            let large_strategy = OptimizationStrategy::for_size(black_box(262144));
            
            black_box((small_strategy, medium_strategy, large_strategy))
        });
    });
    
    // Test optimized allocation performance across strategies
    let sizes = [
        (1024, "small"),
        (32768, "medium"), 
        (262144, "large"),
    ];
    
    for (size, name) in sizes.iter() {
        group.bench_with_input(
            BenchmarkId::new("optimized_allocation", name),
            size,
            |b, &size| {
                b.iter(|| {
                    let buffer = allocate_optimized(black_box(size));
                    black_box(buffer)
                });
            },
        );
        
        group.bench_with_input(
            BenchmarkId::new("standard_allocation", name),
            size,
            |b, &size| {
                b.iter(|| {
                    let buffer = vec![0u8; black_box(size)];
                    black_box(buffer)
                });
            },
        );
    }
    
    group.finish();
}

/// Test copy optimization performance
pub fn bench_copy_optimization_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("copy_optimization_performance");
    
    let sizes = [
        (1024, "small"),
        (32768, "medium"),
        (262144, "large"),
    ];
    
    for (size, name) in sizes.iter() {
        let src = vec![0x42u8; *size];
        let mut dst_optimized = vec![0u8; *size];
        let mut dst_standard = vec![0u8; *size];
        
        group.bench_with_input(
            BenchmarkId::new("optimized_copy", name),
            size,
            |b, &_| {
                b.iter(|| {
                    copy_optimized(black_box(&src), black_box(&mut dst_optimized));
                });
            },
        );
        
        group.bench_with_input(
            BenchmarkId::new("standard_copy", name),
            size,
            |b, &_| {
                b.iter(|| {
                    dst_standard.copy_from_slice(black_box(&src));
                });
            },
        );
    }
    
    group.finish();
}

/// Test overall optimization effectiveness
pub fn bench_optimization_effectiveness(c: &mut Criterion) {
    let mut group = c.benchmark_group("optimization_effectiveness");
    
    // Full workload simulation
    group.bench_function("optimized_workload", |b| {
        b.iter(|| {
            reset_pool_stats();
            
            // Simulate a realistic workload with mixed buffer sizes
            let sizes = [1024, 4096, 16384, 65536, 262144];
            let iterations_per_size = 20;
            
            for &size in &sizes {
                for _ in 0..iterations_per_size {
                    let mut buffer = allocate_optimized(black_box(size));
                    let src_data = vec![0x42u8; size];
                    copy_optimized(&src_data, buffer.as_mut_slice());
                    black_box(buffer);
                }
            }
            
            get_pool_stats()
        });
    });
    
    group.bench_function("standard_workload", |b| {
        b.iter(|| {
            // Same workload with standard operations
            let sizes = [1024, 4096, 16384, 65536, 262144];
            let iterations_per_size = 20;
            
            for &size in &sizes {
                for _ in 0..iterations_per_size {
                    let mut buffer = vec![0u8; black_box(size)];
                    let src_data = vec![0x42u8; size];
                    buffer.copy_from_slice(&src_data);
                    black_box(buffer);
                }
            }
        });
    });
    
    group.finish();
}

criterion_group!(
    optimization_benches,
    bench_memory_pool_hit_rates,
    bench_simd_performance_validation,
    bench_adaptive_strategy_selection,
    bench_copy_optimization_performance,
    bench_optimization_effectiveness
);
criterion_main!(optimization_benches);