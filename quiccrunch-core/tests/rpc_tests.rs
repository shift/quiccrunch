//! Comprehensive tests for QuicCrunch RPC framework

use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use bytes::Bytes;
use serde::{Serialize, Deserialize};
use async_trait::async_trait;

use quiccrunch_core::{
    QuicTransport, TlsConfig,
    rpc::{RpcService, RpcServer, RpcClient, RpcContext, RpcResult, RpcError, PooledRpcClient},
    rpc::discovery::{ServiceRegistry, DiscoveryClient, LoadBalancingStrategy},
};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct TestRequest {
    value: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct TestResponse {
    echo: String,
    length: usize,
}

/// Test service for RPC testing
struct TestService {
    name: String,
}

impl TestService {
    fn new(name: String) -> Self {
        Self { name }
    }
}

#[async_trait]
impl RpcService for TestService {
    fn name(&self) -> &str {
        &self.name
    }
    
    async fn call_method(&self, context: &RpcContext, params: Bytes) -> RpcResult<Bytes> {
        match context.method.as_str() {
            "echo" => {
                let request: TestRequest = serde_json::from_slice(&params)
                    .map_err(|e| RpcError::InvalidParameters(e.to_string()))?;
                
                let response = TestResponse {
                    echo: request.value.clone(),
                    length: request.value.len(),
                };
                
                let response_bytes = serde_json::to_vec(&response)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;
                
                Ok(Bytes::from(response_bytes))
            }
            "error" => {
                Err(RpcError::ServiceError("Intentional test error".to_string()))
            }
            "slow" => {
                tokio::time::sleep(Duration::from_millis(100)).await;
                let response = TestResponse {
                    echo: "slow response".to_string(),
                    length: 13,
                };
                
                let response_bytes = serde_json::to_vec(&response)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;
                
                Ok(Bytes::from(response_bytes))
            }
            _ => Err(RpcError::MethodNotFound(context.method.clone())),
        }
    }
    
    fn methods(&self) -> Vec<String> {
        vec!["echo".to_string(), "error".to_string(), "slow".to_string()]
    }
}

async fn setup_test_server(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config)?;
    let mut incoming = transport.listen(addr).await?;
    
    let rpc_server = RpcServer::new();
    rpc_server.register_service(Arc::new(TestService::new("TestService".to_string()))).await;
    
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
async fn test_basic_rpc_call() {
    let addr = "127.0.0.1:50061";
    
    // Start server
    setup_test_server(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Create client
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = RpcClient::new(connection);
    
    // Test echo method
    let request = TestRequest {
        value: "Hello, RPC!".to_string(),
    };
    let params = Bytes::from(serde_json::to_string(&request).unwrap());
    
    let result = client.call("TestService", "echo", params).await.unwrap();
    let response: TestResponse = serde_json::from_slice(&result).unwrap();
    
    assert_eq!(response.echo, "Hello, RPC!");
    assert_eq!(response.length, 11);
}

#[tokio::test]
async fn test_rpc_error_handling() {
    let addr = "127.0.0.1:50062";
    
    // Start server
    setup_test_server(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Create client
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = RpcClient::new(connection);
    
    // Test error method
    let params = Bytes::from("{}");
    let result = client.call("TestService", "error", params).await;
    
    assert!(result.is_err());
    match result.unwrap_err() {
        RpcError::ServiceError(msg) => {
            assert_eq!(msg, "Intentional test error");
        }
        other => panic!("Unexpected error type: {:?}", other),
    }
}

#[tokio::test]
async fn test_method_not_found() {
    let addr = "127.0.0.1:50063";
    
    // Start server
    setup_test_server(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Create client
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = RpcClient::new(connection);
    
    // Test non-existent method
    let params = Bytes::from("{}");
    let result = client.call("TestService", "nonexistent", params).await;
    
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), RpcError::MethodNotFound(_)));
}

#[tokio::test]
async fn test_rpc_timeout() {
    let addr = "127.0.0.1:50064";
    
    // Start server
    setup_test_server(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Create client
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = RpcClient::new(connection);
    
    // Test timeout with slow method
    let mut context = RpcContext::new("TestService".to_string(), "slow".to_string());
    context.timeout = Some(Duration::from_millis(50)); // Shorter than the 100ms delay
    
    let params = Bytes::from("{}");
    let result = client.call_with_context(&mut context, params).await;
    
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), RpcError::Timeout));
}

#[tokio::test]
async fn test_pooled_rpc_client() {
    let addr = "127.0.0.1:50065";
    
    // Start server
    setup_test_server(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Create pooled client
    let client = PooledRpcClient::new(addr.to_string(), 5);
    
    // Test multiple calls
    for i in 0..10 {
        let request = TestRequest {
            value: format!("Message {}", i),
        };
        let params = Bytes::from(serde_json::to_string(&request).unwrap());
        
        let result = client.call("TestService", "echo", params).await.unwrap();
        let response: TestResponse = serde_json::from_slice(&result).unwrap();
        
        assert_eq!(response.echo, format!("Message {}", i));
    }
}

#[tokio::test]
async fn test_service_discovery() {
    let registry = Arc::new(ServiceRegistry::new());
    registry.start().await;
    
    // Create test service info
    let mut service_info = quiccrunch::rpc::ServiceInfo {
        name: "TestService".to_string(),
        version: "1.0.0".to_string(),
        methods: vec![
            quiccrunch::rpc::MethodInfo {
                name: "echo".to_string(),
                input_type: "TestRequest".to_string(),
                output_type: "TestResponse".to_string(),
                streaming: quiccrunch::schema::rpc_capnp::StreamingType::Unary,
                deprecated: false,
            }
        ],
        endpoints: vec![],
        metadata: std::collections::HashMap::new(),
    };
    
    let endpoint = quiccrunch::rpc::EndpointInfo {
        address: "127.0.0.1".to_string(),
        port: 50066,
        protocol: "quic".to_string(),
        weight: 100,
        health: quiccrunch::schema::rpc_capnp::HealthStatus::Healthy,
    };
    
    // Register service
    registry.register_service(service_info.clone(), endpoint.clone()).await.unwrap();
    
    // Test discovery
    let services = registry.discover_services(None).await;
    assert_eq!(services.len(), 1);
    assert_eq!(services[0].name, "TestService");
    
    // Test specific service lookup
    let found_service = registry.get_service("TestService").await;
    assert!(found_service.is_some());
    assert_eq!(found_service.unwrap().name, "TestService");
    
    // Test non-existent service
    let not_found = registry.get_service("NonExistentService").await;
    assert!(not_found.is_none());
}

#[tokio::test]
async fn test_load_balancing() {
    use quiccrunch_core::rpc::discovery::LoadBalancer;
    
    let endpoints = vec![
        quiccrunch::rpc::EndpointInfo {
            address: "127.0.0.1".to_string(),
            port: 50067,
            protocol: "quic".to_string(),
            weight: 100,
            health: quiccrunch::schema::rpc_capnp::HealthStatus::Healthy,
        },
        quiccrunch::rpc::EndpointInfo {
            address: "127.0.0.1".to_string(),
            port: 50068,
            protocol: "quic".to_string(),
            weight: 200,
            health: quiccrunch::schema::rpc_capnp::HealthStatus::Healthy,
        },
        quiccrunch::rpc::EndpointInfo {
            address: "127.0.0.1".to_string(),
            port: 50069,
            protocol: "quic".to_string(),
            weight: 50,
            health: quiccrunch::schema::rpc_capnp::HealthStatus::Unhealthy, // Should be ignored
        },
    ];
    
    // Test round-robin
    let lb = LoadBalancer::new(LoadBalancingStrategy::RoundRobin);
    
    let selected1 = lb.select_endpoint(&endpoints).unwrap();
    let selected2 = lb.select_endpoint(&endpoints).unwrap();
    
    // Should only select from healthy endpoints
    assert!(selected1.port == 50067 || selected1.port == 50068);
    assert!(selected2.port == 50067 || selected2.port == 50068);
    
    // Test with no healthy endpoints
    let unhealthy_endpoints = vec![
        quiccrunch::rpc::EndpointInfo {
            address: "127.0.0.1".to_string(),
            port: 50070,
            protocol: "quic".to_string(),
            weight: 100,
            health: quiccrunch::schema::rpc_capnp::HealthStatus::Unhealthy,
        },
    ];
    
    let no_selection = lb.select_endpoint(&unhealthy_endpoints);
    assert!(no_selection.is_none());
}

#[tokio::test]
async fn test_service_heartbeat() {
    let registry = Arc::new(ServiceRegistry::new());
    registry.start().await;
    
    let service_info = quiccrunch::rpc::ServiceInfo {
        name: "HeartbeatService".to_string(),
        version: "1.0.0".to_string(),
        methods: vec![],
        endpoints: vec![],
        metadata: std::collections::HashMap::new(),
    };
    
    let endpoint = quiccrunch::rpc::EndpointInfo {
        address: "127.0.0.1".to_string(),
        port: 50071,
        protocol: "quic".to_string(),
        weight: 100,
        health: quiccrunch::schema::rpc_capnp::HealthStatus::Healthy,
    };
    
    // Register service
    registry.register_service(service_info, endpoint).await.unwrap();
    
    // Verify service is discoverable
    let services = registry.discover_services(Some("Heartbeat")).await;
    assert_eq!(services.len(), 1);
    
    // Send heartbeat
    let result = registry.heartbeat("HeartbeatService").await;
    assert!(result.is_ok());
    
    // Test heartbeat for non-existent service
    let result = registry.heartbeat("NonExistentService").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_concurrent_rpc_calls() {
    let addr = "127.0.0.1:50072";
    
    // Start server
    setup_test_server(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Create client
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = Arc::new(RpcClient::new(connection));
    
    // Spawn multiple concurrent calls
    let mut handles = vec![];
    for i in 0..10 {
        let client = client.clone();
        let handle = tokio::spawn(async move {
            let request = TestRequest {
                value: format!("Concurrent message {}", i),
            };
            let params = Bytes::from(serde_json::to_string(&request).unwrap());
            
            client.call("TestService", "echo", params).await
        });
        handles.push(handle);
    }
    
    // Wait for all calls to complete
    for (i, handle) in handles.into_iter().enumerate() {
        let result = handle.await.unwrap().unwrap();
        let response: TestResponse = serde_json::from_slice(&result).unwrap();
        assert_eq!(response.echo, format!("Concurrent message {}", i));
    }
}

#[tokio::test]
async fn test_rpc_metadata() {
    let addr = "127.0.0.1:50073";
    
    // Start server
    setup_test_server(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Create client
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(addr).await.unwrap();
    let client = RpcClient::new(connection);
    
    // Test call with metadata
    let mut context = RpcContext::new("TestService".to_string(), "echo".to_string());
    context.metadata.insert("user_id".to_string(), "12345".to_string());
    context.metadata.insert("session_id".to_string(), "abcdef".to_string());
    
    let request = TestRequest {
        value: "Metadata test".to_string(),
    };
    let params = Bytes::from(serde_json::to_string(&request).unwrap());
    
    let result = client.call_with_context(&mut context, params).await.unwrap();
    let response: TestResponse = serde_json::from_slice(&result).unwrap();
    
    assert_eq!(response.echo, "Metadata test");
}

#[tokio::test]
async fn test_service_methods_listing() {
    let service = TestService::new("TestService".to_string());
    let methods = service.methods();
    
    assert_eq!(methods.len(), 3);
    assert!(methods.contains(&"echo".to_string()));
    assert!(methods.contains(&"error".to_string()));
    assert!(methods.contains(&"slow".to_string()));
}

/// Integration test that combines multiple RPC features
#[tokio::test]
async fn test_full_rpc_integration() {
    let addr = "127.0.0.1:50074";
    
    // Start server
    setup_test_server(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Create service registry and discovery
    let registry = Arc::new(ServiceRegistry::new());
    registry.start().await;
    
    // Register service with discovery
    let service_info = quiccrunch::rpc::ServiceInfo {
        name: "TestService".to_string(),
        version: "1.0.0".to_string(),
        methods: vec![
            quiccrunch::rpc::MethodInfo {
                name: "echo".to_string(),
                input_type: "TestRequest".to_string(),
                output_type: "TestResponse".to_string(),
                streaming: quiccrunch::schema::rpc_capnp::StreamingType::Unary,
                deprecated: false,
            }
        ],
        endpoints: vec![],
        metadata: std::collections::HashMap::new(),
    };
    
    let endpoint = quiccrunch::rpc::EndpointInfo {
        address: "127.0.0.1".to_string(),
        port: 50074,
        protocol: "quic".to_string(),
        weight: 100,
        health: quiccrunch::schema::rpc_capnp::HealthStatus::Healthy,
    };
    
    registry.register_service(service_info, endpoint).await.unwrap();
    
    // Use discovery client to find service
    let discovery = DiscoveryClient::new(registry.clone(), LoadBalancingStrategy::RoundRobin);
    let found_endpoint = discovery.find_service_endpoint("TestService").await.unwrap();
    
    assert_eq!(found_endpoint.address, "127.0.0.1");
    assert_eq!(found_endpoint.port, 50074);
    
    // Create client and make call
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config).unwrap();
    let connection = transport.connect(&format!("{}:{}", found_endpoint.address, found_endpoint.port)).await.unwrap();
    let client = RpcClient::new(connection);
    
    let request = TestRequest {
        value: "Integration test".to_string(),
    };
    let params = Bytes::from(serde_json::to_string(&request).unwrap());
    
    let result = client.call("TestService", "echo", params).await.unwrap();
    let response: TestResponse = serde_json::from_slice(&result).unwrap();
    
    assert_eq!(response.echo, "Integration test");
    assert_eq!(response.length, 16);
    
    println!("✅ Full RPC integration test completed successfully!");
}