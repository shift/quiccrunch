/// Performance monitoring and validation for QuicCrunch optimizations
/// 
/// This module provides utilities to measure and validate that optimizations
/// are actually working as expected, based on the performance analysis findings.

use crate::memory::{PooledBuffer, get_pool_stats, reset_pool_stats};
use crate::adaptive::{OptimizationStrategy, allocate_optimized, copy_optimized};
use std::time::Instant;

/// Results from performance validation tests
#[derive(Debug, Clone)]
pub struct PerformanceResults {
    pub memory_pool_hit_rate: f64,
    pub allocation_performance: AllocationPerformance,
    pub copy_performance: CopyPerformance,
    pub adaptive_strategy_validation: AdaptiveValidation,
}

#[derive(Debug, Clone)]
pub struct AllocationPerformance {
    pub small_buffer_speed: f64,    // allocations/sec for 1KB buffers
    pub medium_buffer_speed: f64,   // allocations/sec for 32KB buffers  
    pub large_buffer_speed: f64,    // allocations/sec for 256KB buffers
    pub pool_improvement: f64,      // percentage improvement with pools vs standard
}

#[derive(Debug, Clone)]
pub struct CopyPerformance {
    pub small_copy_speed: f64,      // MB/sec for 1KB copies
    pub medium_copy_speed: f64,     // MB/sec for 32KB copies
    pub large_copy_speed: f64,      // MB/sec for 256KB copies
    pub simd_improvement: f64,      // percentage improvement with SIMD vs standard
}

#[derive(Debug, Clone)]
pub struct AdaptiveValidation {
    pub strategy_1kb: OptimizationStrategy,
    pub strategy_32kb: OptimizationStrategy,
    pub strategy_256kb: OptimizationStrategy,
    pub correct_selection: bool,
}

/// Run comprehensive performance validation to ensure optimizations work
pub fn validate_optimizations() -> PerformanceResults {
    println!("🔍 Running performance validation tests...");
    
    let memory_pool_hit_rate = test_memory_pool_hit_rate();
    let allocation_performance = test_allocation_performance();
    let copy_performance = test_copy_performance();
    let adaptive_strategy_validation = test_adaptive_strategy();
    
    PerformanceResults {
        memory_pool_hit_rate,
        allocation_performance,
        copy_performance,
        adaptive_strategy_validation,
    }
}

/// Test memory pool hit rate to ensure buffers are being reused
fn test_memory_pool_hit_rate() -> f64 {
    println!("  Testing memory pool hit rates...");
    
    reset_pool_stats();
    
    // Create and drop buffers in a pattern that should allow reuse
    {
        let buffer1 = PooledBuffer::get(1024);
        drop(buffer1);
        
        let buffer2 = PooledBuffer::get(1024);  // Should reuse buffer1
        drop(buffer2);
        
        let buffer3 = PooledBuffer::get(2048);  // Should reuse buffer2 (capacity >= 2048)
        drop(buffer3);
    }
    
    let stats = get_pool_stats();
    let hit_rate = stats.hit_rate();
    
    println!("    Pool hits: {}, misses: {}, hit rate: {:.1}%", 
             stats.pool_hits, stats.pool_misses, hit_rate);
    
    hit_rate
}

/// Test allocation performance across different buffer sizes
fn test_allocation_performance() -> AllocationPerformance {
    println!("  Testing allocation performance...");
    
    let iterations = 10000;
    
    // Test small buffers (1KB)
    let start = Instant::now();
    for _ in 0..iterations {
        let _buffer = allocate_optimized(1024);
    }
    let small_duration = start.elapsed();
    let small_speed = iterations as f64 / small_duration.as_secs_f64();
    
    // Test medium buffers (32KB)
    let start = Instant::now();
    for _ in 0..iterations {
        let _buffer = allocate_optimized(32768);
    }
    let medium_duration = start.elapsed();
    let medium_speed = iterations as f64 / medium_duration.as_secs_f64();
    
    // Test large buffers (256KB)
    let start = Instant::now();
    for _ in 0..iterations {
        let _buffer = allocate_optimized(262144);
    }
    let large_duration = start.elapsed();
    let large_speed = iterations as f64 / large_duration.as_secs_f64();
    
    // Compare against standard Vec allocation for medium buffers
    let start = Instant::now();
    for _ in 0..iterations {
        let _buffer = vec![0u8; 32768];
    }
    let standard_duration = start.elapsed();
    let standard_speed = iterations as f64 / standard_duration.as_secs_f64();
    
    let pool_improvement = ((medium_speed - standard_speed) / standard_speed) * 100.0;
    
    println!("    Small (1KB): {:.0} allocs/sec", small_speed);
    println!("    Medium (32KB): {:.0} allocs/sec", medium_speed); 
    println!("    Large (256KB): {:.0} allocs/sec", large_speed);
    println!("    Pool improvement vs standard: {:.1}%", pool_improvement);
    
    AllocationPerformance {
        small_buffer_speed: small_speed,
        medium_buffer_speed: medium_speed,
        large_buffer_speed: large_speed,
        pool_improvement,
    }
}

/// Test copy performance with and without SIMD
fn test_copy_performance() -> CopyPerformance {
    println!("  Testing copy performance...");
    
    let iterations = 1000;
    
    // Test small copies (1KB)
    let src_small = vec![0x42u8; 1024];
    let mut dst_small = vec![0u8; 1024];
    
    let start = Instant::now();
    for _ in 0..iterations {
        copy_optimized(&src_small, &mut dst_small);
    }
    let small_duration = start.elapsed();
    let small_speed = (1024 * iterations) as f64 / small_duration.as_secs_f64() / 1_000_000.0;
    
    // Test medium copies (32KB)
    let src_medium = vec![0x42u8; 32768];
    let mut dst_medium = vec![0u8; 32768];
    
    let start = Instant::now();
    for _ in 0..iterations {
        copy_optimized(&src_medium, &mut dst_medium);
    }
    let medium_duration = start.elapsed();
    let medium_speed = (32768 * iterations) as f64 / medium_duration.as_secs_f64() / 1_000_000.0;
    
    // Test large copies (256KB)
    let src_large = vec![0x42u8; 262144];
    let mut dst_large = vec![0u8; 262144];
    
    let start = Instant::now();
    for _ in 0..iterations {
        copy_optimized(&src_large, &mut dst_large);
    }
    let large_duration = start.elapsed();
    let large_speed = (262144 * iterations) as f64 / large_duration.as_secs_f64() / 1_000_000.0;
    
    // Compare SIMD vs standard for large copies
    let start = Instant::now();
    for _ in 0..iterations {
        dst_large.copy_from_slice(&src_large);
    }
    let standard_duration = start.elapsed();
    let standard_speed = (262144 * iterations) as f64 / standard_duration.as_secs_f64() / 1_000_000.0;
    
    let simd_improvement = ((large_speed - standard_speed) / standard_speed) * 100.0;
    
    println!("    Small (1KB): {:.1} MB/sec", small_speed);
    println!("    Medium (32KB): {:.1} MB/sec", medium_speed);
    println!("    Large (256KB): {:.1} MB/sec", large_speed);
    println!("    SIMD improvement vs standard: {:.1}%", simd_improvement);
    
    CopyPerformance {
        small_copy_speed: small_speed,
        medium_copy_speed: medium_speed,
        large_copy_speed: large_speed,
        simd_improvement,
    }
}

/// Test adaptive strategy selection
fn test_adaptive_strategy() -> AdaptiveValidation {
    println!("  Testing adaptive strategy selection...");
    
    let strategy_1kb = OptimizationStrategy::for_size(1024);
    let strategy_32kb = OptimizationStrategy::for_size(32768);
    let strategy_256kb = OptimizationStrategy::for_size(262144);
    
    // Validate that strategies match expected performance analysis
    let correct_selection = 
        strategy_1kb == OptimizationStrategy::Standard &&
        strategy_32kb == OptimizationStrategy::PoolOnly &&
        strategy_256kb == OptimizationStrategy::Full;
    
    println!("    1KB strategy: {:?}", strategy_1kb);
    println!("    32KB strategy: {:?}", strategy_32kb);
    println!("    256KB strategy: {:?}", strategy_256kb);
    println!("    Strategy selection correct: {}", correct_selection);
    
    AdaptiveValidation {
        strategy_1kb,
        strategy_32kb,
        strategy_256kb,
        correct_selection,
    }
}

/// Run a quick performance regression test
pub fn quick_performance_test() -> bool {
    println!("🚀 Running quick performance regression test...");
    
    let results = validate_optimizations();
    
    // Check that optimizations are working as expected
    let memory_pools_working = results.memory_pool_hit_rate > 50.0;  // Should have >50% hit rate
    let allocations_working = results.allocation_performance.pool_improvement > 0.0;  // Should be faster
    let strategies_correct = results.adaptive_strategy_validation.correct_selection;
    
    let all_working = memory_pools_working && allocations_working && strategies_correct;
    
    println!("\n📊 Performance Test Results:");
    println!("  ✅ Memory pools working: {} (hit rate: {:.1}%)", 
             if memory_pools_working { "YES" } else { "NO" }, results.memory_pool_hit_rate);
    println!("  ✅ Allocation optimization: {} (improvement: {:.1}%)", 
             if allocations_working { "YES" } else { "NO" }, results.allocation_performance.pool_improvement);
    println!("  ✅ Adaptive strategies: {} ", 
             if strategies_correct { "YES" } else { "NO" });
    println!("  ✅ Overall: {}", if all_working { "PASS" } else { "FAIL" });
    
    all_working
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_validation() {
        let results = validate_optimizations();
        
        // Memory pools should have some hit rate (not 0%)
        assert!(results.memory_pool_hit_rate > 0.0, "Memory pools should have some hit rate");
        
        // Allocation performance should be measurable
        assert!(results.allocation_performance.small_buffer_speed > 0.0);
        assert!(results.allocation_performance.medium_buffer_speed > 0.0);
        assert!(results.allocation_performance.large_buffer_speed > 0.0);
        
        // Copy performance should be measurable
        assert!(results.copy_performance.small_copy_speed > 0.0);
        assert!(results.copy_performance.medium_copy_speed > 0.0);
        assert!(results.copy_performance.large_copy_speed > 0.0);
        
        // Adaptive strategies should be correct
        assert!(results.adaptive_strategy_validation.correct_selection);
    }
    
    #[test]
    fn test_quick_performance_test() {
        // This test validates that the optimization fixes are working
        let success = quick_performance_test();
        assert!(success, "Performance optimizations should be working correctly");
    }
}