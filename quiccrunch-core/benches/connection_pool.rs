use criterion::{black_box, criterion_group, criterion_main, Criterion};
use quiccrunch_core::connection_pool::{ConnectionPool, ConnectionPoolConfig};
use quiccrunch_core::transport::{QuicTransport, TlsConfig};
use std::sync::Arc;
use std::time::Duration;

/// Benchmark connection pool performance
fn bench_connection_pool(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let transport = Arc::new(QuicTransport::new(TlsConfig::default()).unwrap());
        let config = ConnectionPoolConfig {
            max_connections_per_endpoint: 10,
            max_total_connections: 100,
            idle_timeout: Duration::from_secs(300),
            health_check_interval: Duration::from_secs(30),
            max_connection_age: Duration::from_secs(3600),
            enable_health_checks: true,
        };
        let pool = ConnectionPool::new(transport, config);

        c.bench_function("connection_pool_stats", |b| {
            b.iter(|| {
                let stats = rt.block_on(pool.stats());
                black_box(stats);
            });
        });
    });
}

/// Benchmark connection pool with multiple endpoints
fn bench_connection_pool_multi_endpoint(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let transport = Arc::new(QuicTransport::new(TlsConfig::default()).unwrap());
        let config = ConnectionPoolConfig {
            max_connections_per_endpoint: 5,
            max_total_connections: 50,
            idle_timeout: Duration::from_secs(300),
            health_check_interval: Duration::from_secs(30),
            max_connection_age: Duration::from_secs(3600),
            enable_health_checks: false, // Disable for benchmark
        };
        let pool = ConnectionPool::new(transport, config);

        c.bench_function("connection_pool_multi_endpoint_stats", |b| {
            b.iter(|| {
                let stats = rt.block_on(pool.stats());
                black_box(stats);
            });
        });
    });
}

/// Benchmark connection pool eviction
fn bench_connection_pool_eviction(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        let transport = Arc::new(QuicTransport::new(TlsConfig::default()).unwrap());
        let config = ConnectionPoolConfig {
            max_connections_per_endpoint: 10,
            max_total_connections: 100,
            idle_timeout: Duration::from_millis(1), // Very short for testing
            health_check_interval: Duration::from_secs(30),
            max_connection_age: Duration::from_millis(1), // Very short for testing
            enable_health_checks: false,
        };
        let pool = ConnectionPool::new(transport, config);

        c.bench_function("connection_pool_eviction", |b| {
            b.iter(|| {
                rt.block_on(pool.evict_expired_connections());
            });
        });
    });
}

criterion_group!(
    benches,
    bench_connection_pool,
    bench_connection_pool_multi_endpoint,
    bench_connection_pool_eviction
);
criterion_main!(benches);
