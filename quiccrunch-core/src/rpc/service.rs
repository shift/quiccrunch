//! RPC Service Traits and Definitions
//!
//! Core traits and types for defining RPC services in QuicCrunch.

use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::rpc::error::{RpcError, RpcResult};

/// Unique identifier for RPC requests
pub type RequestId = u64;

/// Service identifier
pub type ServiceId = u32;

/// Method identifier within a service
pub type MethodId = u32;

/// Context passed to RPC methods with request metadata
#[derive(Debug, Clone)]
pub struct RpcContext {
    pub request_id: RequestId,
    pub service_id: ServiceId,
    pub method_id: MethodId,
    pub service: String,
    pub method: String,
    pub timeout_ms: u32,
    pub trace_context: Option<TraceContext>,
    pub client_address: Option<String>,
    /// Request metadata (headers)
    pub metadata: HashMap<String, String>,
    /// Extensions for storing arbitrary data (e.g., auth claims)
    pub extensions: HashMap<String, serde_json::Value>,
    /// Start time for latency measurement
    pub start_time: std::time::Instant,
    /// Timeout duration
    pub timeout: Option<std::time::Duration>,
}

/// Trace context for telemetry
#[derive(Debug, Clone)]
pub struct TraceContext {
    pub trace_id: Vec<u8>,
    pub span_id: u64,
    pub flags: u8,
    pub state: String,
}

/// Streaming type for RPC methods
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StreamingType {
    Unary,           // Single request, single response
    ServerStreaming, // Single request, stream of responses
    ClientStreaming, // Stream of requests, single response
    Bidirectional,   // Stream of requests, stream of responses
}

/// Definition of an RPC method
#[derive(Debug, Clone)]
pub struct MethodDefinition {
    pub id: MethodId,
    pub name: String,
    pub streaming_type: StreamingType,
    pub request_schema: Option<Bytes>,
    pub response_schema: Option<Bytes>,
}

/// Definition of an RPC service
#[derive(Debug, Clone)]
pub struct ServiceDefinition {
    pub id: ServiceId,
    pub name: String,
    pub methods: HashMap<MethodId, MethodDefinition>,
}

impl ServiceDefinition {
    pub fn new(id: ServiceId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            methods: HashMap::new(),
        }
    }

    pub fn add_method(&mut self, method: MethodDefinition) {
        self.methods.insert(method.id, method);
    }

    pub fn get_method(&self, method_id: MethodId) -> Option<&MethodDefinition> {
        self.methods.get(&method_id)
    }

    pub fn get_method_by_name(&self, name: &str) -> Option<&MethodDefinition> {
        self.methods.values().find(|m| m.name == name)
    }
}

/// Trait for implementing RPC services
#[async_trait]
pub trait RpcService: Send + Sync {
    /// Get the service definition
    fn definition(&self) -> &ServiceDefinition;

    /// Handle an RPC call
    async fn call(
        &self,
        context: RpcContext,
        method_id: MethodId,
        request: Bytes,
    ) -> RpcResult<Bytes>;

    /// Handle streaming RPC calls (default implementation for non-streaming services)
    async fn call_streaming(
        &self,
        _context: RpcContext,
        _method_id: MethodId,
        _request_stream: tokio::sync::mpsc::Receiver<Bytes>,
        _response_stream: tokio::sync::mpsc::Sender<Bytes>,
    ) -> RpcResult<()> {
        Err(RpcError::MethodNotFound {
            service: self.definition().name.clone(),
            method: "streaming methods not supported".to_string(),
        })
    }

    /// Get service health status
    async fn health(&self) -> RpcResult<HealthStatus> {
        Ok(HealthStatus::Serving)
    }
}

/// Health status for services
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HealthStatus {
    Unknown,
    Serving,
    NotServing,
    ServiceUnknown,
}

/// Registry for RPC services
pub struct ServiceRegistry {
    services: RwLock<HashMap<ServiceId, Arc<dyn RpcService>>>,
    service_names: RwLock<HashMap<String, ServiceId>>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
            service_names: RwLock::new(HashMap::new()),
        }
    }

    /// Register a service
    pub async fn register(&self, service: Arc<dyn RpcService>) -> RpcResult<()> {
        let definition = service.definition();
        let service_id = definition.id;
        let service_name = definition.name.clone();

        let mut services = self.services.write().await;
        let mut names = self.service_names.write().await;

        if services.contains_key(&service_id) {
            return Err(RpcError::InvalidServiceDefinition);
        }

        services.insert(service_id, service);
        names.insert(service_name, service_id);

        Ok(())
    }

    /// Get a service by ID
    pub async fn get_service(&self, service_id: ServiceId) -> Option<Arc<dyn RpcService>> {
        let services = self.services.read().await;
        services.get(&service_id).cloned()
    }

    /// Get a service by name
    pub async fn get_service_by_name(&self, name: &str) -> Option<Arc<dyn RpcService>> {
        let names = self.service_names.read().await;
        if let Some(&service_id) = names.get(name) {
            drop(names);
            self.get_service(service_id).await
        } else {
            None
        }
    }

    /// List all registered services
    pub async fn list_services(&self) -> Vec<ServiceDefinition> {
        let services = self.services.read().await;
        services.values().map(|s| s.definition().clone()).collect()
    }

    /// Check health of a specific service
    pub async fn check_health(&self, service_name: Option<&str>) -> RpcResult<HealthStatus> {
        if let Some(name) = service_name {
            if let Some(service) = self.get_service_by_name(name).await {
                service.health().await
            } else {
                Ok(HealthStatus::ServiceUnknown)
            }
        } else {
            // Overall health - all services must be serving
            let services = self.services.read().await;
            let mut overall_status = HealthStatus::Serving;

            for service in services.values() {
                match service.health().await? {
                    HealthStatus::NotServing => return Ok(HealthStatus::NotServing),
                    HealthStatus::Unknown => overall_status = HealthStatus::Unknown,
                    HealthStatus::Serving => {}
                    HealthStatus::ServiceUnknown => overall_status = HealthStatus::Unknown,
                }
            }

            Ok(overall_status)
        }
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}
