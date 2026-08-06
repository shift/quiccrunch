//! High-performance pipelined QUIC benchmark client - multi-stream approach
//! 
//! Uses multiple streams per connection for parallel message processing.
//! Each stream handles messages independently, allowing the client to
//! process multiple messages concurrently per connection.

use quiccrunch_core::transport::adapter::TquicClient;
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::time::timeout;

/// Handle a single stream - send messages and receive responses
async fn handle_stream(
    stream_idx: usize,
    mut stream: quiccrunch_core::transport::adapter::TquicStream,
    messages_per_stream: usize,
    pipeline_depth: usize,
    msg_size: usize,
    total_bytes_sent: Arc<AtomicU64>,
    total_bytes_received: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
) -> (usize, usize) {
    // Pre-generate message with length-prefixed framing
    let mut message = vec![0u8; 4 + msg_size];
    message[0..4].copy_from_slice(&(msg_size as u32).to_be_bytes());
    message[4..].fill(b'x');
    let message = bytes::Bytes::from(message);
    
    let mut sent = 0;
    let mut received = 0;
    let mut recv_buffer = Vec::new();

    while sent < messages_per_stream || received < messages_per_stream {
        // Send messages up to pipeline depth
        while sent < messages_per_stream && sent - received < pipeline_depth {
            match timeout(Duration::from_secs(5), stream.send_bytes(message.clone(), false)).await {
                Ok(Ok(_)) => {
                    total_bytes_sent.fetch_add(message.len() as u64, Ordering::Relaxed);
                    sent += 1;
                }
                Ok(Err(e)) => {
                    log::warn!("Stream {}: send error: {}", stream_idx, e);
                    errors.fetch_add(1, Ordering::Relaxed);
                    break;
                }
                Err(_) => {
                    log::warn!("Stream {}: send timeout", stream_idx);
                    errors.fetch_add(1, Ordering::Relaxed);
                    break;
                }
            }
        }

        // Receive responses
        if received < messages_per_stream {
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
                    log::warn!("Stream {}: recv error: {}", stream_idx, e);
                    errors.fetch_add(1, Ordering::Relaxed);
                    break;
                }
                Err(_) => {
                    log::warn!("Stream {}: recv timeout (received={}/{})", stream_idx, received, messages_per_stream);
                    errors.fetch_add(1, Ordering::Relaxed);
                    break;
                }
            }
        }
    }

    // Close the stream
    let _ = timeout(Duration::from_secs(2), stream.send(&[], true)).await;
    let _ = timeout(Duration::from_secs(2), stream.close()).await;

    (sent, received)
}

async fn run_client(
    conn_idx: usize,
    addr: String,
    message_count: usize,
    pipeline_depth: usize,
    msg_size: usize,
    num_streams: usize,
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

    // Open multiple streams per connection
    let mut streams = Vec::new();
    for i in 0..num_streams {
        log::debug!("Client {}: attempting to open stream {}", conn_idx, i);
        match timeout(Duration::from_secs(5), connection.open_stream()).await {
            Ok(Ok(s)) => {
                log::debug!("Client {}: stream {} opened successfully", conn_idx, i);
                streams.push((i, s));
            }
            Ok(Err(e)) => {
                log::warn!("Client {}: stream {} open failed: {}", conn_idx, i, e);
                errors.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => {
                log::warn!("Client {}: stream {} open timeout", conn_idx, i);
                errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    log::info!("Client {}: opened {} streams", conn_idx, streams.len());

    // Distribute messages across streams
    let messages_per_stream = message_count / streams.len();
    let remainder = message_count % streams.len();

    // Handle streams concurrently
    let mut handles = Vec::new();
    for (i, stream) in streams {
        let msgs = messages_per_stream + if i < remainder { 1 } else { 0 };
        let total_bytes_sent = total_bytes_sent.clone();
        let total_bytes_received = total_bytes_received.clone();
        let errors = errors.clone();
        
        let handle = tokio::spawn(async move {
            handle_stream(
                i,
                stream,
                msgs,
                pipeline_depth,
                msg_size,
                total_bytes_sent,
                total_bytes_received,
                errors,
            ).await
        });
        handles.push(handle);
    }

    // Wait for all streams to complete
    let mut total_sent = 0;
    let mut total_received = 0;
    for handle in handles {
        match handle.await {
            Ok((sent, received)) => {
                total_sent += sent;
                total_received += received;
            }
            Err(e) => {
                log::error!("Client {}: stream task failed: {}", conn_idx, e);
                errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    total_messages.fetch_add(total_sent as u64, Ordering::Relaxed);
    log::info!("Client {} completed: sent={}, received={}", conn_idx, total_sent, total_received);

    // Close the connection
    let _ = timeout(Duration::from_secs(2), connection.close()).await;
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

    let num_streams = if args.len() > 6 {
        args[6].parse().unwrap_or(4)
    } else {
        4
    };

    log::info!("Starting HIGH-PERFORMANCE multi-stream QUIC benchmark");
    log::info!("Server: {}", addr);
    log::info!("Messages per connection: {}", message_count);
    log::info!("Pipeline depth (max in-flight): {}", pipeline_depth);
    log::info!("Concurrency (connections): {}", concurrency);
    log::info!("Message size: {} bytes", msg_size);
    log::info!("Streams per connection: {}", num_streams);

    let total_messages = Arc::new(AtomicU64::new(0));
    let total_bytes_sent = Arc::new(AtomicU64::new(0));
    let total_bytes_received = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));

    let start = Instant::now();

    let mut handles = vec![];

    for conn_idx in 0..concurrency {
        let addr = addr.to_string();
        let total_messages = total_messages.clone();
        let total_bytes_sent = total_bytes_sent.clone();
        let total_bytes_received = total_bytes_received.clone();
        let errors = errors.clone();

        let handle = tokio::spawn(async move {
            run_client(
                conn_idx,
                addr,
                message_count,
                pipeline_depth,
                msg_size,
                num_streams,
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
    println!("Streams per connection: {}", num_streams);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_main().await
}
