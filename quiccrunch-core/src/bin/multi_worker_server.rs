//! Multi-worker QUIC server with SO_REUSEPORT
//!
//! Each worker has its own TquicRuntime on its own OS thread,
//! enabling true parallel QUIC processing across multiple CPU cores.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    eprintln!("[multi_worker_server] Starting...");

    let args: Vec<String> = std::env::args().collect();
    let addr: SocketAddr = args.get(1)
        .map(|a| a.as_str())
        .unwrap_or("127.0.0.1:4433")
        .parse()?;

    let cert_file = args.get(2).cloned().unwrap_or_else(|| {
        "quiccrunch-core/tests/certs/cert.pem".to_string()
    });
    
    let key_file = args.get(3).cloned().unwrap_or_else(|| {
        "quiccrunch-core/tests/certs/key.pem".to_string()
    });

    let worker_count = args.get(4)
        .and_then(|a| a.parse().ok())
        .unwrap_or(4);

    log::info!("Starting Multi-Worker QUIC server with SO_REUSEPORT");
    log::info!("Address: {}", addr);
    log::info!("Workers: {}", worker_count);

    // Spawn worker tasks - each with its own TquicRuntime
    let mut worker_handles = Vec::new();
    
    for worker_id in 0..worker_count {
        let addr = addr;
        let cert = cert_file.clone();
        let key = key_file.clone();
        
        let handle = std::thread::Builder::new()
            .name(format!("worker-{}", worker_id))
            .spawn(move || {
                // Each worker creates its own runtime on its own OS thread
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create Tokio runtime");
                
                rt.block_on(async move {
                    // Create a separate QUIC runtime for this worker
                    let runtime = match quiccrunch_core::transport::adapter::TquicRuntime::new(
                        true,
                        addr,
                        Some(cert),
                        Some(key),
                    ) {
                        Ok(r) => Arc::new(r),
                        Err(e) => {
                            log::error!("Worker {} failed to create QUIC runtime: {:?}", worker_id, e);
                            return;
                        }
                    };
                    
                    log::info!("Worker {} started on thread {:?}", worker_id, std::thread::current().name());
                    
                    // Create a listener
                    let mut listener = quiccrunch_core::transport::adapter::TquicListener {
                        runtime: runtime.clone(),
                        bind_addr: addr,
                    };
                    
                    // Accept loop for this worker
                    loop {
                        match listener.accept().await {
                            Ok(connection) => {
                                log::debug!("Worker {} accepted connection {}", worker_id, connection.conn_index);
                                
                                // Handle the connection
                                let rt = runtime.clone();
                                tokio::spawn(async move {
                                    handle_connection(worker_id, connection, rt).await;
                                });
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
    
    // Wait for all workers
    for handle in worker_handles {
        handle.join().expect("Worker thread panicked");
    }
    
    Ok(())
}

async fn handle_connection(
    worker_id: usize,
    mut connection: quiccrunch_core::transport::adapter::TquicConnection,
    _runtime: Arc<quiccrunch_core::transport::adapter::TquicRuntime>,
) {
    loop {
        let mut stream = match connection.accept_stream().await {
            Ok(s) => s,
            Err(e) => {
                log::debug!("Worker {}: connection ended: {:?}", worker_id, e);
                return;
            }
        };
        
        log::debug!("Worker {}: stream {} opened", worker_id, stream.stream_id);
        
        // Handle stream
        let mut recv_buffer = Vec::new();
        let mut message_count = 0u64;
        
        loop {
            match stream.recv().await {
                Ok(data) => {
                    if data.is_empty() {
                        break;
                    }
                    
                    recv_buffer.extend_from_slice(&data);
                    
                    // Process complete messages
                    while recv_buffer.len() >= 4 {
                        let msg_len = u32::from_be_bytes([
                            recv_buffer[0], recv_buffer[1],
                            recv_buffer[2], recv_buffer[3]
                        ]) as usize;
                        
                        if recv_buffer.len() < 4 + msg_len {
                            break;
                        }
                        
                        let msg_data = &recv_buffer[4..4 + msg_len];
                        message_count += 1;
                        
                        // Echo back
                        let mut response = Vec::with_capacity(4 + msg_len);
                        response.extend_from_slice(&(msg_len as u32).to_be_bytes());
                        response.extend_from_slice(msg_data);
                        let response = bytes::Bytes::from(response);
                        
                        if let Err(e) = stream.send_bytes(response, false).await {
                            log::error!("Worker {}: send error: {}", worker_id, e);
                            break;
                        }
                        
                        recv_buffer.drain(..4 + msg_len);
                    }
                }
                Err(e) => {
                    log::debug!("Worker {}: recv error: {}", worker_id, e);
                    break;
                }
            }
        }
        
        let _ = stream.close().await;
        log::debug!("Worker {}: stream {} done ({} messages)", worker_id, stream.stream_id, message_count);
    }
}
