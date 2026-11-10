//! Simple TQUIC adapter test - synchronous mio-based
//!
//! Tests the low-level TquicAdapter with mio event loop
//!
//! Usage:
//!   cargo run --example simple_tquic_test server
//!   cargo run --example simple_tquic_test client

use quiccrunch_core::transport::adapter::TquicAdapter;
use quiccrunch_core::transport::messages::WorkerConfig;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug)
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage:");
        eprintln!("  {} server", args[0]);
        eprintln!("  {} client", args[0]);
        std::process::exit(1);
    }

    match args[1].as_str() {
        "server" => run_server()?,
        "client" => run_client()?,
        _ => {
            eprintln!("Invalid mode. Use 'server' or 'client'");
            std::process::exit(1);
        }
    }

    Ok(())
}

fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Starting TQUIC server (synchronous mio)");

    let config = WorkerConfig {
        worker_id: 0,
        bind_addr: "127.0.0.1:4433".parse()?,
        is_server: true,
        cert_file: Some("cert.pem".to_string()),
        key_file: Some("key.pem".to_string()),
        ..Default::default()
    };

    let mut adapter = TquicAdapter::new(config)?;
    log::info!("Server adapter created, starting event loop");

    adapter.run_event_loop()?;

    Ok(())
}

fn run_client() -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Starting TQUIC client (synchronous mio)");

    thread::sleep(Duration::from_secs(1));

    let config = WorkerConfig {
        worker_id: 0,
        bind_addr: "127.0.0.1:0".parse()?,
        is_server: false,
        // Loopback self-signed example certificate: opt out explicitly.
        verify_cert: false,
        ..Default::default()
    };

    let mut adapter = TquicAdapter::new(config)?;
    log::info!("Client adapter created");

    log::info!("Connecting to server at 127.0.0.1:4433");
    let remote_addr = "127.0.0.1:4433".parse()?;
    adapter.connect(remote_addr, "localhost")?;

    log::info!("Starting client event loop");
    adapter.run_event_loop()?;

    Ok(())
}
