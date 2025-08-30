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

//! QuicCrunch server implementation for all streaming patterns

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use bytes::Bytes;
use tokio::sync::{oneshot, RwLock};
use tokio::time::timeout;
use crate::{QuicTransport, Transport, TlsConfig};
use crate::transport::TransportSettings;
use crate::benchmark::quiccrunch_client::QuicCrunchMessage;

/// QuicCrunch benchmark server
pub struct QuicCrunchServer {
    transport: QuicTransport,
    shutdown_tx: Option<oneshot::Sender<()>>,
    stats: Arc<RwLock<ServerStats>>,
}

/// Server statistics tracking
#[derive(Debug, Default, Clone)]
struct ServerStats {
    total_connections: u64,
    total_messages: u64,
    active_connections: u64,
    error_count: u64,
}

impl QuicCrunchServer {
    /// Create a new QuicCrunch server
    pub fn new(tls_config: TlsConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Use larger window settings to support all message sizes
        let settings = TransportSettings {
            stream_receive_window: 10 * 1024 * 1024, // 10MB
            connection_receive_window: 20 * 1024 * 1024, // 20MB
            ..Default::default()
        };
        
        let transport = QuicTransport::with_settings(tls_config, settings)?;
        let stats = Arc::new(RwLock::new(ServerStats::default()));
        
        Ok(Self {
            transport,
            shutdown_tx: None,
            stats,
        })
    }
    
    /// Start the server and listen for connections
    pub async fn start(
        &mut self,
        bind_addr: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Starting QuicCrunch server on {}", bind_addr);
        
        let mut incoming = self.transport.listen(bind_addr).await?;
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);
        
        let stats = self.stats.clone();
        
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        println!("QuicCrunch server shutdown requested");
                        break;
                    }
                    conn_result = incoming.accept() => {
                        match conn_result {
                            Ok(conn) => {
                                {
                                    let mut stats_lock = stats.write().await;
                                    stats_lock.total_connections += 1;
                                    stats_lock.active_connections += 1;
                                }
                                
                                let stats_clone = stats.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = Self::handle_connection(conn, stats_clone.clone()).await {
                                        println!("Connection error: {}", e);
                                        let mut stats_lock = stats_clone.write().await;
                                        stats_lock.error_count += 1;
                                    }
                                    
                                    let mut stats_lock = stats_clone.write().await;
                                    stats_lock.active_connections -= 1;
                                });
                            }
                            Err(e) => {
                                println!("Accept error: {}", e);
                                let mut stats_lock = stats.write().await;
                                stats_lock.error_count += 1;
                            }
                        }
                    }
                }
            }
        });
        
        println!("QuicCrunch server started successfully");
        Ok(())
    }
    
    /// Stop the server gracefully
    pub async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
            
            // Wait for graceful shutdown
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            let stats = self.stats.read().await;
            println!("QuicCrunch server stopped. Stats: total_connections={}, total_messages={}, errors={}", 
                stats.total_connections, stats.total_messages, stats.error_count);
        }
        Ok(())
    }
    
    /// Handle individual connection with all streaming patterns
    async fn handle_connection(
        conn: crate::Connection,
        stats: Arc<RwLock<ServerStats>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Handle multiple streams per connection
        for _stream_id in 0..1000 {
            let stream_result = timeout(
                Duration::from_secs(5),
                conn.accept_bi()
            ).await;
            
            match stream_result {
                Ok(Ok(mut stream)) => {
                    let stats_clone = stats.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_stream(stream, stats_clone).await {
                            println!("Stream error: {}", e);
                        }
                    });
                }
                Ok(Err(_)) | Err(_) => break,
            }
        }
        
        Ok(())
    }
    
    /// Handle individual stream with pattern detection
    async fn handle_stream(
        mut stream: crate::QuicStream,
        stats: Arc<RwLock<ServerStats>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Receive first message to determine pattern
        let first_msg_result = timeout(
            Duration::from_secs(2),
            stream.receive_message()
        ).await;
        
        let first_msg_data = match first_msg_result {
            Ok(Ok(data)) => data,
            Ok(Err(e)) => return Err(format!("First message receive failed: {}", e).into()),
            Err(_) => return Err("First message timeout".into()),
        };
        
        // Empty message indicates end of stream
        if first_msg_data.is_empty() {
            return Ok(());
        }
        
        // Parse message to determine streaming pattern
        let first_msg = match QuicCrunchMessage::from_bytes(&first_msg_data) {
            Ok(msg) => msg,
            Err(_) => {
                // Handle as binary data for simple echo
                return Self::handle_binary_echo(&mut stream, first_msg_data, stats).await;
            }
        };
        
        {
            let mut stats_lock = stats.write().await;
            stats_lock.total_messages += 1;
        }
        
        match first_msg.test_type.as_str() {
            "unary" | "warmup" => {
                Self::handle_unary_pattern(&mut stream, first_msg, stats).await
            }
            "client_streaming" => {
                Self::handle_client_streaming_pattern(&mut stream, first_msg, stats).await
            }
            "server_streaming" => {
                Self::handle_server_streaming_pattern(&mut stream, first_msg, stats).await
            }
            "bidirectional" => {
                Self::handle_bidirectional_pattern(&mut stream, first_msg, stats).await
            }
            _ => {
                // Default to unary echo
                Self::handle_unary_pattern(&mut stream, first_msg, stats).await
            }
        }
    }
    
    /// Handle simple binary echo for backward compatibility
    async fn handle_binary_echo(
        stream: &mut crate::QuicStream,
        data: Bytes,
        stats: Arc<RwLock<ServerStats>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Simple echo back
        stream.send_message(data).await?;
        
        let mut stats_lock = stats.write().await;
        stats_lock.total_messages += 1;
        
        Ok(())
    }
    
    /// Handle unary RPC pattern (single request -> single response)
    async fn handle_unary_pattern(
        stream: &mut crate::QuicStream,
        request: QuicCrunchMessage,
        stats: Arc<RwLock<ServerStats>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Create response message
        let response = QuicCrunchMessage {
            id: format!("resp-{}", request.id),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            payload: request.payload, // Echo back payload
            test_type: "unary_response".to_string(),
            sequence_number: request.sequence_number,
        };
        
        // Send response
        let response_bytes = response.to_bytes()?;
        stream.send_message(response_bytes).await?;
        
        Ok(())
    }
    
    /// Handle client streaming pattern (multiple requests -> single response)
    async fn handle_client_streaming_pattern(
        stream: &mut crate::QuicStream,
        _first_request: QuicCrunchMessage,
        stats: Arc<RwLock<ServerStats>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut message_count = 1; // Already received first message
        let mut total_bytes = 0;
        
        // Receive all messages until end-of-stream
        loop {
            let msg_result = timeout(
                Duration::from_secs(1),
                stream.receive_message()
            ).await;
            
            match msg_result {
                Ok(Ok(data)) => {
                    if data.is_empty() {
                        break; // End of stream
                    }
                    message_count += 1;
                    total_bytes += data.len();
                    
                    let mut stats_lock = stats.write().await;
                    stats_lock.total_messages += 1;
                }
                Ok(Err(_)) | Err(_) => break,
            }
        }
        
        // Send single aggregated response
        let response = QuicCrunchMessage {
            id: format!("client_stream_resp_{}", message_count),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            payload: vec![0u8; 100], // Small response payload
            test_type: "client_streaming_response".to_string(),
            sequence_number: message_count as u32,
        };
        
        let response_bytes = response.to_bytes()?;
        stream.send_message(response_bytes).await?;
        
        Ok(())
    }
    
    /// Handle server streaming pattern (single request -> multiple responses)
    async fn handle_server_streaming_pattern(
        stream: &mut crate::QuicStream,
        request: QuicCrunchMessage,
        stats: Arc<RwLock<ServerStats>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Send multiple responses (default 10, can be customized)
        let response_count = 10;
        
        for i in 0..response_count {
            let response = QuicCrunchMessage {
                id: format!("server_resp_{}_{}", request.id, i),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64,
                payload: request.payload.clone(),
                test_type: "server_streaming_response".to_string(),
                sequence_number: i,
            };
            
            let response_bytes = response.to_bytes()?;
            stream.send_message(response_bytes).await?;
            
            // Small delay between messages
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        
        let mut stats_lock = stats.write().await;
        stats_lock.total_messages += response_count as u64;
        
        Ok(())
    }
    
    /// Handle bidirectional streaming pattern (multiple requests <-> multiple responses)
    async fn handle_bidirectional_pattern(
        stream: &mut crate::QuicStream,
        _first_request: QuicCrunchMessage,
        stats: Arc<RwLock<ServerStats>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Note: QuicStream doesn't have clone, so we'll handle both directions sequentially
        let mut message_count = 1; // Already received first
        
        // Receive remaining messages
        loop {
            let msg_result = timeout(
                Duration::from_secs(1),
                stream.receive_message()
            ).await;
            
            match msg_result {
                Ok(Ok(data)) => {
                    if data.is_empty() {
                        break; // End of stream
                    }
                    message_count += 1;
                    
                    let mut stats_lock = stats.write().await;
                    stats_lock.total_messages += 1;
                }
                Ok(Err(_)) | Err(_) => break,
            }
        }
        
        // Send responses
        for i in 0..10 { // Default response count
            let response = QuicCrunchMessage {
                id: format!("bidir_resp_{}", i),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64,
                payload: vec![42u8; 100], // Small response
                test_type: "bidirectional_response".to_string(),
                sequence_number: i,
            };
            
            if let Ok(response_bytes) = response.to_bytes() {
                if stream.send_message(response_bytes).await.is_err() {
                    break;
                }
            }
            
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        
        let mut stats_lock = stats.write().await;
        stats_lock.total_messages += 10;
        
        Ok(())
    }
    
    /// Get current server statistics
    pub async fn get_stats(&self) -> ServerStats {
        self.stats.read().await.clone()
    }
}

impl Drop for QuicCrunchServer {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::certs::generate_test_certs;
    
    #[tokio::test]
    async fn test_server_creation() {
        let (cert, key, ca_cert) = generate_test_certs().unwrap();
        let tls_config = TlsConfig {
            cert: cert.clone(),
            key: key.clone(),
            ca_cert: ca_cert.clone(),
            cert_pem: vec![],
            key_pem: vec![],
            ca_cert_pem: vec![],
        };
        
        let server = QuicCrunchServer::new(tls_config);
        assert!(server.is_ok());
    }
}