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

//! Simple QUIC server example using the high-level API

use quiccrunch_core::{QuicServer, Result};

#[tokio::main]
async fn main() -> Result<()> {
    // Method 1: Use the convenient echo server function
    // quiccrunch::run_echo_server("127.0.0.1:50051").await
    
    // Method 2: Manual server implementation for custom logic
    let server = QuicServer::new()?;
    let mut listener = server.listen("127.0.0.1:50051").await?;
    
    println!("QUIC server listening on {}", listener.local_addr()?);
    
    while let Ok(connection) = listener.accept().await {
        println!("New connection accepted");
        
        // Handle each connection in a separate task
        tokio::spawn(async move {
            while let Ok(mut stream) = connection.accept_stream().await {
                // Handle each stream in a separate task
                tokio::spawn(async move {
                    while let Ok(message) = stream.receive_message().await {
                        println!("Received: {}", String::from_utf8_lossy(&message));
                        
                        // Echo the message back with a prefix
                        let response = format!("Echo: {}", String::from_utf8_lossy(&message));
                        
                        if let Err(e) = stream.send_message(response.into()).await {
                            eprintln!("Failed to send response: {}", e);
                            break;
                        }
                    }
                });
            }
        });
    }
    
    Ok(())
}