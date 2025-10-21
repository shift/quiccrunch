use quiccrunch_core::transport::adapter::{TquicClient, TquicServer};
use quiccrunch_core::TransportError;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

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

    let server = TquicServer::new(Some(4)).await?;
    let addr = "127.0.0.1:4433";
    let mut listener = server.listen(addr).await?;
    println!("✅ Server listening on {}", addr);

    loop {
        match listener.accept().await {
            Ok(connection) => {
                println!("📥 New connection accepted");
                
                tokio::spawn(async move {
                    match handle_connection(connection).await {
                        Ok(()) => println!("✅ Connection handled successfully"),
                        Err(e) => eprintln!("❌ Connection error: {}", e),
                    }
                });
            }
            Err(e) => {
                eprintln!("❌ Accept error: {}", e);
            }
        }
    }
}

async fn handle_connection(connection: quiccrunch_core::transport::adapter::TquicConnection) -> Result<(), TransportError> {
    println!("📡 Waiting for client to initiate stream...");
    
    let mut stream = connection.accept_stream().await?;
    println!("✅ Stream accepted from client");
    
    loop {
        match stream.receive(8192).await {
            Ok(data) if !data.is_empty() => {
                let message = String::from_utf8_lossy(&data);
                println!("📨 Received from client: {}", message);
                
                let response = format!("Echo: {}", message);
                println!("📤 Sending response: {}", response);
                stream.send(response.as_bytes(), false).await?;
            }
            Ok(_) => {
                println!("⚠️  Received FIN or empty data, closing stream");
                break;
            }
            Err(e) => {
                eprintln!("❌ Error receiving data: {}", e);
                break;
            }
        }
    }
    
    println!("🔌 Closing stream...");
    stream.close().await?;
    Ok(())
}

async fn run_client() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting bidirectional streaming client...");
    
    let client = TquicClient::new(Some(2)).await?;
    let addr = "127.0.0.1:4433";
    
    println!("📡 Connecting to {}...", addr);
    let connection = client.connect(addr).await?;
    println!("✅ Connected successfully");
    
    let mut stream = connection.open_stream().await?;
    println!("✅ Stream opened");
    
    let messages = vec![
        "Hello from client!",
        "This is message 2",
        "Testing bidirectional streaming",
        "Fourth message here",
        "Final message!",
    ];
    
    for (i, msg) in messages.iter().enumerate() {
        println!("\n📤 Sending message {}: {}", i + 1, msg);
        stream.send(msg.as_bytes(), false).await?;
        
        println!("⏳ Waiting for response...");
        match stream.receive(8192).await {
            Ok(data) if !data.is_empty() => {
                let response = String::from_utf8_lossy(&data);
                println!("📨 Received response: {}", response);
            }
            Ok(_) => {
                println!("⚠️  Received empty response");
            }
            Err(e) => {
                eprintln!("❌ Error receiving response: {}", e);
                break;
            }
        }
        
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    println!("\n🔌 Closing stream...");
    stream.send(&[], true).await?;
    stream.close().await?;
    
    println!("🔌 Closing connection...");
    connection.close().await?;
    
    println!("✅ Client finished successfully");
    Ok(())
}
