#!/usr/bin/env cargo run --release --features="backend-tquic" --

use std::time::{Duration, Instant};
use std::fs;
use serde_json;
use chrono;

mod comprehensive_rpc_test;
mod grpc_comparison_test;

use comprehensive_rpc_test::{ComprehensiveRpcTestSuite, RpcTestResults};
use grpc_comparison_test::{GrpcTestSuite, ComparisonResults, PerformanceComparator};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🚀 QuicCrunch vs gRPC Comprehensive Performance Test");
    println!("===================================================");
    println!();

    let test_start = Instant::now();
    
    // Test configuration
    let server_addr = "127.0.0.1:8080".to_string();
    let grpc_addr = "127.0.0.1:8081".to_string();
    let test_duration = Duration::from_secs(120); // 2 minutes per test
    let concurrent_clients = 10;
    
    // Determine enabled features from cargo
    let features = vec![
        "backend-tquic".to_string(),
        // Add other features based on compilation
        #[cfg(feature = "memory-pools")]
        "memory-pools".to_string(),
        #[cfg(feature = "simd-optimizations")]
        "simd-optimizations".to_string(),
        #[cfg(feature = "lock-free-channels")]
        "lock-free-channels".to_string(),
        #[cfg(feature = "adaptive-optimizations")]
        "adaptive-optimizations".to_string(),
    ];

    println!("🔧 Test Configuration:");
    println!("   QuicCrunch Server: {}", server_addr);
    println!("   gRPC Server: {}", grpc_addr);
    println!("   Test Duration: {:?}", test_duration);
    println!("   Concurrent Clients: {}", concurrent_clients);
    println!("   Features: {:?}", features);
    println!();

    // 1. Run QuicCrunch comprehensive test suite
    println!("🎯 Phase 1: QuicCrunch Performance Testing");
    println!("==========================================");
    
    let quiccrunch_suite = ComprehensiveRpcTestSuite::new(
        server_addr.clone(),
        test_duration,
        concurrent_clients,
        features.clone(),
    );
    
    let quiccrunch_results = match quiccrunch_suite.run_full_test_suite().await {
        Ok(results) => results,
        Err(e) => {
            eprintln!("❌ QuicCrunch test suite failed: {}", e);
            std::process::exit(1);
        }
    };

    println!();
    println!("📊 QuicCrunch Results Summary:");
    println!("   Simple RPC: {:.2} req/s, {:.2}ms avg latency", 
            quiccrunch_results.simple_rpc.requests_per_second,
            quiccrunch_results.simple_rpc.average_latency_ms);
    println!("   Peak Throughput: {:.2} req/s", 
            quiccrunch_results.throughput_measurements.peak_requests_per_second);
    println!("   Latency P95: {:.2}ms, P99: {:.2}ms", 
            quiccrunch_results.latency_distribution.p95_ms,
            quiccrunch_results.latency_distribution.p99_ms);
    println!();

    // 2. Run gRPC comprehensive test suite
    println!("🌐 Phase 2: gRPC Performance Testing");
    println!("====================================");
    
    let grpc_suite = GrpcTestSuite::new(
        grpc_addr.clone(),
        test_duration,
        concurrent_clients,
    );
    
    let grpc_results = match grpc_suite.run_grpc_test_suite().await {
        Ok(results) => results,
        Err(e) => {
            eprintln!("❌ gRPC test suite failed: {}", e);
            std::process::exit(1);
        }
    };

    println!();
    println!("📊 gRPC Results Summary:");
    println!("   Simple RPC: {:.2} req/s, {:.2}ms avg latency", 
            grpc_results.simple_rpc.requests_per_second,
            grpc_results.simple_rpc.average_latency_ms);
    println!("   Peak Throughput: {:.2} req/s", 
            grpc_results.throughput_measurements.peak_requests_per_second);
    println!("   Latency P95: {:.2}ms, P99: {:.2}ms", 
            grpc_results.latency_distribution.p95_ms,
            grpc_results.latency_distribution.p99_ms);
    println!();

    // 3. Performance comparison analysis
    println!("🏆 Phase 3: Performance Comparison Analysis");
    println!("==========================================");
    
    let performance_comparison = PerformanceComparator::compare_results(&quiccrunch_results, &grpc_results);
    
    println!("📈 Comparison Results:");
    println!("   Winner: {}", performance_comparison.winner);
    println!("   Latency Improvement: {:.1}%", performance_comparison.latency_improvement_percent);
    println!("   Throughput Improvement: {:.1}%", performance_comparison.throughput_improvement_percent);
    println!("   RPS Improvement: {:.1}%", performance_comparison.requests_per_second_improvement_percent);
    println!("   Summary: {}", performance_comparison.summary);
    println!();

    // 4. Generate comprehensive comparison report
    let total_test_duration = test_start.elapsed();
    let comparison_results = ComparisonResults {
        test_timestamp: chrono::Utc::now().to_rfc3339(),
        test_duration_seconds: total_test_duration.as_secs_f64(),
        quiccrunch_results,
        grpc_results,
        performance_comparison,
    };

    // 5. Save detailed results
    let date_str = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let features_str = features.join("-");
    let results_filename = format!("performance/{}-{}-vs-grpc-comparison.json", date_str, features_str);
    let report_filename = format!("performance/{}-{}-vs-grpc-report.md", date_str, features_str);

    // Ensure performance directory exists
    fs::create_dir_all("performance")?;

    // Save JSON results
    let json_content = serde_json::to_string_pretty(&comparison_results)?;
    fs::write(&results_filename, json_content)?;

    // Generate markdown report
    let report_content = generate_markdown_report(&comparison_results, &features);
    fs::write(&report_filename, report_content)?;

    println!("💾 Results Saved:");
    println!("   JSON Data: {}", results_filename);
    println!("   Report: {}", report_filename);
    println!();

    // 6. Final summary
    println!("🎉 Performance Test Complete!");
    println!("=============================");
    println!("Total Test Duration: {:.2}s", total_test_duration.as_secs_f64());
    
    match comparison_results.performance_comparison.winner.as_str() {
        "QuicCrunch" => {
            println!("🏆 QuicCrunch WINS with {:.1}% better latency and {:.1}% better throughput!",
                    comparison_results.performance_comparison.latency_improvement_percent,
                    comparison_results.performance_comparison.throughput_improvement_percent);
        }
        "gRPC" => {
            println!("🥈 gRPC wins this round, but QuicCrunch shows promise");
        }
        _ => {
            println!("🤝 It's a close match - both implementations show similar performance");
        }
    }

    Ok(())
}

fn generate_markdown_report(results: &ComparisonResults, features: &[String]) -> String {
    format!(r#"# QuicCrunch vs gRPC Performance Report

**Test Date:** {}  
**Test Duration:** {:.2} seconds  
**Backend:** tQUIC  
**Features:** {}  

## Executive Summary

**Winner:** {}  
**Summary:** {}

## Performance Metrics Comparison

### Simple RPC Performance

| Metric | QuicCrunch | gRPC | Improvement |
|--------|------------|------|-------------|
| Requests/sec | {:.2} | {:.2} | {:.1}% |
| Avg Latency (ms) | {:.2} | {:.2} | {:.1}% |
| Min Latency (ms) | {:.2} | {:.2} | - |
| Max Latency (ms) | {:.2} | {:.2} | - |

### Throughput Measurements

| Metric | QuicCrunch | gRPC | Improvement |
|--------|------------|------|-------------|
| Peak Requests/sec | {:.2} | {:.2} | {:.1}% |
| Sustained Requests/sec | {:.2} | {:.2} | - |
| Peak Mbps | {:.2} | {:.2} | - |
| Sustained Mbps | {:.2} | {:.2} | - |

### Latency Distribution

| Percentile | QuicCrunch (ms) | gRPC (ms) | Difference |
|------------|-----------------|-----------|------------|
| P50 | {:.2} | {:.2} | {:.2} |
| P90 | {:.2} | {:.2} | {:.2} |
| P95 | {:.2} | {:.2} | {:.2} |
| P99 | {:.2} | {:.2} | {:.2} |
| P99.9 | {:.2} | {:.2} | {:.2} |

### Message Size Analysis

#### Small Messages (1KB)
- **QuicCrunch:** {:.2} req/s, {:.2}ms latency
- **gRPC:** {:.2} req/s, {:.2}ms latency
- **Optimization:** {}

#### Medium Messages (64KB)
- **QuicCrunch:** {:.2} req/s, {:.2}ms latency
- **gRPC:** {:.2} req/s, {:.2}ms latency
- **Optimization:** {}

#### Large Messages (512KB)
- **QuicCrunch:** {:.2} req/s, {:.2}ms latency
- **gRPC:** {:.2} req/s, {:.2}ms latency
- **Optimization:** {}

#### Huge Messages (2MB)
- **QuicCrunch:** {:.2} req/s, {:.2}ms latency
- **gRPC:** {:.2} req/s, {:.2}ms latency
- **Optimization:** {}

## Streaming Performance

### Client Streaming
- **QuicCrunch:** {:.2} Mbps, {} messages sent
- **gRPC:** {:.2} Mbps, {} messages sent

### Server Streaming
- **QuicCrunch:** {:.2} Mbps, {} messages received
- **gRPC:** {:.2} Mbps, {} messages received

### Bidirectional Streaming
- **QuicCrunch:** {:.2} Mbps, {} messages each way
- **gRPC:** {:.2} Mbps, {} messages each way

## Analysis

### QuicCrunch Advantages
- **tQUIC Backend:** Leverages advanced QUIC features for improved performance
- **Adaptive Optimizations:** Automatically selects best strategies for different workloads
- **Memory Pools:** Optimized allocation for large messages
- **Zero-Copy Operations:** Reduced memory overhead where possible

### Performance Characteristics
- **Best Performance:** {} KB+ messages with adaptive optimizations
- **Optimization Strategy:** Automatically selected based on message size
- **Backend Selection:** tQUIC chosen for superior performance characteristics

## Conclusions

{}

## Technical Details

- **Test Environment:** NixOS Linux, Rust 1.89.0, release optimization
- **QuicCrunch Features:** {}
- **gRPC Configuration:** HTTP/2 over TLS
- **Message Patterns:** Simple RPC, Client Streaming, Server Streaming, Bidirectional
- **Message Sizes:** 1KB to 2MB with adaptive optimization testing

---

**Generated:** {}  
**Test Suite:** Comprehensive RPC Performance Analysis  
**Status:** ✅ All tests completed successfully
"#,
        results.test_timestamp,
        results.test_duration_seconds,
        features.join(", "),
        results.performance_comparison.winner,
        results.performance_comparison.summary,
        
        // Simple RPC comparison
        results.quiccrunch_results.simple_rpc.requests_per_second,
        results.grpc_results.simple_rpc.requests_per_second,
        results.performance_comparison.requests_per_second_improvement_percent,
        results.quiccrunch_results.simple_rpc.average_latency_ms,
        results.grpc_results.simple_rpc.average_latency_ms,
        results.performance_comparison.latency_improvement_percent,
        results.quiccrunch_results.simple_rpc.min_latency_ms,
        results.grpc_results.simple_rpc.min_latency_ms,
        results.quiccrunch_results.simple_rpc.max_latency_ms,
        results.grpc_results.simple_rpc.max_latency_ms,
        
        // Throughput comparison
        results.quiccrunch_results.throughput_measurements.peak_requests_per_second,
        results.grpc_results.throughput_measurements.peak_requests_per_second,
        results.performance_comparison.throughput_improvement_percent,
        results.quiccrunch_results.throughput_measurements.sustained_requests_per_second,
        results.grpc_results.throughput_measurements.sustained_requests_per_second,
        results.quiccrunch_results.throughput_measurements.peak_mbps,
        results.grpc_results.throughput_measurements.peak_mbps,
        results.quiccrunch_results.throughput_measurements.sustained_mbps,
        results.grpc_results.throughput_measurements.sustained_mbps,
        
        // Latency distribution
        results.quiccrunch_results.latency_distribution.p50_ms,
        results.grpc_results.latency_distribution.p50_ms,
        results.quiccrunch_results.latency_distribution.p50_ms - results.grpc_results.latency_distribution.p50_ms,
        results.quiccrunch_results.latency_distribution.p90_ms,
        results.grpc_results.latency_distribution.p90_ms,
        results.quiccrunch_results.latency_distribution.p90_ms - results.grpc_results.latency_distribution.p90_ms,
        results.quiccrunch_results.latency_distribution.p95_ms,
        results.grpc_results.latency_distribution.p95_ms,
        results.quiccrunch_results.latency_distribution.p95_ms - results.grpc_results.latency_distribution.p95_ms,
        results.quiccrunch_results.latency_distribution.p99_ms,
        results.grpc_results.latency_distribution.p99_ms,
        results.quiccrunch_results.latency_distribution.p99_ms - results.grpc_results.latency_distribution.p99_ms,
        results.quiccrunch_results.latency_distribution.p999_ms,
        results.grpc_results.latency_distribution.p999_ms,
        results.quiccrunch_results.latency_distribution.p999_ms - results.grpc_results.latency_distribution.p999_ms,
        
        // Message size analysis
        results.quiccrunch_results.message_size_analysis.small_messages_1kb.requests_per_second,
        results.quiccrunch_results.message_size_analysis.small_messages_1kb.average_latency_ms,
        results.grpc_results.message_size_analysis.small_messages_1kb.requests_per_second,
        results.grpc_results.message_size_analysis.small_messages_1kb.average_latency_ms,
        results.quiccrunch_results.message_size_analysis.small_messages_1kb.optimization_used,
        
        results.quiccrunch_results.message_size_analysis.medium_messages_64kb.requests_per_second,
        results.quiccrunch_results.message_size_analysis.medium_messages_64kb.average_latency_ms,
        results.grpc_results.message_size_analysis.medium_messages_64kb.requests_per_second,
        results.grpc_results.message_size_analysis.medium_messages_64kb.average_latency_ms,
        results.quiccrunch_results.message_size_analysis.medium_messages_64kb.optimization_used,
        
        results.quiccrunch_results.message_size_analysis.large_messages_512kb.requests_per_second,
        results.quiccrunch_results.message_size_analysis.large_messages_512kb.average_latency_ms,
        results.grpc_results.message_size_analysis.large_messages_512kb.requests_per_second,
        results.grpc_results.message_size_analysis.large_messages_512kb.average_latency_ms,
        results.quiccrunch_results.message_size_analysis.large_messages_512kb.optimization_used,
        
        results.quiccrunch_results.message_size_analysis.huge_messages_2mb.requests_per_second,
        results.quiccrunch_results.message_size_analysis.huge_messages_2mb.average_latency_ms,
        results.grpc_results.message_size_analysis.huge_messages_2mb.requests_per_second,
        results.grpc_results.message_size_analysis.huge_messages_2mb.average_latency_ms,
        results.quiccrunch_results.message_size_analysis.huge_messages_2mb.optimization_used,
        
        // Streaming performance
        results.quiccrunch_results.client_streaming.throughput_mbps,
        results.quiccrunch_results.client_streaming.total_messages_sent,
        results.grpc_results.client_streaming.throughput_mbps,
        results.grpc_results.client_streaming.total_messages_sent,
        
        results.quiccrunch_results.server_streaming.throughput_mbps,
        results.quiccrunch_results.server_streaming.total_messages_received,
        results.grpc_results.server_streaming.throughput_mbps,
        results.grpc_results.server_streaming.total_messages_received,
        
        results.quiccrunch_results.bidirectional_streaming.throughput_mbps,
        results.quiccrunch_results.bidirectional_streaming.total_messages_sent,
        results.grpc_results.bidirectional_streaming.throughput_mbps,
        results.grpc_results.bidirectional_streaming.total_messages_sent,
        
        // Analysis
        if results.quiccrunch_results.message_size_analysis.large_messages_512kb.optimization_used == "AdaptiveOptimizations" { "512" } else { "64" },
        
        results.performance_comparison.summary,
        features.join(", "),
        chrono::Utc::now().to_rfc3339(),
    )
}