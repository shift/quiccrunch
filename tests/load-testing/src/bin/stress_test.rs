use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn, error};

use quiccrunch_load_testing::{
    config::{TestConfig, ScenarioConfig, RequestRateConfig, MessageSizeConfig},
    framework::LoadTestFramework,
};

#[derive(Parser)]
#[command(name = "stress-test")]
#[command(about = "QuicCrunch Stress Testing Tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    #[arg(short, long, default_value = "stress_reports")]
    output: PathBuf,
    
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    ResourceExhaustion {
        #[arg(long, default_value = "100")]
        start_connections: u32,
        
        #[arg(long, default_value = "5000")]
        max_connections: u32,
        
        #[arg(long, default_value = "100")]
        step: u32,
    },
    
    BurstTraffic {
        #[arg(long, default_value = "1000")]
        base_rate: u32,
        
        #[arg(long, default_value = "10000")]
        burst_rate: u32,
    },
    
    Suite {
        #[arg(long)]
        quick: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize basic logging
    tracing_subscriber::fmt()
        .with_max_level(if cli.verbose { tracing::Level::DEBUG } else { tracing::Level::INFO })
        .init();
    
    let base_config = TestConfig::default_config();
    
    match cli.command {
        Commands::ResourceExhaustion { start_connections, max_connections, step } => {
            run_resource_exhaustion_test(base_config, cli.output, start_connections, max_connections, step).await?;
        }
        Commands::BurstTraffic { base_rate, burst_rate } => {
            run_burst_traffic_test(base_config, cli.output, base_rate, burst_rate).await?;
        }
        Commands::Suite { quick } => {
            run_stress_test_suite(base_config, cli.output, quick).await?;
        }
    }
    
    Ok(())
}

async fn run_resource_exhaustion_test(
    mut base_config: TestConfig,
    output_dir: PathBuf,
    start_connections: u32,
    max_connections: u32,
    step: u32,
) -> Result<()> {
    info!("🔥 Starting Resource Exhaustion Test");
    info!("Connections: {} -> {} (step: {})", start_connections, max_connections, step);
    
    base_config.reporting.output_dir = output_dir.to_string_lossy().to_string();
    
    let mut current_connections = start_connections;
    let mut breaking_point = None;
    
    while current_connections <= max_connections {
        info!("Testing with {} connections...", current_connections);
        
        let scenario_config = ScenarioConfig {
            name: format!("Resource Exhaustion - {} connections", current_connections),
            description: Some("Resource exhaustion test".to_string()),
            duration: Duration::from_secs(30),
            connections: current_connections,
            request_rate: RequestRateConfig::Constant { rate: 1000 },
            message_size: MessageSizeConfig::Fixed { size: 1024 },
            backend: "quinn".to_string(),
            streaming: None,
            network_conditions: None,
        };
        
        base_config.scenarios.insert("stress_test".to_string(), scenario_config);
        let framework = LoadTestFramework::new(base_config.clone());
        
        match framework.run_scenario("stress_test").await {
            Ok(result) => {
                let error_rate = result.metrics.error_rate * 100.0;
                let latency_p99_ms = result.metrics.latency_p99.as_millis();
                
                info!("✅ {} connections: {:.2}% errors, {}ms P99 latency", 
                      current_connections, error_rate, latency_p99_ms);
                
                if error_rate > 10.0 || latency_p99_ms > 5000 {
                    breaking_point = Some(current_connections);
                    warn!("🚨 Breaking point reached at {} connections", current_connections);
                    break;
                }
            }
            Err(e) => {
                breaking_point = Some(current_connections);
                error!("❌ Test failed at {} connections: {}", current_connections, e);
                break;
            }
        }
        
        current_connections += step;
    }
    
    if let Some(breaking_point) = breaking_point {
        info!("📊 Breaking Point: {} connections", breaking_point);
        info!("📊 Safe Operating Limit: {} connections", breaking_point.saturating_sub(step));
    } else {
        info!("📊 No breaking point found up to {} connections", max_connections);
    }
    
    Ok(())
}

async fn run_burst_traffic_test(
    mut base_config: TestConfig,
    output_dir: PathBuf,
    base_rate: u32,
    burst_rate: u32,
) -> Result<()> {
    info!("💥 Starting Burst Traffic Test");
    info!("Base Rate: {} RPS, Burst Rate: {} RPS", base_rate, burst_rate);
    
    base_config.reporting.output_dir = output_dir.to_string_lossy().to_string();
    
    let scenario_config = ScenarioConfig {
        name: "Burst Traffic Test".to_string(),
        description: Some("Sudden traffic spike simulation".to_string()),
        duration: Duration::from_secs(300),
        connections: 2000,
        request_rate: RequestRateConfig::Burst {
            base_rate,
            burst_rate,
            burst_duration: Duration::from_secs(10),
        },
        message_size: MessageSizeConfig::Fixed { size: 512 },
        backend: "quinn".to_string(),
        streaming: None,
        network_conditions: None,
    };
    
    base_config.scenarios.insert("burst_traffic".to_string(), scenario_config);
    let framework = LoadTestFramework::new(base_config);
    
    let result = framework.run_scenario("burst_traffic").await
        .context("Burst traffic test failed")?;
    
    if result.success {
        info!("✅ Burst traffic test completed successfully");
        info!("📊 Error rate during bursts: {:.2}%", result.metrics.error_rate * 100.0);
        info!("📊 P99 latency: {:?}", result.metrics.latency_p99);
        
        if result.metrics.error_rate < 0.01 {
            info!("✅ System handled traffic bursts well");
        } else {
            warn!("⚠️  High error rate during bursts - consider capacity planning");
        }
    } else {
        error!("❌ Burst traffic test failed: {}", result.error_message.unwrap_or_default());
    }
    
    Ok(())
}

async fn run_stress_test_suite(
    base_config: TestConfig,
    output_dir: PathBuf,
    quick: bool,
) -> Result<()> {
    info!("🧪 Starting Stress Test Suite");
    info!("Mode: {} durations", if quick { "Quick" } else { "Full" });
    
    // Run resource exhaustion test
    info!("1/2: Resource exhaustion test");
    let max_connections = if quick { 1000 } else { 5000 };
    run_resource_exhaustion_test(base_config.clone(), output_dir.clone(), 100, max_connections, 100).await?;
    
    // Run burst traffic test
    info!("2/2: Burst traffic test");
    run_burst_traffic_test(base_config, output_dir.clone(), 1000, 10000).await?;
    
    info!("🏆 Stress Test Suite completed successfully");
    info!("📊 Reports generated in: {:?}", output_dir);
    
    Ok(())
}