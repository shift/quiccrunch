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

//! QuicCrunch client implementation for all streaming patterns

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use bytes::Bytes;
use serde_json;
use tokio::time::sleep;
use crate::{QuicTransport, Transport, TlsConfig};
use crate::transport::TransportSettings;
use crate::benchmark::{
    BenchmarkConfig, StreamingPattern, PerformanceMetrics, 
    generate_test_message, generate_message_id
};
use crate::benchmark::metrics::{MetricsCollector, LatencyTimer, ErrorType};

/// Message structure for QuicCrunch benchmarks
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuicCrunchMessage {
    pub id: String,
    pub timestamp: u64,
    pub payload: Vec<u8>,
    pub test_type: String,
    pub sequence_number: u32,
}

impl QuicCrunchMessage {
    pub fn new(size: usize, sequence: u32, test_type: &str) -> Self {
        // Account for JSON serialization overhead
        let overhead = 100; // Approximate JSON overhead
        let target_payload_size = if size > overhead {
            size - overhead
        } else {
            1
        };
        
        Self {
            id: generate_message_id("quic", sequence),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            payload: generate_test_message(target_payload_size),
            test_type: test_type.to_string(),
            sequence_number: sequence,
        }
    }
    
    pub fn to_bytes(&self) -> Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        let json = serde_json::to_vec(self)?;
        Ok(Bytes::from(json))
    }
    
    pub fn from_bytes(data: &[u8]) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let msg: Self = serde_json::from_slice(data)?;
        Ok(msg)
    }
}

/// QuicCrunch benchmark client
pub struct QuicCrunchClient {
    transport: QuicTransport,
    metrics: Arc<MetricsCollector>,
}

impl QuicCrunchClient {
    /// Create a new QuicCrunch client with appropriate settings
    pub fn new(tls_config: TlsConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Use larger window settings to support all message sizes
        let settings = TransportSettings {
            stream_receive_window: 10 * 1024 * 1024, // 10MB
            connection_receive_window: 20 * 1024 * 1024, // 20MB
            ..Default::default()
        };
        
        let transport = QuicTransport::with_settings(tls_config, settings)?;
        let metrics = Arc::new(MetricsCollector::new()?);
        
        Ok(Self { transport, metrics })
    }
    
    /// Run benchmark for a specific streaming pattern
    pub async fn run_benchmark(
        &self,
        config: &BenchmarkConfig,
        server_addr: &str,
    ) -> Result<PerformanceMetrics, Box<dyn std::error::Error + Send + Sync>> {
        // Reset metrics for new test
        self.metrics.reset()?;
        
        // Warmup phase
        self.run_warmup(config, server_addr).await?;
        
        // Main benchmark phase
        match &config.streaming_pattern {
            StreamingPattern::Unary => {
                self.run_unary_benchmark(config, server_addr).await
            }
            StreamingPattern::ClientStreaming { message_count } => {
                self.run_client_streaming_benchmark(config, server_addr, *message_count).await
            }
            StreamingPattern::ServerStreaming { message_count } => {
                self.run_server_streaming_benchmark(config, server_addr, *message_count).await
            }
            StreamingPattern::Bidirectional { message_count } => {
                self.run_bidirectional_benchmark(config, server_addr, *message_count).await
            }
        }
    }
    
    /// Warmup phase to stabilize connections
    async fn run_warmup(
        &self,
        config: &BenchmarkConfig,
        server_addr: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Running warmup phase...");
        
        let conn = self.transport.connect(server_addr).await
            .map_err(|e| format!("Warmup connection failed: {}", e))?;
        let mut stream = conn.open_bi().await
            .map_err(|e| format!("Warmup stream failed: {}", e))?;
        
        // Send a few simple messages to warm up the connection
        for i in 0..5 {
            let msg = QuicCrunchMessage::new(config.message_size, i, "warmup");
            let payload = msg.to_bytes()?;
            
            if let Err(e) = stream.send_message(payload).await {
                println!("Warmup send failed: {}", e);
                continue;
            }
            
            if let Err(e) = stream.receive_message().await {
                println!("Warmup receive failed: {}", e);
                continue;
            }
        }
        
        sleep(Duration::from_millis(100)).await;
        println!("Warmup completed");
        Ok(())
    }
    
    /// Run unary RPC benchmark
    async fn run_unary_benchmark(
        &self,
        config: &BenchmarkConfig,
        server_addr: &str,
    ) -> Result<PerformanceMetrics, Box<dyn std::error::Error + Send + Sync>> {
        println!("Running unary benchmark...");
        
        let mut handles = Vec::new();
        let requests_per_client = 100; // Fixed number per client
        
        for client_id in 0..config.concurrent_connections {
            let transport = self.transport.clone();
            let metrics = self.metrics.clone();
            let message_size = config.message_size;
            let server_addr = server_addr.to_string();
            
            let handle = tokio::spawn(async move {
                let conn = match transport.connect(&server_addr).await {
                    Ok(conn) => conn,
                    Err(e) => {
                        metrics.record_error(ErrorType::ConnectionFailure);
                        return Err(format!("Connection failed: {}", e));
                    }
                };
                
                let mut stream = match conn.open_bi().await {
                    Ok(stream) => stream,
                    Err(e) => {
                        metrics.record_error(ErrorType::ConnectionFailure);
                        return Err(format!("Stream open failed: {}", e));
                    }
                };
                
                for i in 0..requests_per_client {
                    let msg = QuicCrunchMessage::new(
                        message_size, 
                        (client_id * requests_per_client + i) as u32, 
                        "unary"
                    );
                    let payload = msg.to_bytes().map_err(|e| format!("Serialization failed: {}", e))?;
                    
                    let timer = LatencyTimer::new();
                    
                    // Send request
                    if let Err(e) = stream.send_message(payload.clone()).await {
                        metrics.record_error(ErrorType::MessageFailure);
                        continue;
                    }
                    
                    // Receive response
                    match stream.receive_message().await {
                        Ok(_response) => {
                            let latency_micros = timer.elapsed_micros();
                            let _ = metrics.record_latency(latency_micros);
                            metrics.record_throughput(payload.len() as u64, Duration::from_millis(1));
                        }
                        Err(e) => {
                            metrics.record_error(ErrorType::MessageFailure);
                            continue;
                        }
                    }
                    
                    // Rate limiting if specified
                    if let Some(target_qps) = config.target_qps {
                        let delay = Duration::from_millis(1000 / target_qps);
                        sleep(delay).await;
                    }
                }
                
                Ok::<_, String>(())
            });
            
            handles.push(handle);
        }
        
        // Wait for all clients to complete
        for handle in handles {
            if let Err(e) = handle.await {
                println!("Client error: {:?}", e);
            }
        }
        
        // Update resource usage before generating final metrics
        self.metrics.update_resource_usage();
        
        Ok(self.metrics.generate_metrics())
    }
    
    /// Run client streaming benchmark
    async fn run_client_streaming_benchmark(
        &self,
        config: &BenchmarkConfig,
        server_addr: &str,
        message_count: usize,
    ) -> Result<PerformanceMetrics, Box<dyn std::error::Error + Send + Sync>> {
        println!("Running client streaming benchmark with {} messages per stream...", message_count);
        
        let mut handles = Vec::new();
        
        for client_id in 0..config.concurrent_connections {
            let transport = self.transport.clone();
            let metrics = self.metrics.clone();
            let message_size = config.message_size;
            let server_addr = server_addr.to_string();
            
            let handle = tokio::spawn(async move {
                let conn = match transport.connect(&server_addr).await {
                    Ok(conn) => conn,
                    Err(e) => {
                        metrics.record_error(ErrorType::ConnectionFailure);
                        return Err(format!("Connection failed: {}", e));
                    }
                };
                
                let mut stream = match conn.open_bi().await {
                    Ok(stream) => stream,
                    Err(e) => {
                        metrics.record_error(ErrorType::ConnectionFailure);
                        return Err(format!("Stream open failed: {}", e));
                    }
                };
                
                let timer = LatencyTimer::new();
                
                // Send multiple messages
                for i in 0..message_count {
                    let msg = QuicCrunchMessage::new(
                        message_size, 
                        (client_id * message_count + i) as u32, 
                        "client_streaming"
                    );
                    let payload = msg.to_bytes().map_err(|e| format!("Serialization failed: {}", e))?;
                    
                    if let Err(e) = stream.send_message(payload.clone()).await {
                        metrics.record_error(ErrorType::MessageFailure);
                        continue;
                    }
                    
                    metrics.record_throughput(payload.len() as u64, Duration::from_millis(1));
                }
                
                // Send end-of-stream signal (empty message)
                let _ = stream.send_message(Bytes::new()).await;
                
                // Receive single response
                match stream.receive_message().await {
                    Ok(_response) => {
                        let latency_micros = timer.elapsed_micros();
                        let _ = metrics.record_latency(latency_micros);
                    }
                    Err(e) => {
                        metrics.record_error(ErrorType::MessageFailure);
                    }
                }
                
                Ok::<_, String>(())
            });
            
            handles.push(handle);
        }
        
        // Wait for all clients to complete
        for handle in handles {
            if let Err(e) = handle.await {
                println!("Client error: {:?}", e);
            }
        }
        
        self.metrics.update_resource_usage();
        Ok(self.metrics.generate_metrics())
    }
    
    /// Run server streaming benchmark
    async fn run_server_streaming_benchmark(
        &self,
        config: &BenchmarkConfig,
        server_addr: &str,
        message_count: usize,
    ) -> Result<PerformanceMetrics, Box<dyn std::error::Error + Send + Sync>> {
        println!("Running server streaming benchmark expecting {} messages per stream...", message_count);
        
        let mut handles = Vec::new();
        
        for client_id in 0..config.concurrent_connections {
            let transport = self.transport.clone();
            let metrics = self.metrics.clone();
            let message_size = config.message_size;
            let server_addr = server_addr.to_string();
            
            let handle = tokio::spawn(async move {
                let conn = match transport.connect(&server_addr).await {
                    Ok(conn) => conn,
                    Err(e) => {
                        metrics.record_error(ErrorType::ConnectionFailure);
                        return Err(format!("Connection failed: {}", e));
                    }
                };
                
                let mut stream = match conn.open_bi().await {
                    Ok(stream) => stream,
                    Err(e) => {
                        metrics.record_error(ErrorType::ConnectionFailure);
                        return Err(format!("Stream open failed: {}", e));
                    }
                };
                
                // Send single request with message count in payload
                let msg = QuicCrunchMessage::new(message_size, client_id as u32, "server_streaming");
                let payload = msg.to_bytes().map_err(|e| format!("Serialization failed: {}", e))?;
                
                let timer = LatencyTimer::new();
                
                if let Err(e) = stream.send_message(payload.clone()).await {
                    metrics.record_error(ErrorType::MessageFailure);
                    return Err(format!("Send failed: {}", e));
                }
                
                // Receive multiple responses
                for _i in 0..message_count {
                    match stream.receive_message().await {
                        Ok(response) => {
                            metrics.record_throughput(response.len() as u64, Duration::from_millis(1));
                        }
                        Err(e) => {
                            metrics.record_error(ErrorType::MessageFailure);
                            break;
                        }
                    }
                }
                
                let latency_micros = timer.elapsed_micros();
                let _ = metrics.record_latency(latency_micros);
                
                Ok::<_, String>(())
            });
            
            handles.push(handle);
        }
        
        // Wait for all clients to complete
        for handle in handles {
            if let Err(e) = handle.await {
                println!("Client error: {:?}", e);
            }
        }
        
        self.metrics.update_resource_usage();
        Ok(self.metrics.generate_metrics())
    }
    
    /// Run bidirectional streaming benchmark
    async fn run_bidirectional_benchmark(
        &self,
        config: &BenchmarkConfig,
        server_addr: &str,
        message_count: usize,
    ) -> Result<PerformanceMetrics, Box<dyn std::error::Error + Send + Sync>> {
        println!("Running bidirectional streaming benchmark with {} messages each way...", message_count);
        
        let mut handles = Vec::new();
        
        for client_id in 0..config.concurrent_connections {
            let transport = self.transport.clone();
            let metrics = self.metrics.clone();
            let message_size = config.message_size;
            let server_addr = server_addr.to_string();
            
            let handle = tokio::spawn(async move {
                let conn = match transport.connect(&server_addr).await {
                    Ok(conn) => conn,
                    Err(e) => {
                        metrics.record_error(ErrorType::ConnectionFailure);
                        return Err(format!("Connection failed: {}", e));
                    }
                };
                
                let mut stream = match conn.open_bi().await {
                    Ok(stream) => stream,
                    Err(e) => {
                        metrics.record_error(ErrorType::ConnectionFailure);
                        return Err(format!("Stream open failed: {}", e));
                    }
                };
                
                // Concurrent send and receive (sequential for QuicStream without Clone)
                let timer = LatencyTimer::new();
                
                // Send messages first
                for i in 0..message_count {
                    let msg = QuicCrunchMessage::new(
                        message_size, 
                        (client_id * message_count + i) as u32, 
                        "bidirectional"
                    );
                    let payload = msg.to_bytes().map_err(|e| format!("Serialization failed: {}", e))?;
                    
                    if let Err(e) = stream.send_message(payload.clone()).await {
                        metrics.record_error(ErrorType::MessageFailure);
                        continue;
                    }
                    
                    metrics.record_throughput(payload.len() as u64, Duration::from_millis(1));
                    
                    // Small delay to avoid overwhelming
                    sleep(Duration::from_millis(1)).await;
                }
                
                // Send end signal
                let _ = stream.send_message(Bytes::new()).await;
                
                // Then receive responses
                for _i in 0..message_count {
                    match stream.receive_message().await {
                        Ok(response) => {
                            metrics.record_throughput(response.len() as u64, Duration::from_millis(1));
                        }
                        Err(e) => {
                            metrics.record_error(ErrorType::MessageFailure);
                            break;
                        }
                    }
                }
                
                let latency_micros = timer.elapsed_micros();
                let _ = metrics.record_latency(latency_micros);
                
                Ok::<_, String>(())
            });
            
            handles.push(handle);
        }
        
        // Wait for all clients to complete
        for handle in handles {
            if let Err(e) = handle.await {
                println!("Client error: {:?}", e);
            }
        }
        
        self.metrics.update_resource_usage();
        Ok(self.metrics.generate_metrics())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_quiccrunch_message_serialization() {
        let msg = QuicCrunchMessage::new(1024, 42, "test");
        let bytes = msg.to_bytes().unwrap();
        let deserialized = QuicCrunchMessage::from_bytes(&bytes).unwrap();
        
        assert_eq!(msg.sequence_number, deserialized.sequence_number);
        assert_eq!(msg.test_type, deserialized.test_type);
        assert_eq!(msg.payload.len(), deserialized.payload.len());
    }
}