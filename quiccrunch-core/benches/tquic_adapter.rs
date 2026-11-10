//! tQUIC Adapter Performance Benchmarks
//!
//! Benchmarks the actual tQUIC adapter implementation with
//! connection establishment, streaming, and concurrent operations.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use quiccrunch_core::transport::adapter::{RuntimeOptions, TquicClient, TquicServer};
use std::time::Duration;

const MESSAGE_SIZES: &[usize] = &[64, 1024, 4096, 16384];
const CONCURRENT_CONNECTIONS: &[usize] = &[1, 5, 10];

async fn setup_test_server() -> Result<(TquicServer, String), Box<dyn std::error::Error>> {
    let bind_addr = "127.0.0.1:0".parse()?;
    let cert_file = "/tmp/cert.pem".to_string();
    let key_file = "/tmp/key.pem".to_string();

    // Generate test certificates if they don't exist
    if !std::path::Path::new(&cert_file).exists() {
        generate_test_certificates(&cert_file, &key_file)?;
    }

    let server = TquicServer::new(bind_addr, cert_file, key_file, Some(2)).await?;
    let addr = server.local_addr();
    Ok((server, addr.to_string()))
}

fn generate_test_certificates(
    cert_file: &str,
    key_file: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use rcgen::{Certificate, CertificateParams, KeyPair};
    use std::fs::File;
    use std::io::Write;

    let mut params = CertificateParams::default();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "localhost");

    let cert = Certificate::from_params(params)?;
    let cert_pem = cert.serialize_pem()?;
    let key_pem = cert.serialize_private_key_pem()?;

    let mut cert_file = File::create(cert_file)?;
    cert_file.write_all(cert_pem.as_bytes())?;

    let mut key_file = File::create(key_file)?;
    key_file.write_all(key_pem.as_bytes())?;

    Ok(())
}

fn bench_connection_establishment(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("connection_establishment");

    for &concurrent in CONCURRENT_CONNECTIONS {
        group.bench_with_input(
            BenchmarkId::new("tquic_adapter", concurrent),
            &concurrent,
            |b, &concurrent| {
                b.to_async(&rt).iter(|| async {
                    let (server, addr) = setup_test_server().await.unwrap();
                    let mut listener = server.listen(&addr).await.unwrap();

                    // Start server in background
                    let server_handle = tokio::spawn(async move {
                        let mut count = 0;
                        while count < concurrent {
                            if let Ok(_) = listener.accept().await {
                                count += 1;
                            }
                        }
                    });

                    // Create client and establish connections
                    let client = TquicClient::with_options(RuntimeOptions {
                        // Loopback self-signed test certificate: verification is explicitly off.
                        verify_server_cert: false,
                        ..RuntimeOptions::default()
                    })
                    .await
                    .unwrap();
                    let mut handles = Vec::new();

                    for _ in 0..concurrent {
                        let client = client.clone();
                        let addr = addr.clone();
                        handles.push(tokio::spawn(async move {
                            black_box(client.connect(&addr).await)
                        }));
                    }

                    // Wait for all connections
                    for handle in handles {
                        let _ = handle.await;
                    }

                    // Wait for server to finish
                    let _ = server_handle.await;
                });
            },
        );
    }

    group.finish();
}

fn bench_streaming_performance(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("streaming_performance");

    for &size in MESSAGE_SIZES {
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::new("tquic_streaming", size),
            &size,
            |b, &size| {
                b.to_async(&rt).iter(|| async {
                    let (server, addr) = setup_test_server().await.unwrap();
                    let mut listener = server.listen(&addr).await.unwrap();

                    // Start echo server
                    let server_handle = tokio::spawn(async move {
                        if let Ok(conn) = listener.accept().await {
                            if let Ok(mut stream) = conn.accept_stream().await {
                                let mut data = vec![0u8; size];
                                loop {
                                    match stream.receive(size).await {
                                        Ok(received) if !received.is_empty() => {
                                            if let Err(_) = stream.send(&received, false).await {
                                                break;
                                            }
                                        }
                                        Ok(_) => break,
                                        Err(_) => break,
                                    }
                                }
                            }
                        }
                    });

                    // Give server time to start
                    tokio::time::sleep(Duration::from_millis(10)).await;

                    // Client streaming test
                    let client = TquicClient::with_options(RuntimeOptions {
                        // Loopback self-signed test certificate: verification is explicitly off.
                        verify_server_cert: false,
                        ..RuntimeOptions::default()
                    })
                    .await
                    .unwrap();
                    let connection = client.connect(&addr).await.unwrap();
                    let mut stream = connection.open_stream().await.unwrap();

                    let test_data = vec![0x42u8; size];
                    for _ in 0..5 {
                        // Send message
                        let _ = stream.send(&test_data, false).await;
                        // Receive echo
                        let _ = stream.receive(size).await;
                    }

                    stream.close().await.unwrap();
                    connection.close().await.unwrap();

                    // Wait for server
                    let _ = server_handle.await;
                });
            },
        );
    }

    group.finish();
}

fn bench_concurrent_streams(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("concurrent_streams");

    for &concurrent in [1, 5, 10].iter() {
        group.bench_with_input(
            BenchmarkId::new("concurrent_streams", concurrent),
            &concurrent,
            |b, &concurrent| {
                b.to_async(&rt).iter(|| async {
                    let (server, addr) = setup_test_server().await.unwrap();
                    let mut listener = server.listen(&addr).await.unwrap();

                    // Start server
                    let server_handle = tokio::spawn(async move {
                        for _ in 0..concurrent {
                            if let Ok(conn) = listener.accept().await {
                                tokio::spawn(async move {
                                    if let Ok(mut stream) = conn.accept_stream().await {
                                        let mut data = vec![0u8; 1024];
                                        loop {
                                            match stream.receive(1024).await {
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

                    // Create concurrent client streams
                    let client = TquicClient::with_options(RuntimeOptions {
                        // Loopback self-signed test certificate: verification is explicitly off.
                        verify_server_cert: false,
                        ..RuntimeOptions::default()
                    })
                    .await
                    .unwrap();
                    let mut handles = Vec::new();

                    for _ in 0..concurrent {
                        let client = client.clone();
                        let addr = addr.clone();
                        handles.push(tokio::spawn(async move {
                            let connection = client.connect(&addr).await.unwrap();
                            let mut stream = connection.open_stream().await.unwrap();

                            let test_data = vec![0x42u8; 1024];
                            for _ in 0..3 {
                                let _ = stream.send(&test_data, false).await;
                                let _ = stream.receive(1024).await;
                            }

                            stream.close().await.unwrap();
                            connection.close().await.unwrap();
                        }));
                    }

                    // Wait for all clients
                    for handle in handles {
                        let _ = handle.await;
                    }

                    // Wait for server
                    let _ = server_handle.await;
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_connection_establishment,
    bench_streaming_performance,
    bench_concurrent_streams
);

criterion_main!(benches);
