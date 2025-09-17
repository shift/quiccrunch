use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use quiccrunch_core::transport::{
    worker_pool::TquicWorkerPool, 
    messages::{WorkerConfig, TquicCommand}
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::sync::oneshot;

/// Benchmark worker pool performance across different worker counts
fn bench_worker_scaling(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("worker_scaling");
    
    for worker_count in [1, 2, 4, 8, 16].iter() {
        group.bench_function(format!("workers_{}", worker_count), |b| {
            let pool = create_worker_pool(*worker_count);
            
            b.iter(|| {
                rt.block_on(async {
                    let (tx, rx) = oneshot::channel();
                    let cmd = TquicCommand::Connect {
                        remote_addr: "127.0.0.1:8080".parse().unwrap(),
                        response: tx,
                    };
                    
                    let _ = pool.submit_command(black_box(cmd)).await;
                    let _ = rx.await;
                })
            })
        });
    }
    
    group.finish();
}

/// Benchmark latency vs throughput tradeoffs
fn bench_latency_vs_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("latency_throughput");
    group.throughput(Throughput::Elements(1000));
    
    let pool = create_worker_pool(4);
    
    group.bench_function("single_command_latency", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (tx, rx) = oneshot::channel();
                let cmd = TquicCommand::Connect {
                    remote_addr: "127.0.0.1:8080".parse().unwrap(),
                    response: tx,
                };
                
                let start = std::time::Instant::now();
                let _ = pool.submit_command(black_box(cmd)).await;
                let _ = rx.await;
                black_box(start.elapsed())
            })
        })
    });
    
    group.bench_function("batch_command_throughput", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut handles = Vec::new();
                
                for _ in 0..1000 {
                    let (tx, rx) = oneshot::channel();
                    let cmd = TquicCommand::Connect {
                        remote_addr: "127.0.0.1:8080".parse().unwrap(),
                        response: tx,
                    };
                    
                    let _ = pool.submit_command(cmd).await;
                    handles.push(rx);
                }
                
                for handle in handles {
                    let _ = handle.await;
                }
            })
        })
    });
    
    group.finish();
}

/// Benchmark load balancing efficiency
fn bench_load_balancing(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("load_balancing");
    
    let pool = create_worker_pool(8);
    
    group.bench_function("connection_distribution", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut handles = Vec::new();
                
                // Create many connections to test distribution
                for i in 0..100 {
                    let (tx, rx) = oneshot::channel();
                    let cmd = TquicCommand::Connect {
                        remote_addr: format!("127.0.0.1:{}", 8000 + i).parse().unwrap(),
                        response: tx,
                    };
                    
                    let _ = pool.submit_command(cmd).await;
                    handles.push(rx);
                }
                
                for handle in handles {
                    let _ = handle.await;
                }
                
                // Analyze load distribution
                let distribution = pool.get_load_distribution();
                black_box(distribution)
            })
        })
    });
    
    group.bench_function("socket_distribution_analysis", |b| {
        b.iter(|| {
            rt.block_on(async {
                // Simulate packet processing across workers
                for _ in 0..1000 {
                    let (tx, rx) = oneshot::channel();
                    let cmd = TquicCommand::Connect {
                        remote_addr: "127.0.0.1:8080".parse().unwrap(),
                        response: tx,
                    };
                    
                    let _ = pool.submit_command(cmd).await;
                    let _ = rx.await;
                }
                
                let analysis = pool.analyze_socket_distribution();
                black_box(analysis)
            })
        })
    });
    
    group.finish();
}

/// Benchmark memory usage and allocation patterns
fn bench_memory_usage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("memory_usage");
    
    let pool = create_worker_pool(4);
    
    group.bench_function("connection_memory_scaling", |b| {
        b.iter(|| {
            rt.block_on(async {
                let initial_memory = get_memory_usage();
                
                let mut handles = Vec::new();
                let mut connection_ids = Vec::new();
                
                // Create many connections
                for i in 0..1000 {
                    let (tx, rx) = oneshot::channel();
                    let cmd = TquicCommand::Connect {
                        remote_addr: format!("127.0.0.1:{}", 8000 + i).parse().unwrap(),
                        response: tx,
                    };
                    
                    let _ = pool.submit_command(cmd).await;
                    if let Ok(conn_id) = rx.await.unwrap() {
                        connection_ids.push(conn_id);
                    }
                }
                
                let peak_memory = get_memory_usage();
                
                // Clean up connections
                for conn_id in connection_ids {
                    pool.unmap_connection(conn_id);
                }
                
                let final_memory = get_memory_usage();
                
                black_box((initial_memory, peak_memory, final_memory))
            })
        })
    });
    
    group.finish();
}

/// Benchmark SO_REUSEPORT performance characteristics
fn bench_socket_reuse_efficiency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("socket_reuse");
    
    // Compare single-threaded vs multi-threaded with SO_REUSEPORT
    for worker_count in [1, 4, 8].iter() {
        group.bench_function(format!("reuseport_workers_{}", worker_count), |b| {
            let pool = create_worker_pool(*worker_count);
            
            b.iter(|| {
                rt.block_on(async {
                    let mut handles = Vec::new();
                    
                    // Simulate burst of connections that would benefit from SO_REUSEPORT
                    for _ in 0..50 {
                        let (tx, rx) = oneshot::channel();
                        let cmd = TquicCommand::Connect {
                            remote_addr: "127.0.0.1:8080".parse().unwrap(),
                            response: tx,
                        };
                        
                        let _ = pool.submit_command(cmd).await;
                        handles.push(rx);
                    }
                    
                    for handle in handles {
                        let _ = handle.await;
                    }
                })
            })
        });
    }
    
    group.finish();
}

/// Benchmark worker pool performance under different workload patterns
fn bench_workload_patterns(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("workload_patterns");
    
    let pool = create_worker_pool(4);
    
    // Test small RPC pattern (frequent small operations)
    group.bench_function("small_rpc_pattern", |b| {
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..100 {
                    let (tx, rx) = oneshot::channel();
                    let cmd = TquicCommand::Send {
                        connection_id: 1,
                        stream_id: 1,
                        data: vec![0u8; 64], // Small payload
                        fin: false,
                        response: tx,
                    };
                    
                    let _ = pool.submit_command(cmd).await;
                    let _ = rx.await;
                }
            })
        })
    });
    
    // Test medium RPC pattern (moderate sized operations)
    group.bench_function("medium_rpc_pattern", |b| {
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..50 {
                    let (tx, rx) = oneshot::channel();
                    let cmd = TquicCommand::Send {
                        connection_id: 1,
                        stream_id: 1,
                        data: vec![0u8; 8192], // Medium payload
                        fin: false,
                        response: tx,
                    };
                    
                    let _ = pool.submit_command(cmd).await;
                    let _ = rx.await;
                }
            })
        })
    });
    
    // Test large RPC pattern (fewer large operations)
    group.bench_function("large_rpc_pattern", |b| {
        b.iter(|| {
            rt.block_on(async {
                for _ in 0..10 {
                    let (tx, rx) = oneshot::channel();
                    let cmd = TquicCommand::Send {
                        connection_id: 1,
                        stream_id: 1,
                        data: vec![0u8; 65536], // Large payload
                        fin: false,
                        response: tx,
                    };
                    
                    let _ = pool.submit_command(cmd).await;
                    let _ = rx.await;
                }
            })
        })
    });
    
    group.finish();
}

/// Helper function to create a worker pool for testing
fn create_worker_pool(worker_count: usize) -> Arc<TquicWorkerPool> {
    let config = WorkerConfig {
        worker_id: 0,
        bind_addr: "0.0.0.0:0".parse().unwrap(),
        cert_file: None,
        key_file: None,
        is_server: false,
        max_idle_timeout: Duration::from_secs(30),
        max_udp_payload_size: 1350,
        enable_early_data: false,
        application_protocols: vec![b"h3".to_vec()],
    };
    
    Arc::new(TquicWorkerPool::new(worker_count, config).unwrap())
}

/// Helper function to get current memory usage (simplified)
fn get_memory_usage() -> usize {
    // In a real implementation, this would use system calls or libraries
    // to get actual memory usage. For benchmarking, we'll use a placeholder.
    0
}

criterion_group!(
    worker_pool_benches,
    bench_worker_scaling,
    bench_latency_vs_throughput,
    bench_load_balancing,
    bench_memory_usage,
    bench_socket_reuse_efficiency,
    bench_workload_patterns
);
criterion_main!(worker_pool_benches);