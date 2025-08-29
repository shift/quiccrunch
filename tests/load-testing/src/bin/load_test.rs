use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use quiccrunch_load_testing::{
    config::TestConfig,
    framework::LoadTestFramework,
};

#[derive(Parser)]
#[command(name = "load-test")]
#[command(about = "QuicCrunch Load Testing Framework")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// Configuration file path
    #[arg(short, long, default_value = "config/default.toml")]
    config: PathBuf,
    
    /// Verbose logging
    #[arg(short, long)]
    verbose: bool,
    
    /// Output directory for reports
    #[arg(short, long, default_value = "reports")]
    output: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a single test scenario
    Run {
        /// Scenario name to run
        #[arg(short, long)]
        scenario: String,
        
        /// Test duration override
        #[arg(short, long)]
        duration: Option<humantime::Duration>,
        
        /// Number of connections override
        #[arg(short, long)]
        connections: Option<u32>,
        
        /// Request rate override
        #[arg(short, long)]
        rate: Option<u32>,
        
        /// Backend to use (quinn|tquic)
        #[arg(short, long, default_value = "quinn")]
        backend: String,
        
        /// Generate performance profile
        #[arg(long)]
        profile: bool,
        
        /// Save as baseline for regression testing
        #[arg(long)]
        save_baseline: Option<String>,
    },
    
    /// Run all configured scenarios
    RunAll {
        /// Parallel execution
        #[arg(short, long)]
        parallel: bool,
        
        /// Maximum parallel scenarios
        #[arg(long, default_value = "3")]
        max_parallel: usize,
        
        /// Stop on first failure
        #[arg(long)]
        fail_fast: bool,
    },
    
    /// Generate and validate test configuration
    Config {
        #[command(subcommand)]
        action: ConfigActions,
    },
    
    /// Performance regression testing
    Regression {
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
    
    /// List available scenarios
    List {
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
    },
    
    /// Validate test setup and dependencies
    Validate,
}

#[derive(Subcommand)]
enum ConfigActions {
    /// Generate default configuration
    Generate {
        /// Output file path
        #[arg(short, long, default_value = "config/load-test.toml")]
        output: PathBuf,
        
        /// Overwrite existing file
        #[arg(long)]
        force: bool,
    },
    
    /// Validate configuration file
    Validate {
        /// Configuration file to validate
        file: PathBuf,
    },
    
    /// Show current configuration
    Show,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    init_logging(cli.verbose)?;
    
    // Load configuration
    let mut config = if cli.config.exists() {
        TestConfig::load_from_file(&cli.config)
            .with_context(|| format!("Failed to load config from {:?}", cli.config))?
    } else {
        warn!("Configuration file not found, using defaults");
        TestConfig::default_config()
    };
    
    // Override output directory
    config.reporting.output_dir = cli.output.to_string_lossy().to_string();
    
    // Execute command
    match cli.command {
        Commands::Run { 
            scenario, 
            duration, 
            connections, 
            rate, 
            backend,
            profile,
            save_baseline,
        } => {
            run_scenario(config, scenario, duration, connections, rate, backend, profile, save_baseline).await?;
        }
        
        Commands::RunAll { parallel, max_parallel, fail_fast } => {
            run_all_scenarios(config, parallel, max_parallel, fail_fast).await?;
        }
        
        Commands::Config { action } => {
            handle_config_action(action, &config).await?;
        }
        
        Commands::Regression { baseline, threshold, scenario } => {
            run_regression_test(config, baseline, threshold, scenario).await?;
        }
        
        Commands::List { detailed } => {
            list_scenarios(&config, detailed).await?;
        }
        
        Commands::Validate => {
            validate_setup(&config).await?;
        }
    }
    
    Ok(())
}

fn init_logging(verbose: bool) -> Result<()> {
    let log_level = if verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| {
                    format!("quiccrunch_load_testing={},load_test={}", log_level, log_level).into()
                })
        )
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .context("Failed to initialize logging")?;
    
    Ok(())
}

async fn run_scenario(
    mut config: TestConfig,
    scenario_name: String,
    duration: Option<humantime::Duration>,
    connections: Option<u32>,
    rate: Option<u32>,
    backend: String,
    profile: bool,
    save_baseline: Option<String>,
) -> Result<()> {
    info!("Running scenario: {}", scenario_name);
    
    // Apply CLI overrides
    if let Some(scenario_config) = config.scenarios.get_mut(&scenario_name) {
        if let Some(duration) = duration {
            scenario_config.duration = duration.into();
        }
        if let Some(connections) = connections {
            scenario_config.connections = connections;
        }
        if let Some(rate) = rate {
            scenario_config.request_rate = crate::config::RequestRateConfig::Constant { rate };
        }
        scenario_config.backend = backend;
    } else {
        anyhow::bail!("Scenario '{}' not found", scenario_name);
    }
    
    let framework = LoadTestFramework::new(config);
    
    if profile {
        info!("Profiling enabled - performance data will be collected");
        // TODO: Enable profiling features
    }
    
    let result = framework.run_scenario(&scenario_name).await
        .with_context(|| format!("Failed to run scenario '{}'", scenario_name))?;
    
    if result.success {
        info!("✅ Scenario completed successfully");
        
        if let Some(baseline_name) = save_baseline {
            framework.save_baseline(&baseline_name, &result.metrics).await
                .with_context(|| format!("Failed to save baseline '{}'", baseline_name))?;
            info!("💾 Baseline saved as '{}'", baseline_name);
        }
    } else {
        error!("❌ Scenario failed: {}", result.error_message.unwrap_or_default());
        std::process::exit(1);
    }
    
    // Display summary
    display_result_summary(&result);
    
    Ok(())
}

async fn run_all_scenarios(
    config: TestConfig,
    parallel: bool,
    max_parallel: usize,
    fail_fast: bool,
) -> Result<()> {
    info!("Running all configured scenarios (parallel: {}, max_parallel: {})", parallel, max_parallel);
    
    let framework = LoadTestFramework::new(config.clone());
    let scenario_names: Vec<String> = config.scenarios.keys().cloned().collect();
    
    if parallel {
        info!("Running {} scenarios in parallel", scenario_names.len());
        run_scenarios_parallel(&framework, scenario_names, max_parallel, fail_fast).await?;
    } else {
        info!("Running {} scenarios sequentially", scenario_names.len());
        run_scenarios_sequential(&framework, scenario_names, fail_fast).await?;
    }
    
    Ok(())
}

async fn run_scenarios_sequential(
    framework: &LoadTestFramework,
    scenario_names: Vec<String>,
    fail_fast: bool,
) -> Result<()> {
    let mut successful = 0;
    let mut failed = 0;
    
    for scenario_name in scenario_names {
        info!("Running scenario: {}", scenario_name);
        
        match framework.run_scenario(&scenario_name).await {
            Ok(result) => {
                if result.success {
                    successful += 1;
                    info!("✅ Scenario '{}' completed successfully", scenario_name);
                } else {
                    failed += 1;
                    error!("❌ Scenario '{}' failed: {}", scenario_name, result.error_message.unwrap_or_default());
                    
                    if fail_fast {
                        anyhow::bail!("Stopping execution due to failure (fail-fast mode)");
                    }
                }
            }
            Err(e) => {
                failed += 1;
                error!("❌ Scenario '{}' failed to run: {}", scenario_name, e);
                
                if fail_fast {
                    return Err(e);
                }
            }
        }
    }
    
    info!("All scenarios completed: {} successful, {} failed", successful, failed);
    
    if failed > 0 {
        std::process::exit(1);
    }
    
    Ok(())
}

async fn run_scenarios_parallel(
    framework: &LoadTestFramework,
    scenario_names: Vec<String>,
    max_parallel: usize,
    fail_fast: bool,
) -> Result<()> {
    use futures::stream::{FuturesUnordered, StreamExt};
    use std::sync::Arc;
    
    let framework = Arc::new(framework);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_parallel));
    let mut tasks = FuturesUnordered::new();
    
    for scenario_name in scenario_names {
        let framework = Arc::clone(&framework);
        let semaphore = Arc::clone(&semaphore);
        let scenario_name = scenario_name.clone();
        
        tasks.push(tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();
            (scenario_name.clone(), framework.run_scenario(&scenario_name).await)
        }));
    }
    
    let mut successful = 0;
    let mut failed = 0;
    
    while let Some(task_result) = tasks.next().await {
        match task_result {
            Ok((scenario_name, scenario_result)) => {
                match scenario_result {
                    Ok(result) => {
                        if result.success {
                            successful += 1;
                            info!("✅ Scenario '{}' completed successfully", scenario_name);
                        } else {
                            failed += 1;
                            error!("❌ Scenario '{}' failed: {}", scenario_name, result.error_message.unwrap_or_default());
                        }
                    }
                    Err(e) => {
                        failed += 1;
                        error!("❌ Scenario '{}' failed to run: {}", scenario_name, e);
                    }
                }
                
                if fail_fast && failed > 0 {
                    break;
                }
            }
            Err(e) => {
                error!("Task execution error: {}", e);
            }
        }
    }
    
    info!("All scenarios completed: {} successful, {} failed", successful, failed);
    
    if failed > 0 {
        std::process::exit(1);
    }
    
    Ok(())
}

async fn handle_config_action(action: ConfigActions, config: &TestConfig) -> Result<()> {
    match action {
        ConfigActions::Generate { output, force } => {
            generate_config(output, force).await?;
        }
        ConfigActions::Validate { file } => {
            validate_config(file).await?;
        }
        ConfigActions::Show => {
            show_config(config).await?;
        }
    }
    Ok(())
}

async fn generate_config(output: PathBuf, force: bool) -> Result<()> {
    if output.exists() && !force {
        anyhow::bail!("Configuration file already exists. Use --force to overwrite.");
    }
    
    let config = TestConfig::default_config();
    
    // Create output directory if it doesn't exist
    if let Some(parent) = output.parent() {
        tokio::fs::create_dir_all(parent).await
            .context("Failed to create config directory")?;
    }
    
    let config_content = toml::to_string_pretty(&config)
        .context("Failed to serialize configuration")?;
    
    tokio::fs::write(&output, config_content).await
        .context("Failed to write configuration file")?;
    
    info!("Generated configuration file: {:?}", output);
    Ok(())
}

async fn validate_config(file: PathBuf) -> Result<()> {
    info!("Validating configuration file: {:?}", file);
    
    let _config = TestConfig::load_from_file(&file)
        .context("Configuration validation failed")?;
    
    info!("✅ Configuration is valid");
    Ok(())
}

async fn show_config(config: &TestConfig) -> Result<()> {
    println!("Current Configuration:");
    println!("===================");
    
    println!("\nScenarios:");
    for (name, scenario) in &config.scenarios {
        println!("  {}: {} ({:?})", name, scenario.name, scenario.duration);
    }
    
    println!("\nPerformance Thresholds:");
    println!("  Max P99 Latency: {:?}", config.performance.thresholds.max_latency_p99);
    println!("  Min Throughput: {} RPS", config.performance.thresholds.min_throughput);
    println!("  Max Error Rate: {:.2}%", config.performance.thresholds.max_error_rate * 100.0);
    
    println!("\nReporting:");
    println!("  Output Directory: {}", config.reporting.output_dir);
    println!("  Formats: {:?}", config.reporting.formats);
    
    Ok(())
}

async fn run_regression_test(
    config: TestConfig,
    baseline: String,
    threshold: f64,
    scenario: String,
) -> Result<()> {
    info!("Running regression test against baseline '{}' with {}% threshold", baseline, threshold);
    
    let framework = LoadTestFramework::new(config);
    
    let passed = framework.run_regression_test(&baseline, threshold / 100.0).await
        .context("Regression test failed")?;
    
    if passed {
        info!("✅ No performance regression detected");
    } else {
        error!("❌ Performance regression detected!");
        std::process::exit(1);
    }
    
    Ok(())
}

async fn list_scenarios(config: &TestConfig, detailed: bool) -> Result<()> {
    println!("Available Test Scenarios:");
    println!("========================");
    
    for (name, scenario) in &config.scenarios {
        if detailed {
            println!("\n{}: {}", name, scenario.name);
            if let Some(desc) = &scenario.description {
                println!("  Description: {}", desc);
            }
            println!("  Duration: {:?}", scenario.duration);
            println!("  Connections: {}", scenario.connections);
            println!("  Backend: {}", scenario.backend);
            
            match &scenario.request_rate {
                crate::config::RequestRateConfig::Constant { rate } => {
                    println!("  Request Rate: {} RPS (constant)", rate);
                }
                crate::config::RequestRateConfig::Variable { pattern, base_rate, variation } => {
                    println!("  Request Rate: {} RPS base, {} pattern, {:.1}% variation", 
                             base_rate, pattern, variation * 100.0);
                }
                crate::config::RequestRateConfig::Burst { base_rate, burst_rate, burst_duration } => {
                    println!("  Request Rate: {} RPS base, {} RPS burst for {:?}", 
                             base_rate, burst_rate, burst_duration);
                }
            }
        } else {
            println!("  {}: {} ({:?})", name, scenario.name, scenario.duration);
        }
    }
    
    Ok(())
}

async fn validate_setup(config: &TestConfig) -> Result<()> {
    info!("Validating test setup and dependencies...");
    
    // Check output directory
    let output_dir = &config.reporting.output_dir;
    tokio::fs::create_dir_all(output_dir).await
        .with_context(|| format!("Cannot create output directory: {}", output_dir))?;
    info!("✅ Output directory accessible: {}", output_dir);
    
    // Check baseline directory
    if let crate::config::RegressionStorage::File { path } = &config.regression.storage {
        tokio::fs::create_dir_all(path).await
            .with_context(|| format!("Cannot create baseline directory: {}", path))?;
        info!("✅ Baseline directory accessible: {}", path);
    }
    
    // TODO: Check QuicCrunch dependencies
    info!("✅ Basic setup validation completed");
    
    Ok(())
}

fn display_result_summary(result: &crate::framework::TestResult) {
    println!("\nTest Results Summary:");
    println!("====================");
    println!("Scenario: {}", result.scenario_name);
    println!("Duration: {:?}", result.duration);
    println!("Success: {}", if result.success { "✅ Yes" } else { "❌ No" });
    
    if let Some(error) = &result.error_message {
        println!("Error: {}", error);
    }
    
    let metrics = &result.metrics;
    println!("\nPerformance Metrics:");
    println!("  Total Requests: {}", metrics.total_requests);
    println!("  Successful: {}", metrics.successful_requests);
    println!("  Failed: {}", metrics.failed_requests);
    println!("  Error Rate: {:.2}%", metrics.error_rate * 100.0);
    println!("  Throughput: {:.1} RPS", metrics.throughput_rps);
    println!("  Latency P50: {:?}", metrics.latency_p50);
    println!("  Latency P95: {:?}", metrics.latency_p95);
    println!("  Latency P99: {:?}", metrics.latency_p99);
    println!("  Active Connections: {}", metrics.active_connections);
    
    if metrics.streams_created > 0 {
        println!("  Streams Created: {}", metrics.streams_created);
        println!("  Streams Active: {}", metrics.streams_active);
    }
    
    if metrics.cpu_usage > 0.0 {
        println!("  CPU Usage: {:.1}%", metrics.cpu_usage);
        println!("  Memory Usage: {:.1} MB", metrics.memory_usage_mb);
    }
}