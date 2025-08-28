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

//! Simple QUIC client example using the high-level API

use quiccrunch_core::{QuicClient, Result};

#[tokio::main]
async fn main() -> Result<()> {
    // Create a client with self-signed certificates (for testing)
    let client = QuicClient::new()?;
    
    // Connect to server
    let connection = client.connect("127.0.0.1:50051").await?;
    
    // Send a request and get response
    let response = connection.request("Hello, QUIC World!").await?;
    
    println!("Server response: {}", String::from_utf8_lossy(&response));
    
    // You can also use streams directly for more control
    let mut stream = connection.open_stream().await?;
    stream.send_message("Direct stream message".into()).await?;
    let stream_response = stream.receive_message().await?;
    
    println!("Stream response: {}", String::from_utf8_lossy(&stream_response));
    
    Ok(())
}