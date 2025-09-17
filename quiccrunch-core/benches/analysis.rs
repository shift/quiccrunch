use criterion::measurement::WallTime;
use plotters::prelude::*;
use serde::{Deserialize, Serialize};
use stats_alloc::{StatsAlloc, INSTRUMENTED_SYSTEM};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[global_allocator]
static GLOBAL: &StatsAlloc<std::alloc::System> = &INSTRUMENTED_SYSTEM;

#[derive(Debug, Serialize, Deserialize)]
pub struct LatencyProfile {
    p50: Duration,
    p90: Duration,
    p99: Duration,
    p999: Duration,
    p9999: Duration,
    min: Duration,
    max: Duration,
    mean: Duration,
    samples: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionProfile {
    establishment_time: Duration,
    tls_handshake_time: Duration,
    quic_handshake_time: Duration,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MultiplexingMetrics {
    stream_creation_latency: LatencyProfile,
    concurrent_streams: usize,
    stream_switch_overhead: Duration,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CongestionMetrics {
    window_size: usize,
    retransmissions: usize,
    backpressure_events: usize,
    throughput_variation: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryMetrics {
    total_allocations: usize,
    peak_memory: usize,
    allocation_sizes: BTreeMap<usize, usize>,
    deallocations: usize,
}

#[derive(Debug)]
pub struct PerformanceCollector {
    latencies: Vec<Duration>,
    connection_times: Vec<ConnectionProfile>,
    multiplexing_data: Vec<MultiplexingMetrics>,
    congestion_data: Vec<CongestionMetrics>,
    memory_snapshots: Vec<MemoryMetrics>,
    active_streams: Arc<AtomicUsize>,
}

impl PerformanceCollector {
    pub fn new() -> Self {
        Self {
            latencies: Vec::new(),
            connection_times: Vec::new(),
            multiplexing_data: Vec::new(),
            congestion_data: Vec::new(),
            memory_snapshots: Vec::new(),
            active_streams: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn record_latency(&mut self, duration: Duration) {
        self.latencies.push(duration);
    }

    pub fn record_connection(&mut self, profile: ConnectionProfile) {
        self.connection_times.push(profile);
    }

    pub fn start_stream(&self) {
        self.active_streams.fetch_add(1, Ordering::SeqCst);
    }

    pub fn end_stream(&self) {
        self.active_streams.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn take_memory_snapshot(&mut self) {
        let stats = GLOBAL.stats();
        let mut sizes = BTreeMap::new();

        for size in stats.allocation_sizes() {
            sizes.insert(size, stats.allocations_of_size(size));
        }

        self.memory_snapshots.push(MemoryMetrics {
            total_allocations: stats.allocations(),
            peak_memory: stats.peak_memory(),
            allocation_sizes: sizes,
            deallocations: stats.deallocations(),
        });
    }

    pub fn compute_latency_profile(&self) -> LatencyProfile {
        let mut sorted_latencies = self.latencies.clone();
        sorted_latencies.sort_unstable();

        let len = sorted_latencies.len();
        let sum: Duration = sorted_latencies.iter().sum();

        LatencyProfile {
            p50: sorted_latencies[len * 50 / 100],
            p90: sorted_latencies[len * 90 / 100],
            p99: sorted_latencies[len * 99 / 100],
            p999: sorted_latencies[len * 999 / 1000],
            p9999: sorted_latencies[len * 9999 / 10000],
            min: *sorted_latencies.first().unwrap(),
            max: *sorted_latencies.last().unwrap(),
            mean: sum / len as u32,
            samples: len,
        }
    }

    pub fn detect_head_of_line_blocking(&self) -> Vec<(Instant, Duration)> {
        let mut blocking_events = Vec::new();
        let mut last_latency = self.latencies.first().unwrap();

        for (idx, &latency) in self.latencies.iter().enumerate().skip(1) {
            // If latency spikes while multiple streams are active, likely HOL blocking
            if latency > *last_latency * 2 && self.active_streams.load(Ordering::SeqCst) > 1 {
                blocking_events.push((Instant::now() - latency, latency));
            }
            last_latency = &latency;
        }

        blocking_events
    }

    pub fn analyze_congestion(&self) -> CongestionMetrics {
        let mut metrics = CongestionMetrics {
            window_size: 0,
            retransmissions: 0,
            backpressure_events: 0,
            throughput_variation: 0.0,
        };

        for data in &self.congestion_data {
            metrics.retransmissions += data.retransmissions;
            metrics.backpressure_events += data.backpressure_events;
        }

        // Calculate throughput variation coefficient
        if !self.congestion_data.is_empty() {
            let throughputs: Vec<f64> = self
                .congestion_data
                .iter()
                .map(|d| d.window_size as f64)
                .collect();

            let mean = throughputs.iter().sum::<f64>() / throughputs.len() as f64;
            let variance = throughputs.iter().map(|&t| (t - mean).powi(2)).sum::<f64>()
                / throughputs.len() as f64;

            metrics.throughput_variation = (variance.sqrt() / mean);
        }

        metrics
    }

    pub fn generate_report(
        &self,
        group: &mut criterion::BenchmarkGroup<WallTime>,
    ) -> std::io::Result<()> {
        // Generate plots
        self.plot_latency_distribution()?;
        self.plot_memory_usage()?;
        self.plot_stream_concurrency()?;

        // Generate markdown report
        let mut report = std::fs::File::create("target/benchmark/analysis.md")?;

        writeln!(report, "# QuicCrunch Performance Analysis\n")?;

        // Latency Analysis
        let profile = self.compute_latency_profile();
        writeln!(report, "## Latency Analysis\n")?;
        writeln!(report, "- p50: {:?}", profile.p50)?;
        writeln!(report, "- p90: {:?}", profile.p90)?;
        writeln!(report, "- p99: {:?}", profile.p99)?;
        writeln!(report, "- p99.9: {:?}", profile.p999)?;
        writeln!(report, "- p99.99: {:?}", profile.p9999)?;
        writeln!(report, "- min: {:?}", profile.min)?;
        writeln!(report, "- max: {:?}", profile.max)?;
        writeln!(report, "- mean: {:?}", profile.mean)?;
        writeln!(report, "- samples: {}", profile.samples)?;

        // Connection Profiling
        writeln!(report, "\n## Connection Profiling\n")?;
        let avg_establishment = self
            .connection_times
            .iter()
            .map(|c| c.establishment_time)
            .sum::<Duration>()
            / self.connection_times.len() as u32;
        writeln!(
            report,
            "Average connection establishment: {:?}",
            avg_establishment
        )?;

        // Memory Analysis
        if let Some(last_snapshot) = self.memory_snapshots.last() {
            writeln!(report, "\n## Memory Usage\n")?;
            writeln!(
                report,
                "- Total allocations: {}",
                last_snapshot.total_allocations
            )?;
            writeln!(report, "- Peak memory: {} bytes", last_snapshot.peak_memory)?;
            writeln!(report, "- Deallocations: {}", last_snapshot.deallocations)?;

            writeln!(report, "\nAllocation size distribution:")?;
            for (size, count) in &last_snapshot.allocation_sizes {
                writeln!(report, "- {} bytes: {} allocations", size, count)?;
            }
        }

        // Head-of-line Blocking
        let blocking_events = self.detect_head_of_line_blocking();
        if !blocking_events.is_empty() {
            writeln!(report, "\n## Head-of-line Blocking Events\n")?;
            for (time, duration) in blocking_events {
                writeln!(report, "- At {:?}: blocked for {:?}", time, duration)?;
            }
        }

        // Congestion Analysis
        let congestion = self.analyze_congestion();
        writeln!(report, "\n## Congestion Analysis\n")?;
        writeln!(report, "- Retransmissions: {}", congestion.retransmissions)?;
        writeln!(
            report,
            "- Backpressure events: {}",
            congestion.backpressure_events
        )?;
        writeln!(
            report,
            "- Throughput variation: {:.2}%",
            congestion.throughput_variation * 100.0
        )?;

        Ok(())
    }

    fn plot_latency_distribution(&self) -> std::io::Result<()> {
        let root = BitMapBackend::new("target/benchmark/latency_distribution.png", (800, 600))
            .into_drawing_area();
        root.fill(&WHITE)?;

        let mut sorted_latencies: Vec<f64> = self
            .latencies
            .iter()
            .map(|d| d.as_secs_f64() * 1000.0) // Convert to milliseconds
            .collect();
        sorted_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let mut chart = ChartBuilder::on(&root)
            .caption("Latency Distribution", ("sans-serif", 30))
            .margin(10)
            .x_label_area_size(30)
            .y_label_area_size(50)
            .build_cartesian_2d(
                0f64..100f64,
                sorted_latencies[0]..sorted_latencies[sorted_latencies.len() - 1],
            )?;

        chart.configure_mesh().draw()?;

        let percentiles: Vec<(f64, f64)> = (0..100)
            .map(|i| {
                let idx = (i as f64 / 100.0 * (sorted_latencies.len() - 1) as f64) as usize;
                (i as f64, sorted_latencies[idx])
            })
            .collect();

        chart
            .draw_series(LineSeries::new(percentiles, &RED))?
            .label("Latency (ms)")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));

        Ok(())
    }

    fn plot_memory_usage(&self) -> std::io::Result<()> {
        let root =
            BitMapBackend::new("target/benchmark/memory_usage.png", (800, 600)).into_drawing_area();
        root.fill(&WHITE)?;

        let snapshots: Vec<(usize, usize)> = self
            .memory_snapshots
            .iter()
            .enumerate()
            .map(|(i, s)| (i, s.peak_memory))
            .collect();

        let mut chart = ChartBuilder::on(&root)
            .caption("Memory Usage Over Time", ("sans-serif", 30))
            .margin(10)
            .x_label_area_size(30)
            .y_label_area_size(50)
            .build_cartesian_2d(
                0..snapshots.len(),
                0..snapshots.iter().map(|(_, m)| *m).max().unwrap_or(1),
            )?;

        chart.configure_mesh().draw()?;

        chart
            .draw_series(LineSeries::new(snapshots, &BLUE))?
            .label("Peak Memory (bytes)")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLUE));

        Ok(())
    }

    fn plot_stream_concurrency(&self) -> std::io::Result<()> {
        let root = BitMapBackend::new("target/benchmark/stream_concurrency.png", (800, 600))
            .into_drawing_area();
        root.fill(&WHITE)?;

        let data: Vec<(usize, usize)> = self
            .multiplexing_data
            .iter()
            .enumerate()
            .map(|(i, m)| (i, m.concurrent_streams))
            .collect();

        let mut chart = ChartBuilder::on(&root)
            .caption("Stream Concurrency", ("sans-serif", 30))
            .margin(10)
            .x_label_area_size(30)
            .y_label_area_size(50)
            .build_cartesian_2d(
                0..data.len(),
                0..data.iter().map(|(_, s)| *s).max().unwrap_or(1),
            )?;

        chart.configure_mesh().draw()?;

        chart
            .draw_series(LineSeries::new(data, &GREEN))?
            .label("Active Streams")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &GREEN));

        Ok(())
    }
}
