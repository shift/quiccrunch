//! Transport Performance Comparison Benchmarks
//!
//! Compares different transport implementations including:
//! - tQUIC adapter with optimizations
//! - Connection pooling vs fresh connections
//! - Worker pool scaling

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use quiccrunch_core::transport::{QuicTransport, Transport};
use quiccrunch_core::transport::adapter::{TquicClient, TquicServer};
use quiccrunch_core::transport::worker_pool::WorkerPool;
use std::sync::Arc;
use std::time::Duration;

const MESSAGE_SIZES: &[usize] = &[64, 1024, 4096, 16384, 65536];
const CONCURRENT_CLIENTS: &[usize] = &[1, 5, 10, 25];

async fn setup_comparison_server() -> Result<(TquicServer, String), Box<dyn std::error::Error>> {
    let bind_addr = "127.0.0.1:0".parse()?;
    let cert_file = "/tmp/cert.pem".to_string();
    let key_file = "/tmp/key.pem".to_string();
    
    // Generate test certificates if they don't exist
    if !std::path::Path::new(&cert_file).exists() {
        generate_test_certificates(&cert_file, &key_file)?;
    }
    
    let server = TquicServer::new(bind_addr, cert_file, key_file, Some(4)).await?;
    let addr = server.local_addr();
    Ok((server, addr.to_string()))
}

fn generate_test_certificates(cert_file: &str, key_file: &str) -> Result<(), Box<dyn std::error::Error>> {
    use rcgen::{Certificate, CertificateParams, KeyPair};
    use std::fs::File;
    use std::io::Write;
    
    let mut params = CertificateParams::default();
    params.distinguished_name.push(rcgen::DnType::CommonName, "localhost");
    
    let cert = Certificate::from_params(params)?;
    let cert_pem = cert.serialize_pem()?;
    let key_pem = cert.serialize_private_key_pem()?;
    
    let mut cert_file = File::create(cert_file)?;
    cert_file.write_all(cert_pem.as_bytes())?;
    
    let mut key_file = File::create(key_file)?;
    key_file.write_all(key_pem.as_bytes())?;
    
    Ok(())
}

fn bench_fresh_vs_pooled_connections(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("fresh_vs_pooled_connections");
    
    for &size in MESSAGE_SIZES {
        group.throughput(Throughput::Bytes(size as u64));
        
        // Fresh connections benchmark
        group.bench_with_input(
            BenchmarkId::new("fresh_connections", size),
            &size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let (server, addr) = setup_comparison_server().await.unwrap();
                    let mut listener = server.listen(&addr).await.unwrap();
                    
                    // Start echo server
                    let server_handle = tokio::spawn(async move {
                        for _ in 0..10 {
                            if let Ok(conn) = listener.accept().await {
                                tokio::spawn(async move {
                                    if let Ok(mut stream) = conn.accept_stream().await {
                                        let mut data = vec![0u8; size];
                                        loop {
                                            match stream.receive(size).await {
                                                Ok(received) if !received.is_empty() => {
                                                    let _ = stream.send(&received, false).await;
                                                }
                                                Ok(_) => break,
                                                Err(_) => break,
                                            }
                                        }
                                    }
                                });
                            }
                        }
                    });
                    
                    // Give server time to start
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    
                    // Fresh connections for each request
                    for _ in 0..5 {
                        let client = TquicClient::new(Some(1)).await.unwrap();
                        let connection = client.connect(&addr).await.unwrap();
                        let mut stream = connection.open_stream().await.unwrap();
                        
                        let test_data = vec![0x42u8; size];
                        let _ = stream.send(&test_data, false).await;
                        let _ = stream.receive(size).await;
                        
                        stream.close().await.unwrap();
                        connection.close().await.unwrap();
                    }
                    
                    let _ = server_handle.await;
                });
            },
        );
        
        // Pooled connections benchmark
        group.bench_with_input(
            BenchmarkId::new("pooled_connections", size),
            &size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let (server, addr) = setup_comparison_server().await.unwrap();
                    let mut listener = server.listen(&addr).await.unwrap();
                    
                    // Start echo server
                    let server_handle = tokio::spawn(async move {
                        for _ in 0..10 {
                            if let Ok(conn) = listener.accept().await {
                                tokio::spawn(async move {
                                    if let Ok(mut stream) = conn.accept_stream().await {
                                        let mut data = vec![0u8; size];
                                        loop {
                                            match stream.receive(size).await {
                                                Ok(received) if !received.is_empty() => {
                                                    let _ = stream.send(&received, false).await;
                                                }
                                                Ok(_) => break,
                                                Err(_) => break,
                                            }
                                        }
                                    }
                                });
                            }
                        }
                    });
                    
                    // Give server time to start
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    
                    // Create connection pool
                    let transport = Arc::new(QuicTransport::new(Default::default()).await.unwrap());
                    let pool_config = quiccrunch_core::transport::worker_pool::PoolConfig {
                        max_connections: 5,
                        idle_timeout: Duration::from_secs(30),
                    };
                    let pool = Arc::new(WorkerPool::new(transport, pool_config));
                    
                    // Reuse pooled connections
                    for _ in 0..5 {
                        let connection = pool.get_connection(&addr).await.unwrap();
                        let mut stream = connection.open_stream().await.unwrap();
                        
                        let test_data = vec![0x42u8; size];
                        let _ = stream.send(&test_data, false).await;
                        let _ = stream.receive(size).await;
                        
                        stream.close().await.unwrap();
                    }
                    
                    let _ = server_handle.await;
                });
            },
        );
    }
    
    group.finish();
}

fn bench_concurrent_client_scaling(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("concurrent_client_scaling");
    
    for &clients in CONCURRENT_CLIENTS {
        for &size in [1024, 4096].iter() {
            group.throughput(Throughput::Bytes((size * clients) as u64));
            
            group.bench_with_input(
                BenchmarkId::new(format!("clients_{}_size_{}", clients, size), size),
                &size,
                |b, &size| {
                    b.to_async(&rt).iter(|| async {
                        let (server, addr) = setup_comparison_server().await.unwrap();
                        let mut listener = server.listen(&addr).await.unwrap();
                        
                        // Start echo server
                        let server_handle = tokio::spawn(async move {
                            for _ in 0..clients * 2 {
                                if let Ok(conn) = listener.accept().await {
                                    tokio::spawn(async move {
                                        if let Ok(mut stream) = conn.accept_stream().await {
                                            let mut data = vec![0u8; size];
                                            loop {
                                                match stream.receive(size).await {
                                                    Ok(received) if !received.is_empty() => {
                                                        let _ = stream.send(&received, false).await;
                                                    }
                                                    Ok(_) => break,
                                                    Err(_) => break,
                                                }
                                            }
                                        }
                                    });
                                }
                            }
                        });
                        
                        // Give server time to start
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        
                        // Create concurrent clients
                        let mut handles = Vec::new();
                        for _ in 0..clients {
                            let client = TquicClient::new(Some(1)).await.unwrap();
                            let addr = addr.clone();
                            handles.push(tokio::spawn(async move {
                                let connection = client.connect(&addr).await.unwrap();
                                let mut stream = connection.open_stream().await.unwrap();
                                
                                let test_data = vec![0x42u8; size];
                                for _ in 0..3 {
                                    let _ = stream.send(&test_data, false).await;
                                    let _ = stream.receive(size).await;
                                }
                                
                                stream.close().await.unwrap();
                                connection.close().await.unwrap();
                            }));
                        }
                        
                        // Wait for all clients
                        for handle in handles {
                            let _ = handle.await;
                        }
                        
                        let _ = server_handle.await;
                    });
                },
            );
        }
    }
    
    group.finish();
}

fn bench_message_size_scaling(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("message_size_scaling");
    
    for &size in MESSAGE_SIZES {
        group.throughput(Throughput::Bytes(size as u64));
        
        group.bench_with_input(
            BenchmarkId::new("tquic_throughput", size),
            &size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let (server, addr) = setup_comparison_server().await.unwrap();
                    let mut listener = server.listen(&addr).await.unwrap();
                    
                    // Start echo server
                    let server_handle = tokio::spawn(async move {
                        if let Ok(conn) = listener.accept().await {
                            tokio::spawn(async move {
                                if let Ok(mut stream) = conn.accept_stream().await {
                                    let mut data = vec![0u8; size];
                                    loop {
                                        match stream.receive(size).await {
                                            Ok(received) if !received.is_empty() => {
                                                let _ = stream.send(&received, false).await;
                                            }
                                            Ok(_) => break,
                                            Err(_) => break,
                                        }
                                    }
                                }
                            });
                        }
                    });
                    
                    // Give server time to start
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    
                    // Client throughput test
                    let client = TquicClient::new(Some(1)).await.unwrap();
                    let connection = client.connect(&addr).await.unwrap();
                    let mut stream = connection.open_stream().await.unwrap();
                    
                    let test_data = vec![0x42u8; size];
                    for _ in 0..10 {
                        let _ = stream.send(&test_data, false).await;
                        let _ = stream.receive(size).await;
                    }
                    
                    stream.close().await.unwrap();
                    connection.close().await.unwrap();
                    
                    let _ = server_handle.await;
                });
            },
        );
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_fresh_vs_pooled_connections,
    bench_concurrent_client_scaling,
    bench_message_size_scaling
);

criterion_main!(benches);