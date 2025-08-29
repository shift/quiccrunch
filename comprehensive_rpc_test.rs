use std::time::{Duration, Instant};
use std::sync::Arc;
use tokio::time::timeout;
use bytes::Bytes;
use quiccrunch_core::{
    rpc::{RpcServer, RpcClient},
    transport::TransportSettings,
    config::QuicTransportSettings,
};
use serde::{Deserialize, Serialize};

/// Comprehensive RPC Test Results
#[derive(Debug, Serialize, Deserialize)]
pub struct RpcTestResults {
    pub test_timestamp: String,
    pub backend: String,
    pub features: Vec<String>,
    pub simple_rpc: SimpleRpcResults,
    pub client_streaming: StreamingResults,
    pub server_streaming: StreamingResults,
    pub bidirectional_streaming: StreamingResults,
    pub latency_distribution: LatencyDistribution,
    pub throughput_measurements: ThroughputMeasurements,
    pub message_size_analysis: MessageSizeAnalysis,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimpleRpcResults {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub average_latency_ms: f64,
    pub min_latency_ms: f64,
    pub max_latency_ms: f64,
    pub requests_per_second: f64,
    pub total_test_duration_ms: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamingResults {
    pub total_streams: u64,
    pub successful_streams: u64,
    pub failed_streams: u64,
    pub average_stream_duration_ms: f64,
    pub total_messages_sent: u64,
    pub total_messages_received: u64,
    pub average_message_latency_ms: f64,
    pub throughput_mbps: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LatencyDistribution {
    pub p50_ms: f64,
    pub p90_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub p999_ms: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThroughputMeasurements {
    pub peak_requests_per_second: f64,
    pub sustained_requests_per_second: f64,
    pub peak_mbps: f64,
    pub sustained_mbps: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageSizeAnalysis {
    pub small_messages_1kb: MessageSizeResult,
    pub medium_messages_64kb: MessageSizeResult,
    pub large_messages_512kb: MessageSizeResult,
    pub huge_messages_2mb: MessageSizeResult,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MessageSizeResult {
    pub size_bytes: usize,
    pub average_latency_ms: f64,
    pub throughput_mbps: f64,
    pub requests_per_second: f64,
    pub optimization_used: String,
}

/// Test message structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestRequest {
    pub id: u64,
    pub data: Vec<u8>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResponse {
    pub id: u64,
    pub echo_data: Vec<u8>,
    pub server_timestamp: u64,
    pub processing_time_ns: u64,
}

/// Comprehensive RPC Test Suite
pub struct ComprehensiveRpcTestSuite {
    pub server_addr: String,
    pub test_duration: Duration,
    pub concurrent_clients: usize,
    pub features: Vec<String>,
}

impl ComprehensiveRpcTestSuite {
    pub fn new(server_addr: String, test_duration: Duration, concurrent_clients: usize, features: Vec<String>) -> Self {
        Self {
            server_addr,
            test_duration,
            concurrent_clients,
            features,
        }
    }

    /// Run comprehensive RPC test suite
    pub async fn run_full_test_suite(&self) -> Result<RpcTestResults, Box<dyn std::error::Error + Send + Sync>> {
        println!("🚀 Starting Comprehensive RPC Test Suite");
        println!("Backend: tQUIC");
        println!("Features: {:?}", self.features);
        println!("Server: {}", self.server_addr);
        println!("Duration: {:?}", self.test_duration);
        println!("Concurrent clients: {}", self.concurrent_clients);
        println!();

        // Start test server
        let server_handle = self.start_test_server().await?;
        
        // Wait for server to be ready
        tokio::time::sleep(Duration::from_millis(500)).await;

        let test_start = Instant::now();

        // 1. Simple RPC Test
        println!("📡 Testing Simple RPC...");
        let simple_rpc = self.test_simple_rpc().await?;
        println!("   ✅ Simple RPC: {:.2} req/s, {:.2}ms avg latency", 
                simple_rpc.requests_per_second, simple_rpc.average_latency_ms);

        // 2. Client Streaming Test
        println!("📤 Testing Client Streaming...");
        let client_streaming = self.test_client_streaming().await?;
        println!("   ✅ Client Streaming: {:.2} Mbps, {:.0} messages", 
                client_streaming.throughput_mbps, client_streaming.total_messages_sent);

        // 3. Server Streaming Test
        println!("📥 Testing Server Streaming...");
        let server_streaming = self.test_server_streaming().await?;
        println!("   ✅ Server Streaming: {:.2} Mbps, {:.0} messages", 
                server_streaming.throughput_mbps, server_streaming.total_messages_received);

        // 4. Bidirectional Streaming Test
        println!("🔄 Testing Bidirectional Streaming...");
        let bidirectional_streaming = self.test_bidirectional_streaming().await?;
        println!("   ✅ Bidirectional: {:.2} Mbps, {:.0} messages each way", 
                bidirectional_streaming.throughput_mbps, 
                bidirectional_streaming.total_messages_sent);

        // 5. Latency Distribution Analysis
        println!("📊 Analyzing Latency Distribution...");
        let latency_distribution = self.analyze_latency_distribution().await?;
        println!("   ✅ Latency P95: {:.2}ms, P99: {:.2}ms", 
                latency_distribution.p95_ms, latency_distribution.p99_ms);

        // 6. Throughput Measurements
        println!("🚄 Measuring Peak Throughput...");
        let throughput_measurements = self.measure_peak_throughput().await?;
        println!("   ✅ Peak: {:.2} req/s, {:.2} Mbps", 
                throughput_measurements.peak_requests_per_second, 
                throughput_measurements.peak_mbps);

        // 7. Message Size Analysis
        println!("📏 Testing Different Message Sizes...");
        let message_size_analysis = self.test_message_sizes().await?;
        println!("   ✅ 1KB: {:.2} req/s, 2MB: {:.2} req/s", 
                message_size_analysis.small_messages_1kb.requests_per_second,
                message_size_analysis.huge_messages_2mb.requests_per_second);

        let total_duration = test_start.elapsed();
        println!();
        println!("🎉 Test Suite Completed in {:.2}s", total_duration.as_secs_f64());

        Ok(RpcTestResults {
            test_timestamp: chrono::Utc::now().to_rfc3339(),
            backend: "tQUIC".to_string(),
            features: self.features.clone(),
            simple_rpc,
            client_streaming,
            server_streaming,
            bidirectional_streaming,
            latency_distribution,
            throughput_measurements,
            message_size_analysis,
        })
    }

    /// Start test server
    async fn start_test_server(&self) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error + Send + Sync>> {
        let server_addr = self.server_addr.clone();
        let handle = tokio::spawn(async move {
            let settings = QuicTransportSettings::default();
            let mut server = RpcServer::new(settings).await.expect("Failed to create server");
            
            // Register test services
            server.register_service("TestService", Box::new(TestServiceImpl::new())).await
                .expect("Failed to register service");
            
            // Start listening
            let _ = server.listen(&server_addr).await;
        });
        
        Ok(handle)
    }

    /// Test simple request-response RPC
    async fn test_simple_rpc(&self) -> Result<SimpleRpcResults, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        let mut latencies = Vec::new();
        let mut successful = 0u64;
        let mut failed = 0u64;
        let total_requests = 1000u64;

        let settings = QuicTransportSettings::default();
        let client = RpcClient::new(settings).await?;
        client.connect(&self.server_addr).await?;

        for i in 0..total_requests {
            let request_start = Instant::now();
            
            let request = TestRequest {
                id: i,
                data: vec![0u8; 1024], // 1KB test data
                timestamp: request_start.duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64,
            };

            match timeout(Duration::from_secs(5), client.call("TestService", "echo", &request)).await {
                Ok(Ok(_response)) => {
                    let latency = request_start.elapsed().as_millis() as f64;
                    latencies.push(latency);
                    successful += 1;
                }
                _ => {
                    failed += 1;
                }
            }
        }

        let total_duration = start_time.elapsed();
        let avg_latency = latencies.iter().sum::<f64>() / latencies.len() as f64;
        let min_latency = latencies.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max_latency = latencies.iter().fold(0.0, |a, &b| a.max(b));
        let requests_per_second = successful as f64 / total_duration.as_secs_f64();

        Ok(SimpleRpcResults {
            total_requests,
            successful_requests: successful,
            failed_requests: failed,
            average_latency_ms: avg_latency,
            min_latency_ms: min_latency,
            max_latency_ms: max_latency,
            requests_per_second,
            total_test_duration_ms: total_duration.as_millis() as f64,
        })
    }

    /// Test client streaming
    async fn test_client_streaming(&self) -> Result<StreamingResults, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        let settings = QuicTransportSettings::default();
        let client = RpcClient::new(settings).await?;
        client.connect(&self.server_addr).await?;

        let stream_count = 10u64;
        let messages_per_stream = 100u64;
        let mut total_messages_sent = 0u64;
        let mut successful_streams = 0u64;

        for _stream_id in 0..stream_count {
            let mut stream = client.client_stream("TestService", "client_echo").await?;
            
            for i in 0..messages_per_stream {
                let request = TestRequest {
                    id: i,
                    data: vec![0u8; 4096], // 4KB per message
                    timestamp: start_time.elapsed().as_nanos() as u64,
                };
                
                if stream.send(&request).await.is_ok() {
                    total_messages_sent += 1;
                }
            }
            
            if stream.finish().await.is_ok() {
                successful_streams += 1;
            }
        }

        let total_duration = start_time.elapsed();
        let total_bytes = total_messages_sent * 4096;
        let throughput_mbps = (total_bytes as f64 * 8.0) / (total_duration.as_secs_f64() * 1_000_000.0);

        Ok(StreamingResults {
            total_streams: stream_count,
            successful_streams,
            failed_streams: stream_count - successful_streams,
            average_stream_duration_ms: total_duration.as_millis() as f64 / stream_count as f64,
            total_messages_sent,
            total_messages_received: 0, // Client streaming doesn't receive during stream
            average_message_latency_ms: 0.0, // Would need server timestamps to calculate
            throughput_mbps,
        })
    }

    /// Test server streaming
    async fn test_server_streaming(&self) -> Result<StreamingResults, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        let settings = QuicTransportSettings::default();
        let client = RpcClient::new(settings).await?;
        client.connect(&self.server_addr).await?;

        let stream_count = 10u64;
        let mut total_messages_received = 0u64;
        let mut successful_streams = 0u64;

        for stream_id in 0..stream_count {
            let request = TestRequest {
                id: stream_id,
                data: vec![0u8; 1024],
                timestamp: start_time.elapsed().as_nanos() as u64,
            };

            let mut stream = client.server_stream("TestService", "server_echo", &request).await?;
            
            while let Some(_response) = stream.recv().await? {
                total_messages_received += 1;
            }
            
            successful_streams += 1;
        }

        let total_duration = start_time.elapsed();
        let total_bytes = total_messages_received * 4096; // Assume 4KB responses
        let throughput_mbps = (total_bytes as f64 * 8.0) / (total_duration.as_secs_f64() * 1_000_000.0);

        Ok(StreamingResults {
            total_streams: stream_count,
            successful_streams,
            failed_streams: stream_count - successful_streams,
            average_stream_duration_ms: total_duration.as_millis() as f64 / stream_count as f64,
            total_messages_sent: 0,
            total_messages_received,
            average_message_latency_ms: 0.0,
            throughput_mbps,
        })
    }

    /// Test bidirectional streaming
    async fn test_bidirectional_streaming(&self) -> Result<StreamingResults, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        let settings = QuicTransportSettings::default();
        let client = RpcClient::new(settings).await?;
        client.connect(&self.server_addr).await?;

        let stream_count = 5u64;
        let messages_per_stream = 50u64;
        let mut total_messages_sent = 0u64;
        let mut total_messages_received = 0u64;
        let mut successful_streams = 0u64;

        for _stream_id in 0..stream_count {
            let mut stream = client.bidi_stream("TestService", "bidi_echo").await?;
            
            // Send messages
            for i in 0..messages_per_stream {
                let request = TestRequest {
                    id: i,
                    data: vec![0u8; 8192], // 8KB per message
                    timestamp: start_time.elapsed().as_nanos() as u64,
                };
                
                if stream.send(&request).await.is_ok() {
                    total_messages_sent += 1;
                }
            }
            
            // Receive responses
            for _ in 0..messages_per_stream {
                if let Some(_response) = stream.recv().await? {
                    total_messages_received += 1;
                }
            }
            
            successful_streams += 1;
        }

        let total_duration = start_time.elapsed();
        let total_bytes = (total_messages_sent + total_messages_received) * 8192;
        let throughput_mbps = (total_bytes as f64 * 8.0) / (total_duration.as_secs_f64() * 1_000_000.0);

        Ok(StreamingResults {
            total_streams: stream_count,
            successful_streams,
            failed_streams: stream_count - successful_streams,
            average_stream_duration_ms: total_duration.as_millis() as f64 / stream_count as f64,
            total_messages_sent,
            total_messages_received,
            average_message_latency_ms: 0.0,
            throughput_mbps,
        })
    }

    /// Analyze latency distribution
    async fn analyze_latency_distribution(&self) -> Result<LatencyDistribution, Box<dyn std::error::Error + Send + Sync>> {
        let mut latencies = Vec::new();
        let settings = QuicTransportSettings::default();
        let client = RpcClient::new(settings).await?;
        client.connect(&self.server_addr).await?;

        // Collect 1000 latency samples
        for i in 0..1000 {
            let start = Instant::now();
            let request = TestRequest {
                id: i,
                data: vec![0u8; 512],
                timestamp: start.duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64,
            };

            let _ = client.call("TestService", "echo", &request).await;
            let latency_ms = start.elapsed().as_millis() as f64;
            latencies.push(latency_ms);
        }

        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

        Ok(LatencyDistribution {
            p50_ms: latencies[499],
            p90_ms: latencies[899],
            p95_ms: latencies[949],
            p99_ms: latencies[989],
            p999_ms: latencies[998],
        })
    }

    /// Measure peak throughput
    async fn measure_peak_throughput(&self) -> Result<ThroughputMeasurements, Box<dyn std::error::Error + Send + Sync>> {
        // Peak burst test
        let peak_duration = Duration::from_secs(10);
        let peak_start = Instant::now();
        let mut peak_requests = 0u64;
        
        let settings = QuicTransportSettings::default();
        let client = RpcClient::new(settings).await?;
        client.connect(&self.server_addr).await?;

        while peak_start.elapsed() < peak_duration {
            let request = TestRequest {
                id: peak_requests,
                data: vec![0u8; 1024],
                timestamp: peak_start.elapsed().as_nanos() as u64,
            };
            
            if let Ok(_) = timeout(Duration::from_millis(100), client.call("TestService", "echo", &request)).await {
                peak_requests += 1;
            }
        }

        let peak_req_per_sec = peak_requests as f64 / peak_duration.as_secs_f64();
        let peak_mbps = (peak_requests * 1024 * 8) as f64 / (peak_duration.as_secs_f64() * 1_000_000.0);

        // Sustained throughput test
        let sustained_duration = Duration::from_secs(30);
        let sustained_start = Instant::now();
        let mut sustained_requests = 0u64;

        while sustained_start.elapsed() < sustained_duration {
            let request = TestRequest {
                id: sustained_requests,
                data: vec![0u8; 1024],
                timestamp: sustained_start.elapsed().as_nanos() as u64,
            };
            
            if let Ok(_) = client.call("TestService", "echo", &request).await {
                sustained_requests += 1;
            }
            
            // Small delay to simulate realistic load
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        let sustained_req_per_sec = sustained_requests as f64 / sustained_duration.as_secs_f64();
        let sustained_mbps = (sustained_requests * 1024 * 8) as f64 / (sustained_duration.as_secs_f64() * 1_000_000.0);

        Ok(ThroughputMeasurements {
            peak_requests_per_second: peak_req_per_sec,
            sustained_requests_per_second: sustained_req_per_sec,
            peak_mbps,
            sustained_mbps,
        })
    }

    /// Test different message sizes
    async fn test_message_sizes(&self) -> Result<MessageSizeAnalysis, Box<dyn std::error::Error + Send + Sync>> {
        let message_sizes = vec![
            (1024, "small_messages_1kb"),
            (65536, "medium_messages_64kb"),
            (524288, "large_messages_512kb"),
            (2097152, "huge_messages_2mb"),
        ];

        let mut results = Vec::new();
        let settings = QuicTransportSettings::default();
        let client = RpcClient::new(settings).await?;
        client.connect(&self.server_addr).await?;

        for (size, name) in message_sizes {
            let test_start = Instant::now();
            let mut latencies = Vec::new();
            let test_requests = 100u64;

            for i in 0..test_requests {
                let request_start = Instant::now();
                let request = TestRequest {
                    id: i,
                    data: vec![0u8; size],
                    timestamp: request_start.duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64,
                };

                if let Ok(_) = timeout(Duration::from_secs(10), client.call("TestService", "echo", &request)).await {
                    latencies.push(request_start.elapsed().as_millis() as f64);
                }
            }

            let total_duration = test_start.elapsed();
            let avg_latency = latencies.iter().sum::<f64>() / latencies.len() as f64;
            let req_per_sec = latencies.len() as f64 / total_duration.as_secs_f64();
            let throughput_mbps = (latencies.len() * size * 8) as f64 / (total_duration.as_secs_f64() * 1_000_000.0);
            
            // Determine which optimization would be used
            let optimization_used = match size {
                0..=8192 => "Standard".to_string(),
                8193..=65536 => "MemoryPools".to_string(),
                _ => "AdaptiveOptimizations".to_string(),
            };

            results.push(MessageSizeResult {
                size_bytes: size,
                average_latency_ms: avg_latency,
                throughput_mbps,
                requests_per_second: req_per_sec,
                optimization_used,
            });
        }

        Ok(MessageSizeAnalysis {
            small_messages_1kb: results[0].clone(),
            medium_messages_64kb: results[1].clone(),
            large_messages_512kb: results[2].clone(),
            huge_messages_2mb: results[3].clone(),
        })
    }
}

/// Test service implementation
struct TestServiceImpl;

impl TestServiceImpl {
    fn new() -> Self {
        Self
    }

    async fn echo(&self, request: TestRequest) -> Result<TestResponse, Box<dyn std::error::Error + Send + Sync>> {
        let processing_start = Instant::now();
        
        // Simulate some processing
        if request.data.len() > 1024 * 1024 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        
        let processing_time = processing_start.elapsed();
        
        Ok(TestResponse {
            id: request.id,
            echo_data: request.data,
            server_timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64,
            processing_time_ns: processing_time.as_nanos() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_comprehensive_rpc_suite() {
        let test_suite = ComprehensiveRpcTestSuite::new(
            "127.0.0.1:8080".to_string(),
            Duration::from_secs(60),
            10,
            vec!["backend-tquic".to_string()],
        );

        let results = test_suite.run_full_test_suite().await.expect("Test suite failed");
        
        // Verify we got meaningful results
        assert!(results.simple_rpc.requests_per_second > 0.0);
        assert!(results.latency_distribution.p95_ms > 0.0);
        assert!(results.throughput_measurements.peak_requests_per_second > 0.0);
        
        println!("Test Results: {:#?}", results);
    }
}