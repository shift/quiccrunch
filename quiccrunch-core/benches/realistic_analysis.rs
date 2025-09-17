use serde::Serialize;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Serialize)]
pub struct BatchMetrics {
    pub batch_size: usize,
    pub interval: Duration,
    pub message_size: usize,
    pub total_messages: usize,
    pub total_bytes: usize,
    pub total_time: Duration,
    pub batch_timings: Vec<(usize, Duration)>, // (batch_bytes, send_time)
}

#[derive(Debug, Serialize)]
pub struct WorkloadMetrics {
    pub message_counts: HashMap<usize, usize>, // size -> count
    pub message_latencies: Vec<Duration>,      // Individual message latencies
    pub burst_latencies: Vec<Duration>,        // Full burst latencies
    pub total_bytes: usize,
    pub start_time: Instant,
}

#[derive(Debug, Serialize)]
pub struct WorkloadResults {
    pub duration: Duration,
    pub pattern_metrics: Vec<WorkloadMetrics>,
    pub total_time: Duration,
}

impl WorkloadMetrics {
    pub fn new() -> Self {
        Self {
            message_counts: HashMap::new(),
            message_latencies: Vec::new(),
            burst_latencies: Vec::new(),
            total_bytes: 0,
            start_time: Instant::now(),
        }
    }

    pub fn record_message(&mut self, size: usize, latency: Duration) {
        *self.message_counts.entry(size).or_insert(0) += 1;
        self.message_latencies.push(latency);
        self.total_bytes += size;
    }

    pub fn record_burst(&mut self, latency: Duration) {
        self.burst_latencies.push(latency);
    }
}

impl PerformanceCollector {
    pub fn record_batch_metrics(&mut self, metrics: BatchMetrics) {
        // Calculate batch efficiency
        let avg_batch_time = metrics
            .batch_timings
            .iter()
            .map(|(_, t)| t.as_nanos() as f64)
            .sum::<f64>()
            / metrics.batch_timings.len() as f64;

        let batch_throughput = metrics.total_bytes as f64 / metrics.total_time.as_secs_f64();

        let overhead_ratio = metrics
            .batch_timings
            .iter()
            .map(|(bytes, time)| time.as_secs_f64() / bytes as f64)
            .sum::<f64>()
            / metrics.batch_timings.len() as f64;

        self.batch_stats.push(BatchStats {
            size: metrics.batch_size,
            interval: metrics.interval,
            avg_time: Duration::from_nanos(avg_batch_time as u64),
            throughput: batch_throughput,
            overhead_ratio,
        });
    }

    pub fn record_workload_results(&mut self, results: WorkloadResults) {
        let mut pattern_stats = Vec::new();

        for metrics in results.pattern_metrics {
            let total_messages: usize = metrics.message_counts.values().sum();
            let avg_latency =
                metrics.message_latencies.iter().sum::<Duration>() / total_messages as u32;

            let mut latencies = metrics.message_latencies.clone();
            latencies.sort_unstable();

            pattern_stats.push(PatternStats {
                message_sizes: metrics.message_counts,
                avg_latency,
                p50: latencies[latencies.len() * 50 / 100],
                p99: latencies[latencies.len() * 99 / 100],
                throughput: metrics.total_bytes as f64 / results.duration.as_secs_f64(),
            });
        }

        self.workload_stats.push(WorkloadStats {
            duration: results.duration,
            patterns: pattern_stats,
            total_time: results.total_time,
        });
    }

    fn plot_batch_efficiency(&self) -> std::io::Result<()> {
        let root = BitMapBackend::new("target/benchmark/batch_efficiency.png", (800, 600))
            .into_drawing_area();
        root.fill(&WHITE)?;

        let max_throughput = self
            .batch_stats
            .iter()
            .map(|s| s.throughput)
            .fold(0.0, f64::max);

        let mut chart = ChartBuilder::on(&root)
            .caption("Batch Processing Efficiency", ("sans-serif", 30))
            .margin(10)
            .x_label_area_size(30)
            .y_label_area_size(50)
            .build_cartesian_2d(0f64..self.batch_stats.len() as f64, 0f64..max_throughput)?;

        chart.configure_mesh().draw()?;

        // Plot throughput by batch size
        let throughput_data: Vec<(f64, f64)> = self
            .batch_stats
            .iter()
            .enumerate()
            .map(|(i, s)| (i as f64, s.throughput))
            .collect();

        chart
            .draw_series(LineSeries::new(throughput_data, &BLUE))?
            .label("Throughput (bytes/s)")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLUE));

        Ok(())
    }

    fn analyze_workload_patterns(&self) -> WorkloadAnalysis {
        let mut analysis = WorkloadAnalysis {
            pattern_correlations: Vec::new(),
            throughput_stability: 0.0,
            latency_impact: HashMap::new(),
        };

        // Analyze correlations between different patterns
        for stats in &self.workload_stats {
            if stats.patterns.len() < 2 {
                continue;
            }

            for i in 0..stats.patterns.len() {
                for j in i + 1..stats.patterns.len() {
                    let correlation =
                        self.calculate_pattern_correlation(&stats.patterns[i], &stats.patterns[j]);
                    analysis.pattern_correlations.push(PatternCorrelation {
                        pattern1: i,
                        pattern2: j,
                        correlation,
                    });
                }
            }

            // Calculate throughput stability
            let throughputs: Vec<f64> = stats.patterns.iter().map(|p| p.throughput).collect();

            let mean = throughputs.iter().sum::<f64>() / throughputs.len() as f64;
            let variance = throughputs.iter().map(|&t| (t - mean).powi(2)).sum::<f64>()
                / throughputs.len() as f64;

            analysis.throughput_stability = variance.sqrt() / mean;

            // Analyze latency impact
            for pattern in &stats.patterns {
                for (&size, &count) in &pattern.message_sizes {
                    let entry = analysis.latency_impact.entry(size).or_insert_with(Vec::new);
                    entry.push((count, pattern.avg_latency));
                }
            }
        }

        analysis
    }

    fn calculate_pattern_correlation(&self, p1: &PatternStats, p2: &PatternStats) -> f64 {
        // Calculate correlation coefficient between pattern throughputs
        let t1 = p1.throughput;
        let t2 = p2.throughput;

        if t1 == 0.0 || t2 == 0.0 {
            return 0.0;
        }

        // Simple correlation based on relative throughput impact
        (t1.min(t2) / t1.max(t2)).powf(2.0)
    }
}

#[derive(Debug)]
struct BatchStats {
    size: usize,
    interval: Duration,
    avg_time: Duration,
    throughput: f64,
    overhead_ratio: f64,
}

#[derive(Debug)]
struct PatternStats {
    message_sizes: HashMap<usize, usize>,
    avg_latency: Duration,
    p50: Duration,
    p99: Duration,
    throughput: f64,
}

#[derive(Debug)]
struct WorkloadStats {
    duration: Duration,
    patterns: Vec<PatternStats>,
    total_time: Duration,
}

#[derive(Debug)]
struct PatternCorrelation {
    pattern1: usize,
    pattern2: usize,
    correlation: f64,
}

#[derive(Debug)]
struct WorkloadAnalysis {
    pattern_correlations: Vec<PatternCorrelation>,
    throughput_stability: f64,
    latency_impact: HashMap<usize, Vec<(usize, Duration)>>,
}
