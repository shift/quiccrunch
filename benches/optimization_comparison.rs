/// QuicCrunch Performance Comparison Benchmark
/// 
/// This benchmark compares performance with and without optimizations
/// to quantify the actual performance improvements achieved.

use quiccrunch::{
    QuicTransport, Transport, TlsConfig,
    enable_maximum_performance, optimizations::{get_pool_stats, reset_pool_stats}
};
use std::time::{Duration, Instant};
use bytes::Bytes;
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

pub fn bench_message_processing(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    // Setup
    let (ca_cert, ca_key) = generate_test_ca().unwrap();
    let (cert, key) = generate_test_cert(&ca_cert, "localhost").unwrap();
    let tls_config = TlsConfig { cert, key, ca_cert };

    let mut group = c.benchmark_group("message_processing");
    
    // Test different message sizes
    for size in [1024, 4096, 16384, 65536, 262144].iter() {
        let test_data = Bytes::from(vec![0xAB; *size]);
        
        // Benchmark with optimizations
        group.bench_with_input(
            BenchmarkId::new("optimized", size),
            size,
            |b, &_size| {
                b.to_async(&rt).iter(|| async {
                    reset_pool_stats();
                    let _ = enable_maximum_performance().unwrap();
                    
                    let transport = QuicTransport::new(tls_config.clone()).unwrap();
                    let mut incoming = transport.listen("127.0.0.1:0").await.unwrap();
                    let server_addr = incoming.local_addr().unwrap();

                    // Echo server
                    tokio::spawn(async move {
                        while let Ok(conn) = incoming.accept().await {
                            tokio::spawn(async move {
                                let mut stream = conn.accept_bi().await.unwrap();
                                while let Ok(data) = stream.receive_message().await {
                                    let _ = stream.send_message(data).await;
                                }
                            });
                        }
                    });

                    tokio::time::sleep(Duration::from_millis(10)).await;

                    // Client
                    let conn = transport.connect(&server_addr).await.unwrap();
                    let mut stream = conn.open_bi().await.unwrap();
                    
                    black_box(stream.send_message(test_data.clone()).await.unwrap());
                    black_box(stream.receive_message().await.unwrap());
                    
                    get_pool_stats()
                });
            },
        );
    }
    
    group.finish();
}

pub fn bench_memory_allocations(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocations");
    
    // Benchmark buffer allocation patterns
    group.bench_function("pooled_buffer_small", |b| {
        b.iter(|| {
            reset_pool_stats();
            for _ in 0..1000 {
                let buffer = quiccrunch::memory::PooledBuffer::get(black_box(1024));
                black_box(buffer);
            }
            get_pool_stats()
        });
    });
    
    group.bench_function("pooled_buffer_large", |b| {
        b.iter(|| {
            reset_pool_stats();
            for _ in 0..100 {
                let buffer = quiccrunch::memory::PooledBuffer::get(black_box(262144));
                black_box(buffer);
            }
            get_pool_stats()
        });
    });
    
    group.bench_function("vec_allocation_small", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                let buffer = Vec::with_capacity(black_box(1024));
                black_box(buffer);
            }
        });
    });
    
    group.bench_function("vec_allocation_large", |b| {
        b.iter(|| {
            for _ in 0..100 {
                let buffer = Vec::with_capacity(black_box(262144));
                black_box(buffer);
            }
        });
    });
    
    group.finish();
}

pub fn bench_simd_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("simd_operations");
    
    let sizes = [1024, 4096, 16384, 65536, 262144];
    
    for size in sizes.iter() {
        let src = vec![0xAB; *size];
        let mut dst = vec![0x00; *size];
        
        group.bench_with_input(
            BenchmarkId::new("simd_copy", size),
            size,
            |b, &_| {
                b.iter(|| {
                    quiccrunch::simd::fast_copy(black_box(&src), black_box(&mut dst));
                });
            },
        );
        
        group.bench_with_input(
            BenchmarkId::new("standard_copy", size), 
            size,
            |b, &_| {
                b.iter(|| {
                    dst.copy_from_slice(black_box(&src));
                });
            },
        );
    }
    
    group.finish();
}

pub fn bench_throughput_comparison(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let (ca_cert, ca_key) = generate_test_ca().unwrap();
    let (cert, key) = generate_test_cert(&ca_cert, "localhost").unwrap();
    let tls_config = TlsConfig { cert, key, ca_cert };
    
    let mut group = c.benchmark_group("throughput_comparison");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));
    
    // Large message throughput test
    let message_count = 100;
    let message_size = 65536; // 64KB messages
    let test_data = Bytes::from(vec![0xCD; message_size]);
    
    group.bench_function("optimized_throughput", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = enable_maximum_performance().unwrap();
            
            let transport = QuicTransport::new(tls_config.clone()).unwrap();
            let mut incoming = transport.listen("127.0.0.1:0").await.unwrap();
            let server_addr = incoming.local_addr().unwrap();

            // Server
            tokio::spawn(async move {
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

            tokio::time::sleep(Duration::from_millis(50)).await;

            let conn = transport.connect(&server_addr).await.unwrap();
            
            for _ in 0..message_count {
                let mut stream = conn.open_bi().await.unwrap();
                black_box(stream.send_message(test_data.clone()).await.unwrap());
                black_box(stream.receive_message().await.unwrap());
            }
        });
    });
    
    group.finish();
}

// Helper functions for certificate generation
fn generate_test_ca() -> Result<(rustls::Certificate, rustls::PrivateKey), Box<dyn std::error::Error>> {
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

fn generate_test_cert(
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

criterion_group!(
    benches,
    bench_message_processing,
    bench_memory_allocations, 
    bench_simd_operations,
    bench_throughput_comparison
);
criterion_main!(benches);