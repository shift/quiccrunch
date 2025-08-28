use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use quiccrunch_core::{
    QuicTransport,
    Transport,
    io_uring_optimized::IoUringOptimizedClient
};
use bytes::Bytes;
use tokio::runtime::Runtime;
use std::time::{Duration, Instant};

// Advanced test configurations
const BUFFER_SIZES: &[usize] = &[4096, 8192, 16384, 32768, 65536];
const BUFFER_COUNTS: &[usize] = &[64, 128, 256, 512, 1024];
const POLLING_MODES: &[(&str, bool)] = &[
    ("normal", false),
    ("kernel-poll", true)
];

/// Tests zero-copy buffer ring performance
async fn buffer_ring_test(
    collector: &mut PerformanceCollector,
    buffer_size: usize,
    buffer_count: usize,
    messages: usize,
) -> std::io::Result<()> {
    let mut client = IoUringOptimizedClient::new(
        1024,
        buffer_size,
        buffer_count,
        false
    )?;

    let data = vec![0u8; buffer_size];
    let start = Instant::now();
    let mut buffer_stats = Vec::new();

    for i in 0..messages {
        let buf_start = Instant::now();
        client.send_message_multishot(&data).await?;
        buffer_stats.push(buf_start.elapsed());

        if i % buffer_count == 0 {
            // Record buffer ring stats
            let stats = client.get_stats();
            collector.record_buffer_efficiency(BufferEfficiency {
                buffer_size,
                buffer_count,
                hits: stats.buffer_hits.load(std::sync::atomic::Ordering::Relaxed),
                misses: stats.buffer_misses.load(std::sync::atomic::Ordering::Relaxed),
                operation_count: i,
            });
        }
    }

    collector.record_zero_copy_metrics(ZeroCopyMetrics {
        buffer_size,
        buffer_count,
        total_messages: messages,
        total_time: start.elapsed(),
        buffer_operations: buffer_stats,
    });

    Ok(())
}

/// Tests kernel polling mode effectiveness
async fn polling_test(
    collector: &mut PerformanceCollector,
    use_polling: bool,
    message_size: usize,
    duration: Duration,
) -> std::io::Result<()> {
    let mut client = IoUringOptimizedClient::new(
        1024,
        message_size,
        256,
        use_polling
    )?;

    let data = vec![0u8; message_size];
    let end_time = Instant::now() + duration;
    let mut latencies = Vec::new();
    let mut submission_times = Vec::new();
    let mut completion_times = Vec::new();

    while Instant::now() < end_time {
        let op_start = Instant::now();
        
        // Record submission time
        let submit_start = Instant::now();
        client.send_message_multishot(&data).await?;
        submission_times.push(submit_start.elapsed());

        // Record completion time
        let complete_start = Instant::now();
        let response = client.receive_message_fixed().await?;
        completion_times.push(complete_start.elapsed());

        latencies.push(op_start.elapsed());
    }

    collector.record_polling_metrics(PollingMetrics {
        use_polling,
        message_size,
        operation_latencies: latencies,
        submission_times,
        completion_times,
        stats: client.get_stats(),
    });

    Ok(())
}

/// Tests multishot submission efficiency
async fn multishot_test(
    collector: &mut PerformanceCollector,
    batch_size: usize,
    message_size: usize,
    iterations: usize,
) -> std::io::Result<()> {
    let mut client = IoUringOptimizedClient::new(
        1024,
        message_size,
        batch_size * 2, // Double buffer count for smooth operation
        true // Use polling for best multishot performance
    )?;

    let data = vec![0u8; message_size];
    let mut batch_times = Vec::new();
    let start = Instant::now();

    for _ in 0..iterations {
        let batch_start = Instant::now();
        
        // Submit batch of messages
        for _ in 0..batch_size {
            client.send_message_multishot(&data).await?;
        }

        batch_times.push(batch_start.elapsed());
    }

    collector.record_multishot_metrics(MultishotMetrics {
        batch_size,
        message_size,
        iterations,
        total_time: start.elapsed(),
        batch_times,
        stats: client.get_stats(),
    });

    Ok(())
}

fn bench_optimized_io_uring(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut collector = PerformanceCollector::new();

    // Buffer ring tests
    let mut group = c.benchmark_group("buffer_ring");
    for &buffer_size in BUFFER_SIZES {
        for &buffer_count in BUFFER_COUNTS {
            group.throughput(Throughput::Bytes((buffer_size * buffer_count) as u64));
            group.bench_function(
                BenchmarkId::new(
                    format!("size_{}_count_{}", buffer_size, buffer_count),
                    buffer_size
                ),
                |b| {
                    b.iter(|| {
                        rt.block_on(buffer_ring_test(
                            &mut collector,
                            buffer_size,
                            buffer_count,
                            10000,
                        ))
                    })
                },
            );
        }
    }
    group.finish();

    // Polling mode tests
    let mut group = c.benchmark_group("polling");
    for &(mode_name, use_polling) in POLLING_MODES {
        for &(size, _) in MESSAGE_SIZES {
            group.bench_function(
                BenchmarkId::new(format!("{}_{}", mode_name, size), size),
                |b| {
                    b.iter(|| {
                        rt.block_on(polling_test(
                            &mut collector,
                            use_polling,
                            size,
                            Duration::from_secs(5),
                        ))
                    })
                },
            );
        }
    }
    group.finish();

    // Multishot submission tests
    let mut group = c.benchmark_group("multishot");
    for batch_size in [10, 50, 100, 500] {
        for &(size, _) in MESSAGE_SIZES {
            group.throughput(Throughput::Bytes((size * batch_size) as u64));
            group.bench_function(
                BenchmarkId::new(format!("batch_{}_size_{}", batch_size, size), size),
                |b| {
                    b.iter(|| {
                        rt.block_on(multishot_test(
                            &mut collector,
                            batch_size,
                            size,
                            100,
                        ))
                    })
                },
            );
        }
    }
    group.finish();

    // Generate detailed analysis report
    collector.generate_optimized_report(c.benchmark_group("advanced_analysis")).unwrap();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(30))
        .warm_up_time(Duration::from_secs(5));
    targets = bench_optimized_io_uring
}
criterion_main!(benches);