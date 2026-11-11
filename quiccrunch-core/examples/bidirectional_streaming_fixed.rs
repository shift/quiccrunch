//! Fixed Bidirectional streaming example for QuicCrunch
//!
//! This example demonstrates bidirectional streaming using the tQUIC transport layer.
//! Fixed version that properly handles runtime lifecycle.
//!
//! Usage:
//!   cargo run --example bidirectional_streaming_fixed server
//!   cargo run --example bidirectional_streaming_fixed client

use quiccrunch_core::transport::adapter::{RuntimeOptions, TquicClient, TquicServer};
use quiccrunch_core::TransportError;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

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

    // Generate test certificates if they don't exist
    if !std::path::Path::new(&cert_file).exists() {
        generate_test_certificates(&cert_file, &key_file)?;
    }

    let server = TquicServer::new(bind_addr, cert_file, key_file, Some(4)).await?;
    let addr = "127.0.0.1:4433";
    let mut listener = server.listen(addr).await?;
    log::info!("Server listening on {}", addr);

    // Use a shutdown signal for graceful shutdown
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel();

    // Handle Ctrl+C
    let shutdown_tx = Arc::new(tokio::sync::Mutex::new(Some(shutdown_tx)));
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        log::info!("Received shutdown signal");
        let mut tx = shutdown_tx.lock().await;
        if let Some(tx) = tx.take() {
            let _ = tx.send(());
        }
    });

    loop {
        tokio::select! {
            // Accept new connections
            connection_result = listener.accept() => {
                match connection_result {
                    Ok(connection) => {
                        log::info!("Connection accepted, conn_index={:?}", connection.conn_index);
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(connection).await {
                                log::error!("Connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        log::error!("Accept error: {}", e);
                        // Don't sleep too long to be responsive
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
            }
            // Handle shutdown signal
            _ = &mut shutdown_rx => {
                log::info!("Shutting down server");
                break;
            }
        }
    }

    log::info!("Server shutdown complete");
    Ok(())
}

async fn handle_connection(
    connection: quiccrunch_core::transport::adapter::TquicConnection,
) -> Result<(), TransportError> {
    log::info!("Handling new connection");

    match connection.accept_stream().await {
        Ok(stream) => {
            log::info!("Stream accepted, starting echo loop");

            loop {
                match stream.receive(8192).await {
                    Ok(data) => {
                        if data.is_empty() {
                            log::info!("Received empty data, closing stream");
                            break;
                        }

                        let message = String::from_utf8_lossy(&data);
                        log::info!("Received: {}", message);

                        let response = format!("Echo: {}", message);
                        match stream.send(response.as_bytes(), false).await {
                            Ok(_) => {
                                log::info!("Sent echo response");
                            }
                            Err(e) => {
                                log::error!("Failed to send response: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Stream receive error: {}", e);
                        break;
                    }
                }
            }

            if let Err(e) = stream.close().await {
                log::error!("Error closing stream: {}", e);
            }
        }
        Err(e) => {
            log::error!("Failed to accept stream: {}", e);
        }
    }

    log::info!("Connection handler complete");
    Ok(())
}

async fn run_client() -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Starting bidirectional streaming client");

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    let client = TquicClient::with_options(RuntimeOptions {
        // Loopback self-signed test certificate: verification is explicitly off.
        verify_server_cert: false,
        ..RuntimeOptions::default()
    })
    .await?;
    let addr = "127.0.0.1:4433";

    log::info!("Connecting to {}", addr);
    match client.connect(addr).await {
        Ok(connection) => {
            log::info!("Connected: conn_index={:?}", connection.conn_index);

            match connection.open_stream().await {
                Ok(stream) => {
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

                        match stream.send(msg.as_bytes(), false).await {
                            Ok(_) => {
                                log::info!("Message sent successfully");
                            }
                            Err(e) => {
                                log::error!("Failed to send message: {}", e);
                                break;
                            }
                        }

                        match stream.receive(8192).await {
                            Ok(data) if !data.is_empty() => {
                                let response = String::from_utf8_lossy(&data);
                                log::info!("Received: {}", response);
                            }
                            Ok(_) => {
                                log::warn!("Empty response from server");
                            }
                            Err(e) => {
                                log::error!("Error receiving response: {}", e);
                                break;
                            }
                        }

                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }

                    log::debug!("Closing stream");
                    if let Err(e) = stream.send(&[], true).await {
                        log::error!("Error sending close: {}", e);
                    }
                    if let Err(e) = stream.close().await {
                        log::error!("Error closing stream: {}", e);
                    }
                }
                Err(e) => {
                    log::error!("Failed to open stream: {}", e);
                }
            }

            log::debug!("Closing connection");
            if let Err(e) = connection.close().await {
                log::error!("Error closing connection: {}", e);
            }
        }
        Err(e) => {
            log::error!("Failed to connect: {}", e);
        }
    }

    log::info!("Client finished successfully");
    Ok(())
}

fn generate_test_certificates(
    cert_file: &str,
    key_file: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use rcgen::{Certificate, CertificateParams};
    use std::fs::File;
    use std::io::Write;

    let mut params = CertificateParams::default();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "localhost");

    let cert = Certificate::from_params(params)?;
    let cert_pem = cert.serialize_pem()?;
    let key_pem = cert.serialize_private_key_pem();

    let mut cert_file = File::create(cert_file)?;
    cert_file.write_all(cert_pem.as_bytes())?;

    let mut key_file = File::create(key_file)?;
    key_file.write_all(key_pem.as_bytes())?;

    Ok(())
}
