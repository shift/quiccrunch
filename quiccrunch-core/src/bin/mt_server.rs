use quiccrunch_core::transport::worker_pool::TquicWorkerPool;
use quiccrunch_core::transport::messages::WorkerConfig;
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;
use std::io::Write;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
    
    eprintln!("[mt_server] Starting...");

    let args: Vec<String> = std::env::args().collect();
    let addr = if args.len() > 1 {
        args[1].as_str()
    } else {
        "127.0.0.1:4433"
    };

    let cert_file = if args.len() > 2 {
        args[2].clone()
    } else {
        "quiccrunch-core/tests/certs/cert.pem".to_string()
    };
    
    let key_file = if args.len() > 3 {
        args[3].clone()
    } else {
        "quiccrunch-core/tests/certs/key.pem".to_string()
    };

    let worker_count = if args.len() > 4 {
        args[4].parse().unwrap_or(4)
    } else {
        4
    };

    log::info!("Starting Multi-threaded QUIC server (TquicWorkerPool)");
    log::info!("Address: {}", addr);
    log::info!("Workers: {}", worker_count);

    let bind_addr: std::net::SocketAddr = addr.parse()?;
    
    let base_config = WorkerConfig {
        worker_id: 0,
        bind_addr,
        is_server: true,
        cert_file: Some(cert_file),
        key_file: Some(key_file),
        max_idle_timeout: Duration::from_secs(30),
        max_udp_payload_size: 65527,
        enable_early_data: false,
        application_protocols: vec![b"h3".to_vec(), b"http/0.9".to_vec()],
        verify_cert: true,
        ca_cert_file: None,
    };

    let pool = Arc::new(TquicWorkerPool::new(worker_count, base_config)?);
    
    log::info!("Worker pool created with {} workers", worker_count);
    log::info!("Server listening on {}", addr);

    let connection_counter = Arc::new(AtomicU64::new(0));

    loop {
        // Use accept command on the worker pool
        let (tx, rx) = oneshot::channel();
        let cmd = quiccrunch_core::transport::messages::TquicCommand::Accept { response: tx };
        
        if let Err(e) = pool.submit_command(cmd).await {
            log::error!("Failed to submit accept command: {}", e);
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        }
        
        match rx.await {
            Ok(Ok((connection_id, _remote))) => {
                let conn_id = connection_counter.fetch_add(1, Ordering::Relaxed);
                log::info!("Connection #{} accepted: worker connection_id={}", conn_id, connection_id);
                
                // Spawn handler for this connection
                let pool_clone = pool.clone();
                tokio::spawn(async move {
                    let mut message_count = 0u64;
                    let mut total_bytes = 0usize;
                    
                    loop {
                        // Receive message
                        let (tx2, rx2) = oneshot::channel();
                        let cmd2 = quiccrunch_core::transport::messages::TquicCommand::Receive {
                            connection_id,
                            stream_id: 0,
                            max_length: 8192,
                            response: tx2,
                        };
                        
                        if let Err(e) = pool_clone.submit_command(cmd2).await {
                            log::error!("Connection #{}: submit receive failed: {}", conn_id, e);
                            break;
                        }
                        
                        match rx2.await {
                            Ok(Ok(data)) => {
                                if data.is_empty() {
                                    break;
                                }
                                
                                message_count += 1;
                                total_bytes += data.len();
                                
                                log::trace!("Connection #{}: received {} bytes (message #{})", 
                                           conn_id, data.len(), message_count);
                                
                                let response = format!("Echo: {}", String::from_utf8_lossy(&data));
                                let (tx3, rx3) = oneshot::channel();
                                let cmd3 = quiccrunch_core::transport::messages::TquicCommand::Send {
                                    connection_id,
                                    stream_id: 0,
                                    data: response.into_bytes(),
                                    fin: false,
                                    response: tx3,
                                };
                                
                                if let Err(e) = pool_clone.submit_command(cmd3).await {
                                    log::error!("Connection #{}: submit send failed: {}", conn_id, e);
                                    break;
                                }
                                
                                match rx3.await {
                                    Ok(Ok(_)) => {}
                                    Ok(Err(e)) => {
                                        log::error!("Connection #{}: send error: {}", conn_id, e);
                                        break;
                                    }
                                    Err(_) => {
                                        log::error!("Connection #{}: send response channel closed", conn_id);
                                        break;
                                    }
                                }
                            }
                            Ok(Err(e)) => {
                                log::error!("Connection #{}: receive error: {}", conn_id, e);
                                break;
                            }
                            Err(_) => {
                                log::error!("Connection #{}: receive response channel closed", conn_id);
                                break;
                            }
                        }
                    }
                    
                    log::info!("Connection #{}: processed {} messages, {} bytes total", 
                               conn_id, message_count, total_bytes);
                    
                    // Close connection
                    let cmd_close = quiccrunch_core::transport::messages::TquicCommand::CloseConnection {
                        connection_id,
                        error_code: 0,
                        reason: b"done".to_vec(),
                    };
                    let _ = pool_clone.submit_command(cmd_close).await;
                });
            }
            Ok(Err(e)) => {
                log::debug!("Accept returned error (no pending connections): {}", e);
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(_) => {
                log::error!("Accept response channel closed");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}