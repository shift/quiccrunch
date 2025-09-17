use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use quiccrunch_core::{io_uring::IoUringQuicClient, QuicTransport, TlsConfig, Transport};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

// Test configurations
const RING_SIZES: &[u32] = &[128, 256, 512, 1024, 2048];
const BATCH_SIZES: &[usize] = &[1, 10, 50, 100, 500];
const MESSAGE_SIZES: &[(usize, &str)] = &[
    (64, "tiny"),
    (1024, "small"),
    (4096, "medium"),
    (16384, "large"),
    (65536, "xlarge"),
];

async fn io_uring_throughput_test(
    collector: &mut PerformanceCollector,
    ring_size: u32,
    batch_size: usize,
    message_size: usize,
) -> std::io::Result<()> {
    let mut client = IoUringQuicClient::new(ring_size)?;
    let addr = "127.0.0.1:50051".parse().unwrap();
    client.connect(addr).await?;

    let data = Bytes::from(&vec![0u8; message_size][..]);
    let total_messages = 10_000;
    let start = Instant::now();

    let mut completed = 0;
    let mut batch = Vec::with_capacity(batch_size);

    while completed < total_messages {
        // Fill batch
        for _ in 0..batch_size {
            batch.push(data.clone());
        }

        // Send batch
        let batch_start = Instant::now();
        for message in batch.drain(..) {
            client.send_message(message).await?;
        }
        collector.record_batch_latency(batch_start.elapsed());

        completed += batch_size;
    }

    collector.record_io_uring_metrics(IoUringMetrics {
        ring_size,
        batch_size,
        message_size,
        total_messages,
        total_time: start.elapsed(),
    });

    Ok(())
}

async fn compare_transports(
    collector: &mut PerformanceCollector,
    message_size: usize,
    duration: Duration,
) -> std::io::Result<()> {
    // Standard QUIC transport
    let (transport, addr) = setup_transport().await;
    let quic_client = transport.connect(&addr).await.unwrap();
    let mut quic_stream = quic_client.open_bi().await.unwrap();

    // io_uring transport
    let mut io_uring_client = IoUringQuicClient::new(1024)?;
    io_uring_client.connect(addr).await?;

    let data = Bytes::from(&vec![0u8; message_size][..]);
    let end_time = Instant::now() + duration;

    // Run both transports simultaneously
    let (quic_stats, io_uring_stats) = tokio::join!(
        async {
            let mut messages = 0;
            let mut total_latency = Duration::default();
            let start = Instant::now();

            while Instant::now() < end_time {
                let msg_start = Instant::now();
                quic_stream.send_message(data.clone()).await.unwrap();
                total_latency += msg_start.elapsed();
                messages += 1;
            }

            TransportStats {
                messages,
                total_time: start.elapsed(),
                avg_latency: total_latency / messages as u32,
            }
        },
        async {
            let mut messages = 0;
            let mut total_latency = Duration::default();
            let start = Instant::now();

            while Instant::now() < end_time {
                let msg_start = Instant::now();
                io_uring_client.send_message(data.clone()).await?;
                total_latency += msg_start.elapsed();
                messages += 1;
            }

            Ok(TransportStats {
                messages,
                total_time: start.elapsed(),
                avg_latency: total_latency / messages as u32,
            })
        }
    );

    collector.record_transport_comparison(TransportComparison {
        message_size,
        quic_stats,
        io_uring_stats: io_uring_stats?,
    });

    Ok(())
}

fn bench_io_uring(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut collector = PerformanceCollector::new();

    // Throughput tests with different ring sizes
    let mut group = c.benchmark_group("io_uring_throughput");
    for &ring_size in RING_SIZES {
        for &batch_size in BATCH_SIZES {
            for &(size, name) in MESSAGE_SIZES {
                group.throughput(Throughput::Bytes((size * batch_size) as u64));
                group.bench_function(
                    BenchmarkId::new(
                        format!("ring_{}_batch_{}_size_{}", ring_size, batch_size, name),
                        size,
                    ),
                    |b| {
                        b.iter(|| {
                            rt.block_on(io_uring_throughput_test(
                                &mut collector,
                                ring_size,
                                batch_size,
                                size,
                            ))
                        })
                    },
                );
            }
        }
    }
    group.finish();

    // Transport comparison tests
    let mut group = c.benchmark_group("transport_comparison");
    for &(size, name) in MESSAGE_SIZES {
        group.bench_function(BenchmarkId::new("comparison", name), |b| {
            b.iter(|| {
                rt.block_on(compare_transports(
                    &mut collector,
                    size,
                    Duration::from_secs(10),
                ))
            })
        });
    }
    group.finish();

    // Generate detailed comparison report
    collector
        .generate_comparison_report(c.benchmark_group("analysis"))
        .unwrap();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(30))
        .warm_up_time(Duration::from_secs(5));
    targets = bench_io_uring
}
criterion_main!(benches);
