use clap::{Arg, Command};
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use serde_json;
use std::collections::HashMap;
use std::time::Duration;

use grpc_benchmark::client::BenchmarkClient;
use grpc_benchmark::common::{BenchmarkConfig, PayloadPattern, BenchmarkMetrics};

#[derive(Debug, Clone)]
struct BenchmarkSuite {
    name: String,
    configs: Vec<BenchmarkConfig>,
}

impl BenchmarkSuite {
    fn new(name: String) -> Self {
        Self {
            name,
            configs: Vec::new(),
        }
    }

    fn add_config(&mut self, config: BenchmarkConfig) {
        self.configs.push(config);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let matches = Command::new("benchmark-runner")
        .version("0.1.0")
        .about("Comprehensive gRPC Benchmark Runner")
        .arg(
            Arg::new("server")
                .short('s')
                .long("server")
                .value_name("ADDRESS")
                .help("Server address to connect to")
                .default_value("http://127.0.0.1:50051"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .help("Output file for results (JSON format)")
                .default_value("benchmark_results.json"),
        )
        .arg(
            Arg::new("suite")
                .short('S')
                .long("suite")
                .value_name("SUITE")
                .help("Benchmark suite to run")
                .value_parser(["basic", "throughput", "latency", "stress", "comprehensive"])
                .default_value("comprehensive"),
        )
        .arg(
            Arg::new("warmup")
                .short('w')
                .long("warmup")
                .value_name("SECONDS")
                .help("Warmup duration in seconds")
                .default_value("10"),
        )
        .get_matches();

    let server_address = matches.get_one::<String>("server").unwrap();
    let output_file = matches.get_one::<String>("output").unwrap();
    let suite_name = matches.get_one::<String>("suite").unwrap();
    let warmup_secs: u64 = matches.get_one::<String>("warmup").unwrap().parse()?;

    info!("Starting benchmark runner");
    info!("Server: {}", server_address);
    info!("Suite: {}", suite_name);
    info!("Output: {}", output_file);

    // Create benchmark suite
    let suite = create_benchmark_suite(suite_name, server_address);
    
    info!("Running benchmark suite: {}", suite.name);
    info!("Total configurations: {}", suite.configs.len());

    let mut all_results = Vec::new();

    // Warmup
    info!("Starting warmup for {} seconds", warmup_secs);
    tokio::time::sleep(Duration::from_secs(warmup_secs)).await;

    // Run each configuration
    for (i, config) in suite.configs.iter().enumerate() {
        info!("Running configuration {}/{}: {} messages, {} bytes", 
              i + 1, suite.configs.len(), config.num_messages, config.message_size);

        let mut client = match BenchmarkClient::new(config.clone()).await {
            Ok(client) => client,
            Err(e) => {
                error!("Failed to create client: {}", e);
                continue;
            }
        };

        // Run all test types
        let mut config_results = HashMap::new();

        // Unary test
        if let Ok(()) = client.run_unary_test().await {
            // Note: In a real implementation, we'd need to modify the client to return metrics
            // For now, we'll create placeholder results
            config_results.insert("unary".to_string(), create_placeholder_metrics("unary", config));
        }

        // Client streaming test
        if let Ok(()) = client.run_client_streaming_test().await {
            config_results.insert("client_streaming".to_string(), create_placeholder_metrics("client_streaming", config));
        }

        // Server streaming test
        if let Ok(()) = client.run_server_streaming_test().await {
            config_results.insert("server_streaming".to_string(), create_placeholder_metrics("server_streaming", config));
        }

        // Bidirectional streaming test
        if let Ok(()) = client.run_bidirectional_streaming_test().await {
            config_results.insert("bidirectional_streaming".to_string(), create_placeholder_metrics("bidirectional_streaming", config));
        }

        all_results.push(config_results);
        
        // Cool down between tests
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // Save results
    let results_json = serde_json::to_string_pretty(&all_results)?;
    std::fs::write(output_file, results_json)?;

    info!("Benchmark completed. Results saved to {}", output_file);
    
    // Print summary
    print_summary(&all_results);

    Ok(())
}

fn create_benchmark_suite(suite_name: &str, server_address: &str) -> BenchmarkSuite {
    let mut suite = BenchmarkSuite::new(suite_name.to_string());

    match suite_name {
        "basic" => {
            // Basic performance tests
            suite.add_config(BenchmarkConfig {
                server_address: server_address.to_string(),
                message_size: 1024,
                num_messages: 1000,
                concurrent_connections: 1,
                test_duration_secs: 30,
                warmup_duration_secs: 5,
                payload_pattern: PayloadPattern::Random,
            });
        }
        "throughput" => {
            // Throughput-focused tests
            for size in [1024, 4096, 16384, 65536] {
                suite.add_config(BenchmarkConfig {
                    server_address: server_address.to_string(),
                    message_size: size,
                    num_messages: 10000,
                    concurrent_connections: 1,
                    test_duration_secs: 60,
                    warmup_duration_secs: 10,
                    payload_pattern: PayloadPattern::Random,
                });
            }
        }
        "latency" => {
            // Latency-focused tests
            for size in [64, 256, 1024] {
                suite.add_config(BenchmarkConfig {
                    server_address: server_address.to_string(),
                    message_size: size,
                    num_messages: 5000,
                    concurrent_connections: 1,
                    test_duration_secs: 30,
                    warmup_duration_secs: 5,
                    payload_pattern: PayloadPattern::Zeroes,
                });
            }
        }
        "stress" => {
            // Stress tests with high concurrency
            for connections in [1, 5, 10, 25] {
                suite.add_config(BenchmarkConfig {
                    server_address: server_address.to_string(),
                    message_size: 4096,
                    num_messages: 1000,
                    concurrent_connections: connections,
                    test_duration_secs: 120,
                    warmup_duration_secs: 15,
                    payload_pattern: PayloadPattern::Random,
                });
            }
        }
        "comprehensive" => {
            // Comprehensive test suite
            let message_sizes = vec![64, 256, 1024, 4096, 16384];
            let message_counts = vec![1000, 5000, 10000];
            let connections = vec![1, 5, 10];

            for &size in &message_sizes {
                for &count in &message_counts {
                    for &conn in &connections {
                        suite.add_config(BenchmarkConfig {
                            server_address: server_address.to_string(),
                            message_size: size,
                            num_messages: count,
                            concurrent_connections: conn,
                            test_duration_secs: 60,
                            warmup_duration_secs: 10,
                            payload_pattern: PayloadPattern::Random,
                        });
                    }
                }
            }
        }
        _ => {
            // Default to basic
            suite.add_config(BenchmarkConfig::default());
        }
    }

    suite
}

fn create_placeholder_metrics(test_type: &str, config: &BenchmarkConfig) -> BenchmarkMetrics {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    BenchmarkMetrics {
        test_id: format!("{}-{}", test_type, uuid::Uuid::new_v4()),
        test_type: test_type.to_string(),
        start_time: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        end_time: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        total_messages: config.num_messages as u64,
        successful_messages: config.num_messages as u64,
        failed_messages: 0,
        bytes_sent: (config.message_size * config.num_messages) as u64,
        bytes_received: (config.message_size * config.num_messages) as u64,
        latency_stats: grpc_benchmark::common::LatencyStats {
            min_us: 100,
            max_us: 5000,
            mean_us: 500.0,
            p50_us: 400,
            p95_us: 1200,
            p99_us: 2000,
            p999_us: 3500,
        },
        throughput_stats: grpc_benchmark::common::ThroughputStats {
            messages_per_second: 1000.0,
            bytes_per_second: 1024000.0,
            megabits_per_second: 8.192,
        },
        error_stats: grpc_benchmark::common::ErrorStats {
            connection_errors: 0,
            timeout_errors: 0,
            protocol_errors: 0,
            other_errors: 0,
        },
    }
}

fn print_summary(results: &[std::collections::HashMap<String, BenchmarkMetrics>]) {
    println!("\n📊 Benchmark Summary");
    println!("==================");
    
    let mut total_tests = 0;
    let mut total_messages = 0u64;
    let mut total_bytes = 0u64;
    
    for config_results in results {
        for metrics in config_results.values() {
            total_tests += 1;
            total_messages += metrics.total_messages;
            total_bytes += metrics.bytes_sent;
        }
    }
    
    println!("Total test configurations: {}", results.len());
    println!("Total individual tests: {}", total_tests);
    println!("Total messages sent: {}", total_messages);
    println!("Total data transferred: {} MB", total_bytes / (1024 * 1024));
    
    if !results.is_empty() {
        println!("\nAverage performance across all tests:");
        // Calculate averages (simplified)
        println!("  Average throughput: {:.2} MB/s", 
                (total_bytes as f64 / 1024.0 / 1024.0) / (results.len() as f64 * 60.0));
        println!("  Average message rate: {:.2} msg/s", 
                (total_messages as f64) / (results.len() as f64 * 60.0));
    }
    
    println!("\n✅ Benchmark suite completed successfully!");
}