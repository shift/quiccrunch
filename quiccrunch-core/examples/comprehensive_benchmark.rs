// Copyright (c) 2025 QuicCrunch Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Comprehensive benchmark suite comparing QuicCrunch and gRPC performance
//! across all streaming patterns with accurate performance measurement.

use std::collections::HashMap;
use std::time::Duration;
use serde::{Serialize, Deserialize};
use tokio::time::sleep;
use clap::{Parser, ValueEnum};

use quiccrunch_core::TlsConfig;
use quiccrunch_core::benchmark::{
    BenchmarkConfig, StreamingPattern, PerformanceMetrics,
    BenchmarkComparison, ComparisonMetrics, MESSAGE_SIZES, CONNECTION_COUNTS
};
use quiccrunch_core::benchmark::quiccrunch_client::QuicCrunchClient;
use quiccrunch_core::benchmark::quiccrunch_server::QuicCrunchServer;
use quiccrunch_core::benchmark::grpc_client::GrpcClient;
use quiccrunch_core::benchmark::grpc_server::GrpcServer;

#[path = "cert_utils.rs"]
mod cert_utils;

/// Command line arguments
#[derive(Parser, Debug)]
#[command(name = "comprehensive_benchmark")]
#[command(about = "Comprehensive QuicCrunch vs gRPC benchmark suite")]
struct Args {
    /// Test patterns to run
    #[arg(short, long, value_enum, default_values_t = [TestPattern::All])]
    patterns: Vec<TestPattern>,
    
    /// Message sizes to test (bytes)
    #[arg(short, long, value_delimiter = ',')]
    message_sizes: Option<Vec<usize>>,
    
    /// Connection counts to test
    #[arg(short, long, value_delimiter = ',')]
    connection_counts: Option<Vec<usize>>,
    
    /// Test duration per configuration (seconds)
    #[arg(short, long, default_value = "30")]
    duration: u64,
    
    /// Warmup duration (seconds)
    #[arg(short, long, default_value = "5")]
    warmup: u64,
    
    /// Skip gRPC tests (QuicCrunch only)
    #[arg(long)]
    skip_grpc: bool,
    
    /// Output format
    #[arg(short, long, value_enum, default_value = "json")]
    output: OutputFormat,
    
    /// Output file prefix
    #[arg(short = 'f', long, default_value = "benchmark_results")]
    output_file: String,
    
    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(ValueEnum, Debug, Clone)]
enum TestPattern {
    All,
    Unary,
    ClientStreaming,
    ServerStreaming,
    Bidirectional,
}

#[derive(ValueEnum, Debug, Clone)]
enum OutputFormat {
    Json,
    Csv,
    Markdown,
}

/// Comprehensive test results
#[derive(Debug, Serialize, Deserialize)]
struct ComprehensiveResults {
    timestamp: String,
    test_matrix: TestMatrix,
    results: Vec<BenchmarkComparison>,
    summary: TestSummary,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestMatrix {
    message_sizes: Vec<usize>,
    connection_counts: Vec<usize>,
    streaming_patterns: Vec<String>,
    duration_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestSummary {
    total_tests: usize,
    quiccrunch_wins: usize,
    grpc_wins: usize,
    avg_latency_improvement: f64,
    avg_throughput_improvement: f64,
    best_quiccrunch_scenarios: Vec<String>,
    best_grpc_scenarios: Vec<String>,
}

async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();
    
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(
        if args.verbose { "debug" } else { "info" }
    )).init();
    
    println!("🚀 Starting Comprehensive QuicCrunch vs gRPC Benchmark Suite");
    println!("=================================================================");
    
    // Generate test certificates
    let (cert, key, ca_cert) = cert_utils::generate_ca_cert()
        .map_err(|e| format!("Failed to generate CA cert: {}", e))?;
    let (node_cert, node_key) = cert_utils::generate_node_cert(&ca_cert, "localhost")
        .map_err(|e| format!("Failed to generate node cert: {}", e))?;
        
    let tls_config = TlsConfig {
        cert: node_cert,
        key: node_key,
        ca_cert: cert,
        cert_pem: vec![],
        key_pem: vec![],
        ca_cert_pem: vec![],
    };
    
    // Set up test matrix
    let message_sizes = args.message_sizes.unwrap_or_else(|| {
        MESSAGE_SIZES[..8].to_vec() // Use first 8 sizes for faster testing
    });
    
    let connection_counts = args.connection_counts.unwrap_or_else(|| {
        vec![1, 10, 50] // Reduced for faster testing
    });
    
    let streaming_patterns = get_streaming_patterns(&args.patterns);
    
    let test_matrix = TestMatrix {
        message_sizes: message_sizes.clone(),
        connection_counts: connection_counts.clone(),
        streaming_patterns: streaming_patterns.iter().map(|p| format!("{:?}", p)).collect(),
        duration_seconds: args.duration,
    };
    
    println!("📊 Test Matrix:");
    println!("  Message sizes: {:?}", message_sizes);
    println!("  Connection counts: {:?}", connection_counts);
    println!("  Streaming patterns: {:?}", streaming_patterns);
    println!("  Duration per test: {}s", args.duration);
    println!();
    
    // Start servers
    let mut servers = ServerManager::new(tls_config.clone()).await?;
    servers.start_all().await?;
    
    // Allow servers to fully start
    sleep(Duration::from_millis(500)).await;
    
    // Run comprehensive tests
    let mut results = Vec::new();
    let total_tests = message_sizes.len() * connection_counts.len() * streaming_patterns.len();
    let mut test_count = 0;
    
    for &message_size in &message_sizes {
        for &conn_count in &connection_counts {
            for pattern in &streaming_patterns {
                test_count += 1;
                println!("🧪 Test {}/{}: {}B messages, {} connections, {:?} pattern", 
                    test_count, total_tests, message_size, conn_count, pattern);
                
                let config = BenchmarkConfig {
                    message_size,
                    streaming_pattern: pattern.clone(),
                    concurrent_connections: conn_count,
                    duration_seconds: args.duration,
                    warmup_seconds: args.warmup,
                    target_qps: None,
                };
                
                let comparison = run_single_benchmark(&config, &tls_config, args.skip_grpc).await?;
                results.push(comparison);
                
                println!("   ✅ Completed\n");
                
                // Small delay between tests
                sleep(Duration::from_millis(100)).await;
            }
        }
    }
    
    // Stop servers
    servers.stop_all().await?;
    
    // Generate summary
    let summary = generate_summary(&results);
    
    let comprehensive_results = ComprehensiveResults {
        timestamp: chrono::Utc::now().to_rfc3339(),
        test_matrix,
        results,
        summary,
    };
    
    // Output results
    output_results(&comprehensive_results, &args).await?;
    
    // Print final summary
    print_final_summary(&comprehensive_results.summary);
    
    Ok(())
}

/// Get streaming patterns from command line args
fn get_streaming_patterns(patterns: &[TestPattern]) -> Vec<StreamingPattern> {
    let mut result = Vec::new();
    
    for pattern in patterns {
        match pattern {
            TestPattern::All => {
                result.push(StreamingPattern::Unary);
                result.push(StreamingPattern::ClientStreaming { message_count: 10 });
                result.push(StreamingPattern::ServerStreaming { message_count: 10 });
                result.push(StreamingPattern::Bidirectional { message_count: 10 });
                break;
            }
            TestPattern::Unary => result.push(StreamingPattern::Unary),
            TestPattern::ClientStreaming => result.push(StreamingPattern::ClientStreaming { message_count: 10 }),
            TestPattern::ServerStreaming => result.push(StreamingPattern::ServerStreaming { message_count: 10 }),
            TestPattern::Bidirectional => result.push(StreamingPattern::Bidirectional { message_count: 10 }),
        }
    }
    
    result
}

/// Server management
struct ServerManager {
    quiccrunch_server: Option<QuicCrunchServer>,
    grpc_server: Option<GrpcServer>,
    tls_config: TlsConfig,
}

impl ServerManager {
    async fn new(tls_config: TlsConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            quiccrunch_server: None,
            grpc_server: None,
            tls_config,
        })
    }
    
    async fn start_all(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Start QuicCrunch server
        let mut quiccrunch_server = QuicCrunchServer::new(self.tls_config.clone())?;
        quiccrunch_server.start("127.0.0.1:50051").await?;
        self.quiccrunch_server = Some(quiccrunch_server);
        
        // Start gRPC server
        #[cfg(feature = "grpc-comparison")]
        {
            let mut grpc_server = GrpcServer::new();
            grpc_server.start("127.0.0.1:50053").await?;
            self.grpc_server = Some(grpc_server);
        }
        
        println!("🖥️  All servers started successfully");
        Ok(())
    }
    
    async fn stop_all(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(mut server) = self.quiccrunch_server.take() {
            server.stop().await?;
        }
        
        if let Some(mut server) = self.grpc_server.take() {
            server.stop().await?;
        }
        
        println!("🛑 All servers stopped");
        Ok(())
    }
}

/// Run a single benchmark comparison
async fn run_single_benchmark(
    config: &BenchmarkConfig,
    tls_config: &TlsConfig,
    skip_grpc: bool,
) -> Result<BenchmarkComparison, Box<dyn std::error::Error + Send + Sync>> {
    // Run QuicCrunch benchmark
    print!("  🔧 Running QuicCrunch test... ");
    let quiccrunch_client = QuicCrunchClient::new(tls_config.clone())?;
    let quiccrunch_results = quiccrunch_client.run_benchmark(config, "127.0.0.1:50051").await?;
    println!("✅");
    
    // Run gRPC benchmark
    let grpc_results = if skip_grpc {
        print!("  ⏭️  Skipping gRPC test... ");
        println!("✅");
        create_dummy_metrics()
    } else {
        #[cfg(feature = "grpc-comparison")]
        {
            print!("  🔧 Running gRPC test... ");
            let mut grpc_client = GrpcClient::new("127.0.0.1:50053").await?;
            let results = grpc_client.run_benchmark(config).await?;
            println!("✅");
            results
        }
        
        #[cfg(not(feature = "grpc-comparison"))]
        {
            print!("  ⚠️  gRPC not available (feature disabled)... ");
            println!("✅");
            create_dummy_metrics()
        }
    };
    
    // Calculate comparison metrics
    let comparison = if skip_grpc || !cfg!(feature = "grpc-comparison") {
        ComparisonMetrics {
            throughput_ratio: 1.0,
            latency_ratio: 1.0,
            winner: "QuicCrunch (only tested)".to_string(),
        }
    } else {
        let throughput_ratio = quiccrunch_results.throughput.mbps / grpc_results.throughput.mbps.max(0.001);
        let latency_ratio = grpc_results.latency_percentiles.p99_ms / quiccrunch_results.latency_percentiles.p99_ms.max(0.001);
        let winner = if throughput_ratio > 1.0 && latency_ratio > 1.0 {
            "QuicCrunch".to_string()
        } else if throughput_ratio > 1.0 || latency_ratio > 1.0 {
            "Mixed".to_string()
        } else {
            "gRPC".to_string()
        };
        
        ComparisonMetrics {
            throughput_ratio,
            latency_ratio,
            winner,
        }
    };
    
    Ok(BenchmarkComparison {
        timestamp: chrono::Utc::now().to_rfc3339(),
        config: config.clone(),
        quiccrunch_results,
        grpc_results,
        comparison,
    })
}

/// Create dummy metrics for when gRPC is not available
fn create_dummy_metrics() -> PerformanceMetrics {
    use quiccrunch_core::benchmark::{LatencyMetrics, ThroughputMetrics, ResourceMetrics, ErrorMetrics};
    
    PerformanceMetrics {
        latency_percentiles: LatencyMetrics {
            p50_ms: 0.0,
            p95_ms: 0.0,
            p99_ms: 0.0,
            mean_ms: 0.0,
            std_dev_ms: 0.0,
        },
        throughput: ThroughputMetrics {
            messages_per_second: 0.0,
            bytes_per_second: 0.0,
            mbps: 0.0,
        },
        resource_usage: ResourceMetrics {
            cpu_usage_percent: 0.0,
            memory_usage_bytes: 0,
            network_io_bytes: 0,
        },
        error_metrics: ErrorMetrics {
            connection_failures: 0,
            message_failures: 0,
            timeout_failures: 0,
            total_errors: 0,
        },
    }
}

/// Generate test summary
fn generate_summary(results: &[BenchmarkComparison]) -> TestSummary {
    let total_tests = results.len();
    let mut quiccrunch_wins = 0;
    let mut grpc_wins = 0;
    let mut latency_improvements = Vec::new();
    let mut throughput_improvements = Vec::new();
    let mut best_quiccrunch_scenarios = Vec::new();
    let mut best_grpc_scenarios = Vec::new();
    
    for result in results {
        match result.comparison.winner.as_str() {
            "QuicCrunch" => {
                quiccrunch_wins += 1;
                if result.comparison.throughput_ratio > 1.5 || result.comparison.latency_ratio > 1.5 {
                    best_quiccrunch_scenarios.push(format!(
                        "{}B {:?} {}conn", 
                        result.config.message_size,
                        result.config.streaming_pattern,
                        result.config.concurrent_connections
                    ));
                }
            }
            "gRPC" => {
                grpc_wins += 1;
                if result.comparison.throughput_ratio < 0.5 || result.comparison.latency_ratio < 0.5 {
                    best_grpc_scenarios.push(format!(
                        "{}B {:?} {}conn", 
                        result.config.message_size,
                        result.config.streaming_pattern,
                        result.config.concurrent_connections
                    ));
                }
            }
            _ => {}
        }
        
        if result.comparison.latency_ratio > 0.0 {
            latency_improvements.push((result.comparison.latency_ratio - 1.0) * 100.0);
        }
        if result.comparison.throughput_ratio > 0.0 {
            throughput_improvements.push((result.comparison.throughput_ratio - 1.0) * 100.0);
        }
    }
    
    let avg_latency_improvement = if latency_improvements.is_empty() {
        0.0
    } else {
        latency_improvements.iter().sum::<f64>() / latency_improvements.len() as f64
    };
    
    let avg_throughput_improvement = if throughput_improvements.is_empty() {
        0.0
    } else {
        throughput_improvements.iter().sum::<f64>() / throughput_improvements.len() as f64
    };
    
    TestSummary {
        total_tests,
        quiccrunch_wins,
        grpc_wins,
        avg_latency_improvement,
        avg_throughput_improvement,
        best_quiccrunch_scenarios,
        best_grpc_scenarios,
    }
}

/// Output results in specified format
async fn output_results(
    results: &ComprehensiveResults,
    args: &Args,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    
    match args.output {
        OutputFormat::Json => {
            let filename = format!("{}_{}.json", args.output_file, timestamp);
            let json = serde_json::to_string_pretty(results)?;
            tokio::fs::write(&filename, json).await?;
            println!("📁 Results saved to: {}", filename);
        }
        OutputFormat::Csv => {
            let filename = format!("{}_{}.csv", args.output_file, timestamp);
            let csv = generate_csv(results)?;
            tokio::fs::write(&filename, csv).await?;
            println!("📁 Results saved to: {}", filename);
        }
        OutputFormat::Markdown => {
            let filename = format!("{}_{}.md", args.output_file, timestamp);
            let markdown = generate_markdown(results)?;
            tokio::fs::write(&filename, markdown).await?;
            println!("📁 Results saved to: {}", filename);
        }
    }
    
    Ok(())
}

/// Generate CSV output
fn generate_csv(results: &ComprehensiveResults) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut csv = String::new();
    
    // Header
    csv.push_str("Pattern,MessageSize,Connections,Protocol,P50Latency,P95Latency,P99Latency,Throughput,Winner\n");
    
    // Data rows
    for result in &results.results {
        let pattern = format!("{:?}", result.config.streaming_pattern);
        
        // QuicCrunch row
        csv.push_str(&format!(
            "{},{},{},QuicCrunch,{:.2},{:.2},{:.2},{:.2},{}\n",
            pattern,
            result.config.message_size,
            result.config.concurrent_connections,
            result.quiccrunch_results.latency_percentiles.p50_ms,
            result.quiccrunch_results.latency_percentiles.p95_ms,
            result.quiccrunch_results.latency_percentiles.p99_ms,
            result.quiccrunch_results.throughput.mbps,
            result.comparison.winner
        ));
        
        // gRPC row
        csv.push_str(&format!(
            "{},{},{},gRPC,{:.2},{:.2},{:.2},{:.2},{}\n",
            pattern,
            result.config.message_size,
            result.config.concurrent_connections,
            result.grpc_results.latency_percentiles.p50_ms,
            result.grpc_results.latency_percentiles.p95_ms,
            result.grpc_results.latency_percentiles.p99_ms,
            result.grpc_results.throughput.mbps,
            result.comparison.winner
        ));
    }
    
    Ok(csv)
}

/// Generate Markdown output
fn generate_markdown(results: &ComprehensiveResults) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let mut md = String::new();
    
    md.push_str("# Comprehensive QuicCrunch vs gRPC Benchmark Results\n\n");
    md.push_str(&format!("**Generated:** {}\n\n", results.timestamp));
    
    // Test Matrix
    md.push_str("## Test Matrix\n\n");
    md.push_str(&format!("- **Message Sizes:** {:?}\n", results.test_matrix.message_sizes));
    md.push_str(&format!("- **Connection Counts:** {:?}\n", results.test_matrix.connection_counts));
    md.push_str(&format!("- **Streaming Patterns:** {:?}\n", results.test_matrix.streaming_patterns));
    md.push_str(&format!("- **Duration per Test:** {}s\n\n", results.test_matrix.duration_seconds));
    
    // Summary
    md.push_str("## Summary\n\n");
    md.push_str(&format!("- **Total Tests:** {}\n", results.summary.total_tests));
    md.push_str(&format!("- **QuicCrunch Wins:** {}\n", results.summary.quiccrunch_wins));
    md.push_str(&format!("- **gRPC Wins:** {}\n", results.summary.grpc_wins));
    md.push_str(&format!("- **Average Latency Improvement:** {:.1}%\n", results.summary.avg_latency_improvement));
    md.push_str(&format!("- **Average Throughput Improvement:** {:.1}%\n\n", results.summary.avg_throughput_improvement));
    
    // Best scenarios
    if !results.summary.best_quiccrunch_scenarios.is_empty() {
        md.push_str("### Best QuicCrunch Scenarios\n");
        for scenario in &results.summary.best_quiccrunch_scenarios {
            md.push_str(&format!("- {}\n", scenario));
        }
        md.push_str("\n");
    }
    
    if !results.summary.best_grpc_scenarios.is_empty() {
        md.push_str("### Best gRPC Scenarios\n");
        for scenario in &results.summary.best_grpc_scenarios {
            md.push_str(&format!("- {}\n", scenario));
        }
        md.push_str("\n");
    }
    
    // Detailed Results Table
    md.push_str("## Detailed Results\n\n");
    md.push_str("| Pattern | Size | Connections | Protocol | P50 Latency | P95 Latency | P99 Latency | Throughput | Winner |\n");
    md.push_str("|---------|------|-------------|----------|-------------|-------------|-------------|------------|--------|\n");
    
    for result in &results.results {
        let pattern = format!("{:?}", result.config.streaming_pattern);
        
        // QuicCrunch row
        md.push_str(&format!(
            "| {} | {}B | {} | QuicCrunch | {:.2}ms | {:.2}ms | {:.2}ms | {:.2} Mbps | {} |\n",
            pattern,
            result.config.message_size,
            result.config.concurrent_connections,
            result.quiccrunch_results.latency_percentiles.p50_ms,
            result.quiccrunch_results.latency_percentiles.p95_ms,
            result.quiccrunch_results.latency_percentiles.p99_ms,
            result.quiccrunch_results.throughput.mbps,
            result.comparison.winner
        ));
        
        // gRPC row
        md.push_str(&format!(
            "| {} | {}B | {} | gRPC | {:.2}ms | {:.2}ms | {:.2}ms | {:.2} Mbps | {} |\n",
            pattern,
            result.config.message_size,
            result.config.concurrent_connections,
            result.grpc_results.latency_percentiles.p50_ms,
            result.grpc_results.latency_percentiles.p95_ms,
            result.grpc_results.latency_percentiles.p99_ms,
            result.grpc_results.throughput.mbps,
            result.comparison.winner
        ));
    }
    
    Ok(md)
}

/// Print final summary to console
fn print_final_summary(summary: &TestSummary) {
    println!("🏆 FINAL RESULTS SUMMARY");
    println!("========================");
    println!("Total tests: {}", summary.total_tests);
    println!("QuicCrunch wins: {} ({:.1}%)", 
        summary.quiccrunch_wins,
        (summary.quiccrunch_wins as f64 / summary.total_tests as f64) * 100.0
    );
    println!("gRPC wins: {} ({:.1}%)", 
        summary.grpc_wins,
        (summary.grpc_wins as f64 / summary.total_tests as f64) * 100.0
    );
    println!("Average latency improvement: {:.1}%", summary.avg_latency_improvement);
    println!("Average throughput improvement: {:.1}%", summary.avg_throughput_improvement);
    
    if summary.quiccrunch_wins > summary.grpc_wins {
        println!("\n🎉 QuicCrunch is the overall winner!");
    } else if summary.grpc_wins > summary.quiccrunch_wins {
        println!("\n📊 gRPC is the overall winner!");
    } else {
        println!("\n🤝 It's a tie between QuicCrunch and gRPC!");
    }
}