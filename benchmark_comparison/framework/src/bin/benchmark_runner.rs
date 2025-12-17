use anyhow::Result;
use benchmark_framework::{
    config::BenchmarkConfig,
    orchestrator::{DefaultTestOrchestrator, TestOrchestrator},
    storage::{FileSystemStorage, ResultStorage},
    reporting::ReportGenerator,
    utils::SystemUtils,
};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, error, Level};
use tracing_subscriber;

#[derive(Parser)]
#[command(name = "benchmark_runner")]
#[command(about = "Comprehensive benchmarking framework for gRPC and QUICCRUNCH")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    
    #[arg(short, long, default_value = "info")]
    pub log_level: String,
    
    #[arg(short, long)]
    pub config: Option<PathBuf>,
    
    #[arg(short, long, default_value = "./results")]
    pub output_dir: PathBuf,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run a complete benchmark suite
    Run {
        /// Configuration file path
        #[arg(short, long)]
        config: Option<PathBuf>,
        
        /// Override implementations (comma-separated)
        #[arg(long)]
        implementations: Option<String>,
        
        /// Override scenarios (comma-separated)
        #[arg(long)]
        scenarios: Option<String>,
        
        /// Number of parallel tests
        #[arg(long, default_value = "2")]
        parallel: usize,
    },
    
    /// Generate reports from existing results
    Report {
        /// Input directory containing results
        #[arg(short, long, default_value = "./results")]
        input: PathBuf,
        
        /// Output directory for reports
        #[arg(short, long, default_value = "./reports")]
        output: PathBuf,
        
        /// Report format (json, csv, yaml, html)
        #[arg(long, default_value = "html")]
        format: String,
    },
    
    /// Validate test environment
    Validate {
        /// Check system requirements
        #[arg(long)]
        system: bool,
        
        /// Check implementation dependencies
        #[arg(long)]
        dependencies: bool,
    },
    
    /// Clean up test artifacts
    Cleanup {
        /// Remove all results
        #[arg(long)]
        results: bool,
        
        /// Remove temporary files
        #[arg(long)]
        temp: bool,
        
        /// Kill running processes
        #[arg(long)]
        processes: bool,
    },
    
    /// Initialize benchmark configuration
    Init {
        /// Configuration file path
        #[arg(short, long, default_value = "benchmark_config.yaml")]
        config: PathBuf,
        
        /// Use default configuration
        #[arg(long)]
        default: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    let level = match cli.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };
    
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .with_thread_ids(true)
        .with_thread_names(true)
        .init();
    
    match cli.command {
        Commands::Run { config, implementations, scenarios, parallel } => {
            run_benchmarks(config, implementations, scenarios, parallel, cli.output_dir).await
        }
        Commands::Report { input, output, format } => {
            generate_reports(input, output, &format).await
        }
        Commands::Validate { system, dependencies } => {
            validate_environment(system, dependencies).await
        }
        Commands::Cleanup { results, temp, processes } => {
            cleanup_artifacts(results, temp, processes).await
        }
        Commands::Init { config, default } => {
            init_config(config, default).await
        }
    }
}

async fn run_benchmarks(
    config_path: Option<PathBuf>,
    implementations: Option<String>,
    scenarios: Option<String>,
    parallel: usize,
    output_dir: PathBuf,
) -> Result<()> {
    info!("Starting benchmark execution");
    
    // Load or create configuration
    let mut config = if let Some(path) = config_path {
        BenchmarkConfig::load_from_file(&path)?
    } else {
        BenchmarkConfig::default()
    };
    
    // Apply overrides
    if let Some(impl_list) = implementations {
        let impl_names: Vec<String> = impl_list.split(',').map(|s| s.trim().to_string()).collect();
        config.implementations.retain(|impl_config| impl_names.contains(&impl_config.name));
    }
    
    if let Some(scenario_list) = scenarios {
        let scenario_names: Vec<String> = scenario_list.split(',').map(|s| s.trim().to_string()).collect();
        config.test_scenarios.retain(|scenario| scenario_names.contains(&scenario.name));
    }
    
    config.global_settings.parallel_tests = parallel;
    config.output_settings.results_directory = output_dir;
    
    // Validate configuration
    config.validate()?;
    
    info!("Configuration loaded: {} implementations, {} scenarios", 
          config.implementations.len(), config.test_scenarios.len());
    
    // Initialize storage
    let storage = std::sync::Arc::new(FileSystemStorage::new(
        config.output_settings.results_directory.clone(),
        config.output_settings.compression,
    ));
    
    // Initialize orchestrator
    let orchestrator = DefaultTestOrchestrator::new(storage.clone(), parallel);
    
    // Run benchmarks
    let results = orchestrator.run_benchmark(config.clone()).await?;
    
    info!("Benchmark completed: {} results generated", results.len());
    
    // Generate reports
    let report_generator = ReportGenerator::new(storage, config.output_settings.results_directory.clone());
    let report_path = report_generator.generate_comprehensive_report(&config).await?;
    
    info!("Reports generated in: {:?}", report_path);
    
    // Print summary
    let successful = results.iter().filter(|r| r.success).count();
    let failed = results.len() - successful;
    
    println!("\n=== Benchmark Summary ===");
    println!("Total tests: {}", results.len());
    println!("Successful: {}", successful);
    println!("Failed: {}", failed);
    println!("Success rate: {:.1}%", (successful as f64 / results.len() as f64) * 100.0);
    println!("Reports available at: {:?}", report_path);
    
    Ok(())
}

async fn generate_reports(input: PathBuf, output: PathBuf, format: &str) -> Result<()> {
    info!("Generating reports from results in {:?}", input);
    
    // Initialize storage
    let storage = std::sync::Arc::new(FileSystemStorage::new(input, false));
    
    // Create output directory
    tokio::fs::create_dir_all(&output).await?;
    
    // Generate reports
    let report_generator = ReportGenerator::new(storage, output);
    
    // This would need to be adapted to work with the existing results
    // For now, we'll create a simple report
    let results = storage.get_results(None).await?;
    
    if results.is_empty() {
        error!("No results found in {:?}", input);
        return Ok(());
    }
    
    info!("Found {} results, generating reports", results.len());
    
    // Generate different format reports based on the format parameter
    match format {
        "json" => {
            let report_path = output.join("benchmark_report.json");
            let content = serde_json::to_string_pretty(&results)?;
            tokio::fs::write(&report_path, content).await?;
            info!("JSON report generated: {:?}", report_path);
        }
        "csv" => {
            let report_path = output.join("benchmark_report.csv");
            // CSV generation would be implemented here
            info!("CSV report would be generated: {:?}", report_path);
        }
        "yaml" => {
            let report_path = output.join("benchmark_report.yaml");
            let content = serde_yaml::to_string(&results)?;
            tokio::fs::write(&report_path, content).await?;
            info!("YAML report generated: {:?}", report_path);
        }
        "html" => {
            let report_path = output.join("benchmark_report.html");
            // HTML report generation would be implemented here
            info!("HTML report would be generated: {:?}", report_path);
        }
        _ => {
            error!("Unsupported format: {}", format);
        }
    }
    
    Ok(())
}

async fn validate_environment(system: bool, dependencies: bool) -> Result<()> {
    info!("Validating test environment");
    
    if system {
        info!("Checking system requirements");
        let system_info = SystemUtils::check_system_requirements()?;
        println!("\n=== System Information ===");
        println!("OS: {}", system_info.os);
        println!("Architecture: {}", system_info.arch);
        println!("CPU Cores: {}", system_info.cpu_cores);
        println!("Total Memory: {} MB", system_info.total_memory_mb);
        println!("Available Memory: {} MB", system_info.available_memory_mb);
        println!("CPU Usage: {:.1}%", system_info.cpu_usage_percent);
        println!("Load Average: {:.2}", system_info.load_average);
        println!("Disk Space: {:.1} GB", system_info.disk_space_gb);
        println!("Network Interfaces: {}", system_info.network_interfaces.join(", "));
    }
    
    if dependencies {
        info!("Checking dependencies");
        let validation = SystemUtils::validate_test_environment()?;
        
        println!("\n=== Environment Validation ===");
        println!("Valid: {}", validation.is_valid);
        
        if !validation.missing_commands.is_empty() {
            println!("Missing Commands: {}", validation.missing_commands.join(", "));
        }
        
        if !validation.warnings.is_empty() {
            println!("Warnings:");
            for warning in &validation.warnings {
                println!("  - {}", warning);
            }
        }
        
        if validation.is_valid {
            info!("Environment validation passed");
        } else {
            error!("Environment validation failed");
        }
    }
    
    if !system && !dependencies {
        // Run both if no specific option provided
        validate_environment(true, true).await?;
    }
    
    Ok(())
}

async fn cleanup_artifacts(results: bool, temp: bool, processes: bool) -> Result<()> {
    info!("Cleaning up test artifacts");
    
    if processes {
        info!("Killing benchmark processes");
        SystemUtils::kill_processes_by_name("benchmark")?;
        SystemUtils::kill_processes_by_name("server")?;
        SystemUtils::kill_processes_by_name("client")?;
    }
    
    if temp {
        info!("Cleaning up temporary files");
        SystemUtils::cleanup_temp_files("benchmark").await?;
        SystemUtils::cleanup_temp_files("quiccrunch").await?;
        SystemUtils::cleanup_temp_files("grpc").await?;
    }
    
    if results {
        info!("Cleaning up results directory");
        let results_dir = PathBuf::from("./results");
        if results_dir.exists() {
            tokio::fs::remove_dir_all(&results_dir).await?;
            info!("Removed results directory: {:?}", results_dir);
        }
    }
    
    if !results && !temp && !processes {
        // Clean up everything if no specific option provided
        cleanup_artifacts(true, true, true).await?;
    }
    
    info!("Cleanup completed");
    Ok(())
}

async fn init_config(config_path: PathBuf, use_default: bool) -> Result<()> {
    info!("Initializing benchmark configuration");
    
    let config = if use_default {
        BenchmarkConfig::default()
    } else {
        // Interactive configuration would go here
        BenchmarkConfig::default()
    };
    
    config.save_to_file(&config_path)?;
    
    info!("Configuration saved to: {:?}", config_path);
    println!("Configuration initialized at: {:?}", config_path);
    println!("You can now edit the configuration file and run benchmarks with:");
    println!("  benchmark_runner run --config {:?}", config_path);
    
    Ok(())
}