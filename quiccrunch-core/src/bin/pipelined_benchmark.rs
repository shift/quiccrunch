//! High-performance pipelined QUIC benchmark client - sliding window approach

use quiccrunch_core::transport::adapter::TquicClient;
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Semaphore;
use tokio::time::timeout;

async fn run_client(
    conn_idx: usize,
    addr: String,
    message_count: usize,
    pipeline_depth: usize,
    msg_size: usize,
    total_messages: Arc<AtomicU64>,
    total_bytes_sent: Arc<AtomicU64>,
    total_bytes_received: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
) {
    let client = match timeout(Duration::from_secs(5), TquicClient::new(None)).await {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => {
            log::error!("Client {}: client creation failed: {}", conn_idx, e);
            errors.fetch_add(1, Ordering::Relaxed);
            return;
        }
        Err(_) => {
            log::error!("Client {}: client creation timeout", conn_idx);
            errors.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };

    let connection = match timeout(Duration::from_secs(5), client.connect(&addr)).await {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => {
            log::error!("Client {}: connect failed: {}", conn_idx, e);
            errors.fetch_add(1, Ordering::Relaxed);
            return;
        }
        Err(_) => {
            log::error!("Client {}: connect timeout", conn_idx);
            errors.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };

    let mut stream = match timeout(Duration::from_secs(5), connection.open_stream()).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            log::error!("Client {}: stream open failed: {}", conn_idx, e);
            errors.fetch_add(1, Ordering::Relaxed);
            return;
        }
        Err(_) => {
            log::error!("Client {}: stream open timeout", conn_idx);
            errors.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };

    // Pre-generate message with length-prefixed framing
    let mut message = vec![0u8; 4 + msg_size];
    message[0..4].copy_from_slice(&(msg_size as u32).to_be_bytes());
    message[4..].fill(b'x');
    let message = Arc::new(message);
    
    // SLIDING WINDOW: Send up to pipeline_depth, then receive one, then send one
    let in_flight_semaphore = Arc::new(Semaphore::new(pipeline_depth));
    let mut sent = 0;
    let mut received = 0;
    let mut recv_buffer = Vec::new();

    while sent < message_count || received < message_count {
        // Fill pipeline up to depth
        while sent < message_count && sent - received < pipeline_depth {
            let permit = in_flight_semaphore.clone().acquire_owned().await.unwrap();
            
            match timeout(Duration::from_secs(5), stream.send(&message, false)).await {
                Ok(Ok(_)) => {
                    total_bytes_sent.fetch_add(message.len() as u64, Ordering::Relaxed);
                    sent += 1;
                }
                Ok(Err(e)) => {
                    log::warn!("Client {}: send error: {}", conn_idx, e);
                    errors.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    log::warn!("Client {}: send timeout", conn_idx);
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            
            // Release permit immediately after send (receive will release it)
            drop(permit);
            
            if sent % 100 == 0 {
                log::info!("Client {}: sent={} received={}", conn_idx, sent, received);
            }
        }

        // Receive one message to slide the window
        if received < message_count {
            match timeout(Duration::from_secs(5), stream.recv()).await {
                Ok(Ok(data)) => {
                    if !data.is_empty() {
                        recv_buffer.extend_from_slice(&data);
                        // Parse all complete length-prefixed messages
                        while recv_buffer.len() >= 4 {
                            let msg_len = u32::from_be_bytes([
                                recv_buffer[0], recv_buffer[1],
                                recv_buffer[2], recv_buffer[3]
                            ]) as usize;
                            if recv_buffer.len() < 4 + msg_len {
                                break;
                            }
                            received += 1;
                            total_bytes_received.fetch_add(4 + msg_len as u64, Ordering::Relaxed);
                            recv_buffer.drain(..4 + msg_len);
                        }
                    }
                }
                Ok(Err(e)) => {
                    log::warn!("Client {}: recv error: {}", conn_idx, e);
                    errors.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    log::warn!("Client {}: recv timeout (received={}/{})", conn_idx, received, message_count);
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            
            if sent % 100 == 0 {
                log::info!("Client {}: sent={} received={}", conn_idx, sent, received);
            }
        }

        // Receive any remaining
        while received < sent {
            match timeout(Duration::from_secs(5), stream.recv()).await {
                Ok(Ok(data)) => {
                    if !data.is_empty() {
                        recv_buffer.extend_from_slice(&data);
                        while recv_buffer.len() >= 4 {
                            let msg_len = u32::from_be_bytes([
                                recv_buffer[0], recv_buffer[1],
                                recv_buffer[2], recv_buffer[3]
                            ]) as usize;
                            if recv_buffer.len() < 4 + msg_len {
                                break;
                            }
                            received += 1;
                            total_bytes_received.fetch_add(4 + msg_len as u64, Ordering::Relaxed);
                            recv_buffer.drain(..4 + msg_len);
                        }
                    }
                }
                Ok(Err(e)) => {
                    log::warn!("Client {}: recv error: {}", conn_idx, e);
                    errors.fetch_add(1, Ordering::Relaxed);
                    break;
                }
                Err(_) => {
                    log::warn!("Client {}: recv timeout", conn_idx);
                    errors.fetch_add(1, Ordering::Relaxed);
                    break;
                }
            }
        }
    } // end while loop

    // Close stream (outside the while loop)
    let _ = timeout(Duration::from_secs(2), stream.send(&[], true)).await;
    let _ = timeout(Duration::from_secs(2), stream.close()).await;
    let _ = timeout(Duration::from_secs(2), connection.close()).await;

    total_messages.fetch_add(sent as u64, Ordering::Relaxed);
    
    log::info!("Client {} completed: sent={}, received={}", conn_idx, sent, received);
}

async fn run_main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let args: Vec<String> = std::env::args().collect();
    let addr = if args.len() > 1 {
        args[1].as_str()
    } else {
        "127.0.0.1:4433"
    };

    let message_count = if args.len() > 2 {
        args[2].parse().unwrap_or(10000)
    } else {
        10000
    };

    let pipeline_depth = if args.len() > 3 {
        args[3].parse().unwrap_or(100)
    } else {
        100
    };

    let concurrency = if args.len() > 4 {
        args[4].parse().unwrap_or(4)
    } else {
        4
    };

    let msg_size = if args.len() > 5 {
        args[5].parse().unwrap_or(64)
    } else {
        64
    };

    log::info!("Starting HIGH-PERFORMANCE pipelined QUIC benchmark");
    log::info!("Server: {}", addr);
    log::info!("Messages per connection: {}", message_count);
    log::info!("Pipeline depth (max in-flight): {}", pipeline_depth);
    log::info!("Concurrency (connections): {}", concurrency);
    log::info!("Message size: {} bytes", msg_size);

    let total_messages = Arc::new(AtomicU64::new(0));
    let total_bytes_sent = Arc::new(AtomicU64::new(0));
    let total_bytes_received = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let start = Instant::now();

    let mut handles = vec![];

    for conn_idx in 0..concurrency {
        let semaphore = semaphore.clone();
        let addr = addr.to_string();
        let total_messages = total_messages.clone();
        let total_bytes_sent = total_bytes_sent.clone();
        let total_bytes_received = total_bytes_received.clone();
        let errors = errors.clone();

        let handle = tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();
            run_client(
                conn_idx,
                addr,
                message_count,
                pipeline_depth,
                msg_size,
                total_messages,
                total_bytes_sent,
                total_bytes_received,
                errors,
            ).await;
        });

        handles.push(handle);
    }

    // Wait for all connections
    let overall_timeout = Duration::from_secs(60);
    let results = timeout(overall_timeout, async {
        for handle in handles {
            let _ = handle.await;
        }
    }).await;

    if results.is_err() {
        log::warn!("Overall benchmark timeout reached");
    }

    let duration = start.elapsed();
    let total_sent = total_messages.load(Ordering::Relaxed);
    let total_b_sent = total_bytes_sent.load(Ordering::Relaxed);
    let total_b_recv = total_bytes_received.load(Ordering::Relaxed);
    let total_errs = errors.load(Ordering::Relaxed);

    let throughput = total_sent as f64 / duration.as_secs_f64();
    let throughput_mbps = (total_b_sent + total_b_recv) as f64 * 8.0 / duration.as_secs_f64() / 1_000_000.0;
    let avg_latency_us = if total_sent > 0 { duration.as_micros() as f64 / total_sent as f64 } else { 0.0 };

    println!("\n=== HIGH-PERFORMANCE BENCHMARK RESULTS ===");
    println!("Duration: {:.3}s", duration.as_secs_f64());
    println!("Total messages: {}", total_sent);
    println!("Throughput: {:.0} msg/s", throughput);
    println!("Throughput: {:.2} Mbps", throughput_mbps);
    println!("Avg latency: {:.2} μs", avg_latency_us);
    println!("Bytes sent: {} ({:.2} MB)", total_b_sent, total_b_sent as f64 / 1_000_000.0);
    println!("Bytes received: {} ({:.2} MB)", total_b_recv, total_b_recv as f64 / 1_000_000.0);
    println!("Errors: {}", total_errs);
    println!("Pipeline depth: {}", pipeline_depth);
    println!("Concurrency: {}", concurrency);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_main().await
}