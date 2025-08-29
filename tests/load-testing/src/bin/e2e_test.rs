use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn, error};

use quiccrunch_load_testing::{
    config::{TestConfig, ScenarioConfig, RequestRateConfig, MessageSizeConfig, StreamingConfig},
    framework::LoadTestFramework,
};

#[derive(Parser)]
#[command(name = "e2e-test")]
#[command(about = "QuicCrunch End-to-End Testing Suite")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    #[arg(short, long, default_value = "e2e_reports")]
    output: PathBuf,
    
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Full RPC lifecycle test
    FullLifecycle {
        #[arg(long, default_value = "5")]
        services: u32,
        
        #[arg(long, default_value = "100")]
        connections_per_service: u32,
    },
    
    /// Multi-service communication test
    MultiService {
        #[arg(long, default_value = "3")]
        service_count: u32,
        
        #[arg(long, default_value = "50")]
        cross_service_calls: u32,
    },
    
    /// Streaming RPC end-to-end test
    StreamingRpc {
        #[arg(long, default_value = "10")]
        concurrent_streams: u32,
        
        #[arg(long)]
        bidirectional: bool,
    },
    
    /// Error scenario testing
    ErrorScenarios {
        #[arg(long)]
        include_network_failures: bool,
        
        #[arg(long)]
        include_timeouts: bool,
    },
    
    /// Complete E2E test suite
    Suite {
        #[arg(long)]
        quick: bool,
        
        #[arg(long)]
        include_error_scenarios: bool,
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
        Commands::FullLifecycle { services, connections_per_service } => {
            run_full_lifecycle_test(base_config, cli.output, services, connections_per_service).await?;
        }
        Commands::MultiService { service_count, cross_service_calls } => {
            run_multi_service_test(base_config, cli.output, service_count, cross_service_calls).await?;
        }
        Commands::StreamingRpc { concurrent_streams, bidirectional } => {
            run_streaming_rpc_test(base_config, cli.output, concurrent_streams, bidirectional).await?;
        }
        Commands::ErrorScenarios { include_network_failures, include_timeouts } => {
            run_error_scenarios_test(base_config, cli.output, include_network_failures, include_timeouts).await?;
        }
        Commands::Suite { quick, include_error_scenarios } => {
            run_e2e_test_suite(base_config, cli.output, quick, include_error_scenarios).await?;
        }
    }
    
    Ok(())
}

async fn run_full_lifecycle_test(
    mut base_config: TestConfig,
    output_dir: PathBuf,
    services: u32,
    connections_per_service: u32,
) -> Result<()> {
    info!("🔄 Starting Full RPC Lifecycle Test");
    info!("Services: {}, Connections per service: {}", services, connections_per_service);
    
    base_config.reporting.output_dir = output_dir.to_string_lossy().to_string();
    
    let total_connections = services * connections_per_service;
    
    let scenario_config = ScenarioConfig {
        name: "Full RPC Lifecycle Test".to_string(),
        description: Some("Complete RPC lifecycle from connection to graceful shutdown".to_string()),
        duration: Duration::from_secs(180), // 3 minutes
        connections: total_connections,
        request_rate: RequestRateConfig::Variable {
            pattern: "lifecycle".to_string(),
            base_rate: 1000,
            variation: 0.2, // 20% variation
        },
        message_size: MessageSizeConfig::Distribution {
            pattern: "realistic".to_string(),
        },
        backend: "quinn".to_string(),
        streaming: None,
        network_conditions: None,
    };
    
    base_config.scenarios.insert("full_lifecycle".to_string(), scenario_config);
    let framework = LoadTestFramework::new(base_config);
    
    let result = framework.run_scenario("full_lifecycle").await
        .context("Full lifecycle test failed")?;
    
    if result.success {
        info!("✅ Full lifecycle test completed successfully");
        analyze_lifecycle_results(&result);
    } else {
        error!("❌ Full lifecycle test failed: {}", result.error_message.unwrap_or_default());
    }
    
    Ok(())
}

async fn run_multi_service_test(
    mut base_config: TestConfig,
    output_dir: PathBuf,
    service_count: u32,
    cross_service_calls: u32,
) -> Result<()> {
    info!("🌐 Starting Multi-Service Communication Test");
    info!("Services: {}, Cross-service calls: {}", service_count, cross_service_calls);
    
    base_config.reporting.output_dir = output_dir.to_string_lossy().to_string();
    
    let scenario_config = ScenarioConfig {
        name: "Multi-Service Communication Test".to_string(),
        description: Some("Test complex multi-service RPC interaction patterns".to_string()),
        duration: Duration::from_secs(240), // 4 minutes
        connections: service_count * 50, // 50 connections per service
        request_rate: RequestRateConfig::Constant { rate: cross_service_calls * 10 },
        message_size: MessageSizeConfig::Range { min: 512, max: 4096 },
        backend: "quinn".to_string(),
        streaming: None,
        network_conditions: None,
    };
    
    base_config.scenarios.insert("multi_service".to_string(), scenario_config);
    let framework = LoadTestFramework::new(base_config);
    
    let result = framework.run_scenario("multi_service").await
        .context("Multi-service test failed")?;
    
    if result.success {
        info!("✅ Multi-service test completed successfully");
        analyze_multi_service_results(&result);
    } else {
        error!("❌ Multi-service test failed: {}", result.error_message.unwrap_or_default());
    }
    
    Ok(())
}

async fn run_streaming_rpc_test(
    mut base_config: TestConfig,
    output_dir: PathBuf,
    concurrent_streams: u32,
    bidirectional: bool,
) -> Result<()> {
    info!("📡 Starting Streaming RPC End-to-End Test");
    info!("Concurrent streams: {}, Bidirectional: {}", concurrent_streams, bidirectional);
    
    base_config.reporting.output_dir = output_dir.to_string_lossy().to_string();
    
    let streaming_config = StreamingConfig {
        stream_count: concurrent_streams,
        stream_duration: Duration::from_secs(60),
        chunk_size: 8192,
        bidirectional,
    };
    
    let scenario_config = ScenarioConfig {
        name: "Streaming RPC E2E Test".to_string(),
        description: Some("End-to-end streaming RPC validation".to_string()),
        duration: Duration::from_secs(300), // 5 minutes
        connections: 100,
        request_rate: RequestRateConfig::Constant { rate: 200 },
        message_size: MessageSizeConfig::Fixed { size: 4096 },
        backend: "quinn".to_string(),
        streaming: Some(streaming_config),
        network_conditions: None,
    };
    
    base_config.scenarios.insert("streaming_rpc".to_string(), scenario_config);
    let framework = LoadTestFramework::new(base_config);
    
    let result = framework.run_scenario("streaming_rpc").await
        .context("Streaming RPC test failed")?;
    
    if result.success {
        info!("✅ Streaming RPC test completed successfully");
        analyze_streaming_results(&result);
    } else {
        error!("❌ Streaming RPC test failed: {}", result.error_message.unwrap_or_default());
    }
    
    Ok(())
}

async fn run_error_scenarios_test(
    mut base_config: TestConfig,
    output_dir: PathBuf,
    include_network_failures: bool,
    include_timeouts: bool,
) -> Result<()> {
    info!("⚠️  Starting Error Scenarios Test");
    info!("Network failures: {}, Timeouts: {}", include_network_failures, include_timeouts);
    
    base_config.reporting.output_dir = output_dir.to_string_lossy().to_string();
    
    // Simulate error conditions
    let network_conditions = if include_network_failures {
        Some(crate::config::NetworkConditionsConfig {
            latency: Some(Duration::from_millis(100)),
            packet_loss: Some(0.02), // 2% packet loss
            bandwidth_mbps: None,
            jitter: Some(Duration::from_millis(20)),
        })
    } else {
        None
    };
    
    let scenario_config = ScenarioConfig {
        name: "Error Scenarios Test".to_string(),
        description: Some("Test error handling and recovery mechanisms".to_string()),
        duration: Duration::from_secs(180),
        connections: 200,
        request_rate: RequestRateConfig::Variable {
            pattern: "error_injection".to_string(),
            base_rate: 500,
            variation: 0.5,
        },
        message_size: MessageSizeConfig::Range { min: 1024, max: 8192 },
        backend: "quinn".to_string(),
        streaming: None,
        network_conditions,
    };
    
    base_config.scenarios.insert("error_scenarios".to_string(), scenario_config);
    let framework = LoadTestFramework::new(base_config);
    
    let result = framework.run_scenario("error_scenarios").await
        .context("Error scenarios test failed")?;
    
    if result.success {
        info!("✅ Error scenarios test completed successfully");
        analyze_error_handling_results(&result);
    } else {
        error!("❌ Error scenarios test failed: {}", result.error_message.unwrap_or_default());
    }
    
    Ok(())
}

async fn run_e2e_test_suite(
    base_config: TestConfig,
    output_dir: PathBuf,
    quick: bool,
    include_error_scenarios: bool,
) -> Result<()> {
    info!("🧪 Starting Complete E2E Test Suite");
    info!("Mode: {}, Error scenarios: {}", 
          if quick { "Quick" } else { "Full" }, 
          include_error_scenarios);
    
    let mut test_count = 0;
    let mut successful_tests = 0;
    
    // Test 1: Full Lifecycle
    test_count += 1;
    info!("Test {}: Full RPC Lifecycle", test_count);
    let services = if quick { 3 } else { 5 };
    let connections_per_service = if quick { 20 } else { 50 };
    
    match run_full_lifecycle_test(base_config.clone(), output_dir.clone(), services, connections_per_service).await {
        Ok(_) => {
            successful_tests += 1;
            info!("✅ Full lifecycle test passed");
        }
        Err(e) => {
            error!("❌ Full lifecycle test failed: {}", e);
        }
    }
    
    // Test 2: Multi-Service Communication
    test_count += 1;
    info!("Test {}: Multi-Service Communication", test_count);
    let service_count = if quick { 2 } else { 3 };
    let cross_service_calls = if quick { 25 } else { 50 };
    
    match run_multi_service_test(base_config.clone(), output_dir.clone(), service_count, cross_service_calls).await {
        Ok(_) => {
            successful_tests += 1;
            info!("✅ Multi-service test passed");
        }
        Err(e) => {
            error!("❌ Multi-service test failed: {}", e);
        }
    }
    
    // Test 3: Streaming RPC
    test_count += 1;
    info!("Test {}: Streaming RPC", test_count);
    let concurrent_streams = if quick { 5 } else { 10 };
    
    match run_streaming_rpc_test(base_config.clone(), output_dir.clone(), concurrent_streams, true).await {
        Ok(_) => {
            successful_tests += 1;
            info!("✅ Streaming RPC test passed");
        }
        Err(e) => {
            error!("❌ Streaming RPC test failed: {}", e);
        }
    }
    
    // Test 4: Error Scenarios (optional)
    if include_error_scenarios {
        test_count += 1;
        info!("Test {}: Error Scenarios", test_count);
        
        match run_error_scenarios_test(base_config.clone(), output_dir.clone(), true, true).await {
            Ok(_) => {
                successful_tests += 1;
                info!("✅ Error scenarios test passed");
            }
            Err(e) => {
                error!("❌ Error scenarios test failed: {}", e);
            }
        }
    }
    
    // Summary
    info!("🏆 E2E Test Suite Results:");
    info!("   Total Tests: {}", test_count);
    info!("   Successful: {}", successful_tests);
    info!("   Failed: {}", test_count - successful_tests);
    info!("   Success Rate: {:.1}%", (successful_tests as f64 / test_count as f64) * 100.0);
    info!("📊 Reports generated in: {:?}", output_dir);
    
    if successful_tests == test_count {
        info!("🎉 All E2E tests passed!");
    } else {
        warn!("⚠️  Some E2E tests failed - review the reports for details");
        std::process::exit(1);
    }
    
    Ok(())
}

fn analyze_lifecycle_results(result: &crate::framework::TestResult) {
    info!("📊 Lifecycle Analysis:");
    info!("   Connection Success Rate: {:.2}%", 
          ((result.metrics.total_connections - result.metrics.connection_errors) as f64 
           / result.metrics.total_connections as f64) * 100.0);
    info!("   Average Request Latency: {:?}", result.metrics.latency_mean);
    info!("   Request Success Rate: {:.2}%", 
          (result.metrics.successful_requests as f64 / result.metrics.total_requests as f64) * 100.0);
    
    if result.metrics.connection_errors == 0 {
        info!("✅ Perfect connection handling");
    } else {
        warn!("⚠️  {} connection errors detected", result.metrics.connection_errors);
    }
}

fn analyze_multi_service_results(result: &crate::framework::TestResult) {
    info!("📊 Multi-Service Analysis:");
    info!("   Total Cross-Service Calls: {}", result.metrics.total_requests);
    info!("   Service Call Success Rate: {:.2}%", 
          (result.metrics.successful_requests as f64 / result.metrics.total_requests as f64) * 100.0);
    info!("   Average Latency: {:?}", result.metrics.latency_mean);
    info!("   P99 Latency: {:?}", result.metrics.latency_p99);
    
    if result.metrics.error_rate < 0.01 {
        info!("✅ Excellent multi-service reliability");
    } else {
        warn!("⚠️  High error rate in multi-service calls: {:.2}%", result.metrics.error_rate * 100.0);
    }
}

fn analyze_streaming_results(result: &crate::framework::TestResult) {
    info!("📊 Streaming RPC Analysis:");
    info!("   Streams Created: {}", result.metrics.streams_created);
    info!("   Streams Active (at end): {}", result.metrics.streams_active);
    info!("   Data Transferred: {:.1} MB", 
          (result.metrics.bytes_sent + result.metrics.bytes_received) as f64 / 1024.0 / 1024.0);
    
    let stream_success_rate = if result.metrics.streams_created > 0 {
        ((result.metrics.streams_created - result.metrics.streams_active) as f64 
         / result.metrics.streams_created as f64) * 100.0
    } else {
        0.0
    };
    
    info!("   Stream Completion Rate: {:.1}%", stream_success_rate);
    
    if stream_success_rate > 95.0 {
        info!("✅ Excellent streaming performance");
    } else {
        warn!("⚠️  Some streams did not complete successfully");
    }
}

fn analyze_error_handling_results(result: &crate::framework::TestResult) {
    info!("📊 Error Handling Analysis:");
    info!("   Total Errors: {}", result.metrics.failed_requests);
    info!("   Connection Errors: {}", result.metrics.connection_errors);
    info!("   Timeout Errors: {}", result.metrics.timeout_errors);
    info!("   Protocol Errors: {}", result.metrics.protocol_errors);
    info!("   Error Recovery Rate: {:.2}%", 
          (result.metrics.successful_requests as f64 / result.metrics.total_requests as f64) * 100.0);
    
    if result.metrics.error_rate < 0.05 { // Less than 5% error rate under adverse conditions
        info!("✅ Good error handling and recovery");
    } else {
        warn!("⚠️  High error rate - error handling may need improvement");
    }
}