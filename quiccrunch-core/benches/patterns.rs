use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use quiccrunch::{QuicTransport, Transport, TlsConfig};
use bytes::Bytes;
use tokio::runtime::Runtime;
use std::time::Duration;

// Message size matrix (in bytes)
const MESSAGE_SIZES: &[usize] = &[
    64,        // Small RPC messages
    1024,      // 1KB - Typical small payload
    4096,      // 4KB - Common page size
    65536,     // 64KB - Large messages
    262144,    // 256KB - Very large messages
    1048576,   // 1MB - Bulk transfer
];

// Message patterns
#[derive(Debug, Copy, Clone)]
enum Pattern {
    Unidirectional,
    Bidirectional,
    PingPong,
    Streaming,
}

fn bench_message_patterns(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    for &size in MESSAGE_SIZES {
        let mut group = c.benchmark_group(format!("message_size_{}", size));
        group.throughput(Throughput::Bytes(size as u64));
        group.measurement_time(Duration::from_secs(10));
        
        // Unidirectional client->server
        bench_pattern(&mut group, &rt, size, Pattern::Unidirectional);
        
        // Bidirectional simultaneous
        bench_pattern(&mut group, &rt, size, Pattern::Bidirectional);
        
        // Ping-pong (request-response)
        bench_pattern(&mut group, &rt, size, Pattern::PingPong);
        
        // Continuous streaming
        bench_pattern(&mut group, &rt, size, Pattern::Streaming);
        
        group.finish();
    }
}

fn bench_pattern(group: &mut criterion::BenchmarkGroup<criterion::measurement::WallTime>, 
                rt: &Runtime, 
                size: usize, 
                pattern: Pattern) {
    let (transport, addr) = rt.block_on(setup_transport());
    let data = Bytes::from(&vec![0u8; size][..]);
    
    match pattern {
        Pattern::Unidirectional => {
            group.bench_function(BenchmarkId::new("unidirectional", size), |b| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut stream = setup_client_stream(&transport, &addr).await;
                        stream.send_message(data.clone()).await.unwrap();
                    })
                })
            });
        },
        Pattern::Bidirectional => {
            group.bench_function(BenchmarkId::new("bidirectional", size), |b| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut stream = setup_client_stream(&transport, &addr).await;
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        
                        // Client send and server send simultaneously
                        tokio::spawn(async move {
                            stream.send_message(data.clone()).await.unwrap();
                            tx.send(()).unwrap();
                        });
                        
                        let mut server_stream = setup_server_stream(&transport, &addr).await;
                        server_stream.send_message(data.clone()).await.unwrap();
                        
                        rx.await.unwrap();
                    })
                })
            });
        },
        Pattern::PingPong => {
            group.bench_function(BenchmarkId::new("ping_pong", size), |b| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut stream = setup_client_stream(&transport, &addr).await;
                        stream.send_message(data.clone()).await.unwrap();
                        stream.receive_message().await.unwrap();
                    })
                })
            });
        },
        Pattern::Streaming => {
            group.bench_function(BenchmarkId::new("streaming", size), |b| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut stream = setup_client_stream(&transport, &addr).await;
                        for _ in 0..100 {
                            stream.send_message(data.clone()).await.unwrap();
                        }
                    })
                })
            });
        },
    }
}

fn bench_concurrent_patterns(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let concurrent_streams = [1, 10, 50, 100];
    
    for &size in MESSAGE_SIZES {
        let mut group = c.benchmark_group(format!("concurrent_size_{}", size));
        group.throughput(Throughput::Bytes(size as u64));
        
        for &num_streams in &concurrent_streams {
            bench_concurrent(&mut group, &rt, size, num_streams);
        }
        
        group.finish();
    }
}

fn bench_concurrent(group: &mut criterion::BenchmarkGroup<criterion::measurement::WallTime>,
                   rt: &Runtime,
                   size: usize,
                   num_streams: usize) {
    let (transport, addr) = rt.block_on(setup_transport());
    let data = Bytes::from(&vec![0u8; size][..]);
    
    group.bench_function(BenchmarkId::new("concurrent", num_streams), |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut handles = Vec::new();
                
                for _ in 0..num_streams {
                    let mut stream = setup_client_stream(&transport, &addr).await;
                    let data = data.clone();
                    
                    handles.push(tokio::spawn(async move {
                        stream.send_message(data).await.unwrap();
                        stream.receive_message().await.unwrap();
                    }));
                }
                
                for handle in handles {
                    handle.await.unwrap();
                }
            })
        })
    });
}

fn bench_backpressure(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (transport, addr) = rt.block_on(setup_transport());
    
    let mut group = c.benchmark_group("backpressure");
    
    for &size in MESSAGE_SIZES {
        group.bench_function(BenchmarkId::new("flow_control", size), |b| {
            let data = Bytes::from(&vec![0u8; size][..]);
            b.iter(|| {
                rt.block_on(async {
                    let mut stream = setup_client_stream(&transport, &addr).await;
                    // Send messages until backpressure kicks in
                    loop {
                        match stream.send_message(data.clone()).await {
                            Ok(_) => continue,
                            Err(_) => break,
                        }
                    }
                })
            })
        });
    }
    
    group.finish();
}

criterion_group!(
    benches,
    bench_message_patterns,
    bench_concurrent_patterns,
    bench_backpressure
);
criterion_main!(benches);

async fn setup_transport() -> (QuicTransport, String) {
    let (ca_cert, ca_key) = generate_ca_cert().unwrap();
    let (cert, key) = generate_node_cert(&ca_cert, "bench-node").unwrap();
    
    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };

    let transport = QuicTransport::new(tls_config).unwrap();
    let addr = "127.0.0.1:0".to_string();
    
    // Start echo server
    let mut incoming = transport.listen(&addr).await.unwrap();
    
    tokio::spawn(async move {
        while let Ok(conn) = incoming.accept().await {
            let mut stream = conn.open_bi().await.unwrap();
            tokio::spawn(async move {
                while let Ok(msg) = stream.receive_message().await {
                    if let Err(_) = stream.send_message(msg).await {
                        break;
                    }
                }
            });
        }
    });
    
    (transport, addr)
}

async fn setup_client_stream(transport: &QuicTransport, addr: &str) -> QuicStream {
    let client = transport.connect(addr).await.unwrap();
    client.open_bi().await.unwrap()
}

async fn setup_server_stream(transport: &QuicTransport, addr: &str) -> QuicStream {
    let mut incoming = transport.listen(addr).await.unwrap();
    let conn = incoming.accept().await.unwrap();
    conn.open_bi().await.unwrap()
}