//! Simple Performance Test
//!
//! Tests basic performance without complex tQUIC setup

use std::time::Instant;
use std::sync::Arc;
use tokio::sync::Barrier;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== QuicCrunch Simple Performance Test ===");
    
    // Test 1: Basic operations
    test_basic_operations();
    
    // Test 2: Concurrent task performance
    test_concurrent_performance().await;
    
    // Test 3: Memory allocation patterns
    test_memory_performance();
    
    println!("=== Performance Test Complete ===");
    Ok(())
}

fn test_basic_operations() {
    println!("\n--- Basic Operations ---");
    
    let iterations = 1_000_000;
    
    // Test addition
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = 42 + 42;
    }
    let duration = start.elapsed();
    println!("Addition ({} ops): {:.2} ns/op", iterations, duration.as_nanos() as f64 / iterations as f64);
    
    // Test string operations
    let start = Instant::now();
    let s = "Hello, World!";
    for _ in 0..iterations {
        let _ = s.len();
    }
    let duration = start.elapsed();
    println!("String length ({} ops): {:.2} ns/op", iterations, duration.as_nanos() as f64 / iterations as f64);
    
    // Test vector operations
    let start = Instant::now();
    for _ in 0..iterations {
        let v: Vec<i32> = (0..100).collect();
        let _ = v.iter().sum::<i32>();
    }
    let duration = start.elapsed();
    println!("Vector sum ({} ops): {:.2} ns/op", iterations, duration.as_nanos() as f64 / iterations as f64);
    
    // Test hash map operations
    let start = Instant::now();
    use std::collections::HashMap;
    let mut map = HashMap::new();
    map.insert("key", "value");
    for _ in 0..iterations {
        let _ = map.get("key");
    }
    let duration = start.elapsed();
    println!("HashMap lookup ({} ops): {:.2} ns/op", iterations, duration.as_nanos() as f64 / iterations as f64);
}

async fn test_concurrent_performance() {
    println!("\n--- Concurrent Performance ---");
    
    let task_counts = vec![1, 2, 4, 8, 16];
    
    for &task_count in &task_counts {
        let barrier = Arc::new(Barrier::new(task_count));
        let start = Instant::now();
        
        let mut handles = Vec::new();
        
        for i in 0..task_count {
            let barrier = barrier.clone();
            let handle = tokio::spawn(async move {
                barrier.wait().await;
                
                // Do some work
                let mut sum = 0;
                for j in 0..100_000 {
                    sum += i * j;
                }
                sum
            });
            handles.push(handle);
        }
        
        // Wait for all tasks
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }
        
        let duration = start.elapsed();
        let total_ops = task_count * 100_000;
        println!("{} tasks: {:.2} ms total, {:.2} ops/sec", 
                task_count, 
                duration.as_millis(),
                total_ops as f64 / duration.as_secs_f64());
    }
}

fn test_memory_performance() {
    println!("\n--- Memory Performance ---");
    
    // Test small allocations
    let iterations = 100_000;
    let start = Instant::now();
    let mut vecs = Vec::new();
    for _ in 0..iterations {
        vecs.push(vec![0u8; 64]);
    }
    let duration = start.elapsed();
    println!("Small allocations (64B x {}): {:.2} ns/alloc", iterations, duration.as_nanos() as f64 / iterations as f64);
    
    // Test medium allocations
    let start = Instant::now();
    let mut vecs = Vec::new();
    for _ in 0..iterations / 10 {
        vecs.push(vec![0u8; 1024]);
    }
    let duration = start.elapsed();
    println!("Medium allocations (1KB x {}): {:.2} ns/alloc", iterations / 10, duration.as_nanos() as f64 / (iterations / 10) as f64);
    
    // Test large allocations
    let start = Instant::now();
    let mut vecs = Vec::new();
    for _ in 0..iterations / 100 {
        vecs.push(vec![0u8; 16384]);
    }
    let duration = start.elapsed();
    println!("Large allocations (16KB x {}): {:.2} ns/alloc", iterations / 100, duration.as_nanos() as f64 / (iterations / 100) as f64);
    
    // Test memory copy performance
    let size = 16384;
    let iterations = 10_000;
    let src = vec![42u8; size];
    let mut dst = vec![0u8; size];
    
    let start = Instant::now();
    for _ in 0..iterations {
        dst.copy_from_slice(&src);
    }
    let duration = start.elapsed();
    let total_bytes = (size * iterations) as u64;
    let throughput_mb = total_bytes as f64 / (1024.0 * 1024.0) / duration.as_secs_f64();
    println!("Memory copy (16KB x {}): {:.2} MB/sec", iterations, throughput_mb);
}