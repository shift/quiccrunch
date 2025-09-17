//! BDD tests for RPC Server Connection Handling
//!
//! Testing core server functionality following Given-When-Then patterns

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use quiccrunch_core::{
    rpc::{RpcClient, RpcContext, RpcError, RpcResult, RpcServer, RpcService},
    QuicTransport, TlsConfig, Transport,
};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct CalculatorRequest {
    a: i32,
    b: i32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct CalculatorResponse {
    result: i32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct UserRequest {
    user_id: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct UserResponse {
    name: String,
    email: String,
}

/// Calculator service for testing
struct CalculatorService;

#[async_trait]
impl RpcService for CalculatorService {
    async fn call(&self, context: &RpcContext, method: &str, params: Bytes) -> RpcResult<Bytes> {
        match method {
            "add" => {
                let request: CalculatorRequest = serde_json::from_slice(&params)
                    .map_err(|e| RpcError::InvalidParameters(e.to_string()))?;

                let response = CalculatorResponse {
                    result: request.a + request.b,
                };

                let response_bytes = serde_json::to_vec(&response)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;

                Ok(Bytes::from(response_bytes))
            }
            "multiply" => {
                let request: CalculatorRequest = serde_json::from_slice(&params)
                    .map_err(|e| RpcError::InvalidParameters(e.to_string()))?;

                let response = CalculatorResponse {
                    result: request.a * request.b,
                };

                let response_bytes = serde_json::to_vec(&response)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;

                Ok(Bytes::from(response_bytes))
            }
            _ => Err(RpcError::MethodNotFound(method.to_string())),
        }
    }
}

/// User service for testing service routing
struct UserService;

#[async_trait]
impl RpcService for UserService {
    async fn call(&self, context: &RpcContext, method: &str, params: Bytes) -> RpcResult<Bytes> {
        match method {
            "get_user" => {
                let request: UserRequest = serde_json::from_slice(&params)
                    .map_err(|e| RpcError::InvalidParameters(e.to_string()))?;

                let response = UserResponse {
                    name: format!("User {}", request.user_id),
                    email: format!("user{}@example.com", request.user_id),
                };

                let response_bytes = serde_json::to_vec(&response)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;

                Ok(Bytes::from(response_bytes))
            }
            _ => Err(RpcError::MethodNotFound(method.to_string())),
        }
    }
}

async fn setup_test_server_with_services(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config)?;
    let mut incoming = transport.listen(addr).await?;

    let rpc_server = RpcServer::new();

    // Register Calculator service
    rpc_server
        .register_service("Calculator".to_string(), Arc::new(CalculatorService))
        .await;

    // Register User service
    rpc_server
        .register_service("UserService".to_string(), Arc::new(UserService))
        .await;

    // Handle incoming connections
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

#[tokio::test]
async fn test_bdd_scenario_server_accepts_and_processes_rpc_request() {
    // Given: an RPC server is listening on port 8080
    // And: a test service "Calculator" is registered
    let addr = "127.0.0.1:50080";
    setup_test_server_with_services(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create client connection
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = RpcClient::new(connection);

    // When: a client sends an "add" request with parameters {a: 5, b: 3}
    let request = CalculatorRequest { a: 5, b: 3 };
    let params = Bytes::from(serde_json::to_string(&request).unwrap());

    let result = timeout(
        Duration::from_millis(1000),
        client.call("Calculator", "add", params),
    )
    .await
    .expect("Request should not timeout");

    // Then: the server should respond with result 8
    assert!(result.is_ok(), "Request should succeed, got: {:?}", result);
    let response_bytes = result.unwrap();
    let response: CalculatorResponse = serde_json::from_slice(&response_bytes).unwrap();
    assert_eq!(response.result, 8);

    // And: the connection should remain open for further requests
    let request2 = CalculatorRequest { a: 10, b: 20 };
    let params2 = Bytes::from(serde_json::to_string(&request2).unwrap());

    let result2 = timeout(
        Duration::from_millis(1000),
        client.call("Calculator", "multiply", params2),
    )
    .await
    .expect("Second request should not timeout");

    assert!(result2.is_ok(), "Second request should succeed");
    let response2_bytes = result2.unwrap();
    let response2: CalculatorResponse = serde_json::from_slice(&response2_bytes).unwrap();
    assert_eq!(response2.result, 200);
}

#[tokio::test]
async fn test_bdd_scenario_server_handles_malformed_requests_gracefully() {
    // Given: an RPC server is running
    let addr = "127.0.0.1:50081";
    setup_test_server_with_services(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = RpcClient::new(connection);

    // When: a client sends malformed Cap'n Proto data
    let malformed_params = Bytes::from("invalid json data");

    let result = timeout(
        Duration::from_millis(1000),
        client.call("Calculator", "add", malformed_params),
    )
    .await
    .expect("Request should not timeout");

    // Then: the server should respond with InvalidRequest error
    assert!(result.is_err(), "Malformed request should fail");
    match result.unwrap_err() {
        RpcError::InvalidParameters(_) | RpcError::SerializationError(_) => {
            // Expected error types for malformed data
        }
        other => panic!("Unexpected error type: {:?}", other),
    }

    // And: the connection should remain stable
    let valid_request = CalculatorRequest { a: 1, b: 2 };
    let valid_params = Bytes::from(serde_json::to_string(&valid_request).unwrap());

    let valid_result = timeout(
        Duration::from_millis(1000),
        client.call("Calculator", "add", valid_params),
    )
    .await
    .expect("Valid request should not timeout");

    assert!(
        valid_result.is_ok(),
        "Connection should remain stable for valid requests"
    );
}

#[tokio::test]
async fn test_bdd_scenario_server_routes_requests_to_correct_service() {
    // Given: services "Calculator" and "UserService" are registered
    let addr = "127.0.0.1:50082";
    setup_test_server_with_services(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = RpcClient::new(connection);

    // When: a client requests "Calculator.add"
    let calc_request = CalculatorRequest { a: 10, b: 15 };
    let calc_params = Bytes::from(serde_json::to_string(&calc_request).unwrap());

    let calc_result = timeout(
        Duration::from_millis(1000),
        client.call("Calculator", "add", calc_params),
    )
    .await
    .expect("Calculator request should not timeout");

    // Then: only the Calculator service should receive the request
    assert!(calc_result.is_ok(), "Calculator request should succeed");
    let calc_response: CalculatorResponse = serde_json::from_slice(&calc_result.unwrap()).unwrap();
    assert_eq!(calc_response.result, 25);

    // And: UserService should handle its own requests correctly
    let user_request = UserRequest { user_id: 123 };
    let user_params = Bytes::from(serde_json::to_string(&user_request).unwrap());

    let user_result = timeout(
        Duration::from_millis(1000),
        client.call("UserService", "get_user", user_params),
    )
    .await
    .expect("UserService request should not timeout");

    assert!(user_result.is_ok(), "UserService request should succeed");
    let user_response: UserResponse = serde_json::from_slice(&user_result.unwrap()).unwrap();
    assert_eq!(user_response.name, "User 123");
    assert_eq!(user_response.email, "user123@example.com");
}

#[tokio::test]
async fn test_bdd_scenario_server_handles_nonexistent_service() {
    // Given: an RPC server with registered services
    let addr = "127.0.0.1:50083";
    setup_test_server_with_services(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = RpcClient::new(connection);

    // When: a client requests a nonexistent service
    let params = Bytes::from("{}");

    let result = timeout(
        Duration::from_millis(1000),
        client.call("NonexistentService", "someMethod", params),
    )
    .await
    .expect("Request should not timeout");

    // Then: the server should respond with ServiceNotFound error
    assert!(
        result.is_err(),
        "Request to nonexistent service should fail"
    );
    match result.unwrap_err() {
        RpcError::ServiceError(_) | RpcError::MethodNotFound(_) => {
            // Expected error for nonexistent service
        }
        other => panic!("Unexpected error type: {:?}", other),
    }
}

#[tokio::test]
async fn test_bdd_scenario_server_handles_nonexistent_method() {
    // Given: an RPC server with Calculator service
    let addr = "127.0.0.1:50084";
    setup_test_server_with_services(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = RpcClient::new(connection);

    // When: a client requests a nonexistent method
    let params = Bytes::from("{}");

    let result = timeout(
        Duration::from_millis(1000),
        client.call("Calculator", "nonexistentMethod", params),
    )
    .await
    .expect("Request should not timeout");

    // Then: the server should respond with MethodNotFound error
    assert!(result.is_err(), "Request to nonexistent method should fail");
    match result.unwrap_err() {
        RpcError::MethodNotFound(method) => {
            assert_eq!(method, "nonexistentMethod");
        }
        other => panic!("Expected MethodNotFound error, got: {:?}", other),
    }
}
