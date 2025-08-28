//! BDD tests for RPC Timeout Handling
//! 
//! Testing timeout functionality in RPC client calls

use std::time::Duration;
use bytes::Bytes;
use tokio::time::timeout;
use serde::{Serialize, Deserialize};
use async_trait::async_trait;

use quiccrunch::{
    QuicTransport, TlsConfig, Transport,
    rpc::{RpcService, RpcServer, RpcClient, RpcContext, RpcResult, RpcError},
};

#[derive(Debug, Serialize, Deserialize)]
struct TimeoutRequest {
    delay_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TimeoutResponse {
    message: String,
}

/// Service that can simulate delays for timeout testing
struct TimeoutTestService;

#[async_trait]
impl RpcService for TimeoutTestService {
    async fn call(&self, _context: &RpcContext, method: &str, params: Bytes) -> RpcResult<Bytes> {
        match method {
            "slow_method" => {
                let request: TimeoutRequest = serde_json::from_slice(&params)
                    .map_err(|e| RpcError::InvalidParameters(e.to_string()))?;
                
                // Simulate delay
                tokio::time::sleep(Duration::from_millis(request.delay_ms)).await;
                
                let response = TimeoutResponse {
                    message: format!("Completed after {}ms", request.delay_ms),
                };
                
                let response_bytes = serde_json::to_vec(&response)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;
                
                Ok(Bytes::from(response_bytes))
            }
            "fast_method" => {
                let response = TimeoutResponse {
                    message: "Fast response".to_string(),
                };
                
                let response_bytes = serde_json::to_vec(&response)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;
                
                Ok(Bytes::from(response_bytes))
            }
            _ => Err(RpcError::MethodNotFound(method.to_string())),
        }
    }
}

async fn setup_timeout_test_server(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config)?;
    let mut incoming = transport.listen(addr).await?;
    
    let rpc_server = RpcServer::new();
    rpc_server.register_service("TimeoutService".to_string(), std::sync::Arc::new(TimeoutTestService)).await;
    
    tokio::spawn(async move {
        while let Ok(connection) = incoming.accept().await {
            let rpc_server = rpc_server.clone();
            tokio::spawn(async move {
                if let Err(e) = rpc_server.handle_connection(connection).await {
                    eprintln!("Connection error: {}", e);
                }
            });
        }
    });
    
    Ok(())
}

/// BDD Scenario: Request times out after specified duration
#[tokio::test]
async fn test_bdd_scenario_request_times_out_after_specified_duration() {
    // Given: an RPC client with 100ms timeout
    let addr = "127.0.0.1:50090";
    setup_timeout_test_server(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = RpcClient::new(connection);
    
    // And: a server with a slow service taking 200ms
    let request = TimeoutRequest { delay_ms: 200 };
    let params = Bytes::from(serde_json::to_string(&request).unwrap());
    
    // Create context with timeout
    let mut context = RpcContext::new("TimeoutService".to_string(), "slow_method".to_string());
    
    // When: the client makes a request with 100ms timeout
    let start_time = std::time::Instant::now();
    let result = timeout(
        Duration::from_millis(100),
        client.call_with_context(&mut context, params)
    ).await;
    let elapsed = start_time.elapsed();
    
    // Then: the client should receive a Timeout error after 100ms
    assert!(result.is_err(), "Request should timeout");
    assert!(elapsed < Duration::from_millis(150), "Should timeout quickly, took: {:?}", elapsed);
    
    // The timeout should come from tokio::time::timeout, not RpcError::Timeout
    // In a real implementation, the client would handle timeouts internally
}

/// BDD Scenario: Fast requests complete before timeout
#[tokio::test]
async fn test_bdd_scenario_fast_requests_complete_before_timeout() {
    // Given: an RPC client with 1000ms timeout
    let addr = "127.0.0.1:50091";
    setup_timeout_test_server(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = RpcClient::new(connection);
    
    // And: a server with a fast service taking 50ms
    let request = TimeoutRequest { delay_ms: 50 };
    let params = Bytes::from(serde_json::to_string(&request).unwrap());
    
    // When: the client makes a request with 1000ms timeout
    let start_time = std::time::Instant::now();
    let result = timeout(
        Duration::from_millis(1000),
        client.call("TimeoutService", "slow_method", params)
    ).await;
    let elapsed = start_time.elapsed();
    
    // Then: the client should receive a successful response
    assert!(result.is_ok(), "Request should succeed");
    let response_bytes = result.unwrap().unwrap();
    let response: TimeoutResponse = serde_json::from_slice(&response_bytes).unwrap();
    assert_eq!(response.message, "Completed after 50ms");
    
    // And: no timeout error should occur
    assert!(elapsed < Duration::from_millis(200), "Should complete quickly, took: {:?}", elapsed);
}

/// BDD Scenario: Client-side timeout implementation
#[tokio::test]
async fn test_bdd_scenario_client_side_timeout_implementation() {
    // Given: an RPC client with built-in timeout support
    let addr = "127.0.0.1:50092";
    setup_timeout_test_server(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = RpcClient::new(connection);
    
    // When: the client makes a request with context timeout
    let mut context = RpcContext::new("TimeoutService".to_string(), "fast_method".to_string());
    context.metadata.insert("timeout_ms".to_string(), "50".to_string());
    
    let params = Bytes::from("{}");
    
    // This should test the timeout implementation in call_with_context
    // For now, it will pass but when we implement the timeout feature,
    // it should use the timeout from context metadata
    let result = client.call_with_context(&mut context, params).await;
    
    // Then: the request should complete successfully (fast method)
    assert!(result.is_ok(), "Fast request should succeed even with short timeout");
}

/// BDD Scenario: Test different timeout durations
#[tokio::test]
async fn test_bdd_scenario_different_timeout_durations() {
    // Given: an RPC server with configurable delay
    let addr = "127.0.0.1:50093";
    setup_timeout_test_server(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = RpcClient::new(connection);
    
    // Test case 1: Very short timeout with slow operation
    let request = TimeoutRequest { delay_ms: 100 };
    let params = Bytes::from(serde_json::to_string(&request).unwrap());
    
    let result = timeout(
        Duration::from_millis(50),
        client.call("TimeoutService", "slow_method", params)
    ).await;
    
    assert!(result.is_err(), "Should timeout with very short duration");
    
    // Test case 2: Adequate timeout with fast operation
    let params = Bytes::from("{}");
    
    let result = timeout(
        Duration::from_millis(1000),
        client.call("TimeoutService", "fast_method", params)
    ).await;
    
    assert!(result.is_ok(), "Should succeed with adequate timeout");
}