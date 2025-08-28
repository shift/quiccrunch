use std::time::{Duration, Instant};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ThroughputMetrics {
    pub bytes: usize,
    pub duration: Duration,
    pub window_size: usize,
}

#[derive(Debug, Serialize)]
pub struct PoolMetrics {
    pub pool_size: usize,
    pub operations: usize,
    pub total_time: Duration,
}

#[derive(Debug, Serialize)]
pub struct ChurnMetrics {
    pub operations: usize,
    pub total_time: Duration,
}

impl PerformanceCollector {
    pub fn record_throughput(&mut self, bytes_per_second: f64) {
        self.throughput_samples.push(bytes_per_second);
    }
    
    pub fn record_total_transfer(&mut self, metrics: ThroughputMetrics) {
        self.transfer_metrics.push(metrics);
    }
    
    pub fn record_probe_latency(&mut self, duration: Duration) {
        self.probe_latencies.push(duration);
    }
    
    pub fn record_operation_latency(&mut self, duration: Duration) {
        self.operation_latencies.push(duration);
    }
    
    pub fn record_pool_metrics(&mut self, metrics: PoolMetrics) {
        self.pool_metrics.push(metrics);
    }
    
    pub fn record_recovery_time(&mut self, duration: Duration) {
        self.recovery_times.push(duration);
    }
    
    pub fn record_churn_operation(&mut self, duration: Duration) {
        self.churn_operations.push(duration);
    }
    
    pub fn record_churn_test(&mut self, metrics: ChurnMetrics) {
        self.churn_metrics.push(metrics);
    }
    
    pub fn analyze_bandwidth_stability(&self) -> f64 {
        if self.throughput_samples.is_empty() {
            return 0.0;
        }
        
        let mean = self.throughput_samples.iter().sum::<f64>() / self.throughput_samples.len() as f64;
        let variance = self.throughput_samples.iter()
            .map(|&x| (x - mean).powi(2))
            .sum::<f64>() / self.throughput_samples.len() as f64;
            
        (variance.sqrt() / mean) * 100.0 // Return as percentage
    }
    
    pub fn analyze_recovery_performance(&self) -> RecoveryStats {
        let mean = self.recovery_times.iter().sum::<Duration>() / self.recovery_times.len() as u32;
        let max = self.recovery_times.iter().max().copied().unwrap_or_default();
        let min = self.recovery_times.iter().min().copied().unwrap_or_default();
        
        RecoveryStats { mean, max, min }
    }
    
    pub fn analyze_pool_efficiency(&self) -> Vec<PoolEfficiency> {
        self.pool_metrics.iter().map(|m| {
            let ops_per_second = m.operations as f64 / m.total_time.as_secs_f64();
            let ops_per_stream = m.operations as f64 / m.pool_size as f64;
            
            PoolEfficiency {
                pool_size: m.pool_size,
                ops_per_second,
                ops_per_stream,
            }
        }).collect()
    }
    
    fn plot_bandwidth_patterns(&self) -> std::io::Result<()> {
        let root = BitMapBackend::new("target/benchmark/bandwidth_patterns.png", (800, 600))
            .into_drawing_area();
        root.fill(&WHITE)?;
        
        let max_throughput = self.transfer_metrics.iter()
            .map(|m| (m.bytes as f64 / m.duration.as_secs_f64()))
            .fold(0.0, f64::max);
            
        let mut chart = ChartBuilder::on(&root)
            .caption("Bandwidth Patterns", ("sans-serif", 30))
            .margin(10)
            .x_label_area_size(30)
            .y_label_area_size(50)
            .build_cartesian_2d(
                0f64..self.transfer_metrics.len() as f64,
                0f64..max_throughput
            )?;
            
        chart.configure_mesh().draw()?;
        
        let throughput_data: Vec<(f64, f64)> = self.transfer_metrics.iter()
            .enumerate()
            .map(|(i, m)| (i as f64, m.bytes as f64 / m.duration.as_secs_f64()))
            .collect();
            
        chart.draw_series(LineSeries::new(throughput_data, &BLUE))?
            .label("Throughput (bytes/s)")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLUE));
            
        Ok(())
    }
    
    pub fn extend_report(&self, report: &mut std::fs::File) -> std::io::Result<()> {
        writeln!(report, "\n## Advanced Analysis\n")?;
        
        // Bandwidth stability
        let stability = self.analyze_bandwidth_stability();
        writeln!(report, "### Bandwidth Stability\n")?;
        writeln!(report, "- Throughput variation: {:.2}%", stability)?;
        writeln!(report, "- Mean throughput: {:.2} MB/s", 
            self.throughput_samples.iter().sum::<f64>() / 
            self.throughput_samples.len() as f64 / 1_000_000.0)?;
            
        // Recovery performance
        let recovery = self.analyze_recovery_performance();
        writeln!(report, "\n### Recovery Performance\n")?;
        writeln!(report, "- Mean recovery time: {:?}", recovery.mean)?;
        writeln!(report, "- Max recovery time: {:?}", recovery.max)?;
        writeln!(report, "- Min recovery time: {:?}", recovery.min)?;
        
        // Connection pool efficiency
        let pool_stats = self.analyze_pool_efficiency();
        writeln!(report, "\n### Connection Pool Efficiency\n")?;
        writeln!(report, "| Pool Size | Ops/second | Ops/stream |")?;
        writeln!(report, "|-----------|------------|------------|")?;
        for stat in pool_stats {
            writeln!(report, "| {} | {:.2} | {:.2} |",
                stat.pool_size, stat.ops_per_second, stat.ops_per_stream)?;
        }
        
        Ok(())
    }
}

#[derive(Debug)]
pub struct RecoveryStats {
    pub mean: Duration,
    pub max: Duration,
    pub min: Duration,
}

#[derive(Debug)]
pub struct PoolEfficiency {
    pub pool_size: usize,
    pub ops_per_second: f64,
    pub ops_per_stream: f64,
}