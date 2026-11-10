//! Multi-worker QUIC echo server with SO_REUSEPORT.
//!
//! Each worker owns a UDP socket bound with `SO_REUSEPORT` and a dedicated
//! `TquicRuntime` thread, so the kernel spreads QUIC flows across cores.
//!
//! The echo path is deliberately byte-oriented: a QUIC stream is a byte stream,
//! so echoing a received chunk preserves the client's framing without parsing
//! it and without allocating a response buffer per message.
//!
//! Usage:
//!   multi_worker_server [addr] [cert] [key] [workers] [--bench]
//!
//! `--bench` selects pacing-free, deep-window transport settings suitable for
//! same-host throughput measurement. Without it, production defaults apply.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use quiccrunch_core::transport::adapter::{
    RuntimeOptions, TquicConnection, TquicListener, TquicRuntime, TquicStream,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let args: Vec<String> = std::env::args().collect();
    let addr: SocketAddr = args
        .get(1)
        .map(|a| a.as_str())
        .unwrap_or("127.0.0.1:4433")
        .parse()?;

    let cert_file = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "quiccrunch-core/tests/certs/cert.pem".to_string());
    let key_file = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| "quiccrunch-core/tests/certs/key.pem".to_string());

    let worker_count = args
        .get(4)
        .and_then(|a| a.parse().ok())
        .unwrap_or_else(num_cpus::get);

    let bench_mode = args.iter().any(|a| a == "--bench");
    // Exact worker distribution: worker w listens on port + w instead of
    // sharing one SO_REUSEPORT group. Useful for isolating worker scaling from
    // kernel hash skew. Production default remains SO_REUSEPORT.
    let spread = args.iter().any(|a| a == "--spread");

    log::info!(
        "multi_worker_server: addr={} workers={} mode={}",
        addr,
        worker_count,
        if bench_mode { "bench" } else { "default" }
    );

    let mut worker_handles = Vec::new();

    for worker_id in 0..worker_count {
        let cert = cert_file.clone();
        let key = key_file.clone();
        let worker_addr = if spread {
            let mut a = addr;
            a.set_port(addr.port() + worker_id as u16);
            a
        } else {
            addr
        };

        let handle = std::thread::Builder::new()
            .name(format!("worker-{}", worker_id))
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create Tokio runtime");

                rt.block_on(async move {
                    let options = if bench_mode {
                        RuntimeOptions::benchmark(worker_id)
                    } else {
                        RuntimeOptions {
                            worker_id,
                            ..RuntimeOptions::default()
                        }
                    };

                    if spread {
                        log::info!(
                            "Worker {} listening on {} (spread mode)",
                            worker_id,
                            worker_addr
                        );
                    }
                    let runtime = match TquicRuntime::with_options(
                        true,
                        worker_addr,
                        Some(cert),
                        Some(key),
                        options,
                    ) {
                        Ok(r) => Arc::new(r),
                        Err(e) => {
                            log::error!("Worker {} failed to start: {:?}", worker_id, e);
                            return;
                        }
                    };

                    let mut listener = TquicListener {
                        runtime: runtime.clone(),
                        bind_addr: worker_addr,
                    };

                    let accepted = Arc::new(AtomicU64::new(0));

                    loop {
                        match listener.accept().await {
                            Ok(connection) => {
                                let n = accepted.fetch_add(1, Ordering::Relaxed) + 1;
                                log::debug!(
                                    "Worker {} accepted connection {} (total {})",
                                    worker_id,
                                    connection.conn_index,
                                    n
                                );
                                tokio::spawn(handle_connection(worker_id, connection));
                            }
                            Err(e) => {
                                log::error!("Worker {} accept error: {:?}", worker_id, e);
                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            }
                        }
                    }
                });
            })
            .expect("Failed to spawn worker thread");

        worker_handles.push(handle);
    }

    log::info!("All {} workers started", worker_count);

    for handle in worker_handles {
        handle.join().expect("Worker thread panicked");
    }

    Ok(())
}

async fn handle_connection(worker_id: usize, connection: TquicConnection) {
    // One task per stream (not per message). Streams on the same connection are
    // independent QUIC streams, so serialising them starves the pipeline.
    loop {
        let stream = match connection.accept_stream().await {
            Ok(s) => s,
            Err(e) => {
                log::debug!(
                    "Worker {}: connection {} ended: {:?}",
                    worker_id,
                    connection.conn_index,
                    e
                );
                return;
            }
        };

        tokio::spawn(handle_stream(worker_id, stream));
    }
}

async fn handle_stream(worker_id: usize, stream: TquicStream) {
    let mut bytes = 0u64;
    loop {
        match stream.recv().await {
            Ok(data) => {
                if data.is_empty() {
                    log::debug!(
                        "Worker {}: stream {} EOF after {} bytes",
                        worker_id,
                        stream.stream_id,
                        bytes
                    );
                    break;
                }
                bytes += data.len() as u64;
                // Echo the chunk as-is: no per-message parse or allocation.
                if let Err(e) = stream.send_bytes(bytes::Bytes::from(data), false).await {
                    log::debug!("Worker {}: send error: {}", worker_id, e);
                    break;
                }
            }
            Err(e) => {
                log::debug!(
                    "Worker {}: stream {} recv error after {} bytes: {}",
                    worker_id,
                    stream.stream_id,
                    bytes,
                    e
                );
                break;
            }
        }
    }
    log::debug!(
        "Worker {}: stream {} done ({} bytes)",
        worker_id,
        stream.stream_id,
        bytes
    );
    let _ = stream.close().await;
}
