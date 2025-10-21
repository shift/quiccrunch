//! Bidirectional streaming example for QuicCrunch
//!
//! This example demonstrates a mock bidirectional streaming pattern showing the API structure.
//! Note: The underlying tQUIC transport implementation is currently in stub mode and returns
//! empty data. This example shows the intended API usage pattern.
//!
//! Usage:
//!   cargo run --example bidirectional_streaming server
//!   cargo run --example bidirectional_streaming client

use quiccrunch_core::transport::adapter::{TquicClient, TquicServer};
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
    println!("🚀 Starting bidirectional streaming server...");
    println!("⚠️  Note: Running in mock mode - tQUIC transport stubs return empty data");

    let server = TquicServer::new(Some(4)).await?;
    let addr = "127.0.0.1:4433";
    let mut listener = server.listen(addr).await?;
    println!("✅ Server API initialized (mock mode) on {}", addr);
    println!();

    // In mock mode, we handle a single connection then exit
    println!("📥 Accepting connection...");
    match listener.accept().await {
        Ok(connection) => {
            println!("✅ Connection accepted (mock)");
            match handle_connection(connection).await {
                Ok(()) => println!("✅ Connection handled successfully"),
                Err(e) => eprintln!("❌ Connection error: {}", e),
            }
        }
        Err(e) => {
            eprintln!("❌ Accept error: {}", e);
        }
    }

    Ok(())
}

async fn handle_connection(connection: quiccrunch_core::transport::adapter::TquicConnection) -> Result<(), TransportError> {
    println!("📡 Accepting stream from client...");
    
    let mut stream = connection.accept_stream().await?;
    println!("✅ Stream accepted (mock stream ID)");
    println!();
    
    // Mock mode: demonstrate the intended echo loop structure
    // In real mode, this would read actual data from the stream
    println!("⚠️  Mock mode: Would normally receive and echo messages here");
    println!("    Expected pattern:");
    println!("    1. stream.receive() -> get client data");
    println!("    2. process/echo the data");
    println!("    3. stream.send() -> send response");
    println!("    4. repeat until FIN");
    println!();
    
    // Simulate the receive loop
    for i in 1..=5 {
        println!("📨 [Mock] Would receive message {}", i);
        
        // In mock mode, receive() returns empty Vec
        let data = stream.receive(8192).await?;
        
        if data.is_empty() {
            println!("⚠️  Mock transport returned empty data (expected behavior in stub mode)");
            break;
        }
        
        let message = String::from_utf8_lossy(&data);
        println!("📤 [Mock] Would echo: {}", message);
        stream.send(format!("Echo: {}", message).as_bytes(), false).await?;
    }
    
    println!();
    println!("🔌 Closing stream...");
    stream.close().await?;
    println!("✅ Stream closed");
    Ok(())
}

async fn run_client() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting bidirectional streaming client...");
    println!("⚠️  Note: Running in mock mode - tQUIC transport stubs return empty data");
    println!();
    
    let client = TquicClient::new(Some(2)).await?;
    let addr = "127.0.0.1:4433";
    
    println!("📡 Connecting to {}...", addr);
    let connection = client.connect(addr).await?;
    println!("✅ Connected (mock connection ID)");
    
    let mut stream = connection.open_stream().await?;
    println!("✅ Stream opened (mock stream ID)");
    println!();
    
    let messages = vec![
        "Hello from client!",
        "This is message 2",
        "Testing bidirectional streaming",
        "Fourth message here",
        "Final message!",
    ];
    
    println!("📤 Sending {} messages...", messages.len());
    println!();
    
    for (i, msg) in messages.iter().enumerate() {
        println!("📤 Message {}: {}", i + 1, msg);
        
        // Send message (in mock mode, this is accepted but not transmitted)
        stream.send(msg.as_bytes(), false).await?;
        println!("✅ Send accepted by API");
        
        // Try to receive response
        match stream.receive(8192).await {
            Ok(data) if !data.is_empty() => {
                let response = String::from_utf8_lossy(&data);
                println!("📨 Received: {}", response);
            }
            Ok(_) => {
                println!("⚠️  Mock transport returned empty data (expected in stub mode)");
            }
            Err(e) => {
                eprintln!("❌ Error: {}", e);
                break;
            }
        }
        
        println!();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    println!("🔌 Closing stream...");
    stream.send(&[], true).await?;
    stream.close().await?;
    
    println!("🔌 Closing connection...");
    connection.close().await?;
    
    println!();
    println!("✅ Client finished successfully");
    println!();
    println!("📋 Summary:");
    println!("   This example demonstrates the QuicCrunch bidirectional streaming API.");
    println!("   The underlying tQUIC transport is currently in stub mode.");
    println!("   To enable real QUIC communication, the worker pool implementation");
    println!("   needs to be connected to the TquicWorker with actual tQUIC endpoints.");
    Ok(())
}
