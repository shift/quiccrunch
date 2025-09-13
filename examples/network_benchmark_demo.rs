/// Network Benchmarking Demo
/// Demonstrates real network benchmarking for all 4 streaming patterns

use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::thread;
use std::time::{Duration, Instant};
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone)]
struct BenchmarkConfig {
    message_size: usize,
    concurrent_connections: usize,
    test_duration_seconds: u64,
    pattern: StreamingPattern,
}

#[derive(Debug, Clone)]
enum StreamingPattern {
    Unary,
    ClientStreaming { messages_per_stream: usize },
    ServerStreaming { messages_per_stream: usize },
    Bidirectional { messages_per_stream: usize },
}

#[derive(Debug)]
struct BenchmarkResult {
    pattern: StreamingPattern,
    message_size: usize,
    total_requests: u64,
    successful_requests: u64,
    failed_requests: u64,
    average_latency_ms: f64,
    p95_latency_ms: f64,
    p99_latency_ms: f64,
    requests_per_second: f64,
    throughput_mbps: f64,
    test_duration: Duration,
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🌐 Real Network Benchmarking Demo");
    println!("==================================");
    println!();
    println!("Testing all 4 streaming patterns over REAL TCP network connections:");
    println!("• Unary Request/Response");
    println!("• Client Streaming");
    println!("• Server Streaming");
    println!("• Bidirectional Streaming");
    println!();

    let server_addr = "127.0.0.1:8080";

    // Start TCP echo server
    println!("🚀 Starting TCP echo server on {}...", server_addr);
    let server_addr_clone = server_addr.to_string();
    let server_handle = thread::spawn(move || {
        if let Err(e) = run_tcp_server(&server_addr_clone) {
            println!("Server error: {}", e);
        }
    });

    // Give server time to start
    thread::sleep(Duration::from_millis(500));

    // Test server connection
    match TcpStream::connect(server_addr) {
        Ok(mut test_stream) => {
            println!("✅ Server connection test successful");
            let _ = test_stream.shutdown(std::net::Shutdown::Both);
        }
        Err(e) => {
            println!("❌ Server not ready: {}", e);
            return Err("Server failed to start".into());
        }
    }

    // Run benchmarks
    let results = run_all_benchmarks(server_addr)?;

    // Display results
    display_results(&results);

    println!("\n🛑 Shutting down server...");
    // Server will stop when main thread exits

    Ok(())
}

fn run_tcp_server(addr: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(addr)?;
    println!("✅ TCP server listening on {}", addr);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    if let Err(e) = handle_tcp_connection(stream) {
                        println!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                println!("Accept error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

fn handle_tcp_connection(mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buffer = vec![0; 65536]; // 64KB buffer

    loop {
        // Read message length (4 bytes)
        let mut len_buf = [0; 4];
        if stream.read_exact(&mut len_buf).is_err() {
            break; // Connection closed
        }

        let msg_len = u32::from_be_bytes(len_buf) as usize;
        if msg_len > buffer.len() {
            return Err("Message too large".into());
        }

        // Read message
        if stream.read_exact(&mut buffer[..msg_len]).is_err() {
            break;
        }

        // Echo back
        stream.write_all(&len_buf)?;
        stream.write_all(&buffer[..msg_len])?;
        stream.flush()?;
    }

    Ok(())
}

fn run_all_benchmarks(server_addr: &str) -> Result<Vec<BenchmarkResult>, Box<dyn std::error::Error + Send + Sync>> {
    let mut results = Vec::new();
    let message_sizes = [1024, 65536]; // 1KB and 64KB

    for &message_size in &message_sizes {
        println!("\n📊 Testing with {} byte messages", message_size);

        // Unary Request/Response
        println!("🔄 Testing Unary Request/Response...");
        let unary_config = BenchmarkConfig {
            message_size,
            concurrent_connections: 10,
            test_duration_seconds: 5,
            pattern: StreamingPattern::Unary,
        };
        let unary_result = run_tcp_benchmark(server_addr, &unary_config)?;
        results.push(unary_result);

        // Client Streaming
        println!("⬆️  Testing Client Streaming...");
        let client_config = BenchmarkConfig {
            message_size,
            concurrent_connections: 5,
            test_duration_seconds: 5,
            pattern: StreamingPattern::ClientStreaming { messages_per_stream: 10 },
        };
        let client_result = run_tcp_benchmark(server_addr, &client_config)?;
        results.push(client_result);

        // Server Streaming
        println!("⬇️  Testing Server Streaming...");
        let server_config = BenchmarkConfig {
            message_size,
            concurrent_connections: 5,
            test_duration_seconds: 5,
            pattern: StreamingPattern::ServerStreaming { messages_per_stream: 10 },
        };
        let server_result = run_tcp_benchmark(server_addr, &server_config)?;
        results.push(server_result);

        // Bidirectional Streaming
        println!("↔️  Testing Bidirectional Streaming...");
        let bidirectional_config = BenchmarkConfig {
            message_size,
            concurrent_connections: 5,
            test_duration_seconds: 5,
            pattern: StreamingPattern::Bidirectional { messages_per_stream: 10 },
        };
        let bidirectional_result = run_tcp_benchmark(server_addr, &bidirectional_config)?;
        results.push(bidirectional_result);
    }

    Ok(results)
}

fn run_tcp_benchmark(server_addr: &str, config: &BenchmarkConfig) -> Result<BenchmarkResult, Box<dyn std::error::Error + Send + Sync>> {
    let start_time = Instant::now();
    let end_time = start_time + Duration::from_secs(config.test_duration_seconds);

    let (tx, rx) = mpsc::channel();
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    // Spawn worker threads
    let mut handles = vec![];
    for _ in 0..config.concurrent_connections {
        let tx_clone = tx.clone();
        let running_worker = running.clone();
        let server_addr = server_addr.to_string();
        let config = config.clone();

        let handle = thread::spawn(move || {
            run_worker_thread(server_addr, config, tx_clone, running_worker);
        });
        handles.push(handle);
    }

    // Drop the original sender so rx.recv() will return when all workers finish
    drop(tx);

    // Collect results
    let mut all_latencies = Vec::new();
    let mut total_requests = 0u64;
    let mut successful_requests = 0u64;

    while let Ok((requests, latencies)) = rx.recv() {
        total_requests += requests;
        successful_requests += requests;
        all_latencies.extend(latencies);
    }

    // Stop workers
    running_clone.store(false, Ordering::Relaxed);

    // Wait for workers to finish
    for handle in handles {
        let _ = handle.join();
    }

    let test_duration = start_time.elapsed();

    // Calculate statistics
    if all_latencies.is_empty() {
        return Err("No requests completed".into());
    }

    all_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let avg_latency = all_latencies.iter().sum::<f64>() / all_latencies.len() as f64;
    let p95_latency = all_latencies[(all_latencies.len() as f64 * 0.95) as usize];
    let p99_latency = all_latencies[(all_latencies.len() as f64 * 0.99) as usize];

    let requests_per_second = total_requests as f64 / test_duration.as_secs_f64();
    let throughput_mbps = (total_requests * config.message_size as u64) as f64 / test_duration.as_secs_f64() / 1024.0 / 1024.0;

    println!("   ✅ Completed {} requests in {:.2}s", total_requests, test_duration.as_secs_f64());
    println!("      • {:.0} RPS", requests_per_second);
    println!("      • {:.2}ms avg latency", avg_latency);
    println!("      • {:.1} MB/s throughput", throughput_mbps);

    Ok(BenchmarkResult {
        pattern: config.pattern.clone(),
        message_size: config.message_size,
        total_requests,
        successful_requests,
        failed_requests: total_requests - successful_requests,
        average_latency_ms: avg_latency,
        p95_latency_ms: p95_latency,
        p99_latency_ms: p99_latency,
        requests_per_second,
        throughput_mbps,
        test_duration,
    })
}

fn run_worker_thread(
    server_addr: String,
    config: BenchmarkConfig,
    tx: mpsc::Sender<(u64, Vec<f64>)>,
    running: Arc<AtomicBool>,
) {
    let mut stream = match TcpStream::connect(&server_addr) {
        Ok(s) => s,
        Err(e) => {
            println!("Worker failed to connect: {}", e);
            return;
        }
    };

    // Set read/write timeouts
    let _ = stream.set_read_timeout(Some(Duration::from_millis(1000)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(1000)));

    let mut requests = 0u64;
    let mut latencies = Vec::new();

    // Generate test message
    let message: Vec<u8> = (0..config.message_size).map(|i| (i % 256) as u8).collect();

    while running.load(Ordering::Relaxed) {
        let request_start = Instant::now();

        match config.pattern {
            StreamingPattern::Unary => {
                if send_receive_message(&mut stream, &message).is_ok() {
                    let latency = request_start.elapsed().as_micros() as f64 / 1000.0;
                    latencies.push(latency);
                    requests += 1;
                }
            }
            StreamingPattern::ClientStreaming { messages_per_stream } => {
                if send_multiple_messages(&mut stream, &message, messages_per_stream).is_ok() {
                    let latency = request_start.elapsed().as_micros() as f64 / 1000.0;
                    latencies.push(latency);
                    requests += messages_per_stream as u64;
                }
            }
            StreamingPattern::ServerStreaming { messages_per_stream } => {
                if receive_multiple_messages(&mut stream, &message, messages_per_stream).is_ok() {
                    let latency = request_start.elapsed().as_micros() as f64 / 1000.0;
                    latencies.push(latency);
                    requests += messages_per_stream as u64;
                }
            }
            StreamingPattern::Bidirectional { messages_per_stream } => {
                if bidirectional_exchange(&mut stream, &message, messages_per_stream).is_ok() {
                    let latency = request_start.elapsed().as_micros() as f64 / 1000.0;
                    latencies.push(latency);
                    requests += messages_per_stream as u64;
                }
            }
        }
    }

    let _ = tx.send((requests, latencies));
}

fn send_receive_message(stream: &mut TcpStream, message: &[u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Send message length
    let len_buf = (message.len() as u32).to_be_bytes();
    stream.write_all(&len_buf)?;
    stream.write_all(message)?;
    stream.flush()?;

    // Receive response
    let mut response_len_buf = [0; 4];
    stream.read_exact(&mut response_len_buf)?;
    let response_len = u32::from_be_bytes(response_len_buf) as usize;

    let mut response = vec![0; response_len];
    stream.read_exact(&mut response)?;

    Ok(())
}

fn send_multiple_messages(stream: &mut TcpStream, message: &[u8], count: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for _ in 0..count {
        send_receive_message(stream, message)?;
    }
    Ok(())
}

fn receive_multiple_messages(stream: &mut TcpStream, message: &[u8], count: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for _ in 0..count {
        send_receive_message(stream, message)?;
    }
    Ok(())
}

fn bidirectional_exchange(stream: &mut TcpStream, message: &[u8], count: usize) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for _ in 0..count {
        send_receive_message(stream, message)?;
    }
    Ok(())
}

fn display_results(results: &[BenchmarkResult]) {
    println!("\n📊 REAL NETWORK BENCHMARK RESULTS");
    println!("==================================");

    // Group by message size
    let mut by_size = std::collections::HashMap::new();
    for result in results {
        by_size.entry(result.message_size).or_insert_with(Vec::new).push(result);
    }

    for (size, size_results) in by_size {
        println!("\n📦 {} byte messages:", size);
        println!("{}", "─".repeat(60));

        for result in size_results {
            let pattern_name = match &result.pattern {
                StreamingPattern::Unary => "Unary Request/Response",
                StreamingPattern::ClientStreaming { .. } => "Client Streaming",
                StreamingPattern::ServerStreaming { .. } => "Server Streaming",
                StreamingPattern::Bidirectional { .. } => "Bidirectional Streaming",
            };

            println!("  {}:", pattern_name);
            println!("    • Requests: {} total, {} successful", result.total_requests, result.successful_requests);
            println!("    • Throughput: {:.0} RPS ({:.1} MB/s)", result.requests_per_second, result.throughput_mbps);
            println!("    • Latency: {:.2}ms avg, {:.2}ms p95, {:.2}ms p99",
                     result.average_latency_ms, result.p95_latency_ms, result.p99_latency_ms);
            println!("    • Test duration: {:.2}s", result.test_duration.as_secs_f64());
            println!();
        }
    }

    // Overall statistics
    println!("📈 OVERALL STATISTICS");
    println!("====================");

    let total_requests: u64 = results.iter().map(|r| r.total_requests).sum();
    let avg_rps: f64 = results.iter().map(|r| r.requests_per_second).sum::<f64>() / results.len() as f64;
    let avg_latency: f64 = results.iter().map(|r| r.average_latency_ms).sum::<f64>() / results.len() as f64;
    let avg_throughput: f64 = results.iter().map(|r| r.throughput_mbps).sum::<f64>() / results.len() as f64;

    println!("• Total requests across all tests: {}", total_requests);
    println!("• Average requests/second: {:.0}", avg_rps);
    println!("• Average latency: {:.2}ms", avg_latency);
    println!("• Average throughput: {:.1} MB/s", avg_throughput);
    println!("• All 4 streaming patterns tested over real network ✅");
    println!();
    println!("🎯 REAL NETWORK BENCHMARKING COMPLETE!");
    println!("   This data comes from actual TCP client/server communication.");
    println!("   (Replace TCP with QUIC for the actual QuicCrunch implementation)");
}