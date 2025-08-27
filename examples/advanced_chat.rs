// Copyright (c) 2025 QuicCrunch Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Advanced QUIC client/server example with custom message handling

use quiccrunch::{QuicClient, QuicServer, MessageHandler, Result};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct ChatMessage {
    user: String,
    message: String,
    timestamp: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct ChatResponse {
    status: String,
    echo: ChatMessage,
}

/// Custom message handler that processes JSON chat messages
#[derive(Clone)]
struct ChatHandler;

impl MessageHandler for ChatHandler {
    type Response = Bytes;
    
    async fn handle(&self, message: Bytes) -> Bytes {
        // Try to parse as JSON
        if let Ok(chat_msg) = serde_json::from_slice::<ChatMessage>(&message) {
            println!("Received chat from {}: {}", chat_msg.user, chat_msg.message);
            
            let response = ChatResponse {
                status: "received".to_string(),
                echo: chat_msg,
            };
            
            serde_json::to_vec(&response).unwrap_or_default().into()
        } else {
            // Fallback to simple echo
            format!("Echo: {}", String::from_utf8_lossy(&message)).into()
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: {} <server|client>", args[0]);
        return Ok(());
    }
    
    match args[1].as_str() {
        "server" => run_server().await,
        "client" => run_client().await,
        _ => {
            eprintln!("Invalid mode. Use 'server' or 'client'");
            Ok(())
        }
    }
}

async fn run_server() -> Result<()> {
    println!("Starting chat server...");
    
    // Use the convenience function with our custom handler
    quiccrunch::run_server_with_handler("127.0.0.1:50051", ChatHandler).await
}

async fn run_client() -> Result<()> {
    println!("Starting chat client...");
    
    let client = QuicClient::new()?;
    let connection = client.connect("127.0.0.1:50051").await?;
    
    // Send a structured message
    let chat_msg = ChatMessage {
        user: "alice".to_string(),
        message: "Hello from QUIC client!".to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };
    
    let message_bytes = serde_json::to_vec(&chat_msg)?;
    let response = connection.request(message_bytes).await?;
    
    // Parse response
    if let Ok(chat_response) = serde_json::from_slice::<ChatResponse>(&response) {
        println!("Server response: {:?}", chat_response);
    } else {
        println!("Raw response: {}", String::from_utf8_lossy(&response));
    }
    
    // Send multiple messages concurrently
    let mut handles = Vec::new();
    
    for i in 0..5 {
        let connection = connection.clone();
        let handle = tokio::spawn(async move {
            let msg = ChatMessage {
                user: format!("user_{}", i),
                message: format!("Concurrent message {}", i),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };
            
            let message_bytes = serde_json::to_vec(&msg).unwrap();
            match connection.request(message_bytes).await {
                Ok(response) => {
                    if let Ok(chat_response) = serde_json::from_slice::<ChatResponse>(&response) {
                        println!("Concurrent response {}: {:?}", i, chat_response);
                    }
                }
                Err(e) => eprintln!("Error in concurrent request {}: {}", i, e),
            }
        });
        handles.push(handle);
    }
    
    // Wait for all concurrent requests to complete
    for handle in handles {
        let _ = handle.await;
    }
    
    Ok(())
}