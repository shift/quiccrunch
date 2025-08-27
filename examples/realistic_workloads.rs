/// Realistic Workload Test Suite
/// 
/// This test suite simulates real-world RPC patterns to validate
/// the performance optimizations under realistic conditions.

use quiccrunch::{
    QuicTransport, Transport, TlsConfig, Connection,
    enable_maximum_performance, optimizations::{get_pool_stats, reset_pool_stats}
};
use std::time::{Duration, Instant};
use bytes::Bytes;
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Simulated RPC request/response patterns
#[derive(Serialize, Deserialize, Clone)]
pub struct UserRequest {
    pub user_id: u64,
    pub action: String,
    pub data: Vec<u8>,
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UserResponse {
    pub user_id: u64,
    pub result: String,
    pub data: Vec<u8>,
    pub processing_time_ms: u64,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 16)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌍 QuicCrunch Realistic Workload Testing");
    println!("=========================================\n");

    // Enable optimizations
    let cpu_opts = enable_maximum_performance()?;
    println!("✅ Maximum performance mode enabled");
    println!("   CPU cores: {}, SIMD: AVX2={}, Memory pools: enabled\n", 
             cpu_opts.cpu_count, cpu_opts.features.avx2);

    // Generate test certificates
    let (ca_cert, ca_key) = generate_ca_cert()?;
    let (cert, key) = generate_node_cert(&ca_cert, "localhost")?;
    let tls_config = TlsConfig { cert, key, ca_cert };

    // Run realistic workload tests
    test_microservice_pattern(&tls_config).await?;
    test_api_gateway_pattern(&tls_config).await?;
    test_streaming_data_pattern(&tls_config).await?;
    test_high_frequency_trading_pattern(&tls_config).await?;
    test_file_transfer_pattern(&tls_config).await?;

    println!("\n🎯 All realistic workload tests completed successfully!");
    Ok(())
}

/// Test: Microservice Communication Pattern
/// Characteristics: Many small requests, high concurrency, low latency requirements
async fn test_microservice_pattern(tls_config: &TlsConfig) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔬 Microservice Communication Pattern");
    println!("-------------------------------------");
    
    reset_pool_stats();
    let transport = QuicTransport::new(tls_config.clone())?;
    let mut incoming = transport.listen("127.0.0.1:0").await?;
    let server_addr = incoming.local_addr()?;

    // Microservice server: handles user operations
    tokio::spawn(async move {
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                while let Ok(mut stream) = conn.accept_bi().await {
                    tokio::spawn(async move {
                        while let Ok(request_data) = stream.receive_message().await {
                            let request: UserRequest = bincode::deserialize(&request_data).unwrap();
                            
                            // Simulate processing time
                            let processing_start = Instant::now();
                            tokio::time::sleep(Duration::from_micros(100)).await; // 100μs processing
                            let processing_time = processing_start.elapsed();
                            
                            let response = UserResponse {
                                user_id: request.user_id,
                                result: format!("Processed {}", request.action),
                                data: vec![0xAB; 256], // 256 bytes response data
                                processing_time_ms: processing_time.as_micros() as u64,
                            };
                            
                            let response_data = Bytes::from(bincode::serialize(&response).unwrap());
                            let _ = stream.send_message(response_data).await;
                        }
                    });
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Client: simulate 100 concurrent users making requests
    let concurrent_users = 100;
    let requests_per_user = 50;
    let semaphore = Arc::new(Semaphore::new(concurrent_users));
    
    let start_time = Instant::now();
    let mut tasks = Vec::new();
    
    for user_id in 0..concurrent_users {
        let transport = transport.clone();
        let server_addr = server_addr.clone();
        let semaphore = semaphore.clone();
        
        let task = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();
            let conn = transport.connect(&server_addr).await.unwrap();
            
            for request_id in 0..requests_per_user {
                let request = UserRequest {
                    user_id: user_id as u64,
                    action: format!("action_{}", request_id),
                    data: vec![0xCD; 128], // 128 bytes request data
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64,
                };
                
                let mut stream = conn.open_bi().await.unwrap();
                let request_data = Bytes::from(bincode::serialize(&request).unwrap());
                
                stream.send_message(request_data).await.unwrap();
                let response_data = stream.receive_message().await.unwrap();
                let _response: UserResponse = bincode::deserialize(&response_data).unwrap();
            }
        });
        tasks.push(task);
    }
    
    for task in tasks {
        task.await?;
    }
    
    let elapsed = start_time.elapsed();
    let total_requests = concurrent_users * requests_per_user;
    let rps = total_requests as f64 / elapsed.as_secs_f64();
    let avg_latency = elapsed / total_requests as u32;
    
    let pool_stats = get_pool_stats();
    
    println!("  Results:");
    println!("    Total requests: {}", total_requests);
    println!("    Time taken: {:.2}s", elapsed.as_secs_f64());
    println!("    Requests/sec: {:.0}", rps);
    println!("    Average latency: {:.2}ms", avg_latency.as_millis());
    println!("    Memory pool hit rate: {:.1}%", pool_stats.hit_rate());
    println!();
    
    Ok(())
}

/// Test: API Gateway Pattern
/// Characteristics: Request routing, payload transformation, moderate message sizes
async fn test_api_gateway_pattern(tls_config: &TlsConfig) -> Result<(), Box<dyn std::error::Error>> {
    println!("🌐 API Gateway Pattern");
    println!("---------------------");
    
    reset_pool_stats();
    let transport = QuicTransport::new(tls_config.clone())?;
    let mut incoming = transport.listen("127.0.0.1:0").await?;
    let server_addr = incoming.local_addr()?;

    // API Gateway server
    tokio::spawn(async move {
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                while let Ok(mut stream) = conn.accept_bi().await {
                    tokio::spawn(async move {
                        while let Ok(request_data) = stream.receive_message().await {
                            // Simulate gateway processing: validation, routing, transformation
                            tokio::time::sleep(Duration::from_millis(1)).await;
                            
                            // Transform and proxy request (simulate larger response)
                            let response_size = request_data.len() * 2; // 2x size increase
                            let response = Bytes::from(vec![0xEF; response_size]);
                            let _ = stream.send_message(response).await;
                        }
                    });
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // API clients with different payload sizes
    let payload_sizes = vec![1024, 4096, 16384, 32768]; // 1KB to 32KB
    let requests_per_size = 25;
    
    for payload_size in payload_sizes {
        let start_time = Instant::now();
        let conn = transport.connect(&server_addr).await?;
        
        for _ in 0..requests_per_size {
            let mut stream = conn.open_bi().await?;
            let request = Bytes::from(vec![0xAB; payload_size]);
            
            stream.send_message(request).await?;
            let _response = stream.receive_message().await?;
        }
        
        let elapsed = start_time.elapsed();
        let throughput = (payload_size * requests_per_size * 3) as f64 / elapsed.as_secs_f64(); // Request + response
        
        println!("  {} KB payloads: {:.2} MB/s", 
                 payload_size / 1024, throughput / (1024.0 * 1024.0));
    }
    
    let pool_stats = get_pool_stats();
    println!("  Memory pool hit rate: {:.1}%", pool_stats.hit_rate());
    println!();
    
    Ok(())
}

/// Test: Streaming Data Pattern
/// Characteristics: Large continuous data transfers, sustained throughput
async fn test_streaming_data_pattern(tls_config: &TlsConfig) -> Result<(), Box<dyn std::error::Error>> {
    println!("📡 Streaming Data Pattern");
    println!("-------------------------");
    
    reset_pool_stats();
    let transport = QuicTransport::new(tls_config.clone())?;
    let mut incoming = transport.listen("127.0.0.1:0").await?;
    let server_addr = incoming.local_addr()?;

    // Streaming server
    tokio::spawn(async move {
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                let mut stream = conn.accept_bi().await.unwrap();
                
                // Send continuous stream of data
                let chunk_size = 262144; // 256KB chunks
                let total_chunks = 100;   // 25MB total
                
                for i in 0..total_chunks {
                    let chunk = Bytes::from(vec![i as u8; chunk_size]);
                    let _ = stream.send_message(chunk).await;
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Streaming client
    let start_time = Instant::now();
    let conn = transport.connect(&server_addr).await?;
    let mut stream = conn.open_bi().await?;
    
    let mut total_bytes = 0;
    let mut chunk_count = 0;
    
    loop {
        match tokio::time::timeout(Duration::from_secs(1), stream.receive_message()).await {
            Ok(Ok(chunk)) => {
                total_bytes += chunk.len();
                chunk_count += 1;
            }
            _ => break, // Timeout or stream ended
        }
    }
    
    let elapsed = start_time.elapsed();
    let throughput_mbps = (total_bytes as f64 / elapsed.as_secs_f64()) / (1024.0 * 1024.0);
    
    let pool_stats = get_pool_stats();
    
    println!("  Results:");
    println!("    Total data: {:.2} MB", total_bytes as f64 / (1024.0 * 1024.0));
    println!("    Chunks received: {}", chunk_count);
    println!("    Time taken: {:.2}s", elapsed.as_secs_f64());
    println!("    Throughput: {:.2} MB/s", throughput_mbps);
    println!("    Memory pool hit rate: {:.1}%", pool_stats.hit_rate());
    println!();
    
    Ok(())
}

/// Test: High-Frequency Trading Pattern
/// Characteristics: Ultra-low latency, small messages, high message rate
async fn test_high_frequency_trading_pattern(tls_config: &TlsConfig) -> Result<(), Box<dyn std::error::Error>> {
    println!("⚡ High-Frequency Trading Pattern");
    println!("---------------------------------");
    
    reset_pool_stats();
    let transport = QuicTransport::new(tls_config.clone())?;
    let mut incoming = transport.listen("127.0.0.1:0").await?;
    let server_addr = incoming.local_addr()?;

    // Trading server: ultra-fast order processing
    tokio::spawn(async move {
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                while let Ok(mut stream) = conn.accept_bi().await {
                    tokio::spawn(async move {
                        while let Ok(order_data) = stream.receive_message().await {
                            // Minimal processing time (market data lookups, risk checks)
                            let response = Bytes::from(vec![0xFF; 64]); // 64-byte ack
                            let _ = stream.send_message(response).await;
                        }
                    });
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Trading client: rapid order submission
    let start_time = Instant::now();
    let conn = transport.connect(&server_addr).await?;
    
    let order_count = 10000;
    let order_size = 128; // 128-byte orders
    let mut latencies = Vec::new();
    
    for _ in 0..order_count {
        let order_start = Instant::now();
        let mut stream = conn.open_bi().await?;
        
        let order = Bytes::from(vec![0xAB; order_size]);
        stream.send_message(order).await?;
        let _ack = stream.receive_message().await?;
        
        latencies.push(order_start.elapsed());
    }
    
    let elapsed = start_time.elapsed();
    
    // Calculate latency percentiles
    latencies.sort();
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() * 95) / 100];
    let p99 = latencies[(latencies.len() * 99) / 100];
    let p999 = latencies[(latencies.len() * 999) / 1000];
    
    let orders_per_second = order_count as f64 / elapsed.as_secs_f64();
    let pool_stats = get_pool_stats();
    
    println!("  Results:");
    println!("    Orders processed: {}", order_count);
    println!("    Orders/sec: {:.0}", orders_per_second);
    println!("    Latency P50: {:.1}μs", p50.as_micros());
    println!("    Latency P95: {:.1}μs", p95.as_micros());
    println!("    Latency P99: {:.1}μs", p99.as_micros());
    println!("    Latency P99.9: {:.1}μs", p999.as_micros());
    println!("    Memory pool hit rate: {:.1}%", pool_stats.hit_rate());
    println!();
    
    Ok(())
}

/// Test: File Transfer Pattern
/// Characteristics: Very large messages, maximum throughput focus
async fn test_file_transfer_pattern(tls_config: &TlsConfig) -> Result<(), Box<dyn std::error::Error>> {
    println!("📁 File Transfer Pattern");
    println!("------------------------");
    
    reset_pool_stats();
    let transport = QuicTransport::new(tls_config.clone())?;
    let mut incoming = transport.listen("127.0.0.1:0").await?;
    let server_addr = incoming.local_addr()?;

    // File server
    tokio::spawn(async move {
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                let mut stream = conn.accept_bi().await.unwrap();
                
                // Simulate sending a large file (100MB)
                let chunk_size = 1_048_576; // 1MB chunks
                let total_chunks = 100;
                
                for i in 0..total_chunks {
                    let chunk = Bytes::from(vec![(i % 256) as u8; chunk_size]);
                    let _ = stream.send_message(chunk).await;
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // File client
    let start_time = Instant::now();
    let conn = transport.connect(&server_addr).await?;
    let mut stream = conn.open_bi().await?;
    
    let mut total_bytes = 0;
    let mut chunk_count = 0;
    
    loop {
        match tokio::time::timeout(Duration::from_secs(2), stream.receive_message()).await {
            Ok(Ok(chunk)) => {
                total_bytes += chunk.len();
                chunk_count += 1;
                
                // Print progress every 10MB
                if total_bytes % (10 * 1024 * 1024) == 0 {
                    let partial_elapsed = start_time.elapsed();
                    let partial_mbps = (total_bytes as f64 / partial_elapsed.as_secs_f64()) / (1024.0 * 1024.0);
                    println!("    Progress: {} MB ({:.1} MB/s)", total_bytes / (1024 * 1024), partial_mbps);
                }
            }
            _ => break,
        }
    }
    
    let elapsed = start_time.elapsed();
    let throughput_mbps = (total_bytes as f64 / elapsed.as_secs_f64()) / (1024.0 * 1024.0);
    
    let pool_stats = get_pool_stats();
    
    println!("  Results:");
    println!("    File size: {:.2} MB", total_bytes as f64 / (1024.0 * 1024.0));
    println!("    Chunks: {}", chunk_count);
    println!("    Transfer time: {:.2}s", elapsed.as_secs_f64());
    println!("    Throughput: {:.2} MB/s", throughput_mbps);
    println!("    Memory pool hit rate: {:.1}%", pool_stats.hit_rate());
    println!();
    
    Ok(())
}

// Certificate generation helpers (same as performance_validation.rs)
fn generate_ca_cert() -> Result<(rustls::Certificate, rustls::PrivateKey), Box<dyn std::error::Error>> {
    use rcgen::{CertificateParams, KeyPair};
    
    let mut params = CertificateParams::new(Vec::new());
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    
    let key_pair = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    let cert = rcgen::Certificate::from_params(params)?;
    
    Ok((
        rustls::Certificate(cert.serialize_der()?),
        rustls::PrivateKey(key_pair.serialize_der()),
    ))
}

fn generate_node_cert(
    ca_cert: &rustls::Certificate,
    name: &str,
) -> Result<(rustls::Certificate, rustls::PrivateKey), Box<dyn std::error::Error>> {
    use rcgen::{CertificateParams, KeyPair, SanType};
    
    let mut params = CertificateParams::new(vec![name.to_string()]);
    params.subject_alt_names = vec![
        SanType::DnsName(name.to_string()),
        SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
    ];
    
    let key_pair = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    let cert = rcgen::Certificate::from_params(params)?;
    
    Ok((
        rustls::Certificate(cert.serialize_der()?),
        rustls::PrivateKey(key_pair.serialize_der()),
    ))
}