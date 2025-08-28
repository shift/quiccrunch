use std::time::{Duration, Instant};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct IoUringMetrics {
    pub ring_size: u32,
    pub batch_size: usize,
    pub message_size: usize,
    pub total_messages: usize,
    pub total_time: Duration,
}

#[derive(Debug, Serialize)]
pub struct TransportStats {
    pub messages: usize,
    pub total_time: Duration,
    pub avg_latency: Duration,
}

#[derive(Debug, Serialize)]
pub struct TransportComparison {
    pub message_size: usize,
    pub quic_stats: TransportStats,
    pub io_uring_stats: TransportStats,
}

impl PerformanceCollector {
    pub fn record_io_uring_metrics(&mut self, metrics: IoUringMetrics) {
        let throughput = metrics.total_messages as f64 * metrics.message_size as f64 
            / metrics.total_time.as_secs_f64();
            
        self.io_uring_results.push(IoUringResult {
            metrics,
            throughput,
            efficiency: throughput / metrics.ring_size as f64,
        });
    }
    
    pub fn record_transport_comparison(&mut self, comparison: TransportComparison) {
        let quic_throughput = comparison.message_size as f64 * comparison.quic_stats.messages as f64
            / comparison.quic_stats.total_time.as_secs_f64();
            
        let io_uring_throughput = comparison.message_size as f64 * comparison.io_uring_stats.messages as f64
            / comparison.io_uring_stats.total_time.as_secs_f64();
            
        self.transport_comparisons.push(ComparisonResult {
            comparison,
            quic_throughput,
            io_uring_throughput,
            throughput_ratio: io_uring_throughput / quic_throughput,
        });
    }
    
    pub fn generate_comparison_report(&self, report: &mut std::fs::File) -> std::io::Result<()> {
        writeln!(report, "# Transport Comparison Analysis\n")?;
        
        // Ring size impact
        writeln!(report, "## io_uring Ring Size Impact\n")?;
        writeln!(report, "| Ring Size | Avg Throughput (MB/s) | Efficiency |")?;
        writeln!(report, "|-----------|---------------------|------------|")?;
        
        let mut by_ring_size = std::collections::HashMap::new();
        for result in &self.io_uring_results {
            by_ring_size.entry(result.metrics.ring_size)
                .or_insert_with(Vec::new)
                .push(result);
        }
        
        for (&ring_size, results) in by_ring_size.iter() {
            let avg_throughput = results.iter()
                .map(|r| r.throughput)
                .sum::<f64>() / results.len() as f64;
                
            let avg_efficiency = results.iter()
                .map(|r| r.efficiency)
                .sum::<f64>() / results.len() as f64;
                
            writeln!(report, "| {} | {:.2} | {:.4} |",
                ring_size,
                avg_throughput / 1_000_000.0,
                avg_efficiency
            )?;
        }
        
        // Message size comparison
        writeln!(report, "\n## Performance by Message Size\n")?;
        writeln!(report, "| Size | QUIC (MB/s) | io_uring (MB/s) | Ratio |")?;
        writeln!(report, "|------|-------------|-----------------|--------|")?;
        
        for comp in &self.transport_comparisons {
            writeln!(report, "| {} | {:.2} | {:.2} | {:.2}x |",
                comp.comparison.message_size,
                comp.quic_throughput / 1_000_000.0,
                comp.io_uring_throughput / 1_000_000.0,
                comp.throughput_ratio
            )?;
        }
        
        // Latency analysis
        writeln!(report, "\n## Latency Comparison\n")?;
        writeln!(report, "| Size | QUIC (μs) | io_uring (μs) | Diff |")?;
        writeln!(report, "|------|------------|---------------|------|")?;
        
        for comp in &self.transport_comparisons {
            let quic_latency = comp.comparison.quic_stats.avg_latency.as_micros();
            let io_uring_latency = comp.comparison.io_uring_stats.avg_latency.as_micros();
            let diff = (io_uring_latency as f64 / quic_latency as f64 * 100.0) - 100.0;
            
            writeln!(report, "| {} | {} | {} | {:.1}% |",
                comp.comparison.message_size,
                quic_latency,
                io_uring_latency,
                diff
            )?;
        }
        
        // Generate detailed plots
        self.plot_throughput_comparison()?;
        self.plot_latency_distribution()?;
        self.plot_efficiency_curves()?;
        
        Ok(())
    }
    
    fn plot_throughput_comparison(&self) -> std::io::Result<()> {
        let root = BitMapBackend::new("target/benchmark/transport_comparison.png", (800, 600))
            .into_drawing_area();
        root.fill(&WHITE)?;
        
        let max_throughput = self.transport_comparisons.iter()
            .map(|c| c.quic_throughput.max(c.io_uring_throughput))
            .fold(0.0, f64::max);
            
        let mut chart = ChartBuilder::on(&root)
            .caption("Transport Throughput Comparison", ("sans-serif", 30))
            .margin(10)
            .x_label_area_size(30)
            .y_label_area_size(50)
            .build_cartesian_2d(
                0f64..self.transport_comparisons.len() as f64,
                0f64..max_throughput
            )?;
            
        chart.configure_mesh().draw()?;
        
        // Plot QUIC throughput
        let quic_data: Vec<(f64, f64)> = self.transport_comparisons.iter()
            .enumerate()
            .map(|(i, c)| (i as f64, c.quic_throughput))
            .collect();
            
        chart.draw_series(LineSeries::new(quic_data, &BLUE))?
            .label("QUIC")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLUE));
            
        // Plot io_uring throughput
        let io_uring_data: Vec<(f64, f64)> = self.transport_comparisons.iter()
            .enumerate()
            .map(|(i, c)| (i as f64, c.io_uring_throughput))
            .collect();
            
        chart.draw_series(LineSeries::new(io_uring_data, &RED))?
            .label("io_uring")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));
            
        Ok(())
    }
    
    fn plot_latency_distribution(&self) -> std::io::Result<()> {
        let root = BitMapBackend::new("target/benchmark/latency_distribution.png", (800, 600))
            .into_drawing_area();
        root.fill(&WHITE)?;
        
        let max_latency = self.transport_comparisons.iter()
            .map(|c| {
                c.comparison.quic_stats.avg_latency
                    .max(c.comparison.io_uring_stats.avg_latency)
                    .as_micros() as f64
            })
            .fold(0.0, f64::max);
            
        let mut chart = ChartBuilder::on(&root)
            .caption("Latency Distribution", ("sans-serif", 30))
            .margin(10)
            .x_label_area_size(30)
            .y_label_area_size(50)
            .build_cartesian_2d(
                0f64..self.transport_comparisons.len() as f64,
                0f64..max_latency
            )?;
            
        chart.configure_mesh().draw()?;
        
        // Plot QUIC latencies
        let quic_data: Vec<(f64, f64)> = self.transport_comparisons.iter()
            .enumerate()
            .map(|(i, c)| (i as f64, c.comparison.quic_stats.avg_latency.as_micros() as f64))
            .collect();
            
        chart.draw_series(LineSeries::new(quic_data, &BLUE))?
            .label("QUIC")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &BLUE));
            
        // Plot io_uring latencies
        let io_uring_data: Vec<(f64, f64)> = self.transport_comparisons.iter()
            .enumerate()
            .map(|(i, c)| (i as f64, c.comparison.io_uring_stats.avg_latency.as_micros() as f64))
            .collect();
            
        chart.draw_series(LineSeries::new(io_uring_data, &RED))?
            .label("io_uring")
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], &RED));
            
        Ok(())
    }
    
    fn plot_efficiency_curves(&self) -> std::io::Result<()> {
        let root = BitMapBackend::new("target/benchmark/efficiency_curves.png", (800, 600))
            .into_drawing_area();
        root.fill(&WHITE)?;
        
        let max_efficiency = self.io_uring_results.iter()
            .map(|r| r.efficiency)
            .fold(0.0, f64::max);
            
        let mut chart = ChartBuilder::on(&root)
            .caption("io_uring Efficiency vs Ring Size", ("sans-serif", 30))
            .margin(10)
            .x_label_area_size(30)
            .y_label_area_size(50)
            .build_cartesian_2d(
                0u32..(*RING_SIZES.last().unwrap()),
                0f64..max_efficiency
            )?;
            
        chart.configure_mesh().draw()?;
        
        // Plot efficiency curves for different message sizes
        for &(size, name) in MESSAGE_SIZES {
            let data: Vec<(u32, f64)> = self.io_uring_results.iter()
                .filter(|r| r.metrics.message_size == size)
                .map(|r| (r.metrics.ring_size, r.efficiency))
                .collect();
                
            if !data.is_empty() {
                chart.draw_series(LineSeries::new(data, &Palette99::pick(size % 99)))?
                    .label(name)
                    .legend(|(x, y)| {
                        PathElement::new(
                            vec![(x, y), (x + 20, y)],
                            &Palette99::pick(size % 99)
                        )
                    });
            }
        }
        
        Ok(())
    }
}

#[derive(Debug)]
struct IoUringResult {
    metrics: IoUringMetrics,
    throughput: f64,
    efficiency: f64,
}

#[derive(Debug)]
struct ComparisonResult {
    comparison: TransportComparison,
    quic_throughput: f64,
    io_uring_throughput: f64,
    throughput_ratio: f64,
}