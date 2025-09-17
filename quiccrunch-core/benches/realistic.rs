use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use quiccrunch_core::{QuicTransport, TlsConfig, Transport};
use rand::{thread_rng, Rng};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::{mpsc, Barrier};

// Message batching configurations
const BATCH_SIZES: &[usize] = &[1, 10, 50, 100, 500];
const BATCH_INTERVALS: &[Duration] = &[
    Duration::from_millis(0),   // Immediate
    Duration::from_millis(1),   // Ultra-low latency
    Duration::from_millis(10),  // Low latency
    Duration::from_millis(50),  // Balanced
    Duration::from_millis(100), // Throughput oriented
];

// Workload patterns
#[derive(Debug, Clone)]
enum MessagePattern {
    Small { size: usize, frequency: Duration },
    Large { size: usize, frequency: Duration },
    Burst { size: usize, count: usize },
}

const MIXED_WORKLOADS: &[(&str, &[MessagePattern])] = &[
    (
        "rpc_heavy",
        &[
            MessagePattern::Small {
                size: 64,
                frequency: Duration::from_millis(1),
            },
            MessagePattern::Small {
                size: 256,
                frequency: Duration::from_millis(5),
            },
            MessagePattern::Large {
                size: 4096,
                frequency: Duration::from_millis(100),
            },
        ],
    ),
    (
        "bulk_with_control",
        &[
            MessagePattern::Small {
                size: 64,
                frequency: Duration::from_millis(10),
            },
            MessagePattern::Large {
                size: 65536,
                frequency: Duration::from_millis(50),
            },
            MessagePattern::Burst {
                size: 1024,
                count: 100,
            },
        ],
    ),
    (
        "mixed_streaming",
        &[
            MessagePattern::Small {
                size: 1024,
                frequency: Duration::from_millis(10),
            },
            MessagePattern::Large {
                size: 32768,
                frequency: Duration::from_millis(100),
            },
            MessagePattern::Burst {
                size: 8192,
                count: 50,
            },
        ],
    ),
];

/// Tests message batching with different batch sizes and intervals
async fn batch_optimization_test(
    collector: &mut PerformanceCollector,
    batch_size: usize,
    interval: Duration,
    message_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let (transport, addr) = setup_transport().await;
    let client = transport.connect(&addr).await?;
    let mut stream = client.open_bi().await?;
    let data = Bytes::from(&vec![0u8; message_size][..]);

    let (tx, mut rx) = mpsc::channel(batch_size);
    let start = Instant::now();

    // Producer task
    let producer = tokio::spawn({
        let tx = tx.clone();
        let data = data.clone();
        async move {
            let mut messages_sent = 0;
            while messages_sent < 1000 {
                for _ in 0..batch_size {
                    if tx.send(data.clone()).await.is_err() {
                        return messages_sent;
                    }
                    messages_sent += 1;
                }
                if !interval.is_zero() {
                    tokio::time::sleep(interval).await;
                }
            }
            messages_sent
        }
    });

    // Consumer task - batches messages for sending
    let mut batch = Vec::with_capacity(batch_size);
    let mut total_bytes = 0;
    let mut batch_counts = Vec::new();

    while let Some(msg) = rx.recv().await {
        batch.push(msg);

        if batch.len() == batch_size {
            let batch_start = Instant::now();
            let batch_bytes: usize = batch.iter().map(|b| b.len()).sum();

            // Send the batch
            for msg in batch.drain(..) {
                stream.send_message(msg).await?;
            }

            total_bytes += batch_bytes;
            batch_counts.push((batch_bytes, batch_start.elapsed()));
        }
    }

    let messages_sent = producer.await?;
    let total_time = start.elapsed();

    collector.record_batch_metrics(BatchMetrics {
        batch_size,
        interval,
        message_size,
        total_messages: messages_sent,
        total_bytes,
        total_time,
        batch_timings: batch_counts,
    });

    Ok(())
}

/// Tests realistic mixed workload patterns
async fn mixed_workload_test(
    collector: &mut PerformanceCollector,
    patterns: &[MessagePattern],
    duration: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let (transport, addr) = setup_transport().await;
    let client = transport.connect(&addr).await?;
    let mut stream = client.open_bi().await?;

    let start = Instant::now();
    let mut handles = Vec::new();
    let completion_barrier = Arc::new(Barrier::new(patterns.len()));

    // Spawn a task for each pattern
    for pattern in patterns {
        let barrier = completion_barrier.clone();
        let pattern = pattern.clone();
        let mut stream = client.open_bi().await?;

        handles.push(tokio::spawn(async move {
            let mut rng = thread_rng();
            let mut pattern_metrics = WorkloadMetrics::new();

            let end_time = Instant::now() + duration;
            while Instant::now() < end_time {
                match pattern {
                    MessagePattern::Small { size, frequency } => {
                        let data = Bytes::from(&vec![0u8; size][..]);
                        let start_send = Instant::now();
                        stream.send_message(data).await?;
                        pattern_metrics.record_message(size, start_send.elapsed());
                        tokio::time::sleep(frequency).await;
                    }
                    MessagePattern::Large { size, frequency } => {
                        let data = Bytes::from(&vec![0u8; size][..]);
                        let start_send = Instant::now();
                        stream.send_message(data).await?;
                        pattern_metrics.record_message(size, start_send.elapsed());
                        tokio::time::sleep(frequency).await;
                    }
                    MessagePattern::Burst { size, count } => {
                        let data = Bytes::from(&vec![0u8; size][..]);
                        let burst_start = Instant::now();

                        for _ in 0..count {
                            let start_send = Instant::now();
                            stream.send_message(data.clone()).await?;
                            pattern_metrics.record_message(size, start_send.elapsed());
                        }

                        pattern_metrics.record_burst(burst_start.elapsed());

                        // Random delay between bursts
                        tokio::time::sleep(Duration::from_millis(rng.gen_range(50..200))).await;
                    }
                }
            }

            barrier.wait().await;
            Ok::<_, Box<dyn std::error::Error>>(pattern_metrics)
        }));
    }

    // Collect results
    let mut pattern_results = Vec::new();
    for handle in handles {
        pattern_results.push(handle.await??);
    }

    collector.record_workload_results(WorkloadResults {
        duration,
        pattern_metrics: pattern_results,
        total_time: start.elapsed(),
    });

    Ok(())
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10)  // Reduced because these are long-running tests
        .measurement_time(Duration::from_secs(60))
        .warm_up_time(Duration::from_secs(10));
    targets = bench_realistic_patterns
}
criterion_main!(benches);

fn bench_realistic_patterns(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut collector = PerformanceCollector::new();

    // Message batching optimization tests
    let mut group = c.benchmark_group("batch_optimization");
    for &batch_size in BATCH_SIZES {
        for &interval in BATCH_INTERVALS {
            for &(size, name) in SIZES {
                group.bench_function(
                    BenchmarkId::new(
                        format!(
                            "batch_{}_interval_{}_size_{}",
                            batch_size,
                            interval.as_millis(),
                            name
                        ),
                        size,
                    ),
                    |b| {
                        b.iter(|| {
                            rt.block_on(batch_optimization_test(
                                &mut collector,
                                batch_size,
                                interval,
                                size,
                            ))
                        })
                    },
                );
            }
        }
    }
    group.finish();

    // Mixed workload pattern tests
    let mut group = c.benchmark_group("mixed_workloads");
    for &(name, patterns) in MIXED_WORKLOADS {
        group.bench_function(name, |b| {
            b.iter(|| {
                rt.block_on(mixed_workload_test(
                    &mut collector,
                    patterns,
                    Duration::from_secs(30),
                ))
            })
        });
    }
    group.finish();

    // Generate detailed report
    collector
        .generate_report(c.benchmark_group("realistic_analysis"))
        .unwrap();
}
