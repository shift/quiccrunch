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

//! Example: QUIC vs gRPC Performance Comparison
//! 
//! This example demonstrates how to use QuicCrunch to compare QUIC and gRPC performance
//! for the same workload patterns.

use quiccrunch_core::{QuicTransport, Transport, TlsConfig};
use quiccrunch_core::transport::grpc::GrpcTransport;
use bytes::Bytes;
use std::time::{Duration, Instant};
use std::error::Error;

#[path = "cert_utils.rs"]
mod cert_utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("🚀 QuicCrunch Protocol Comparison Example");
    println!("==========================================");
    
    // Generate TLS certificates
    let (ca_cert, _ca_key, ca_cert_obj) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert_obj, "127.0.0.1")?;
    
    let tls_config = TlsConfig {
        cert,
        key, 
        ca_cert,
    };
    
    // Test configuration
    let message_size = 1024;
    let num_messages = 1000;
    let test_data = vec![0u8; message_size];
    
    println!("\n📊 Test Configuration:");
    println!("  • Message size: {} bytes", message_size);
    println!("  • Number of messages: {}", num_messages);
    
    // Test QUIC performance
    println!("\n🔵 Testing QUIC Performance...");
    let quic_latency = test_protocol_latency(
        Box::new(QuicTransport::new(tls_config.clone())?),
        "127.0.0.1:50055",
        &test_data,
        num_messages,
    ).await?;
    
    // Test gRPC performance  
    println!("🟢 Testing gRPC Performance...");
    let grpc_latency = test_protocol_latency(
        Box::new(GrpcTransport::new(tls_config)?),
        "127.0.0.1:50056", 
        &test_data,
        num_messages,
    ).await?;
    
    // Compare results
    println!("\n📈 Performance Comparison:");
    println!("==========================================");
    println!("QUIC Average Latency:  {:.2} μs", quic_latency.as_micros());
    println!("gRPC Average Latency:  {:.2} μs", grpc_latency.as_micros());
    
    let speedup = grpc_latency.as_nanos() as f64 / quic_latency.as_nanos() as f64;
    if speedup > 1.0 {
        println!("🏆 QUIC is {:.2}x faster than gRPC", speedup);
    } else {
        println!("🏆 gRPC is {:.2}x faster than QUIC", 1.0 / speedup);
    }
    
    println!("\n💡 Use the CLI benchmark tool for comprehensive testing:");
    println!("  cargo run --bin benchmark -- -t quic client latency -n 10000");
    println!("  cargo run --bin benchmark -- -t grpc client latency -n 10000");
    
    Ok(())
}

async fn test_protocol_latency(
    transport: Box<dyn Transport>,
    addr: &str,
    test_data: &[u8],
    num_messages: usize,
) -> Result<Duration, Box<dyn Error>> {
    // Start server
    let server_transport = transport.clone();
    let server_addr = addr.to_string();
    let server_handle = tokio::spawn(async move {
        let mut incoming = server_transport.listen(&server_addr).await?;
        
        // Accept one connection and echo messages
        if let Ok(conn) = incoming.accept().await {
            let mut stream = conn.open_bi().await?;
            for _ in 0..num_messages {
                let msg = stream.receive_message().await?;
                stream.send_message(msg).await?;
            }
        }
        
        Ok::<_, Box<dyn Error + Send + Sync>>(())
    });
    
    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Run client test
    let client_start = Instant::now();
    let conn = transport.connect(addr).await?;
    let mut stream = conn.open_bi().await?;
    
    let test_start = Instant::now();
    for _ in 0..num_messages {
        stream.send_message(Bytes::from(test_data.to_vec())).await?;
        let _response = stream.receive_message().await?;
    }
    let total_time = test_start.elapsed();
    
    // Cleanup
    stream.close().await?;
    conn.close().await?;
    let _ = server_handle.await;
    
    Ok(total_time / num_messages as u32)
}

impl Clone for Box<dyn Transport> {
    fn clone(&self) -> Self {
        // This is a simplified clone for the example
        // In practice, you'd implement proper cloning based on the transport type
        unimplemented!("Transport cloning not implemented for this example")
    }
}