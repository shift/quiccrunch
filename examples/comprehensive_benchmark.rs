/// Comprehensive Real Network Benchmarking
/// Tests all 4 streaming patterns over actual network connections

use std::time::{Duration, Instant};
use quiccrunch_core::benchmark::{
    BenchmarkConfig, StreamingPattern, PerformanceMetrics,
    MESSAGE_SIZES, CONNECTION_COUNTS
};
use quiccrunch_core::benchmark::quiccrunch_client::QuicCrunchClient;
use quiccrunch_core::TlsConfig;
use quiccrunch_core::server::{QuicServer, QuicListener};
use quiccrunch_core::client::QuicClient;
use tokio::task;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🎯 Comprehensive QuicCrunch Network Benchmark");
    println!("=============================================");
    println!();
    println!("Testing all 4 streaming patterns over REAL network connections:");
    println!("• Unary Request/Response");
    println!("• Client Streaming");
    println!("• Server Streaming");
    println!("• Bidirectional Streaming");
    println!();

    // Configuration
    let server_addr = "127.0.0.1:4433";
    let message_sizes = [1024, 65536]; // 1KB and 64KB messages
    let test_duration = Duration::from_secs(10);

    // Start server in background
    println!("🚀 Starting QuicCrunch server on {}...", server_addr);
    let server_handle = task::spawn(async move {
        run_server(server_addr).await
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Run comprehensive benchmarks
    let results = run_comprehensive_benchmarks(server_addr, &message_sizes, test_duration).await?;

    // Display results
    display_results(&results);

    // Stop server
    println!("\n🛑 Shutting down server...");
    server_handle.abort();

    Ok(())
}

async fn run_server(addr: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = QuicServer::new()?;
    let listener = server.listen(addr).await?;

    println!("✅ Server listening on {}", addr);

    // Simple echo handler for benchmarking
    while let Ok(conn) = listener.accept().await {
        println!("📡 New connection from {}", conn.remote_addr()?);

        task::spawn(async move {
            if let Err(e) = handle_connection(conn).await {
                println!("Connection error: {}", e);
            }
        });
    }

    Ok(())
}

async fn handle_connection(conn: quiccrunch_core::transport::Connection) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    loop {
        let stream = match conn.accept_bi().await {
            Ok(stream) => stream,
            Err(_) => break, // Connection closed
        };

        task::spawn(async move {
            if let Err(e) = handle_stream(stream).await {
                println!("Stream error: {}", e);
            }
        });
    }
    Ok(())
}

async fn handle_stream(mut stream: quiccrunch_core::QuicStream) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Simple echo handler - just send back what we receive
    while let Ok(data) = stream.receive_message().await {
        // Echo the data back
        if let Err(e) = stream.send_message(data).await {
            println!("Send error: {}", e);
            break;
        }
    }
    Ok(())
}

async fn run_comprehensive_benchmarks(
    server_addr: &str,
    message_sizes: &[usize],
    test_duration: Duration,
) -> Result<BenchmarkResults, Box<dyn std::error::Error + Send + Sync>> {

    let mut all_results = Vec::new();

    // Test each message size
    for &message_size in message_sizes {
        println!("\n📊 Testing with {} byte messages", message_size);

        // Test Unary Request/Response
        println!("🔄 Testing Unary Request/Response...");
        let unary_config = BenchmarkConfig {
            message_size,
            streaming_pattern: StreamingPattern::Unary,
            concurrent_connections: 10,
            duration_seconds: test_duration.as_secs() as u64,
            warmup_seconds: 2,
            target_qps: None,
        };

        let unary_result = run_single_benchmark(server_addr, &unary_config, "Unary").await?;
        all_results.push(unary_result);

        // Test Client Streaming
        println!("⬆️  Testing Client Streaming...");
        let client_stream_config = BenchmarkConfig {
            message_size,
            streaming_pattern: StreamingPattern::ClientStreaming { message_count: 10 },
            concurrent_connections: 5,
            duration_seconds: test_duration.as_secs() as u64,
            warmup_seconds: 2,
            target_qps: None,
        };

        let client_stream_result = run_single_benchmark(server_addr, &client_stream_config, "Client Streaming").await?;
        all_results.push(client_stream_result);

        // Test Server Streaming
        println!("⬇️  Testing Server Streaming...");
        let server_stream_config = BenchmarkConfig {
            message_size,
            streaming_pattern: StreamingPattern::ServerStreaming { message_count: 10 },
            concurrent_connections: 5,
            duration_seconds: test_duration.as_secs() as u64,
            warmup_seconds: 2,
            target_qps: None,
        };

        let server_stream_result = run_single_benchmark(server_addr, &server_stream_config, "Server Streaming").await?;
        all_results.push(server_stream_result);

        // Test Bidirectional Streaming
        println!("↔️  Testing Bidirectional Streaming...");
        let bidirectional_config = BenchmarkConfig {
            message_size,
            streaming_pattern: StreamingPattern::Bidirectional { message_count: 10 },
            concurrent_connections: 5,
            duration_seconds: test_duration.as_secs() as u64,
            warmup_seconds: 2,
            target_qps: None,
        };

        let bidirectional_result = run_single_benchmark(server_addr, &bidirectional_config, "Bidirectional").await?;
        all_results.push(bidirectional_result);
    }

    Ok(BenchmarkResults {
        test_results: all_results,
        total_duration: Instant::now().elapsed(),
    })
}

async fn run_single_benchmark(
    server_addr: &str,
    config: &BenchmarkConfig,
    pattern_name: &str,
) -> Result<TestResult, Box<dyn std::error::Error + Send + Sync>> {

    let start_time = Instant::now();

    // Create TLS config for client
    let tls_config = TlsConfig::new_client("localhost".to_string(), vec![])?;

    // Create client
    let client = QuicCrunchClient::new(tls_config)?;

    // Run benchmark
    let metrics = client.run_benchmark(config, server_addr).await?;

    let duration = start_time.elapsed();

    println!("   ✅ {} completed in {:.2}s", pattern_name, duration.as_secs_f64());
    println!("      • {:.0} RPS", metrics.throughput.messages_per_second);
    println!("      • {:.2}ms avg latency", metrics.latency.mean_ms);
    println!("      • {:.1} MB/s throughput", metrics.throughput.mbps);

    Ok(TestResult {
        pattern: pattern_name.to_string(),
        message_size: config.message_size,
        metrics,
        duration,
    })
}

fn display_results(results: &BenchmarkResults) {
    println!("\n📊 COMPREHENSIVE BENCHMARK RESULTS");
    println!("==================================");
    println!("Total test time: {:.2}s", results.total_duration.as_secs_f64());
    println!();

    // Group results by message size
    let mut by_size = std::collections::HashMap::new();
    for result in &results.test_results {
        by_size.entry(result.message_size).or_insert_with(Vec::new).push(result);
    }

    for (size, size_results) in by_size {
        println!("📦 {} byte messages:", size);
        println!("{}", "─".repeat(50));

        for result in size_results {
            println!("  {}:", result.pattern);
            println!("    • Throughput: {:.0} RPS ({:.1} MB/s)",
                     result.metrics.throughput.messages_per_second,
                     result.metrics.throughput.mbps);
            println!("    • Latency: {:.2}ms avg, {:.2}ms p95, {:.2}ms p99",
                     result.metrics.latency.mean_ms,
                     result.metrics.latency.p95_ms,
                     result.metrics.latency.p99_ms);
            println!("    • CPU: {:.1}%, Memory: {} bytes",
                     result.metrics.resource_usage.cpu_usage_percent,
                     result.metrics.resource_usage.memory_usage_bytes);
            println!("    • Test duration: {:.2}s", result.duration.as_secs_f64());
            println!();
        }
    }

    // Summary statistics
    println!("📈 SUMMARY STATISTICS");
    println!("====================");

    let total_requests: f64 = results.test_results.iter()
        .map(|r| r.metrics.throughput.messages_per_second * r.duration.as_secs_f64())
        .sum();

    let avg_latency: f64 = results.test_results.iter()
        .map(|r| r.metrics.latency.mean_ms)
        .sum::<f64>() / results.test_results.len() as f64;

    let total_throughput: f64 = results.test_results.iter()
        .map(|r| r.metrics.throughput.mbps)
        .sum::<f64>() / results.test_results.len() as f64;

    println!("• Total requests processed: {:.0}", total_requests);
    println!("• Average latency: {:.2}ms", avg_latency);
    println!("• Average throughput: {:.1} MB/s", total_throughput);
    println!("• All 4 streaming patterns tested successfully ✅");
    println!();
    println!("🎯 REAL NETWORK BENCHMARKING COMPLETE!");
    println!("   This data comes from actual client/server communication over QUIC.");
}

#[derive(Debug)]
struct BenchmarkResults {
    test_results: Vec<TestResult>,
    total_duration: Duration,
}

#[derive(Debug)]
struct TestResult {
    pattern: String,
    message_size: usize,
    metrics: PerformanceMetrics,
    duration: Duration,
}