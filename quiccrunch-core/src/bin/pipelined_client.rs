use quiccrunch_core::transport::adapter::{RuntimeOptions, TquicClient};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        args[2].parse().unwrap_or(5)
    } else {
        5
    };

    let pipeline_depth = if args.len() > 3 {
        args[3].parse().unwrap_or(100)
    } else {
        100
    };

    log::info!("Starting Pipelined QUIC client");
    log::info!("Server address: {}", addr);
    log::info!("Messages to send: {}", message_count);
    log::info!("Pipeline depth: {}", pipeline_depth);

    let client = TquicClient::with_options(RuntimeOptions {
        // Loopback self-signed test certificate: verification is explicitly off.
        verify_server_cert: false,
        ..RuntimeOptions::default()
    })
    .await?;

    log::info!("Connecting to {}", addr);
    let start = Instant::now();
    let connection = client.connect(addr).await?;
    let connect_duration = start.elapsed();

    log::info!(
        "Connected in {:?}: connection_id={:?}",
        connect_duration,
        connection.conn_index
    );

    let mut stream = connection.open_stream().await?;
    log::info!("Stream opened: stream_id={}", stream.stream_id);

    let messages: Vec<String> = (1..=message_count)
        .map(|i| format!("Message #{}", i))
        .collect();

    log::info!(
        "Sending {} messages with pipeline depth {}",
        messages.len(),
        pipeline_depth
    );

    let mut total_bytes_sent = 0usize;
    let mut total_bytes_received = 0usize;

    let send_start = Instant::now();

    // Pipeline: send multiple messages, then receive responses
    let mut in_flight = 0;
    let mut next_send = 0;
    let mut received = 0;

    while received < message_count {
        // Send messages up to pipeline depth
        while in_flight < pipeline_depth && next_send < message_count {
            let msg = &messages[next_send];
            let msg_bytes = msg.as_bytes();
            total_bytes_sent += msg_bytes.len();

            stream.send(msg_bytes, false).await?;

            in_flight += 1;
            next_send += 1;
        }

        // Receive a response
        match stream.receive(8192).await {
            Ok(data) if !data.is_empty() => {
                total_bytes_received += data.len();
                received += 1;
                in_flight -= 1;

                if received % 100 == 0 {
                    log::debug!("Received {}/{} responses", received, message_count);
                }
            }
            Ok(_) => {
                log::warn!("Empty response received");
            }
            Err(e) => {
                log::error!("Receive error: {}", e);
                break;
            }
        }
    }

    let send_duration = Instant::now().duration_since(send_start);

    // Close stream and connection
    stream.send(&[], true).await?;
    stream.close().await?;
    connection.close().await?;

    let throughput_mbps = (total_bytes_sent + total_bytes_received) as f64
        / send_duration.as_secs_f64()
        / 1_000_000.0
        * 8.0;

    log::info!("Client completed successfully");
    log::info!("Messages: {}", message_count);
    log::info!("Bytes sent: {}", total_bytes_sent);
    log::info!("Bytes received: {}", total_bytes_received);
    log::info!("Duration: {:?}", send_duration);
    log::info!("Throughput: {:.2} Mbps", throughput_mbps);
    log::info!(
        "Effective msg/s: {:.2}",
        message_count as f64 / send_duration.as_secs_f64()
    );

    Ok(())
}
