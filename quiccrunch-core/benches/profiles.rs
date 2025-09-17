use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use quiccrunch_core::{QuicTransport, TlsConfig, Transport};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::Barrier;

use crate::analysis::{
    CongestionMetrics, ConnectionProfile, MultiplexingMetrics, PerformanceCollector,
};

// Message patterns with detailed sizes
const SIZES: &[(usize, &str)] = &[
    (64, "tiny"),     // Tiny control messages
    (256, "small"),   // Small RPCs
    (1024, "medium"), // Typical RPCs
    (4096, "large"),  // Large RPCs
    (16384, "xl"),    // Extra large
    (65536, "bulk"),  // Bulk transfer
];

const STREAM_COUNTS: &[usize] = &[1, 10, 50, 100, 500];

async fn analyze_connection_establishment(
    collector: &mut PerformanceCollector,
) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    let (transport, addr) = setup_transport().await;

    // Measure TLS setup
    let tls_start = Instant::now();
    let client = transport.connect(&addr).await?;
    let tls_time = tls_start.elapsed();

    // Measure QUIC handshake
    let quic_start = Instant::now();
    let mut stream = client.open_bi().await?;
    let quic_time = quic_start.elapsed();

    collector.record_connection(ConnectionProfile {
        establishment_time: start.elapsed(),
        tls_handshake_time: tls_time,
        quic_handshake_time: quic_time,
    });

    Ok(())
}

async fn analyze_stream_multiplexing(
    collector: &mut PerformanceCollector,
    stream_count: usize,
    message_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let (transport, addr) = setup_transport().await;
    let client = transport.connect(&addr).await?;
    let barrier = Arc::new(Barrier::new(stream_count));
    let data = Bytes::from(&vec![0u8; message_size][..]);

    let mut handles = Vec::new();
    let mut stream_metrics = MultiplexingMetrics {
        stream_creation_latency: Default::default(),
        concurrent_streams: stream_count,
        stream_switch_overhead: Duration::new(0, 0),
    };

    for _ in 0..stream_count {
        let barrier = barrier.clone();
        let data = data.clone();
        let collector = collector.clone();

        collector.start_stream();
        let stream_start = Instant::now();

        let handle = tokio::spawn(async move {
            let mut stream = client.open_bi().await?;
            let creation_time = stream_start.elapsed();

            // Wait for all streams to be ready
            barrier.wait().await;

            let send_start = Instant::now();
            stream.send_message(data).await?;
            let send_time = send_start.elapsed();

            collector.end_stream();
            Ok::<_, Box<dyn std::error::Error>>((creation_time, send_time))
        });

        handles.push(handle);
    }

    let mut creation_times = Vec::new();
    let mut send_times = Vec::new();

    for handle in handles {
        let (creation, send) = handle.await??;
        creation_times.push(creation);
        send_times.push(send);
    }

    // Calculate stream switching overhead
    if stream_count > 1 {
        let total_time: Duration = send_times.iter().sum();
        let avg_time = total_time / stream_count as u32;
        stream_metrics.stream_switch_overhead = avg_time;
    }

    collector.record_multiplexing(stream_metrics);
    Ok(())
}

async fn analyze_congestion_behavior(
    collector: &mut PerformanceCollector,
    message_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let (transport, addr) = setup_transport().await;
    let client = transport.connect(&addr).await?;
    let mut stream = client.open_bi().await?;
    let data = Bytes::from(&vec![0u8; message_size][..]);

    let mut metrics = CongestionMetrics {
        window_size: 0,
        retransmissions: 0,
        backpressure_events: 0,
        throughput_variation: 0.0,
    };

    // Send messages until we hit backpressure
    let mut sent = 0;
    let start = Instant::now();
    loop {
        match stream.send_message(data.clone()).await {
            Ok(_) => {
                sent += 1;
                if start.elapsed() > Duration::from_secs(1) {
                    break;
                }
            }
            Err(_) => {
                metrics.backpressure_events += 1;
                break;
            }
        }
    }

    metrics.window_size = sent * message_size;
    collector.record_congestion(metrics);

    Ok(())
}

fn bench_performance_profiles(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut collector = PerformanceCollector::new();

    // Connection establishment analysis
    let mut group = c.benchmark_group("connection_establishment");
    group.bench_function("connect", |b| {
        b.iter(|| rt.block_on(analyze_connection_establishment(&mut collector)))
    });
    group.finish();

    // Stream multiplexing analysis
    let mut group = c.benchmark_group("stream_multiplexing");
    for &size in SIZES {
        for &streams in STREAM_COUNTS {
            group.bench_function(
                BenchmarkId::new(format!("{}_{}", size.1, streams), size.0),
                |b| {
                    b.iter(|| {
                        rt.block_on(analyze_stream_multiplexing(&mut collector, streams, size.0))
                    })
                },
            );
        }
    }
    group.finish();

    // Congestion analysis
    let mut group = c.benchmark_group("congestion");
    for &(size, name) in SIZES {
        group.bench_function(BenchmarkId::new("congestion", name), |b| {
            b.iter(|| rt.block_on(analyze_congestion_behavior(&mut collector, size)))
        });
    }
    group.finish();

    // Generate comprehensive report
    collector
        .generate_report(c.benchmark_group("analysis"))
        .unwrap();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(50)
        .measurement_time(Duration::from_secs(30))
        .warm_up_time(Duration::from_secs(5));
    targets = bench_performance_profiles
}
criterion_main!(benches);
