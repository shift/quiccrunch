/// Memory Pool Debug Test
/// Tests memory pool behavior to understand cache miss issue

use quiccrunch::optimizations::{PooledBuffer, get_pool_stats};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Memory Pool Debug Test");
    println!("========================\n");

    #[cfg(feature = "memory-pools")]
    {
        test_pool_basic_functionality();
        test_pool_reuse_patterns();
        test_pool_size_categories();
    }
    
    #[cfg(not(feature = "memory-pools"))]
    {
        println!("❌ memory-pools feature not enabled - no pool testing");
    }

    Ok(())
}

#[cfg(feature = "memory-pools")]
fn test_pool_basic_functionality() {
    println!("🧪 Basic Pool Functionality Test");
    println!("--------------------------------");
    
    // Reset stats
    let _ = get_pool_stats(); // Initial read to reset
    
    // Test 1: Single allocation and deallocation
    {
        let buffer = PooledBuffer::get(1024);
        println!("  Created buffer of size: {}, capacity: {}", buffer.len(), buffer.capacity());
        // Buffer will be dropped here
    }
    
    let stats = get_pool_stats();
    println!("  After first allocation/deallocation:");
    println!("    Allocations: {}", stats.allocations);
    println!("    Deallocations: {}", stats.deallocations);
    println!("    Pool hits: {}", stats.pool_hits);
    println!("    Pool misses: {}", stats.pool_misses);
    
    // Test 2: Second allocation of same size (should be a hit)
    {
        let buffer = PooledBuffer::get(1024);
        println!("  Second buffer of size: {}, capacity: {}", buffer.len(), buffer.capacity());
    }
    
    let stats = get_pool_stats();
    println!("  After second allocation/deallocation:");
    println!("    Allocations: {}", stats.allocations);
    println!("    Deallocations: {}", stats.deallocations);
    println!("    Pool hits: {}", stats.pool_hits);
    println!("    Pool misses: {}", stats.pool_misses);
    println!("    Hit rate: {:.2}%", stats.hit_rate());
    
    if stats.pool_hits > 0 {
        println!("  ✅ Pool reuse is working!");
    } else {
        println!("  ❌ Pool reuse is NOT working!");
    }
    
    println!();
}

#[cfg(feature = "memory-pools")]
fn test_pool_reuse_patterns() {
    println!("🔄 Pool Reuse Pattern Test");
    println!("--------------------------");
    
    let initial_stats = get_pool_stats();
    
    // Test pattern: allocate multiple buffers, drop them, then allocate again
    const BUFFER_SIZE: usize = 4096;
    const NUM_BUFFERS: usize = 5;
    
    // Phase 1: Allocate and immediately drop
    for i in 0..NUM_BUFFERS {
        let buffer = PooledBuffer::get(BUFFER_SIZE);
        println!("  Created buffer {} - size: {}, capacity: {}", i + 1, buffer.len(), buffer.capacity());
        // Buffer drops at end of scope
    }
    
    let after_phase1 = get_pool_stats();
    println!("  After phase 1 (allocate {} buffers):", NUM_BUFFERS);
    println!("    New allocations: {}", after_phase1.allocations - initial_stats.allocations);
    println!("    New deallocations: {}", after_phase1.deallocations - initial_stats.deallocations);
    
    // Phase 2: Allocate again (should reuse from pool)
    for i in 0..NUM_BUFFERS {
        let buffer = PooledBuffer::get(BUFFER_SIZE);
        println!("  Reused buffer {} - size: {}, capacity: {}", i + 1, buffer.len(), buffer.capacity());
    }
    
    let after_phase2 = get_pool_stats();
    println!("  After phase 2 (reuse {} buffers):", NUM_BUFFERS);
    println!("    Total allocations: {}", after_phase2.allocations);
    println!("    Total deallocations: {}", after_phase2.deallocations);
    println!("    Total pool hits: {}", after_phase2.pool_hits);
    println!("    Total pool misses: {}", after_phase2.pool_misses);
    println!("    Overall hit rate: {:.2}%", after_phase2.hit_rate());
    
    println!();
}

#[cfg(feature = "memory-pools")]
fn test_pool_size_categories() {
    println!("📏 Pool Size Category Test");
    println!("-------------------------");
    
    let sizes = vec![
        (1024, "Small (1KB)"),
        (4096, "Small (4KB)"),
        (16384, "Medium (16KB)"),
        (65536, "Medium (64KB)"),
        (256 * 1024, "Large (256KB)"),
        (2 * 1024 * 1024, "Huge (2MB)"),
    ];
    
    let initial_stats = get_pool_stats();
    
    for (size, category) in sizes {
        // Allocate and drop buffer
        {
            let buffer = PooledBuffer::get(size);
            println!("  {} - actual capacity: {}", category, buffer.capacity());
        }
        
        // Try to reuse
        {
            let buffer = PooledBuffer::get(size);
            println!("  {} (reuse) - actual capacity: {}", category, buffer.capacity());
        }
    }
    
    let final_stats = get_pool_stats();
    println!("  Size category test results:");
    println!("    Total allocations: {}", final_stats.allocations - initial_stats.allocations);
    println!("    Total pool hits: {}", final_stats.pool_hits - initial_stats.pool_hits);
    println!("    Hit rate: {:.2}%", 
             if (final_stats.allocations - initial_stats.allocations) > 0 {
                 ((final_stats.pool_hits - initial_stats.pool_hits) as f64 / 
                  (final_stats.allocations - initial_stats.allocations) as f64) * 100.0
             } else { 0.0 });
}