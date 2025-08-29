use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, warn, error};

use quiccrunch_load_testing::{
    config::TestConfig,
    framework::LoadTestFramework,
};

#[derive(Parser)]
#[command(name = "regression-test")]
#[command(about = "QuicCrunch Performance Regression Testing")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    #[arg(short, long, default_value = "regression_reports")]
    output: PathBuf,
    
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Run regression test against a baseline
    Check {
        /// Baseline name to compare against
        #[arg(short, long)]
        baseline: String,
        
        /// Regression threshold (percentage)
        #[arg(short, long, default_value = "10.0")]
        threshold: f64,
        
        /// Scenario to test
        #[arg(short, long, default_value = "simple")]
        scenario: String,
    },
    
    /// Create a new performance baseline
    CreateBaseline {
        /// Baseline name
        #[arg(short, long)]
        name: String,
        
        /// Scenario to use for baseline
        #[arg(short, long, default_value = "simple")]
        scenario: String,
        
        /// Number of runs to average
        #[arg(short, long, default_value = "3")]
        runs: u32,
    },
    
    /// List available baselines
    ListBaselines,
    
    /// Compare two baselines
    Compare {
        /// First baseline name
        baseline1: String,
        
        /// Second baseline name
        baseline2: String,
    },
    
    /// Trend analysis over multiple baselines
    TrendAnalysis {
        /// Pattern to match baseline names (e.g., "v1.*")
        #[arg(short, long)]
        pattern: Option<String>,
        
        /// Number of recent baselines to analyze
        #[arg(short, long, default_value = "10")]
        count: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(if cli.verbose { tracing::Level::DEBUG } else { tracing::Level::INFO })
        .init();
    
    let base_config = TestConfig::default_config();
    
    match cli.command {
        Commands::Check { baseline, threshold, scenario } => {
            run_regression_check(base_config, cli.output, baseline, threshold, scenario).await?;
        }
        Commands::CreateBaseline { name, scenario, runs } => {
            create_performance_baseline(base_config, cli.output, name, scenario, runs).await?;
        }
        Commands::ListBaselines => {
            list_available_baselines(base_config).await?;
        }
        Commands::Compare { baseline1, baseline2 } => {
            compare_baselines(base_config, baseline1, baseline2).await?;
        }
        Commands::TrendAnalysis { pattern, count } => {
            analyze_performance_trends(base_config, pattern, count).await?;
        }
    }
    
    Ok(())
}

async fn run_regression_check(
    base_config: TestConfig,
    output_dir: PathBuf,
    baseline: String,
    threshold: f64,
    scenario: String,
) -> Result<()> {
    info!("🔍 Running Performance Regression Check");
    info!("Baseline: {}, Threshold: {:.1}%, Scenario: {}", baseline, threshold, scenario);
    
    let mut config = base_config;
    config.reporting.output_dir = output_dir.to_string_lossy().to_string();
    
    let framework = LoadTestFramework::new(config);
    
    // Run the current test
    info!("Running current performance test...");
    let current_result = framework.run_scenario(&scenario).await
        .with_context(|| format!("Failed to run scenario '{}'", scenario))?;
    
    if !current_result.success {
        anyhow::bail!("Current test failed: {}", current_result.error_message.unwrap_or_default());
    }
    
    // Compare against baseline
    info!("Comparing against baseline '{}'...", baseline);
    let regression_detected = framework.run_regression_test(&baseline, threshold / 100.0).await
        .context("Regression test failed")?;
    
    // Detailed analysis
    analyze_regression_results(&current_result, &baseline, threshold, !regression_detected);
    
    if regression_detected {
        error!("❌ Performance regression detected!");
        std::process::exit(1);
    } else {
        info!("✅ No performance regression detected");
    }
    
    Ok(())
}

async fn create_performance_baseline(
    base_config: TestConfig,
    output_dir: PathBuf,
    name: String,
    scenario: String,
    runs: u32,
) -> Result<()> {
    info!("📊 Creating Performance Baseline");
    info!("Name: {}, Scenario: {}, Runs: {}", name, scenario, runs);
    
    let mut config = base_config;
    config.reporting.output_dir = output_dir.to_string_lossy().to_string();
    
    let framework = LoadTestFramework::new(config);
    
    let mut all_results = Vec::new();
    
    for run in 1..=runs {
        info!("Running baseline test {}/{}...", run, runs);
        
        let result = framework.run_scenario(&scenario).await
            .with_context(|| format!("Failed to run baseline test {}", run))?;
        
        if !result.success {
            error!("Baseline run {} failed: {}", run, result.error_message.unwrap_or_default());
            continue;
        }
        
        all_results.push(result);
        
        // Brief pause between runs
        if run < runs {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }
    
    if all_results.is_empty() {
        anyhow::bail!("No successful baseline runs completed");
    }
    
    // Calculate average metrics
    let averaged_metrics = calculate_average_metrics(&all_results);
    
    // Save baseline
    framework.save_baseline(&name, &averaged_metrics).await
        .context("Failed to save baseline")?;
    
    info!("✅ Baseline '{}' created successfully", name);
    print_baseline_summary(&averaged_metrics);
    
    Ok(())
}

async fn list_available_baselines(config: TestConfig) -> Result<()> {
    info!("📋 Available Performance Baselines");
    
    let baselines_dir = match &config.regression.storage {
        crate::config::RegressionStorage::File { path } => path,
        crate::config::RegressionStorage::Database { .. } => {
            anyhow::bail!("Database storage not yet supported for listing baselines");
        }
    };
    
    let entries = tokio::fs::read_dir(baselines_dir).await
        .context("Failed to read baselines directory")?;
    
    let mut baseline_files = Vec::new();
    let mut entries = entries;
    
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                let metadata = entry.metadata().await?;
                let modified = metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                baseline_files.push((name.to_string(), modified));
            }
        }
    }
    
    if baseline_files.is_empty() {
        info!("No baselines found in {}", baselines_dir);
        return Ok(());
    }
    
    // Sort by modification time (newest first)
    baseline_files.sort_by_key(|(_, time)| std::cmp::Reverse(*time));
    
    println!("\nBaseline Name                Modified");
    println!("─────────────────────────────────────────────────");
    
    for (name, modified) in baseline_files {
        let duration = modified.elapsed().unwrap_or_default();
        let time_str = if duration.as_secs() < 3600 {
            format!("{}m ago", duration.as_secs() / 60)
        } else if duration.as_secs() < 86400 {
            format!("{}h ago", duration.as_secs() / 3600)
        } else {
            format!("{}d ago", duration.as_secs() / 86400)
        };
        
        println!("{:<25} {}", name, time_str);
    }
    
    Ok(())
}

async fn compare_baselines(
    config: TestConfig,
    baseline1: String,
    baseline2: String,
) -> Result<()> {
    info!("⚖️  Comparing Baselines: {} vs {}", baseline1, baseline2);
    
    let framework = LoadTestFramework::new(config);
    
    // Load both baselines - for now we'll simulate this
    // TODO: Implement actual baseline loading
    warn!("Baseline comparison not yet fully implemented");
    
    Ok(())
async fn analyze_performance_trends(
    config: TestConfig,
    pattern: Option<String>,
    count: usize,
) -> Result<()> {
    info!("📈 Analyzing Performance Trends");
    
    if let Some(ref pattern) = pattern {
        info!("Pattern: {}", pattern);
    }
    info!("Count: {}", count);
    
    // TODO: Implement trend analysis
    warn!("Trend analysis not yet fully implemented");
    
    Ok(())
}

fn analyze_regression_results(
    current_result: &crate::framework::TestResult,
    baseline_name: &str,
    threshold: f64,
    passed: bool,
) {
    info!("📊 Regression Analysis Results:");
    info!("   Baseline: {}", baseline_name);
    info!("   Threshold: {:.1}%", threshold);
    info!("   Result: {}", if passed { "✅ PASS" } else { "❌ FAIL" });
    
    let metrics = &current_result.metrics;
    
    info!("📈 Current Performance Metrics:");
    info!("   Throughput: {:.1} RPS", metrics.throughput_rps);
    info!("   P99 Latency: {:?}", metrics.latency_p99);
    info!("   Error Rate: {:.2}%", metrics.error_rate * 100.0);
    info!("   Total Requests: {}", metrics.total_requests);
    info!("   Successful Requests: {}", metrics.successful_requests);
    
    if passed {
        info!("✅ Performance meets baseline expectations");
    } else {
        warn!("⚠️  Performance regression detected - investigate changes");
        warn!("   Consider optimizations or adjusting performance expectations");
    }
}

fn calculate_average_metrics(results: &[crate::framework::TestResult]) -> crate::metrics::TestMetrics {
    let count = results.len() as f64;
    
    let total_requests: u64 = results.iter().map(|r| r.metrics.total_requests).sum::<u64>() / results.len() as u64;
    let successful_requests: u64 = results.iter().map(|r| r.metrics.successful_requests).sum::<u64>() / results.len() as u64;
    let failed_requests: u64 = results.iter().map(|r| r.metrics.failed_requests).sum::<u64>() / results.len() as u64;
    
    let throughput_rps = results.iter().map(|r| r.metrics.throughput_rps).sum::<f64>() / count;
    let error_rate = results.iter().map(|r| r.metrics.error_rate).sum::<f64>() / count;
    
    let latency_p99_avg = results.iter()
        .map(|r| r.metrics.latency_p99.as_nanos() as u64)
        .sum::<u64>() / results.len() as u64;
    
    let latency_p95_avg = results.iter()
        .map(|r| r.metrics.latency_p95.as_nanos() as u64)
        .sum::<u64>() / results.len() as u64;
    
    let cpu_usage = results.iter().map(|r| r.metrics.cpu_usage).sum::<f64>() / count;
    let memory_usage_mb = results.iter().map(|r| r.metrics.memory_usage_mb).sum::<f64>() / count;
    
    crate::metrics::TestMetrics {
        start_time: results[0].start_time,
        duration: results[0].duration,
        total_requests,
        successful_requests,
        failed_requests,
        throughput_rps,
        error_rate,
        latency_p99: std::time::Duration::from_nanos(latency_p99_avg),
        latency_p95: std::time::Duration::from_nanos(latency_p95_avg),
        cpu_usage,
        memory_usage_mb,
        ..Default::default()
    }
}

fn print_baseline_summary(metrics: &crate::metrics::TestMetrics) {
    info!("📊 Baseline Summary:");
    info!("   Throughput: {:.1} RPS", metrics.throughput_rps);
    info!("   P99 Latency: {:?}", metrics.latency_p99);
    info!("   P95 Latency: {:?}", metrics.latency_p95);
    info!("   Error Rate: {:.2}%", metrics.error_rate * 100.0);
    info!("   Total Requests: {}", metrics.total_requests);
    info!("   CPU Usage: {:.1}%", metrics.cpu_usage);
    info!("   Memory Usage: {:.1} MB", metrics.memory_usage_mb);
}

fn compare_metric(name: &str, value1: f64, value2: f64, higher_is_better: bool) {
    let change = value2 - value1;
    let percent_change = if value1 != 0.0 {
        (change / value1) * 100.0
    } else {
        0.0
    };
    
    let symbol = if change > 0.0 {
        if higher_is_better { "📈" } else { "📉" }
    } else if change < 0.0 {
        if higher_is_better { "📉" } else { "📈" }
    } else {
        "➡️"
    };
    
    let change_str = if change > 0.0 {
        format!("+{:.2}", change)
    } else {
        format!("{:.2}", change)
    };
    
    println!("{:<20} {:>10.2} → {:>10.2} ({:<8} {:>+6.1}%) {}", 
             name, value1, value2, change_str, percent_change, symbol);
}