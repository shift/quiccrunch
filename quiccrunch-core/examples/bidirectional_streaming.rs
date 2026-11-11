//! Bidirectional streaming example for QuicCrunch
//!
//! This example demonstrates bidirectional streaming using the tQUIC transport layer.
//!
//! Usage:
//!   cargo run --example bidirectional_streaming server
//!   cargo run --example bidirectional_streaming client

use quiccrunch_core::transport::adapter::{RuntimeOptions, TquicClient, TquicServer};
use quiccrunch_core::TransportError;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage:");
        eprintln!("  {} server", args[0]);
        eprintln!("  {} client", args[0]);
        std::process::exit(1);
    }

    match args[1].as_str() {
        "server" => run_server().await?,
        "client" => run_client().await?,
        _ => {
            eprintln!("Invalid mode. Use 'server' or 'client'");
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Starting bidirectional streaming server");

    let bind_addr = "127.0.0.1:4433".parse().unwrap();
    let cert_file = "cert.pem".to_string();
    let key_file = "key.pem".to_string();

    let server = TquicServer::new(bind_addr, cert_file, key_file, Some(4)).await?;
    let addr = "127.0.0.1:4433";
    let mut listener = server.listen(addr).await?;
    log::info!("Server listening on {}", addr);

    loop {
        match listener.accept().await {
            Ok(connection) => {
                log::debug!(
                    "Connection accepted, conn_index={:?}",
                    connection.conn_index
                );
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(connection).await {
                        log::error!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                log::error!("Accept error: {}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn handle_connection(
    connection: quiccrunch_core::transport::adapter::TquicConnection,
) -> Result<(), TransportError> {
    let stream = connection.accept_stream().await?;

    loop {
        let data = stream.receive(8192).await?;

        if data.is_empty() {
            break;
        }

        let message = String::from_utf8_lossy(&data);
        let response = format!("Echo: {}", message);
        stream.send(response.as_bytes(), false).await?;
    }

    stream.close().await?;
    Ok(())
}

async fn run_client() -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Starting bidirectional streaming client");

    let client = TquicClient::with_options(RuntimeOptions {
        // Loopback self-signed test certificate: verification is explicitly off.
        verify_server_cert: false,
        ..RuntimeOptions::default()
    })
    .await?;
    let addr = "127.0.0.1:4433";

    log::info!("Connecting to {}", addr);
    let connection = client.connect(addr).await?;
    log::info!("Connected: conn_index={:?}", connection.conn_index);

    let stream = connection.open_stream().await?;
    log::info!("Stream opened: stream_id={}", stream.stream_id);

    let messages = [
        "Hello from client!",
        "This is message 2",
        "Testing bidirectional streaming",
        "Fourth message here",
        "Final message!",
    ];

    log::info!("Sending {} messages", messages.len());

    for (i, msg) in messages.iter().enumerate() {
        log::info!("Message {}: {}", i + 1, msg);
        stream.send(msg.as_bytes(), false).await?;

        match stream.receive(8192).await {
            Ok(data) if !data.is_empty() => {
                let response = String::from_utf8_lossy(&data);
                log::info!("Received: {}", response);
            }
            Ok(_) => {
                log::warn!("Empty response from server");
            }
            Err(e) => {
                log::error!("Error: {}", e);
                break;
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    log::debug!("Closing stream");
    stream.send(&[], true).await?;
    stream.close().await?;

    log::debug!("Closing connection");
    connection.close().await?;

    log::info!("Client finished successfully");
    Ok(())
}
