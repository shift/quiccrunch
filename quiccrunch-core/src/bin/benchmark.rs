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

use clap::{Parser, Subcommand};
use quiccrunch_core::{QuicTransport, Transport, TlsConfig};
#[cfg(feature = "grpc-comparison")]
use quiccrunch_core::transport::grpc::GrpcTransport;
use bytes::Bytes;
use std::time::{Duration, Instant};
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use hdrhistogram::Histogram;
use serde_json;
use chrono::{DateTime, Utc};

#[path = "../../examples/cert_utils.rs"]
mod cert_utils;

#[derive(Parser)]
#[command(name = "benchmark")]
#[command(about = "QUIC + Cap'n Proto Performance Test Suite")]
#[command(version = "1.0")]
struct Args {
    #[command(subcommand)]
    mode: Mode,
    
    /// Bind address for server mode or target address for client mode
    #[arg(short = 'a', long, default_value = "127.0.0.1:50055")]
    address: String,
    
    /// Transport protocol to use (quic or grpc)
    #[arg(short = 't', long, default_value = "quic")]
    transport: String,
    
    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Mode {
    /// Run as server
    Server,
    /// Run as client with specific test
    Client {
        #[command(subcommand)]
        test: TestCase,
    },
}

#[derive(Subcommand)]
enum TestCase {
    /// Test Case 1: RPC Ping-Pong (Latency measurement)
    Latency {
        /// Number of ping-pong iterations
        #[arg(short = 'n', long, default_value = "10000")]
        iterations: u32,
        
        /// Message size in bytes
        #[arg(short = 's', long, default_value = "64")]
        message_size: usize,
    },
    /// Test Case 2: RPC Flood (Message throughput)
    Flood {
        /// Test duration in seconds
        #[arg(short = 'd', long, default_value = "10")]
        duration: u64,
        
        /// Number of concurrent client tasks
        #[arg(short = 'c', long, default_value = "100")]
        concurrency: usize,
        
        /// Message size in bytes
        #[arg(short = 's', long, default_value = "64")]
        message_size: usize,
    },
    /// Test Case 3: Bulk Data Streaming (Bandwidth measurement)
    Stream {
        /// Data size to transfer (in GiB)
        #[arg(short = 's', long, default_value = "1")]
        size_gib: f64,
        
        /// Chunk size for streaming (in bytes)
        #[arg(short = 'c', long, default_value = "65536")]
        chunk_size: usize,
    },
}

#[derive(serde::Serialize)]
struct TestResults {
    timestamp: DateTime<Utc>,
    backend: String,
    test_case: String,
    configuration: serde_json::Value,
    metrics: serde_json::Value,
}

#[derive(serde::Serialize)]
struct LatencyMetrics {
    iterations: u32,
    message_size: usize,
    mean_us: f64,
    median_us: f64,
    p90_us: f64,
    p95_us: f64,
    p99_us: f64,
    max_us: f64,
    min_us: f64,
    histogram: Vec<(f64, u64)>, // (value_us, count)
}

#[derive(serde::Serialize)]
struct FloodMetrics {
    duration_s: f64,
    concurrency: usize,
    message_size: usize,
    total_messages: u64,
    messages_per_second: f64,
    total_bytes: u64,
    throughput_mbps: f64,
    mean_latency_us: f64,
    p99_latency_us: f64,
}

#[derive(serde::Serialize)]
struct StreamMetrics {
    size_gib: f64,
    chunk_size: usize,
    total_bytes: u64,
    duration_s: f64,
    throughput_gbps: f64,
    chunks_transferred: u64,
    chunk_rate_per_s: f64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    
    if args.verbose {
        #[cfg(feature = "telemetry")]
        tracing_subscriber::fmt::init();
    }
    
    // Generate certificates
    let (ca_cert, _ca_key, ca_cert_obj) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert_obj, "127.0.0.1")?;
    
    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };
    
    match args.mode {
        Mode::Server => run_server(tls_config, &args.address, &args.transport, args.verbose).await,
        Mode::Client { test } => run_client(tls_config, &args.address, &args.transport, test, args.verbose).await,
    }
}

async fn run_server(
    tls_config: TlsConfig,
    addr: &str,
    transport_type: &str,
    verbose: bool,
) -> Result<(), Box<dyn Error>> {
    let transport: Box<dyn Transport> = match transport_type {
        #[cfg(feature = "grpc-comparison")]
        "grpc" => Box::new(GrpcTransport::new(tls_config)?),
        #[cfg(not(feature = "grpc-comparison"))]
        "grpc" => {
            eprintln!("❌ gRPC comparison not enabled. Build with --features grpc-comparison");
            std::process::exit(1);
        },
        "quic" | _ => Box::new(QuicTransport::new(tls_config)?),
    };
    
    let mut incoming = transport.listen(addr).await?;
    
    eprintln!("🚀 QuicCrunch Benchmark Server");
    eprintln!("==============================");
    eprintln!("Listening on: {}", addr);
    eprintln!("Protocol: {}", transport_type.to_uppercase());
    eprintln!("Backend: {}", get_backend_name());
    eprintln!("Ready for connections...\n");
    
    // Accept connections and handle streams
    while let Ok(conn) = incoming.accept().await {
        if verbose {
            eprintln!("📡 New connection accepted");
        }
        
        tokio::spawn(async move {
            loop {
                match conn.accept_bi().await {
                    Ok(mut stream) => {
                        tokio::spawn(async move {
                            // Echo server: receive message and send it back
                            while let Ok(msg) = stream.receive_message().await {
                                if let Err(e) = stream.send_message(msg).await {
                                    if verbose {
                                        eprintln!("Failed to echo message: {}", e);
                                    }
                                    break;
                                }
                            }
                        });
                    }
                    Err(_) => break,
                }
            }
        });
    }
    
    Ok(())
}

async fn run_client(
    tls_config: TlsConfig,
    addr: &str,
    transport_type: &str,
    test: TestCase,
    verbose: bool,
) -> Result<(), Box<dyn Error>> {
    let transport: Box<dyn Transport> = match transport_type {
        #[cfg(feature = "grpc-comparison")]
        "grpc" => Box::new(GrpcTransport::new(tls_config)?),
        #[cfg(not(feature = "grpc-comparison"))]
        "grpc" => {
            eprintln!("❌ gRPC comparison not enabled. Build with --features grpc-comparison");
            std::process::exit(1);
        },
        "quic" | _ => Box::new(QuicTransport::new(tls_config)?),
    };
    
    eprintln!("🚀 QuicCrunch Benchmark Client");
    eprintln!("==============================");
    eprintln!("Target: {}", addr);
    eprintln!("Protocol: {}", transport_type.to_uppercase());
    eprintln!("Backend: {}", get_backend_name());
    
    let results = match test {
        TestCase::Latency { iterations, message_size } => {
            eprintln!("Test: RPC Ping-Pong (Latency)");
            eprintln!("Iterations: {}", iterations);
            eprintln!("Message size: {} bytes\n", message_size);
            
            let metrics = run_latency_test(&*transport, addr, iterations, message_size, verbose).await?;
            
            TestResults {
                timestamp: Utc::now(),
                backend: format!("{}-{}", transport_type, get_backend_name()),
                test_case: "latency".to_string(),
                configuration: serde_json::json!({
                    "iterations": iterations,
                    "message_size": message_size
                }),
                metrics: serde_json::to_value(metrics)?,
            }
        }
        TestCase::Flood { duration, concurrency, message_size } => {
            eprintln!("Test: RPC Flood (Throughput)");
            eprintln!("Duration: {}s", duration);
            eprintln!("Concurrency: {} tasks", concurrency);
            eprintln!("Message size: {} bytes\n", message_size);
            
            let metrics = run_flood_test(&*transport, addr, Duration::from_secs(duration), concurrency, message_size, verbose).await?;
            
            TestResults {
                timestamp: Utc::now(),
                backend: format!("{}-{}", transport_type, get_backend_name()),
                test_case: "flood".to_string(),
                configuration: serde_json::json!({
                    "duration_s": duration,
                    "concurrency": concurrency,
                    "message_size": message_size
                }),
                metrics: serde_json::to_value(metrics)?,
            }
        }
        TestCase::Stream { size_gib, chunk_size } => {
            eprintln!("Test: Bulk Data Streaming (Bandwidth)");
            eprintln!("Data size: {:.2} GiB", size_gib);
            eprintln!("Chunk size: {} bytes\n", chunk_size);
            
            let metrics = run_stream_test(&*transport, addr, size_gib, chunk_size, verbose).await?;
            
            TestResults {
                timestamp: Utc::now(),
                backend: format!("{}-{}", transport_type, get_backend_name()),
                test_case: "stream".to_string(),
                configuration: serde_json::json!({
                    "size_gib": size_gib,
                    "chunk_size": chunk_size
                }),
                metrics: serde_json::to_value(metrics)?,
            }
        }
    };
    
    // Output results as JSON
    println!("{}", serde_json::to_string_pretty(&results)?);
    
    Ok(())
}

async fn run_latency_test(
    transport: &dyn Transport,
    addr: &str,
    iterations: u32,
    message_size: usize,
    verbose: bool,
) -> Result<LatencyMetrics, Box<dyn Error>> {
    let client = transport.connect(addr).await?;
    let mut stream = client.open_bi().await?;
    
    let message_data = vec![42u8; message_size];
    let message = Bytes::from(message_data);
    
    let mut histogram = Histogram::<u64>::new(5)?; // 5 significant figures
    let mut min_latency = Duration::from_secs(3600); // Start with 1 hour
    let mut max_latency = Duration::ZERO;
    let mut total_latency = Duration::ZERO;
    
    eprintln!("Starting latency test...");
    
    let start_time = Instant::now();
    
    for i in 0..iterations {
        let iter_start = Instant::now();
        
        // Send message
        stream.send_message(message.clone()).await?;
        
        // Receive echo
        let response = stream.receive_message().await?;
        
        let latency = iter_start.elapsed();
        
        // Verify response
        if response.len() != message.len() {
            return Err("Response size mismatch".into());
        }
        
        // Update statistics
        let latency_us = latency.as_micros() as u64;
        histogram.record(latency_us)?;
        
        if latency < min_latency {
            min_latency = latency;
        }
        if latency > max_latency {
            max_latency = latency;
        }
        total_latency += latency;
        
        // Progress reporting
        if verbose && i % 1000 == 0 && i > 0 {
            eprintln!("Progress: {}/{} iterations", i, iterations);
        }
    }
    
    let total_time = start_time.elapsed();
    let mean_latency = total_latency / iterations;
    
    eprintln!("✅ Latency test completed in {:.2}s", total_time.as_secs_f64());
    
    // Create histogram data for JSON output
    let mut histogram_data = Vec::new();
    for bucket in histogram.iter_recorded() {
        histogram_data.push((bucket.value_iterated_to() as f64, bucket.count_since_last_iteration()));
    }
    
    Ok(LatencyMetrics {
        iterations,
        message_size,
        mean_us: mean_latency.as_micros() as f64,
        median_us: histogram.value_at_quantile(0.5) as f64,
        p90_us: histogram.value_at_quantile(0.9) as f64,
        p95_us: histogram.value_at_quantile(0.95) as f64,
        p99_us: histogram.value_at_quantile(0.99) as f64,
        max_us: max_latency.as_micros() as f64,
        min_us: min_latency.as_micros() as f64,
        histogram: histogram_data,
    })
}

async fn run_flood_test(
    transport: &dyn Transport,
    addr: &str,
    duration: Duration,
    concurrency: usize,
    message_size: usize,
    verbose: bool,
) -> Result<FloodMetrics, Box<dyn Error>> {
    let message_counter = Arc::new(AtomicU64::new(0));
    let byte_counter = Arc::new(AtomicU64::new(0));
    let latency_histogram = Arc::new(parking_lot::Mutex::new(Histogram::<u64>::new(3)?));
    
    eprintln!("Starting flood test...");
    
    let start_time = Instant::now();
    let mut tasks = Vec::new();
    
    // Pre-create connections for each task to avoid transport cloning issues
    let mut connections = Vec::new();
    for _ in 0..concurrency {
        let client = transport.connect(addr).await?;
        connections.push(client);
    }
    
    // Spawn concurrent client tasks
    for connection in connections {
        let message_counter_clone = message_counter.clone();
        let byte_counter_clone = byte_counter.clone();
        let histogram_clone = latency_histogram.clone();
        
        let task = tokio::spawn(async move {
            let message_data = vec![42u8; message_size];
            let message = Bytes::from(message_data);
            
            while start_time.elapsed() < duration {
                if let Ok(mut stream) = connection.open_bi().await {
                    let msg_start = Instant::now();
                    
                    if stream.send_message(message.clone()).await.is_ok() {
                        if let Ok(response) = stream.receive_message().await {
                            let latency = msg_start.elapsed();
                            
                            if response.len() == message.len() {
                                message_counter_clone.fetch_add(1, Ordering::Relaxed);
                                byte_counter_clone.fetch_add(message_size as u64, Ordering::Relaxed);
                                
                                // Record latency
                                if let Some(mut hist) = histogram_clone.try_lock() {
                                    let _ = hist.record(latency.as_micros() as u64);
                                }
                            }
                        }
                    }
                }
            }
            
            Ok::<(), Box<dyn Error + Send + Sync>>(())
        });
        
        tasks.push(task);
    }
    
    // Progress reporting
    if verbose {
        let progress_counter = message_counter.clone();
        let progress_task = tokio::spawn(async move {
            let mut last_count = 0u64;
            
            while start_time.elapsed() < duration {
                tokio::time::sleep(Duration::from_secs(2)).await;
                
                let current_count = progress_counter.load(Ordering::Relaxed);
                let elapsed = start_time.elapsed();
                let rate = (current_count - last_count) as f64 / 2.0;
                
                eprintln!("Progress: {:.1}s, {} messages, {:.0} msg/s", 
                         elapsed.as_secs_f64(), current_count, rate);
                
                last_count = current_count;
            }
        });
        
        // Wait for test completion
        for task in tasks {
            let _ = task.await?;
        }
        
        progress_task.abort();
    } else {
        // Wait for test completion without progress
        for task in tasks {
            let _ = task.await?;
        }
    }
    
    let actual_duration = start_time.elapsed();
    let total_messages = message_counter.load(Ordering::Relaxed);
    let total_bytes = byte_counter.load(Ordering::Relaxed);
    
    // Extract latency statistics
    let hist = latency_histogram.lock();
    let mean_latency_us = hist.mean();
    let p99_latency_us = hist.value_at_quantile(0.99) as f64;
    
    eprintln!("✅ Flood test completed in {:.2}s", actual_duration.as_secs_f64());
    
    Ok(FloodMetrics {
        duration_s: actual_duration.as_secs_f64(),
        concurrency,
        message_size,
        total_messages,
        messages_per_second: total_messages as f64 / actual_duration.as_secs_f64(),
        total_bytes,
        throughput_mbps: (total_bytes as f64 * 8.0) / (actual_duration.as_secs_f64() * 1_000_000.0),
        mean_latency_us,
        p99_latency_us,
    })
}

async fn run_stream_test(
    transport: &dyn Transport,
    addr: &str,
    size_gib: f64,
    chunk_size: usize,
    verbose: bool,
) -> Result<StreamMetrics, Box<dyn Error>> {
    let client = transport.connect(addr).await?;
    let mut stream = client.open_bi().await?;
    
    let total_bytes = (size_gib * 1024.0 * 1024.0 * 1024.0) as u64;
    let chunk_data = vec![42u8; chunk_size];
    let chunk = Bytes::from(chunk_data.clone());
    
    eprintln!("Starting stream test...");
    eprintln!("Total data: {} bytes ({:.2} GiB)", total_bytes, size_gib);
    
    let start_time = Instant::now();
    let mut bytes_sent = 0u64;
    let mut chunks_sent = 0u64;
    
    while bytes_sent < total_bytes {
        let remaining = total_bytes - bytes_sent;
        let send_size = std::cmp::min(chunk_size as u64, remaining) as usize;
        
        let chunk_to_send = if send_size == chunk_size {
            chunk.clone()
        } else {
            let partial_data = vec![42u8; send_size];
            Bytes::from(partial_data)
        };
        
        stream.send_message(chunk_to_send).await?;
        
        // Wait for acknowledgment (server echo)
        let _response = stream.receive_message().await?;
        
        bytes_sent += send_size as u64;
        chunks_sent += 1;
        
        // Progress reporting
        if verbose && chunks_sent % 1000 == 0 {
            let progress = (bytes_sent as f64 / total_bytes as f64) * 100.0;
            eprintln!("Progress: {:.1}% ({} chunks)", progress, chunks_sent);
        }
    }
    
    let duration = start_time.elapsed();
    let throughput_gbps = (total_bytes as f64 * 8.0) / (duration.as_secs_f64() * 1_000_000_000.0);
    
    eprintln!("✅ Stream test completed in {:.2}s", duration.as_secs_f64());
    
    Ok(StreamMetrics {
        size_gib,
        chunk_size,
        total_bytes,
        duration_s: duration.as_secs_f64(),
        throughput_gbps,
        chunks_transferred: chunks_sent,
        chunk_rate_per_s: chunks_sent as f64 / duration.as_secs_f64(),
    })
}

fn get_backend_name() -> String {
    #[cfg(feature = "backend-quinn")]
    {
        "quinn".to_string()
    }
    #[cfg(all(not(feature = "backend-quinn"), feature = "backend-tquic"))]
    {
        "tquic".to_string()
    }
    #[cfg(all(not(feature = "backend-quinn"), not(feature = "backend-tquic"), feature = "backend-alt"))]
    {
        "alt".to_string()
    }
}