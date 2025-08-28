use criterion::{black_box, criterion_group, criterion_main, Criterion};
use quiccrunch_core::{QuicTransport, Transport, TlsConfig};
use bytes::Bytes;
use tokio::runtime::Runtime;

fn bench_message_latency(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (transport, addr) = rt.block_on(setup_transport());
    
    let mut group = c.benchmark_group("latency");
    
    for size in [64, 1024, 65536].iter() {
        group.bench_function(format!("message_{}_bytes", size), |b| {
            let data = Bytes::from(&vec![0u8; *size][..]);
            b.iter(|| {
                rt.block_on(async {
                    let mut stream = setup_stream(&transport, &addr).await;
                    stream.send_message(black_box(data.clone())).await.unwrap();
                    black_box(stream.receive_message().await.unwrap());
                })
            })
        });
    }
    
    group.finish();
}

fn bench_throughput(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (transport, addr) = rt.block_on(setup_transport());
    
    let mut group = c.benchmark_group("throughput");
    group.throughput(criterion::Throughput::Bytes(1024 * 1024)); // 1MB
    
    group.bench_function("throughput_1mb", |b| {
        let data = Bytes::from(&vec![0u8; 1024 * 1024][..]);
        b.iter(|| {
            rt.block_on(async {
                let mut stream = setup_stream(&transport, &addr).await;
                stream.send_message(black_box(data.clone())).await.unwrap();
                black_box(stream.receive_message().await.unwrap());
            })
        })
    });
    
    group.finish();
}

fn bench_concurrent_streams(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (transport, addr) = rt.block_on(setup_transport());
    
    let mut group = c.benchmark_group("concurrency");
    
    for num_streams in [10, 50, 100].iter() {
        group.bench_function(format!("concurrent_{}_streams", num_streams), |b| {
            b.iter(|| {
                rt.block_on(async {
                    let mut handles = Vec::new();
                    let data = Bytes::from("test");
                    
                    for _ in 0..*num_streams {
                        let mut stream = setup_stream(&transport, &addr).await;
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
    
    group.finish();
}

criterion_group!(benches, bench_message_latency, bench_throughput, bench_concurrent_streams);
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
    
    (transport, addr)
}

async fn setup_stream(transport: &QuicTransport, addr: &str) -> QuicStream {
    let mut incoming = transport.listen(addr).await.unwrap();
    
    tokio::spawn(async move {
        while let Ok(conn) = incoming.accept().await {
            let mut stream = conn.open_bi().await.unwrap();
            let msg = stream.receive_message().await.unwrap();
            stream.send_message(msg).await.unwrap();
        }
    });
    
    let client = transport.connect(addr).await.unwrap();
    client.open_bi().await.unwrap()
}