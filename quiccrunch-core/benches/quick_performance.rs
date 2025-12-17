//! Quick Performance Test
//!
//! Fast benchmark that provides immediate results without long compilation

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::time::Instant;

fn bench_basic_operations(c: &mut Criterion) {
    c.bench_function("addition", |b| {
        b.iter(|| {
            black_box(42 + 42)
        })
    });
    
    c.bench_function("string_operations", |b| {
        let s = "Hello, World!".to_string();
        b.iter(|| {
            black_box(s.len())
        })
    });
    
    c.bench_function("vector_operations", |b| {
        b.iter(|| {
            let v: Vec<i32> = (0..100).collect();
            black_box(v.iter().sum::<i32>())
        })
    });
    
    c.bench_function("hash_map_operations", |b| {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        map.insert("key", "value");
        
        b.iter(|| {
            black_box(map.get("key"))
        })
    });
}

fn bench_network_simulation(c: &mut Criterion) {
    c.bench_function("message_serialize", |b| {
        let message = "test message for serialization";
        b.iter(|| {
            // Simulate serialization work
            let serialized = black_box(message.as_bytes().to_vec());
            black_box(serialized.len())
        })
    });
    
    c.bench_function("connection_simulation", |b| {
        b.iter(|| {
            let start = Instant::now();
            // Simulate connection establishment work
            let _result = black_box(42 * 1000);
            black_box(start.elapsed())
        })
    });
}

criterion_group!(
    benches,
    bench_basic_operations,
    bench_network_simulation
);

criterion_main!(benches);