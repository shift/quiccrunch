/// QuicCrunch Performance Validation Example
/// 
/// This example demonstrates the performance improvements achieved by the
/// integrated optimizations: memory pools, SIMD operations, and optimal configurations.

use quiccrunch_core::{
    QuicTransport, Transport, TlsConfig, Connection,
    enable_maximum_performance, optimizations::{CpuOptimizations, get_pool_stats}
};
use std::time::{Duration, Instant};
use bytes::Bytes;
use tokio::time::timeout;

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 QuicCrunch Performance Validation");
    println!("=====================================\n");

    // Initialize optimizations
    let cpu_opts = enable_maximum_performance()?;
    println!("CPU Features Detected:");
    println!("  - AVX2: {}", cpu_opts.features.avx2);
    println!("  - AVX-512: {}", cpu_opts.features.avx512f);
    println!("  - NEON: {}", cpu_opts.features.neon);
    println!("  - CPU cores: {}", cpu_opts.cpu_count);
    println!("  - Optimal chunk size: {} KB", cpu_opts.optimal_chunk_size() / 1024);
    println!();

    // Generate test certificates
    let (ca_cert, ca_key) = generate_ca_cert()?;
    let (cert, key) = generate_node_cert(&ca_cert, "localhost")?;
    let tls_config = TlsConfig { cert, key, ca_cert };

    // Run performance tests
    run_throughput_test(&tls_config).await?;
    run_latency_test(&tls_config).await?;
    run_memory_efficiency_test(&tls_config).await?;
    run_scalability_test(&tls_config).await?;

    println!("\n✅ Performance validation complete!");
    Ok(())
}

/// Test raw throughput with large messages
async fn run_throughput_test(tls_config: &TlsConfig) -> Result<(), Box<dyn std::error::Error>> {
    println!("📊 Throughput Test (Large Messages)");
    println!("-----------------------------------");

    let transport = QuicTransport::new(tls_config.clone())?;
    let mut incoming = transport.listen("127.0.0.1:0").await?;
    let server_addr = incoming.local_addr()?;

    // Server task
    let server_tls = tls_config.clone();
    tokio::spawn(async move {
        let transport = QuicTransport::new(server_tls).unwrap();
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                let mut stream = conn.accept_bi().await.unwrap();
                while let Ok(data) = stream.receive_message().await {
                    // Echo back the data
                    let _ = stream.send_message(data).await;
                }
            });
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Client test
    let conn = transport.connect(&server_addr).await?;
    
    // Test with different message sizes
    let message_sizes = vec![1024, 4096, 16384, 65536, 262144, 1048576]; // 1KB to 1MB
    let iterations = 100;

    for size in message_sizes {
        let test_data = Bytes::from(vec![0xAB; size]);
        let start = Instant::now();

        for _ in 0..iterations {
            let mut stream = conn.open_bi().await?;
            stream.send_message(test_data.clone()).await?;
            let _response = stream.receive_message().await?;
        }

        let elapsed = start.elapsed();
        let total_bytes = (size * iterations * 2) as f64; // Round trip
        let throughput_mbps = (total_bytes / elapsed.as_secs_f64()) / (1024.0 * 1024.0);
        
        println!("  {} KB messages: {:.2} MB/s", size / 1024, throughput_mbps);
    }
    
    println!();
    Ok(())
}

/// Test latency with small messages
async fn run_latency_test(tls_config: &TlsConfig) -> Result<(), Box<dyn std::error::Error>> {
    println!("⚡ Latency Test (Small Messages)");
    println!("-------------------------------");

    let transport = QuicTransport::new(tls_config.clone())?;
    let mut incoming = transport.listen("127.0.0.1:0").await?;
    let server_addr = incoming.local_addr()?;

    // Server task  
    let server_tls = tls_config.clone();
    tokio::spawn(async move {
        let transport = QuicTransport::new(server_tls).unwrap();
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                let mut stream = conn.accept_bi().await.unwrap();
                while let Ok(data) = stream.receive_message().await {
                    let _ = stream.send_message(data).await;
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let conn = transport.connect(&server_addr).await?;
    let test_data = Bytes::from(vec![0xCD; 128]); // 128 byte messages
    let iterations = 1000;

    let mut latencies = Vec::new();

    for _ in 0..iterations {
        let start = Instant::now();
        let mut stream = conn.open_bi().await?;
        stream.send_message(test_data.clone()).await?;
        let _response = stream.receive_message().await?;
        latencies.push(start.elapsed());
    }

    latencies.sort();
    let avg_latency = latencies.iter().sum::<Duration>() / latencies.len() as u32;
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() * 95) / 100];
    let p99 = latencies[(latencies.len() * 99) / 100];

    println!("  Average latency: {:.2} μs", avg_latency.as_micros());
    println!("  P50 latency: {:.2} μs", p50.as_micros());
    println!("  P95 latency: {:.2} μs", p95.as_micros());
    println!("  P99 latency: {:.2} μs", p99.as_micros());
    println!();

    Ok(())
}

/// Test memory pool efficiency
async fn run_memory_efficiency_test(tls_config: &TlsConfig) -> Result<(), Box<dyn std::error::Error>> {
    println!("🧠 Memory Efficiency Test");
    println!("-------------------------");

    let transport = QuicTransport::new(tls_config.clone())?;
    let mut incoming = transport.listen("127.0.0.1:0").await?;
    let server_addr = incoming.local_addr()?;

    // Server
    let server_tls = tls_config.clone();
    tokio::spawn(async move {
        let transport = QuicTransport::new(server_tls).unwrap();
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                let mut stream = conn.accept_bi().await.unwrap();
                while let Ok(data) = stream.receive_message().await {
                    let _ = stream.send_message(data).await;
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let conn = transport.connect(&server_addr).await?;
    
    // Test different allocation patterns
    let patterns = vec![
        ("Small messages (1KB)", 1024, 1000),
        ("Medium messages (16KB)", 16384, 500),
        ("Large messages (256KB)", 262144, 100),
    ];

    for (name, size, count) in patterns {
        let stats_before = get_pool_stats();
        let test_data = Bytes::from(vec![0xEF; size]);

        let start = Instant::now();
        for _ in 0..count {
            let mut stream = conn.open_bi().await?;
            stream.send_message(test_data.clone()).await?;
            let _response = stream.receive_message().await?;
        }
        let elapsed = start.elapsed();

        let stats_after = get_pool_stats();
        let hit_rate = if cfg!(feature = "memory-pools") {
            stats_after.hit_rate()
        } else {
            0.0
        };

        println!("  {}: {:.2} ms, Pool hit rate: {:.1}%", 
                 name, elapsed.as_millis(), hit_rate);
    }
    
    println!();
    Ok(())
}

/// Test concurrent connection scalability
async fn run_scalability_test(tls_config: &TlsConfig) -> Result<(), Box<dyn std::error::Error>> {
    println!("📈 Scalability Test (Concurrent Connections)");
    println!("--------------------------------------------");

    let transport = QuicTransport::new(tls_config.clone())?;
    let mut incoming = transport.listen("127.0.0.1:0").await?;
    let server_addr = incoming.local_addr()?;

    // Server
    let server_tls = tls_config.clone();
    tokio::spawn(async move {
        let transport = QuicTransport::new(server_tls).unwrap();
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                while let Ok(mut stream) = conn.accept_bi().await {
                    tokio::spawn(async move {
                        while let Ok(data) = stream.receive_message().await {
                            let _ = stream.send_message(data).await;
                        }
                    });
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let concurrent_levels = vec![1, 4, 16, 64];
    let messages_per_connection = 50;
    let test_data = Bytes::from(vec![0x12; 4096]);

    for concurrent in concurrent_levels {
        let start = Instant::now();
        
        let mut tasks = Vec::new();
        for _ in 0..concurrent {
            let transport = transport.clone();
            let server_addr = server_addr.clone();
            let test_data = test_data.clone();
            
            let task = tokio::spawn(async move {
                let conn = transport.connect(&server_addr).await.unwrap();
                for _ in 0..messages_per_connection {
                    let mut stream = conn.open_bi().await.unwrap();
                    stream.send_message(test_data.clone()).await.unwrap();
                    let _response = stream.receive_message().await.unwrap();
                }
            });
            tasks.push(task);
        }

        // Wait for all tasks to complete
        for task in tasks {
            task.await?;
        }

        let elapsed = start.elapsed();
        let total_messages = concurrent * messages_per_connection;
        let messages_per_second = total_messages as f64 / elapsed.as_secs_f64();

        println!("  {} connections: {:.0} messages/sec", concurrent, messages_per_second);
    }

    println!();
    Ok(())
}

/// Generate CA certificate for testing
fn generate_ca_cert() -> Result<(rustls::Certificate, rustls::PrivateKey), Box<dyn std::error::Error>> {
    use rcgen::{CertificateParams, KeyPair};
    
    let mut params = CertificateParams::new(Vec::new());
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
    ];
    
    let key_pair = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    let cert = rcgen::Certificate::from_params(params)?;
    
    let cert_der = cert.serialize_der()?;
    let key_der = key_pair.serialize_der();
    
    Ok((
        rustls::Certificate(cert_der),
        rustls::PrivateKey(key_der),
    ))
}

/// Generate node certificate signed by CA
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
    
    let cert_der = cert.serialize_der()?;
    let key_der = key_pair.serialize_der();
    
    Ok((
        rustls::Certificate(cert_der),
        rustls::PrivateKey(key_der),
    ))
}