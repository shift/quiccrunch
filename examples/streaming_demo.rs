//! Comprehensive streaming RPC demo showcasing all streaming types
//! 
//! This example demonstrates:
//! 1. Server streaming - Stock price feeds
//! 2. Client streaming - File upload
//! 3. Bidirectional streaming - Real-time chat

use quiccrunch::rpc::*;
use quiccrunch::{QuicTransport, TlsConfig};
use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, interval};
use tokio_stream::StreamExt;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};

/// Stock price data
#[derive(Serialize, Deserialize, Debug)]
pub struct StockPrice {
    pub symbol: String,
    pub price: f64,
    pub timestamp: u64,
}

/// Chat message
#[derive(Serialize, Deserialize, Debug)]
pub struct ChatMessage {
    pub user: String,
    pub message: String,
    pub timestamp: u64,
}

/// File chunk data
#[derive(Serialize, Deserialize, Debug)]
pub struct FileChunk {
    pub filename: String,
    pub chunk_id: u64,
    pub data: Vec<u8>,
    pub is_last: bool,
}

/// Upload result
#[derive(Serialize, Deserialize, Debug)]
pub struct UploadResult {
    pub filename: String,
    pub size: u64,
    pub checksum: String,
}

/// Real-time data streaming service
pub struct StreamingDataService;

#[async_trait]
impl StreamingRpcService for StreamingDataService {
    fn name(&self) -> &str {
        "StreamingDataService"
    }
    
    /// Server streaming: Stock price feed
    async fn server_streaming_call(
        &self,
        context: &RpcContext,
        method: &str,
        params: Bytes,
        sink: RpcStreamSink,
    ) -> RpcResult<()> {
        match method {
            "subscribe_stock_prices" => {
                // Parse stock symbols from params
                let symbols: Vec<String> = serde_json::from_slice(&params)
                    .unwrap_or_else(|_| vec!["AAPL".to_string(), "GOOGL".to_string(), "MSFT".to_string()]);
                
                println!("📈 Starting stock price stream for symbols: {:?}", symbols);
                
                // Simulate real-time stock price updates
                let mut interval = interval(Duration::from_millis(500));
                let mut price_base = 100.0;
                
                for i in 0..20 {
                    interval.tick().await;
                    
                    for symbol in &symbols {
                        // Simulate price movement
                        price_base += (rand::random::<f64>() - 0.5) * 2.0;
                        let price = StockPrice {
                            symbol: symbol.clone(),
                            price: price_base.max(50.0).min(200.0),
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                        };
                        
                        let price_data = serde_json::to_vec(&price).unwrap();
                        if let Err(e) = sink.send(Bytes::from(price_data)).await {
                            println!("❌ Failed to send stock price: {}", e);
                            return Ok(());
                        }
                        
                        println!("📊 Sent: {} - ${:.2}", price.symbol, price.price);
                    }
                }
                
                // End the stream
                sink.finish(None).await?;
                println!("✅ Stock price stream completed");
            }
            _ => {
                sink.error(RpcError::MethodNotFound(format!("Unknown method: {}", method))).await?;
            }
        }
        
        Ok(())
    }
    
    /// Client streaming: File upload
    async fn client_streaming_call(
        &self,
        context: &RpcContext,
        method: &str,
        mut receiver: RpcStreamReceiver,
    ) -> RpcResult<Bytes> {
        match method {
            "upload_file" => {
                println!("📁 Starting file upload stream");
                
                let mut filename = String::new();
                let mut total_size = 0u64;
                let mut chunks = Vec::new();
                
                // Receive file chunks
                while let Some(chunk_result) = receiver.recv().await {
                    match chunk_result {
                        Ok(chunk_data) => {
                            let chunk: FileChunk = serde_json::from_slice(&chunk_data)?;
                            
                            if filename.is_empty() {
                                filename = chunk.filename.clone();
                                println!("📤 Uploading file: {}", filename);
                            }
                            
                            total_size += chunk.data.len() as u64;
                            chunks.push(chunk.chunk_id);
                            
                            println!("📦 Received chunk {}: {} bytes", chunk.chunk_id, chunk.data.len());
                            
                            if chunk.is_last {
                                break;
                            }
                        }
                        Err(e) => {
                            return Err(RpcError::ServiceError(format!("Upload error: {}", e)));
                        }
                    }
                }
                
                // Simulate file processing
                sleep(Duration::from_millis(100)).await;
                
                let result = UploadResult {
                    filename: filename.clone(),
                    size: total_size,
                    checksum: format!("sha256:{:x}", total_size), // Mock checksum
                };
                
                println!("✅ Upload completed: {} ({} bytes, {} chunks)", 
                    filename, total_size, chunks.len());
                
                let result_data = serde_json::to_vec(&result).unwrap();
                Ok(Bytes::from(result_data))
            }
            _ => Err(RpcError::MethodNotFound(format!("Unknown method: {}", method)))
        }
    }
    
    /// Bidirectional streaming: Real-time chat
    async fn bidirectional_streaming_call(
        &self,
        context: &RpcContext,
        method: &str,
        mut receiver: RpcStreamReceiver,
        sink: RpcStreamSink,
    ) -> RpcResult<()> {
        match method {
            "join_chat" => {
                println!("💬 Starting bidirectional chat stream");
                
                // Spawn task to handle incoming messages
                let sink_clone = sink.clone();
                let receive_handle = tokio::spawn(async move {
                    while let Some(message_result) = receiver.recv().await {
                        match message_result {
                            Ok(message_data) => {
                                let chat_msg: ChatMessage = match serde_json::from_slice(&message_data) {
                                    Ok(msg) => msg,
                                    Err(_) => continue,
                                };
                                
                                println!("📨 Received: {}: {}", chat_msg.user, chat_msg.message);
                                
                                // Echo the message back with server prefix
                                let echo_msg = ChatMessage {
                                    user: "Server".to_string(),
                                    message: format!("Echo: {}", chat_msg.message),
                                    timestamp: std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs(),
                                };
                                
                                let echo_data = serde_json::to_vec(&echo_msg).unwrap();
                                if let Err(e) = sink_clone.send(Bytes::from(echo_data)).await {
                                    println!("❌ Failed to send echo: {}", e);
                                    break;
                                }
                                
                                println!("📤 Sent echo: {}", echo_msg.message);
                            }
                            Err(e) => {
                                println!("❌ Chat receive error: {}", e);
                                break;
                            }
                        }
                    }
                });
                
                // Send periodic server messages
                let mut interval = interval(Duration::from_secs(3));
                for i in 0..5 {
                    interval.tick().await;
                    
                    let server_msg = ChatMessage {
                        user: "Server".to_string(),
                        message: format!("Server announcement #{}", i + 1),
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    };
                    
                    let msg_data = serde_json::to_vec(&server_msg).unwrap();
                    if let Err(e) = sink.send(Bytes::from(msg_data)).await {
                        println!("❌ Failed to send server message: {}", e);
                        break;
                    }
                    
                    println!("📢 Server announcement: {}", server_msg.message);
                }
                
                // End the chat
                receive_handle.abort();
                sink.finish(None).await?;
                println!("✅ Chat stream completed");
            }
            _ => {
                sink.error(RpcError::MethodNotFound(format!("Unknown method: {}", method))).await?;
            }
        }
        
        Ok(())
    }
    
    fn streaming_methods(&self) -> Vec<(String, StreamingType)> {
        vec![
            ("subscribe_stock_prices".to_string(), StreamingType::ServerStreaming),
            ("upload_file".to_string(), StreamingType::ClientStreaming),
            ("join_chat".to_string(), StreamingType::BidirectionalStreaming),
        ]
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    println!("🚀 Starting QuicCrunch Streaming RPC Demo");
    
    // Setup server
    let server = RpcServer::new();
    let streaming_service = Arc::new(StreamingDataService);
    server.register_streaming_service(streaming_service).await;
    
    let transport = QuicTransport::new(TlsConfig::default())?;
    let listener = transport.bind("127.0.0.1:0").await?;
    let server_addr = listener.local_addr()?;
    
    println!("🔗 Server listening on: {}", server_addr);
    
    // Start server
    let server_handle = tokio::spawn(async move {
        while let Ok(connection) = listener.accept().await {
            if let Err(e) = server.handle_connection(connection).await {
                eprintln!("Server error: {}", e);
            }
        }
    });
    
    // Give server time to start
    sleep(Duration::from_millis(100)).await;
    
    // Setup client
    let client_transport = QuicTransport::new(TlsConfig::default())?;
    let connection = client_transport.connect(&server_addr.to_string()).await?;
    let client = RpcClient::new(connection);
    
    println!("\n=== Demo 1: Server Streaming (Stock Prices) ===");
    demo_server_streaming(&client).await?;
    
    println!("\n=== Demo 2: Client Streaming (File Upload) ===");
    demo_client_streaming(&client).await?;
    
    println!("\n=== Demo 3: Bidirectional Streaming (Chat) ===");
    demo_bidirectional_streaming(&client).await?;
    
    server_handle.abort();
    println!("\n🎉 All streaming demos completed successfully!");
    
    Ok(())
}

/// Demo server streaming with stock price feeds
async fn demo_server_streaming(client: &RpcClient) -> RpcResult<()> {
    let symbols = vec!["AAPL".to_string(), "GOOGL".to_string()];
    let params = serde_json::to_vec(&symbols).unwrap();
    
    let mut receiver = client.server_streaming_call(
        "StreamingDataService",
        "subscribe_stock_prices",
        Bytes::from(params),
    ).await?;
    
    let mut count = 0;
    while let Some(price_result) = receiver.recv().await {
        match price_result {
            Ok(price_data) => {
                let price: StockPrice = serde_json::from_slice(&price_data)?;
                println!("📈 Received price: {} - ${:.2}", price.symbol, price.price);
                count += 1;
                
                if count >= 10 {
                    break; // Stop after receiving 10 prices
                }
            }
            Err(e) => {
                println!("❌ Price stream error: {}", e);
                break;
            }
        }
    }
    
    Ok(())
}

/// Demo client streaming with file upload
async fn demo_client_streaming(client: &RpcClient) -> RpcResult<()> {
    let (sink, mut receiver) = client.client_streaming_call(
        "StreamingDataService",
        "upload_file"
    ).await?;
    
    // Simulate file upload
    let filename = "test_file.txt";
    let file_content = b"Hello, this is a test file for streaming upload demo!";
    let chunk_size = 10;
    
    // Send file in chunks
    for (chunk_id, chunk) in file_content.chunks(chunk_size).enumerate() {
        let is_last = chunk_id == (file_content.len() + chunk_size - 1) / chunk_size - 1;
        
        let file_chunk = FileChunk {
            filename: filename.to_string(),
            chunk_id: chunk_id as u64,
            data: chunk.to_vec(),
            is_last,
        };
        
        let chunk_data = serde_json::to_vec(&file_chunk).unwrap();
        sink.send(Bytes::from(chunk_data)).await?;
        
        println!("📤 Sent chunk {}: {} bytes", chunk_id, chunk.len());
        sleep(Duration::from_millis(100)).await;
    }
    
    // Finish upload
    sink.finish(None).await?;
    
    // Receive upload result
    if let Some(result) = receiver.recv().await {
        match result {
            Ok(result_data) => {
                let upload_result: UploadResult = serde_json::from_slice(&result_data)?;
                println!("✅ Upload result: {} ({} bytes)", 
                    upload_result.filename, upload_result.size);
            }
            Err(e) => {
                println!("❌ Upload failed: {}", e);
            }
        }
    }
    
    Ok(())
}

/// Demo bidirectional streaming with chat
async fn demo_bidirectional_streaming(client: &RpcClient) -> RpcResult<()> {
    let (sink, mut receiver) = client.bidirectional_streaming_call(
        "StreamingDataService",
        "join_chat"
    ).await?;
    
    // Spawn task to receive messages
    let receive_handle = tokio::spawn(async move {
        while let Some(message_result) = receiver.recv().await {
            match message_result {
                Ok(message_data) => {
                    let chat_msg: ChatMessage = serde_json::from_slice(&message_data).unwrap();
                    println!("💬 Received: {}: {}", chat_msg.user, chat_msg.message);
                }
                Err(e) => {
                    println!("❌ Chat receive error: {}", e);
                    break;
                }
            }
        }
    });
    
    // Send some chat messages
    let messages = vec![
        "Hello from client!",
        "This is a streaming chat demo",
        "How are you server?",
        "Thanks for the echo!",
    ];
    
    for (i, msg_text) in messages.iter().enumerate() {
        sleep(Duration::from_secs(1)).await;
        
        let chat_msg = ChatMessage {
            user: "Client".to_string(),
            message: msg_text.to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        
        let msg_data = serde_json::to_vec(&chat_msg).unwrap();
        sink.send(Bytes::from(msg_data)).await?;
        
        println!("📤 Sent: {}", msg_text);
    }
    
    // Wait a bit for server responses
    sleep(Duration::from_secs(2)).await;
    
    // End chat
    sink.finish(None).await?;
    receive_handle.abort();
    
    Ok(())
}