use crate::analysis::{AnalysisReport, PerformanceAnalyzer};
use crate::config::{BenchmarkConfig, OutputFormat, ChartType};
use crate::metrics::BenchmarkResult;
use crate::storage::ResultStorage;
use anyhow::Result;
use chrono::Utc;
use plotters::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use tracing::{info, warn, error};

pub struct ReportGenerator {
    storage: std::sync::Arc<dyn ResultStorage>,
    output_directory: PathBuf,
}

impl ReportGenerator {
    pub fn new(storage: std::sync::Arc<dyn ResultStorage>, output_directory: PathBuf) -> Self {
        Self {
            storage,
            output_directory,
        }
    }

    pub async fn generate_comprehensive_report(&self, config: &BenchmarkConfig) -> Result<PathBuf> {
        info!("Generating comprehensive benchmark report");

        // Ensure output directory exists
        fs::create_dir_all(&self.output_directory).await?;

        // Get all results
        let results = self.storage.get_results(None).await?;

        // Generate analysis
        let analyzer = PerformanceAnalyzer::new(self.storage.clone());
        let analysis = analyzer.generate_analysis_report(None).await?;

        // Generate reports in different formats
        let mut generated_files = Vec::new();

        for format in &config.output_settings.formats {
            match format {
                OutputFormat::Json => {
                    let file_path = self.generate_json_report(&analysis).await?;
                    generated_files.push(file_path);
                }
                OutputFormat::Csv => {
                    let file_path = self.generate_csv_report(&results).await?;
                    generated_files.push(file_path);
                }
                OutputFormat::Yaml => {
                    let file_path = self.generate_yaml_report(&analysis).await?;
                    generated_files.push(file_path);
                }
            }
        }

        // Generate charts if requested
        if config.output_settings.generate_charts {
            for chart_type in &config.output_settings.chart_types {
                let chart_path = self.generate_chart(chart_type, &results, &analysis).await?;
                generated_files.push(chart_path);
            }
        }

        // Generate summary HTML report
        let html_path = self.generate_html_report(&analysis, &generated_files).await?;
        generated_files.push(html_path);

        info!("Report generation completed. {} files created.", generated_files.len());
        Ok(self.output_directory.clone())
    }

    async fn generate_json_report(&self, analysis: &AnalysisReport) -> Result<PathBuf> {
        let file_path = self.output_directory.join(format!("benchmark_report_{}.json", 
                                                           analysis.generated_at.format("%Y%m%d_%H%M%S")));
        
        let content = serde_json::to_string_pretty(analysis)?;
        fs::write(&file_path, content).await?;
        
        info!("JSON report generated: {:?}", file_path);
        Ok(file_path)
    }

    async fn generate_csv_report(&self, results: &[BenchmarkResult]) -> Result<PathBuf> {
        let file_path = self.output_directory.join(format!("benchmark_results_{}.csv", 
                                                           Utc::now().format("%Y%m%d_%H%M%S")));
        
        let mut wtr = csv::Writer::from_path(&file_path)?;
        
        // Write header
        wtr.write_record(&[
            "test_id",
            "implementation_name",
            "scenario_name",
            "start_time",
            "end_time",
            "success",
            "total_messages",
            "total_bytes",
            "duration_ms",
            "latency_mean_ms",
            "latency_p95_ms",
            "latency_p99_ms",
            "throughput_mps",
            "throughput_mbps",
            "cpu_usage_percent",
            "memory_used_mb",
            "error_rate_percent",
        ])?;

        // Write data
        for result in results {
            wtr.write_record(&[
                &result.test_id,
                &result.implementation_name,
                &result.scenario_name,
                &result.start_time.to_rfc3339(),
                &result.end_time.to_rfc3339(),
                &result.success.to_string(),
                &result.metrics.total_messages.to_string(),
                &result.metrics.total_bytes.to_string(),
                &result.metrics.duration.as_millis().to_string(),
                &result.metrics.latency_mean.as_millis().to_string(),
                &result.metrics.latency_p95.as_millis().to_string(),
                &result.metrics.latency_p99.as_millis().to_string(),
                &result.metrics.throughput_mps.to_string(),
                &result.metrics.calculate_throughput_mbps().to_string(),
                &result.metrics.system_metrics.cpu_usage_percent.to_string(),
                &result.metrics.system_metrics.memory_used_mb.to_string(),
                &result.metrics.get_error_rate().to_string(),
            ])?;
        }

        wtr.flush()?;
        
        info!("CSV report generated: {:?}", file_path);
        Ok(file_path)
    }

    async fn generate_yaml_report(&self, analysis: &AnalysisReport) -> Result<PathBuf> {
        let file_path = self.output_directory.join(format!("benchmark_report_{}.yaml", 
                                                           analysis.generated_at.format("%Y%m%d_%H%M%S")));
        
        let content = serde_yaml::to_string(analysis)?;
        fs::write(&file_path, content).await?;
        
        info!("YAML report generated: {:?}", file_path);
        Ok(file_path)
    }

    async fn generate_chart(&self, chart_type: &ChartType, results: &[BenchmarkResult], analysis: &AnalysisReport) -> Result<PathBuf> {
        match chart_type {
            ChartType::LatencyComparison => self.generate_latency_comparison_chart(results).await,
            ChartType::ThroughputComparison => self.generate_throughput_comparison_chart(results).await,
            ChartType::ResourceUsage => self.generate_resource_usage_chart(results).await,
            ChartType::ErrorRates => self.generate_error_rates_chart(results).await,
            ChartType::PercentileDistribution => self.generate_percentile_distribution_chart(results).await,
        }
    }

    async fn generate_latency_comparison_chart(&self, results: &[BenchmarkResult]) -> Result<PathBuf> {
        let file_path = self.output_directory.join("latency_comparison.png");
        
        // Group results by implementation and scenario
        let mut data: HashMap<String, HashMap<String, Vec<f64>>> = HashMap::new();
        
        for result in results {
            if result.success {
                let impl_data = data.entry(result.implementation_name.clone()).or_insert_with(HashMap::new);
                let scenario_data = impl_data.entry(result.scenario_name.clone()).or_insert_with(Vec::new);
                scenario_data.push(result.metrics.latency_mean.as_millis() as f64);
            }
        }

        // Calculate averages
        let mut chart_data: Vec<(String, f64)> = Vec::new();
        for (impl_name, scenarios) in &data {
            let all_latencies: Vec<f64> = scenarios.values().flatten().copied().collect();
            if !all_latencies.is_empty() {
                let avg_latency = all_latencies.iter().sum::<f64>() / all_latencies.len() as f64;
                chart_data.push((impl_name.clone(), avg_latency));
            }
        }

        if chart_data.is_empty() {
            warn!("No data available for latency comparison chart");
            return Ok(file_path);
        }

        let root = BitMapBackend::new(&file_path, (800, 600)).into_drawing_area();
        root.fill(&WHITE)?;
        
        let max_latency = chart_data.iter().map(|(_, latency)| *latency).fold(0.0, f64::max);
        
        let mut chart = ChartBuilder::on(&root)
            .caption("Latency Comparison by Implementation", ("sans-serif", 40))
            .margin(20)
            .x_label_area_size(60)
            .y_label_area_size(80)
            .build_cartesian_2d(
                chart_data.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>(),
                0.0..max_latency * 1.1,
            )?;

        chart
            .configure_mesh()
            .y_desc("Mean Latency (ms)")
            .x_desc("Implementation")
            .draw()?;

        chart.draw_series(
            chart_data.iter().map(|(name, latency)| {
                Rectangle::new(
                    ((name.clone(), 0.0), (name.clone(), *latency)),
                    BLUE.filled(),
                )
            })
        )?;

        root.present()?;
        
        info!("Latency comparison chart generated: {:?}", file_path);
        Ok(file_path)
    }

    async fn generate_throughput_comparison_chart(&self, results: &[BenchmarkResult]) -> Result<PathBuf> {
        let file_path = self.output_directory.join("throughput_comparison.png");
        
        // Group results by implementation
        let mut impl_throughput: HashMap<String, Vec<f64>> = HashMap::new();
        
        for result in results {
            if result.success {
                impl_throughput.entry(result.implementation_name.clone())
                    .or_insert_with(Vec::new)
                    .push(result.metrics.throughput_mps);
            }
        }

        // Calculate averages
        let mut chart_data: Vec<(String, f64)> = Vec::new();
        for (impl_name, throughputs) in &impl_throughput {
            if !throughputs.is_empty() {
                let avg_throughput = throughputs.iter().sum::<f64>() / throughputs.len() as f64;
                chart_data.push((impl_name.clone(), avg_throughput));
            }
        }

        if chart_data.is_empty() {
            warn!("No data available for throughput comparison chart");
            return Ok(file_path);
        }

        let root = BitMapBackend::new(&file_path, (800, 600)).into_drawing_area();
        root.fill(&WHITE)?;
        
        let max_throughput = chart_data.iter().map(|(_, throughput)| *throughput).fold(0.0, f64::max);
        
        let mut chart = ChartBuilder::on(&root)
            .caption("Throughput Comparison by Implementation", ("sans-serif", 40))
            .margin(20)
            .x_label_area_size(60)
            .y_label_area_size(80)
            .build_cartesian_2d(
                chart_data.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>(),
                0.0..max_throughput * 1.1,
            )?;

        chart
            .configure_mesh()
            .y_desc("Throughput (messages/sec)")
            .x_desc("Implementation")
            .draw()?;

        chart.draw_series(
            chart_data.iter().map(|(name, throughput)| {
                Rectangle::new(
                    ((name.clone(), 0.0), (name.clone(), *throughput)),
                    GREEN.filled(),
                )
            })
        )?;

        root.present()?;
        
        info!("Throughput comparison chart generated: {:?}", file_path);
        Ok(file_path)
    }

    async fn generate_resource_usage_chart(&self, results: &[BenchmarkResult]) -> Result<PathBuf> {
        let file_path = self.output_directory.join("resource_usage.png");
        
        // Group results by implementation
        let mut impl_resources: HashMap<String, Vec<(f64, f64)>> = HashMap::new(); // (cpu, memory)
        
        for result in results {
            if result.success {
                impl_resources.entry(result.implementation_name.clone())
                    .or_insert_with(Vec::new)
                    .push((
                        result.metrics.system_metrics.cpu_usage_percent,
                        result.metrics.system_metrics.memory_used_mb as f64,
                    ));
            }
        }

        // Calculate averages
        let mut chart_data: Vec<(String, f64, f64)> = Vec::new(); // (name, avg_cpu, avg_memory)
        for (impl_name, resources) in &impl_resources {
            if !resources.is_empty() {
                let avg_cpu = resources.iter().map(|(cpu, _)| *cpu).sum::<f64>() / resources.len() as f64;
                let avg_memory = resources.iter().map(|(_, memory)| *memory).sum::<f64>() / resources.len() as f64;
                chart_data.push((impl_name.clone(), avg_cpu, avg_memory));
            }
        }

        if chart_data.is_empty() {
            warn!("No data available for resource usage chart");
            return Ok(file_path);
        }

        let root = BitMapBackend::new(&file_path, (800, 600)).into_drawing_area();
        root.fill(&WHITE)?;
        
        let max_cpu = chart_data.iter().map(|(_, cpu, _)| *cpu).fold(0.0, f64::max);
        let max_memory = chart_data.iter().map(|(_, _, memory)| *memory).fold(0.0, f64::max);
        
        let mut chart = ChartBuilder::on(&root)
            .caption("Resource Usage Comparison", ("sans-serif", 40))
            .margin(20)
            .x_label_area_size(60)
            .y_label_area_size(80)
            .build_cartesian_2d(
                0.0..max_cpu * 1.1,
                0.0..max_memory * 1.1,
            )?;

        chart
            .configure_mesh()
            .x_desc("CPU Usage (%)")
            .y_desc("Memory Usage (MB)")
            .draw()?;

        chart.draw_series(
            chart_data.iter().map(|(name, cpu, memory)| {
                Circle::new((*cpu, *memory), 20, RED.filled())
            })
        )?
        .label_chart_data(
            chart_data.iter().map(|(name, cpu, memory)| {
                (*cpu, *memory, name.clone())
            })
        )?;

        root.present()?;
        
        info!("Resource usage chart generated: {:?}", file_path);
        Ok(file_path)
    }

    async fn generate_error_rates_chart(&self, results: &[BenchmarkResult]) -> Result<PathBuf> {
        let file_path = self.output_directory.join("error_rates.png");
        
        // Group results by implementation
        let mut impl_errors: HashMap<String, Vec<f64>> = HashMap::new();
        
        for result in results {
            impl_errors.entry(result.implementation_name.clone())
                .or_insert_with(Vec::new)
                .push(result.metrics.get_error_rate());
        }

        // Calculate averages
        let mut chart_data: Vec<(String, f64)> = Vec::new();
        for (impl_name, errors) in &impl_errors {
            if !errors.is_empty() {
                let avg_error_rate = errors.iter().sum::<f64>() / errors.len() as f64;
                chart_data.push((impl_name.clone(), avg_error_rate));
            }
        }

        if chart_data.is_empty() {
            warn!("No data available for error rates chart");
            return Ok(file_path);
        }

        let root = BitMapBackend::new(&file_path, (800, 600)).into_drawing_area();
        root.fill(&WHITE)?;
        
        let max_error_rate = chart_data.iter().map(|(_, error)| *error).fold(0.0, f64::max);
        
        let mut chart = ChartBuilder::on(&root)
            .caption("Error Rates by Implementation", ("sans-serif", 40))
            .margin(20)
            .x_label_area_size(60)
            .y_label_area_size(80)
            .build_cartesian_2d(
                chart_data.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>(),
                0.0..max_error_rate * 1.1,
            )?;

        chart
            .configure_mesh()
            .y_desc("Error Rate (%)")
            .x_desc("Implementation")
            .draw()?;

        chart.draw_series(
            chart_data.iter().map(|(name, error_rate)| {
                Rectangle::new(
                    ((name.clone(), 0.0), (name.clone(), *error_rate)),
                    RED.filled(),
                )
            })
        )?;

        root.present()?;
        
        info!("Error rates chart generated: {:?}", file_path);
        Ok(file_path)
    }

    async fn generate_percentile_distribution_chart(&self, results: &[BenchmarkResult]) -> Result<PathBuf> {
        let file_path = self.output_directory.join("percentile_distribution.png");
        
        // Collect percentile data
        let mut percentile_data: HashMap<String, Vec<(f64, f64)>> = HashMap::new(); // impl -> [(percentile, latency_ms)]
        
        for result in results {
            if result.success {
                let impl_data = percentile_data.entry(result.implementation_name.clone()).or_insert_with(Vec::new);
                impl_data.extend_from_slice(&[
                    (50.0, result.metrics.latency_p50.as_millis() as f64),
                    (95.0, result.metrics.latency_p95.as_millis() as f64),
                    (99.0, result.metrics.latency_p99.as_millis() as f64),
                    (99.9, result.metrics.latency_p99_9.as_millis() as f64),
                ]);
            }
        }

        // Average percentiles per implementation
        let mut chart_data: Vec<(String, Vec<(f64, f64)>)> = Vec::new();
        for (impl_name, percentiles) in &percentile_data {
            let mut averaged: HashMap<f64, Vec<f64>> = HashMap::new();
            for (percentile, latency) in percentiles {
                averaged.entry(*percentile).or_insert_with(Vec::new).push(*latency);
            }
            
            let averaged_data: Vec<(f64, f64)> = averaged.into_iter()
                .map(|(percentile, latencies)| {
                    let avg_latency = latencies.iter().sum::<f64>() / latencies.len() as f64;
                    (percentile, avg_latency)
                })
                .collect();
            
            chart_data.push((impl_name.clone(), averaged_data));
        }

        if chart_data.is_empty() {
            warn!("No data available for percentile distribution chart");
            return Ok(file_path);
        }

        let root = BitMapBackend::new(&file_path, (800, 600)).into_drawing_area();
        root.fill(&WHITE)?;
        
        let max_latency = chart_data.iter()
            .flat_map(|(_, data)| data.iter().map(|(_, latency)| *latency))
            .fold(0.0, f64::max);
        
        let mut chart = ChartBuilder::on(&root)
            .caption("Latency Percentile Distribution", ("sans-serif", 40))
            .margin(20)
            .x_label_area_size(60)
            .y_label_area_size(80)
            .build_cartesian_2d(
                50.0..100.0,
                0.0..max_latency * 1.1,
            )?;

        chart
            .configure_mesh()
            .x_desc("Percentile")
            .y_desc("Latency (ms)")
            .draw()?;

        let colors = [BLUE, RED, GREEN, CYAN, MAGENTA];
        
        for (i, (impl_name, data)) in chart_data.iter().enumerate() {
            let color = colors[i % colors.len()];
            chart.draw_series(LineSeries::new(
                data.iter().map(|(percentile, latency)| (*percentile, *latency)),
                color.stroke_width(3),
            ))?
            .label(impl_name)
            .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 10, y)], color));
        }

        chart.configure_series_labels().draw()?;
        
        root.present()?;
        
        info!("Percentile distribution chart generated: {:?}", file_path);
        Ok(file_path)
    }

    async fn generate_html_report(&self, analysis: &AnalysisReport, generated_files: &[PathBuf]) -> Result<PathBuf> {
        let file_path = self.output_directory.join("benchmark_report.html");
        
        let html_content = format!(r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Benchmark Report - {}</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 20px; }}
        .header {{ background-color: #f0f0f0; padding: 20px; border-radius: 5px; }}
        .summary {{ margin: 20px 0; }}
        .section {{ margin: 30px 0; }}
        .comparison {{ border: 1px solid #ddd; margin: 10px 0; padding: 15px; border-radius: 5px; }}
        .chart {{ text-align: center; margin: 20px 0; }}
        .chart img {{ max-width: 100%; height: auto; border: 1px solid #ddd; }}
        table {{ border-collapse: collapse; width: 100%; margin: 10px 0; }}
        th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
        th {{ background-color: #f2f2f2; }}
        .success {{ color: green; }}
        .failure {{ color: red; }}
        .recommendation {{ background-color: #fff3cd; border: 1px solid #ffeaa7; padding: 15px; margin: 10px 0; border-radius: 5px; }}
        .high {{ border-left: 5px solid #dc3545; }}
        .medium {{ border-left: 5px solid #ffc107; }}
        .low {{ border-left: 5px solid #28a745; }}
    </style>
</head>
<body>
    <div class="header">
        <h1>Benchmark Performance Report</h1>
        <p><strong>Generated:</strong> {}</p>
        <p><strong>Report ID:</strong> {}</p>
    </div>

    <div class="summary">
        <h2>Executive Summary</h2>
        <table>
            <tr><th>Metric</th><th>Value</th></tr>
            <tr><td>Total Tests</td><td>{}</td></tr>
            <tr><td>Successful Tests</td><td class="success">{}</td></tr>
            <tr><td>Failed Tests</td><td class="failure">{}</td></tr>
            <tr><td>Success Rate</td><td>{:.1}%</td></tr>
            <tr><td>Implementations Tested</td><td>{}</td></tr>
            <tr><td>Scenarios Covered</td><td>{}</td></tr>
        </table>
    </div>

    <div class="section">
        <h2>Performance Comparisons</h2>
        {}
    </div>

    <div class="section">
        <h2>Performance Charts</h2>
        {}
    </div>

    <div class="section">
        <h2>Recommendations</h2>
        {}
    </div>

    <div class="section">
        <h2>Generated Files</h2>
        <ul>
            {}
        </ul>
    </div>
</body>
</html>
        "#,
            analysis.generated_at.format("%Y-%m-%d %H:%M:%S"),
            analysis.generated_at.format("%Y-%m-%d %H:%M:%S"),
            analysis.report_id,
            analysis.summary.total_tests,
            analysis.summary.successful_tests,
            analysis.summary.failed_tests,
            (analysis.summary.successful_tests as f64 / analysis.summary.total_tests as f64) * 100.0,
            analysis.summary.implementations.join(", "),
            analysis.summary.scenarios.join(", "),
            self.format_comparisons_html(&analysis.implementation_comparisons),
            self.format_charts_html(generated_files),
            self.format_recommendations_html(&analysis.recommendations),
            generated_files.iter().map(|p| format!("<li>{}</li>", p.file_name().unwrap_or("unknown"))).collect::<Vec<_>>().join("\n            ")
        );

        fs::write(&file_path, html_content).await?;
        
        info!("HTML report generated: {:?}", file_path);
        Ok(file_path)
    }

    fn format_comparisons_html(&self, comparisons: &[crate::analysis::ImplementationComparison]) -> String {
        if comparisons.is_empty() {
            return "<p>No comparison data available.</p>".to_string();
        }

        comparisons.iter().map(|comp| {
            format!(r#"
            <div class="comparison">
                <h3>{} vs {}</h3>
                <table>
                    <tr><th>Metric</th><th>Improvement</th></tr>
                    <tr><td>Latency</td><td>{:.1}%</td></tr>
                    <tr><td>Throughput</td><td>{:.1}%</td></tr>
                    <tr><td>CPU Efficiency</td><td>{:.1}%</td></tr>
                    <tr><td>Memory Efficiency</td><td>{:.1}%</td></tr>
                    <tr><td>Overall Score</td><td>{:.1}</td></tr>
                </table>
            </div>
            "#,
                comp.implementation_a,
                comp.implementation_b,
                comp.performance_comparison.latency_improvement_percent,
                comp.performance_comparison.throughput_improvement_percent,
                comp.performance_comparison.cpu_efficiency_improvement_percent,
                comp.performance_comparison.memory_efficiency_improvement_percent,
                comp.performance_comparison.overall_score
            )
        }).collect::<Vec<_>>().join("\n")
    }

    fn format_charts_html(&self, files: &[PathBuf]) -> String {
        let chart_files: Vec<&PathBuf> = files.iter()
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("png"))
            .collect();

        if chart_files.is_empty() {
            return "<p>No charts generated.</p>".to_string();
        }

        chart_files.iter().map(|path| {
            let file_name = path.file_name().unwrap().to_str().unwrap();
            format!(r#"
            <div class="chart">
                <h3>{}</h3>
                <img src="{}" alt="{}">
            </div>
            "#, 
                file_name.replace("_", " ").replace(".png", ""),
                file_name,
                file_name.replace("_", " ").replace(".png", "")
            )
        }).collect::<Vec<_>>().join("\n")
    }

    fn format_recommendations_html(&self, recommendations: &[crate::analysis::Recommendation]) -> String {
        if recommendations.is_empty() {
            return "<p>No recommendations available.</p>".to_string();
        }

        recommendations.iter().map(|rec| {
            let priority_class = match rec.priority {
                crate::analysis::RecommendationPriority::High => "high",
                crate::analysis::RecommendationPriority::Medium => "medium",
                crate::analysis::RecommendationPriority::Low => "low",
            };

            format!(r#"
            <div class="recommendation {}">
                <h4>{}</h4>
                <p><strong>Priority:</strong> {:?}</p>
                <p><strong>Category:</strong> {:?}</p>
                <p>{}</p>
                <p><strong>Expected Impact:</strong> {}</p>
                <p><strong>Affects:</strong> {}</p>
            </div>
            "#,
                priority_class,
                rec.title,
                rec.priority,
                rec.category,
                rec.description,
                rec.expected_impact,
                rec.affected_implementations.join(", ")
            )
        }).collect::<Vec<_>>().join("\n")
    }
}