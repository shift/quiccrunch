use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use handlebars::Handlebars;

use crate::config::ReportingConfig;
use crate::framework::TestResult;
use crate::metrics::TestMetrics;

pub struct TestReporter {
    config: ReportingConfig,
    handlebars: Handlebars<'static>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestReport {
    pub metadata: ReportMetadata,
    pub results: Vec<TestResult>,
    pub summary: TestSummary,
    pub performance_analysis: PerformanceAnalysis,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReportMetadata {
    pub generated_at: DateTime<Utc>,
    pub test_environment: TestEnvironment,
    pub framework_version: String,
    pub total_duration: std::time::Duration,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestEnvironment {
    pub hostname: String,
    pub os: String,
    pub architecture: String,
    pub cpu_cores: usize,
    pub memory_gb: f64,
    pub rust_version: String,
    pub quiccrunch_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestSummary {
    pub total_scenarios: usize,
    pub successful_scenarios: usize,
    pub failed_scenarios: usize,
    pub total_requests: u64,
    pub total_errors: u64,
    pub average_throughput: f64,
    pub average_latency_p99: std::time::Duration,
    pub peak_connections: u64,
    pub total_data_transferred: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceAnalysis {
    pub throughput_trend: TrendAnalysis,
    pub latency_trend: TrendAnalysis,
    pub error_trend: TrendAnalysis,
    pub resource_utilization: ResourceUtilization,
    pub bottlenecks: Vec<Bottleneck>,
    pub performance_score: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrendAnalysis {
    pub trend: String, // "improving", "degrading", "stable"
    pub change_percent: f64,
    pub confidence: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceUtilization {
    pub cpu_peak: f64,
    pub cpu_average: f64,
    pub memory_peak: f64,
    pub memory_average: f64,
    pub network_utilization: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Bottleneck {
    pub component: String,
    pub description: String,
    pub severity: String, // "low", "medium", "high", "critical"
    pub recommendation: String,
}

impl TestReporter {
    pub fn new(config: ReportingConfig) -> Self {
        let mut handlebars = Handlebars::new();
        
        // Register HTML template
        let html_template = include_str!("../templates/report.html");
        handlebars.register_template_string("html_report", html_template)
            .expect("Failed to register HTML template");
        
        // Register text template
        let text_template = include_str!("../templates/report.txt");
        handlebars.register_template_string("text_report", text_template)
            .expect("Failed to register text template");

        Self {
            config,
            handlebars,
        }
    }

    pub async fn generate_report(&self, result: &TestResult) -> Result<()> {
        let report = self.create_report(vec![result.clone()]).await?;
        
        fs::create_dir_all(&self.config.output_dir)
            .context("Failed to create output directory")?;

        for format in &self.config.formats {
            match format.as_str() {
                "json" => self.generate_json_report(&report).await?,
                "html" => self.generate_html_report(&report).await?,
                "txt" => self.generate_text_report(&report).await?,
                _ => tracing::warn!("Unknown report format: {}", format),
            }
        }

        if self.config.include_graphs {
            self.generate_graphs(&report).await?;
        }

        Ok(())
    }

    pub async fn generate_comprehensive_report(&self, results: &[TestResult]) -> Result<TestReport> {
        self.create_report(results.to_vec()).await
    }

    async fn create_report(&self, results: Vec<TestResult>) -> Result<TestReport> {
        let metadata = self.create_metadata(&results).await?;
        let summary = self.create_summary(&results);
        let performance_analysis = self.analyze_performance(&results);
        let recommendations = self.generate_recommendations(&performance_analysis);

        Ok(TestReport {
            metadata,
            results,
            summary,
            performance_analysis,
            recommendations,
        })
    }

    async fn create_metadata(&self, results: &[TestResult]) -> Result<ReportMetadata> {
        let total_duration = results.iter()
            .map(|r| r.duration)
            .max()
            .unwrap_or(std::time::Duration::ZERO);

        Ok(ReportMetadata {
            generated_at: Utc::now(),
            test_environment: self.get_test_environment().await?,
            framework_version: "0.1.0".to_string(),
            total_duration,
        })
    }

    async fn get_test_environment(&self) -> Result<TestEnvironment> {
        Ok(TestEnvironment {
            hostname: hostname::get()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            os: std::env::consts::OS.to_string(),
            architecture: std::env::consts::ARCH.to_string(),
            cpu_cores: num_cpus::get(),
            memory_gb: self.get_total_memory_gb().await.unwrap_or(0.0),
            rust_version: self.get_rust_version(),
            quiccrunch_version: "0.1.0".to_string(),
        })
    }

    async fn get_total_memory_gb(&self) -> Result<f64> {
        // TODO: Implement actual memory detection
        Ok(16.0) // Default assumption
    }

    fn get_rust_version(&self) -> String {
        std::env::var("RUSTC_VERSION")
            .unwrap_or_else(|_| "unknown".to_string())
    }

    fn create_summary(&self, results: &[TestResult]) -> TestSummary {
        let total_scenarios = results.len();
        let successful_scenarios = results.iter().filter(|r| r.success).count();
        let failed_scenarios = total_scenarios - successful_scenarios;

        let total_requests: u64 = results.iter().map(|r| r.metrics.total_requests).sum();
        let total_errors: u64 = results.iter().map(|r| r.metrics.failed_requests).sum();
        
        let average_throughput = if !results.is_empty() {
            results.iter().map(|r| r.metrics.throughput_rps).sum::<f64>() / results.len() as f64
        } else {
            0.0
        };

        let average_latency_p99 = if !results.is_empty() {
            let total_latency: std::time::Duration = results.iter().map(|r| r.metrics.latency_p99).sum();
            total_latency / results.len() as u32
        } else {
            std::time::Duration::ZERO
        };

        let peak_connections = results.iter()
            .map(|r| r.metrics.active_connections)
            .max()
            .unwrap_or(0);

        let total_data_transferred = results.iter()
            .map(|r| r.metrics.bytes_sent + r.metrics.bytes_received)
            .sum();

        TestSummary {
            total_scenarios,
            successful_scenarios,
            failed_scenarios,
            total_requests,
            total_errors,
            average_throughput,
            average_latency_p99,
            peak_connections,
            total_data_transferred,
        }
    }

    fn analyze_performance(&self, results: &[TestResult]) -> PerformanceAnalysis {
        let throughput_trend = self.analyze_throughput_trend(results);
        let latency_trend = self.analyze_latency_trend(results);
        let error_trend = self.analyze_error_trend(results);
        let resource_utilization = self.analyze_resource_utilization(results);
        let bottlenecks = self.identify_bottlenecks(results);
        let performance_score = self.calculate_performance_score(results);

        PerformanceAnalysis {
            throughput_trend,
            latency_trend,
            error_trend,
            resource_utilization,
            bottlenecks,
            performance_score,
        }
    }

    fn analyze_throughput_trend(&self, results: &[TestResult]) -> TrendAnalysis {
        if results.len() < 2 {
            return TrendAnalysis {
                trend: "insufficient_data".to_string(),
                change_percent: 0.0,
                confidence: 0.0,
            };
        }

        let throughputs: Vec<f64> = results.iter().map(|r| r.metrics.throughput_rps).collect();
        let first_half_avg = throughputs[..throughputs.len()/2].iter().sum::<f64>() / (throughputs.len()/2) as f64;
        let second_half_avg = throughputs[throughputs.len()/2..].iter().sum::<f64>() / (throughputs.len() - throughputs.len()/2) as f64;
        
        let change_percent = ((second_half_avg - first_half_avg) / first_half_avg) * 100.0;
        
        let trend = if change_percent > 5.0 {
            "improving"
        } else if change_percent < -5.0 {
            "degrading"
        } else {
            "stable"
        }.to_string();

        TrendAnalysis {
            trend,
            change_percent,
            confidence: 0.8, // TODO: Calculate actual confidence interval
        }
    }

    fn analyze_latency_trend(&self, results: &[TestResult]) -> TrendAnalysis {
        if results.len() < 2 {
            return TrendAnalysis {
                trend: "insufficient_data".to_string(),
                change_percent: 0.0,
                confidence: 0.0,
            };
        }

        let latencies: Vec<f64> = results.iter().map(|r| r.metrics.latency_p99.as_secs_f64()).collect();
        let first_half_avg = latencies[..latencies.len()/2].iter().sum::<f64>() / (latencies.len()/2) as f64;
        let second_half_avg = latencies[latencies.len()/2..].iter().sum::<f64>() / (latencies.len() - latencies.len()/2) as f64;
        
        let change_percent = ((second_half_avg - first_half_avg) / first_half_avg) * 100.0;
        
        let trend = if change_percent > 5.0 {
            "degrading" // Higher latency is worse
        } else if change_percent < -5.0 {
            "improving" // Lower latency is better
        } else {
            "stable"
        }.to_string();

        TrendAnalysis {
            trend,
            change_percent,
            confidence: 0.8,
        }
    }

    fn analyze_error_trend(&self, results: &[TestResult]) -> TrendAnalysis {
        if results.len() < 2 {
            return TrendAnalysis {
                trend: "insufficient_data".to_string(),
                change_percent: 0.0,
                confidence: 0.0,
            };
        }

        let error_rates: Vec<f64> = results.iter().map(|r| r.metrics.error_rate).collect();
        let first_half_avg = error_rates[..error_rates.len()/2].iter().sum::<f64>() / (error_rates.len()/2) as f64;
        let second_half_avg = error_rates[error_rates.len()/2..].iter().sum::<f64>() / (error_rates.len() - error_rates.len()/2) as f64;
        
        let change_percent = if first_half_avg > 0.0 {
            ((second_half_avg - first_half_avg) / first_half_avg) * 100.0
        } else {
            0.0
        };
        
        let trend = if change_percent > 5.0 {
            "degrading" // Higher error rate is worse
        } else if change_percent < -5.0 {
            "improving" // Lower error rate is better
        } else {
            "stable"
        }.to_string();

        TrendAnalysis {
            trend,
            change_percent,
            confidence: 0.8,
        }
    }

    fn analyze_resource_utilization(&self, results: &[TestResult]) -> ResourceUtilization {
        let cpu_values: Vec<f64> = results.iter().map(|r| r.metrics.cpu_usage).collect();
        let memory_values: Vec<f64> = results.iter().map(|r| r.metrics.memory_usage_mb).collect();

        ResourceUtilization {
            cpu_peak: cpu_values.iter().fold(0.0f64, |a, &b| a.max(b)),
            cpu_average: cpu_values.iter().sum::<f64>() / cpu_values.len() as f64,
            memory_peak: memory_values.iter().fold(0.0f64, |a, &b| a.max(b)),
            memory_average: memory_values.iter().sum::<f64>() / memory_values.len() as f64,
            network_utilization: 0.0, // TODO: Calculate network utilization
        }
    }

    fn identify_bottlenecks(&self, results: &[TestResult]) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();

        // Analyze average latency
        let avg_latency_ms = results.iter()
            .map(|r| r.metrics.latency_p99.as_millis() as f64)
            .sum::<f64>() / results.len() as f64;

        if avg_latency_ms > 1000.0 {
            bottlenecks.push(Bottleneck {
                component: "Network/Protocol".to_string(),
                description: format!("High P99 latency: {:.1}ms", avg_latency_ms),
                severity: "high".to_string(),
                recommendation: "Investigate network conditions and protocol optimization".to_string(),
            });
        }

        // Analyze error rates
        let avg_error_rate = results.iter()
            .map(|r| r.metrics.error_rate)
            .sum::<f64>() / results.len() as f64;

        if avg_error_rate > 0.01 {
            bottlenecks.push(Bottleneck {
                component: "Error Handling".to_string(),
                description: format!("High error rate: {:.2}%", avg_error_rate * 100.0),
                severity: "high".to_string(),
                recommendation: "Review error handling and connection management".to_string(),
            });
        }

        // Analyze throughput vs connections
        for result in results {
            let throughput_per_connection = if result.metrics.active_connections > 0 {
                result.metrics.throughput_rps / result.metrics.active_connections as f64
            } else {
                0.0
            };

            if throughput_per_connection < 1.0 {
                bottlenecks.push(Bottleneck {
                    component: "Connection Efficiency".to_string(),
                    description: format!("Low throughput per connection: {:.2} RPS", throughput_per_connection),
                    severity: "medium".to_string(),
                    recommendation: "Optimize connection pooling and request multiplexing".to_string(),
                });
                break;
            }
        }

        bottlenecks
    }

    fn calculate_performance_score(&self, results: &[TestResult]) -> f64 {
        if results.is_empty() {
            return 0.0;
        }

        let mut score = 100.0;

        // Latency penalty
        let avg_latency_ms = results.iter()
            .map(|r| r.metrics.latency_p99.as_millis() as f64)
            .sum::<f64>() / results.len() as f64;
        
        if avg_latency_ms > 100.0 {
            score -= (avg_latency_ms - 100.0) / 10.0; // -1 point per 10ms over 100ms
        }

        // Error rate penalty
        let avg_error_rate = results.iter()
            .map(|r| r.metrics.error_rate)
            .sum::<f64>() / results.len() as f64;
        
        score -= avg_error_rate * 5000.0; // -50 points per 1% error rate

        // Throughput bonus
        let avg_throughput = results.iter()
            .map(|r| r.metrics.throughput_rps)
            .sum::<f64>() / results.len() as f64;
        
        if avg_throughput > 10000.0 {
            score += (avg_throughput - 10000.0) / 1000.0; // +1 point per 1000 RPS over 10k
        }

        score.max(0.0).min(100.0)
    }

    fn generate_recommendations(&self, analysis: &PerformanceAnalysis) -> Vec<String> {
        let mut recommendations = Vec::new();

        if analysis.performance_score < 50.0 {
            recommendations.push("Overall performance is below acceptable levels. Consider comprehensive optimization.".to_string());
        }

        if analysis.throughput_trend.trend == "degrading" {
            recommendations.push("Throughput is declining. Review connection management and request handling efficiency.".to_string());
        }

        if analysis.latency_trend.trend == "degrading" {
            recommendations.push("Latency is increasing. Investigate network conditions and protocol optimizations.".to_string());
        }

        if analysis.error_trend.trend == "degrading" {
            recommendations.push("Error rate is increasing. Review error handling, timeout settings, and connection stability.".to_string());
        }

        if analysis.resource_utilization.cpu_peak > 80.0 {
            recommendations.push("High CPU utilization detected. Consider scaling horizontally or optimizing CPU-intensive operations.".to_string());
        }

        if analysis.resource_utilization.memory_peak > 8000.0 {
            recommendations.push("High memory usage detected. Review memory management and consider implementing memory limits.".to_string());
        }

        for bottleneck in &analysis.bottlenecks {
            if bottleneck.severity == "high" || bottleneck.severity == "critical" {
                recommendations.push(format!("{}: {}", bottleneck.component, bottleneck.recommendation));
            }
        }

        if recommendations.is_empty() {
            recommendations.push("Performance looks good! Consider running longer tests to validate sustained performance.".to_string());
        }

        recommendations
    }

    async fn generate_json_report(&self, report: &TestReport) -> Result<()> {
        let json = serde_json::to_string_pretty(report)
            .context("Failed to serialize report to JSON")?;
        
        let output_path = PathBuf::from(&self.config.output_dir).join("report.json");
        tokio::fs::write(output_path, json).await
            .context("Failed to write JSON report")?;
        
        Ok(())
    }

    async fn generate_html_report(&self, report: &TestReport) -> Result<()> {
        let html = self.handlebars.render("html_report", report)
            .context("Failed to render HTML report")?;
        
        let output_path = PathBuf::from(&self.config.output_dir).join("report.html");
        tokio::fs::write(output_path, html).await
            .context("Failed to write HTML report")?;
        
        Ok(())
    }

    async fn generate_text_report(&self, report: &TestReport) -> Result<()> {
        let text = self.handlebars.render("text_report", report)
            .context("Failed to render text report")?;
        
        let output_path = PathBuf::from(&self.config.output_dir).join("report.txt");
        tokio::fs::write(output_path, text).await
            .context("Failed to write text report")?;
        
        Ok(())
    }

    async fn generate_graphs(&self, report: &TestReport) -> Result<()> {
        #[cfg(feature = "visualization")]
        {
            use plotters::prelude::*;
            
            let output_dir = Path::new(&self.config.output_dir);
            
            // Generate throughput graph
            self.generate_throughput_graph(&report.results, output_dir).await?;
            
            // Generate latency graph
            self.generate_latency_graph(&report.results, output_dir).await?;
            
            // Generate error rate graph
            self.generate_error_rate_graph(&report.results, output_dir).await?;
        }
        
        Ok(())
    }

    #[cfg(feature = "visualization")]
    async fn generate_throughput_graph(&self, results: &[TestResult], output_dir: &Path) -> Result<()> {
        use plotters::prelude::*;
        
        let output_path = output_dir.join("throughput.png");
        let root = BitMapBackend::new(&output_path, (800, 600)).into_drawing_area();
        root.fill(&WHITE)?;
        
        let max_throughput = results.iter()
            .map(|r| r.metrics.throughput_rps)
            .fold(0.0f64, |a, b| a.max(b));
        
        let mut chart = ChartBuilder::on(&root)
            .caption("Throughput Over Time", ("sans-serif", 40))
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(60)
            .build_cartesian_2d(0f64..results.len() as f64, 0f64..max_throughput * 1.1)?;
        
        chart.configure_mesh().draw()?;
        
        chart.draw_series(LineSeries::new(
            results.iter().enumerate().map(|(i, r)| (i as f64, r.metrics.throughput_rps)),
            &RED,
        ))?;
        
        root.present()?;
        Ok(())
    }

    #[cfg(feature = "visualization")]
    async fn generate_latency_graph(&self, results: &[TestResult], output_dir: &Path) -> Result<()> {
        use plotters::prelude::*;
        
        let output_path = output_dir.join("latency.png");
        let root = BitMapBackend::new(&output_path, (800, 600)).into_drawing_area();
        root.fill(&WHITE)?;
        
        let max_latency = results.iter()
            .map(|r| r.metrics.latency_p99.as_millis() as f64)
            .fold(0.0f64, |a, b| a.max(b));
        
        let mut chart = ChartBuilder::on(&root)
            .caption("Latency P99 Over Time", ("sans-serif", 40))
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(60)
            .build_cartesian_2d(0f64..results.len() as f64, 0f64..max_latency * 1.1)?;
        
        chart.configure_mesh().draw()?;
        
        chart.draw_series(LineSeries::new(
            results.iter().enumerate().map(|(i, r)| (i as f64, r.metrics.latency_p99.as_millis() as f64)),
            &BLUE,
        ))?;
        
        root.present()?;
        Ok(())
    }

    #[cfg(feature = "visualization")]
    async fn generate_error_rate_graph(&self, results: &[TestResult], output_dir: &Path) -> Result<()> {
        use plotters::prelude::*;
        
        let output_path = output_dir.join("error_rate.png");
        let root = BitMapBackend::new(&output_path, (800, 600)).into_drawing_area();
        root.fill(&WHITE)?;
        
        let max_error_rate = results.iter()
            .map(|r| r.metrics.error_rate * 100.0)
            .fold(0.0f64, |a, b| a.max(b));
        
        let mut chart = ChartBuilder::on(&root)
            .caption("Error Rate Over Time", ("sans-serif", 40))
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(60)
            .build_cartesian_2d(0f64..results.len() as f64, 0f64..max_error_rate.max(1.0))?;
        
        chart.configure_mesh().draw()?;
        
        chart.draw_series(LineSeries::new(
            results.iter().enumerate().map(|(i, r)| (i as f64, r.metrics.error_rate * 100.0)),
            &GREEN,
        ))?;
        
        root.present()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framework::TestResult;
    use crate::metrics::TestMetrics;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn test_report_generation() {
        let config = ReportingConfig {
            output_dir: "test_reports".to_string(),
            formats: vec!["json".to_string()],
            include_graphs: false,
            include_raw_data: false,
            dashboard_port: None,
        };
        
        let reporter = TestReporter::new(config);
        
        let test_result = TestResult {
            scenario_name: "test".to_string(),
            start_time: Instant::now(),
            duration: Duration::from_secs(60),
            metrics: TestMetrics::default(),
            success: true,
            error_message: None,
        };
        
        let result = reporter.generate_report(&test_result).await;
        assert!(result.is_ok());
        
        // Clean up
        std::fs::remove_dir_all("test_reports").ok();
    }

    #[test]
    fn test_performance_score_calculation() {
        let config = ReportingConfig {
            output_dir: "test".to_string(),
            formats: vec!["json".to_string()],
            include_graphs: false,
            include_raw_data: false,
            dashboard_port: None,
        };
        
        let reporter = TestReporter::new(config);
        
        // Good performance metrics
        let good_metrics = TestMetrics {
            latency_p99: Duration::from_millis(50),
            error_rate: 0.001,
            throughput_rps: 15000.0,
            ..Default::default()
        };
        
        let good_result = TestResult {
            scenario_name: "good".to_string(),
            start_time: Instant::now(),
            duration: Duration::from_secs(60),
            metrics: good_metrics,
            success: true,
            error_message: None,
        };
        
        let score = reporter.calculate_performance_score(&[good_result]);
        assert!(score > 80.0);
        
        // Poor performance metrics
        let poor_metrics = TestMetrics {
            latency_p99: Duration::from_millis(500),
            error_rate: 0.05,
            throughput_rps: 1000.0,
            ..Default::default()
        };
        
        let poor_result = TestResult {
            scenario_name: "poor".to_string(),
            start_time: Instant::now(),
            duration: Duration::from_secs(60),
            metrics: poor_metrics,
            success: true,
            error_message: None,
        };
        
        let score = reporter.calculate_performance_score(&[poor_result]);
        assert!(score < 50.0);
    }
}