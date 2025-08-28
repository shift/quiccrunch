use std::sync::Arc;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use quiccrunch::{QuicTransport, Transport, TlsConfig};
use bytes::Bytes;
use tokio::runtime::Runtime;
use tokio::sync::Barrier;
use std::time::{Duration, Instant};

/// Tests bi-directional bandwidth scenarios to find optimal window sizes
/// and identify throughput cliffs
async fn bandwidth_test(
    collector: &mut PerformanceCollector,
    message_size: usize,
    window_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let (transport, addr) = setup_transport().await;
    let client = transport.connect(&addr).await?;
    let mut stream = client.open_bi().await?;
    let data = Bytes::from(&vec![0u8; message_size][..]);
    
    let total_data = window_size * message_size;
    let start = Instant::now();
    let mut bytes_sent = 0;
    
    while bytes_sent < total_data {
        stream.send_message(data.clone()).await?;
        bytes_sent += message_size;
        
        // Record instantaneous throughput
        if bytes_sent % (total_data / 10) == 0 {
            collector.record_throughput(bytes_sent as f64 / start.elapsed().as_secs_f64());
        }
    }
    
    collector.record_total_transfer(ThroughputMetrics {
        bytes: bytes_sent,
        duration: start.elapsed(),
        window_size,
    });

    Ok(())
}

/// Tests latency under various load conditions to identify
/// optimal concurrency levels
async fn latency_under_load(
    collector: &mut PerformanceCollector,
    background_streams: usize,
    message_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let (transport, addr) = setup_transport().await;
    let client = transport.connect(&addr).await?;
    let background_data = Bytes::from(&vec![0u8; message_size][..]);
    let probe_data = Bytes::from(&vec![0u8; 64][..]); // Small probe messages
    
    // Start background load
    let mut handles = Vec::new();
    for _ in 0..background_streams {
        let data = background_data.clone();
        let transport = transport.clone();
        let addr = addr.clone();
        
        handles.push(tokio::spawn(async move {
            let client = transport.connect(&addr).await?;
            let mut stream = client.open_bi().await?;
            loop {
                stream.send_message(data.clone()).await?;
            }
            #[allow(unreachable_code)]
            Ok::<_, Box<dyn std::error::Error>>(())
        }));
    }
    
    // Measure latency with probe messages
    let mut probe_stream = client.open_bi().await?;
    for _ in 0..100 {
        let start = Instant::now();
        probe_stream.send_message(probe_data.clone()).await?;
        let response = probe_stream.receive_message().await?;
        collector.record_probe_latency(start.elapsed());
    }
    
    // Cleanup background load
    for handle in handles {
        handle.abort();
    }
    
    Ok(())
}

/// Tests stream reuse patterns to identify optimal connection pooling
async fn stream_reuse_test(
    collector: &mut PerformanceCollector,
    pool_size: usize,
    operations: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let (transport, addr) = setup_transport().await;
    let client = transport.connect(&addr).await?;
    let data = Bytes::from(&vec![0u8; 1024][..]);
    
    let mut streams = Vec::with_capacity(pool_size);
    for _ in 0..pool_size {
        streams.push(client.open_bi().await?);
    }
    
    let start = Instant::now();
    for i in 0..operations {
        let stream_idx = i % pool_size;
        let start_op = Instant::now();
        streams[stream_idx].send_message(data.clone()).await?;
        let response = streams[stream_idx].receive_message().await?;
        collector.record_operation_latency(start_op.elapsed());
    }
    
    collector.record_pool_metrics(PoolMetrics {
        pool_size,
        operations,
        total_time: start.elapsed(),
    });
    
    Ok(())
}

/// Tests error recovery and connection resilience
async fn resilience_test(
    collector: &mut PerformanceCollector,
) -> Result<(), Box<dyn std::error::Error>> {
    let (transport, addr) = setup_transport().await;
    let client = transport.connect(&addr).await?;
    let data = Bytes::from(&vec![0u8; 1024][..]);
    
    // Test connection drop recovery
    for _ in 0..10 {
        let start = Instant::now();
        let mut stream = client.open_bi().await?;
        
        // Force stream reset
        drop(stream);
        
        // Measure recovery
        let new_stream = client.open_bi().await?;
        collector.record_recovery_time(start.elapsed());
        
        // Test with new stream
        new_stream.send_message(data.clone()).await?;
    }
    
    Ok(())
}

/// Stress test connection with rapid stream creation/destruction
async fn stream_churn_test(
    collector: &mut PerformanceCollector,
    concurrent_ops: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let (transport, addr) = setup_transport().await;
    let client = transport.connect(&addr).await?;
    let data = Bytes::from(&vec![0u8; 64][..]);
    
    let start = Instant::now();
    let mut handles = Vec::new();
    
    for _ in 0..concurrent_ops {
        let data = data.clone();
        let client = client.clone();
        
        handles.push(tokio::spawn(async move {
            let start_op = Instant::now();
            let mut stream = client.open_bi().await?;
            stream.send_message(data).await?;
            drop(stream);
            Ok::<_, Box<dyn std::error::Error>>(start_op.elapsed())
        }));
    }
    
    for handle in handles {
        let duration = handle.await??;
        collector.record_churn_operation(duration);
    }
    
    collector.record_churn_test(ChurnMetrics {
        operations: concurrent_ops,
        total_time: start.elapsed(),
    });
    
    Ok(())
}

fn bench_advanced_patterns(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut collector = PerformanceCollector::new();
    
    // Bandwidth tests
    let mut group = c.benchmark_group("bandwidth");
    for &(size, name) in SIZES {
        for window in [1, 10, 100, 1000] {
            group.throughput(Throughput::Bytes((size * window) as u64));
            group.bench_function(
                BenchmarkId::new(format!("{}_{}", name, window), window),
                |b| {
                    b.iter(|| {
                        rt.block_on(bandwidth_test(&mut collector, size, window))
                    })
                },
            );
        }
    }
    group.finish();
    
    // Latency under load
    let mut group = c.benchmark_group("latency_under_load");
    for &(size, name) in SIZES {
        for load in [0, 1, 10, 50] {
            group.bench_function(
                BenchmarkId::new(format!("{}_{}", name, load), load),
                |b| {
                    b.iter(|| {
                        rt.block_on(latency_under_load(&mut collector, load, size))
                    })
                },
            );
        }
    }
    group.finish();
    
    // Stream reuse patterns
    let mut group = c.benchmark_group("stream_reuse");
    for pool_size in [1, 5, 10, 50] {
        for ops in [100, 1000, 10000] {
            group.bench_function(
                BenchmarkId::new(format!("pool_{}_ops_{}", pool_size, ops), pool_size),
                |b| {
                    b.iter(|| {
                        rt.block_on(stream_reuse_test(&mut collector, pool_size, ops))
                    })
                },
            );
        }
    }
    group.finish();
    
    // Resilience tests
    let mut group = c.benchmark_group("resilience");
    group.bench_function("recovery", |b| {
        b.iter(|| rt.block_on(resilience_test(&mut collector)))
    });
    group.finish();
    
    // Stream churn tests
    let mut group = c.benchmark_group("stream_churn");
    for ops in [10, 100, 1000] {
        group.bench_function(BenchmarkId::new("concurrent", ops), |b| {
            b.iter(|| rt.block_on(stream_churn_test(&mut collector, ops)))
        });
    }
    group.finish();
    
    // Generate final report with all metrics
    collector.generate_report(c.benchmark_group("advanced_analysis")).unwrap();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(Duration::from_secs(30))
        .warm_up_time(Duration::from_secs(5));
    targets = bench_advanced_patterns
}
criterion_main!(benches);