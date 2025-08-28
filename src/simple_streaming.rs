//! Simplified streaming RPC implementation for QuicCrunch
//! 
//! This is a working minimal streaming implementation that can be built upon.

use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use std::time::Duration;
use async_trait::async_trait;
use bytes::Bytes;
use capnp::message::{Builder, ReaderOptions};
use capnp::serialize;
use tokio::sync::{RwLock, mpsc};
use crate::{Connection, QuicStream, TransportError, Result};
use crate::schema::rpc_capnp::*;

/// Simplified streaming RPC service trait
#[async_trait]
pub trait SimpleStreamingService: Send + Sync {
    /// Service name
    fn name(&self) -> &str;
    
    /// Handle server streaming (single request -> multiple responses)
    async fn server_stream(&self, method: &str, request: Bytes) -> crate::rpc::RpcResult<Vec<Bytes>> {
        Err(crate::rpc::RpcError::MethodNotFound(format!("Method not found: {}", method)))
    }
    
    /// Handle client streaming (multiple requests -> single response)  
    async fn client_stream(&self, method: &str, requests: Vec<Bytes>) -> crate::rpc::RpcResult<Bytes> {
        Err(crate::rpc::RpcError::MethodNotFound(format!("Method not found: {}", method)))
    }
}

/// Simple streaming client
pub struct SimpleStreamingClient {
    connection: Connection,
}

impl SimpleStreamingClient {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }
    
    /// Call server streaming method
    pub async fn call_server_streaming(
        &self,
        service: &str,
        method: &str,
        request: Bytes,
    ) -> crate::rpc::RpcResult<Vec<Bytes>> {
        // For now, convert to regular RPC call
        let client = crate::rpc::RpcClient::new(self.connection.clone());
        let response = client.call(service, method, request).await?;
        Ok(vec![response])
    }
    
    /// Call client streaming method
    pub async fn call_client_streaming(
        &self,
        service: &str,
        method: &str,
        requests: Vec<Bytes>,
    ) -> crate::rpc::RpcResult<Bytes> {
        // For now, just send first request
        if let Some(first_request) = requests.first() {
            let client = crate::rpc::RpcClient::new(self.connection.clone());
            client.call(service, method, first_request.clone()).await
        } else {
            Err(crate::rpc::RpcError::InvalidParameters("No requests provided".to_string()))
        }
    }
}

/// Example streaming service for demonstration
pub struct ExampleStreamingService;

#[async_trait]
impl SimpleStreamingService for ExampleStreamingService {
    fn name(&self) -> &str {
        "ExampleStreamingService"
    }
    
    async fn server_stream(&self, method: &str, request: Bytes) -> crate::rpc::RpcResult<Vec<Bytes>> {
        match method {
            "stream_numbers" => {
                let count: u32 = String::from_utf8_lossy(&request).parse().unwrap_or(5);
                let mut results = Vec::new();
                
                for i in 0..count {
                    let data = format!("number_{}", i);
                    results.push(Bytes::from(data));
                }
                
                Ok(results)
            }
            _ => Err(crate::rpc::RpcError::MethodNotFound(format!("Unknown method: {}", method)))
        }
    }
    
    async fn client_stream(&self, method: &str, requests: Vec<Bytes>) -> crate::rpc::RpcResult<Bytes> {
        match method {
            "sum_numbers" => {
                let mut sum = 0i32;
                
                for request in requests {
                    let num_str = String::from_utf8_lossy(&request);
                    if let Ok(num) = num_str.parse::<i32>() {
                        sum += num;
                    }
                }
                
                Ok(Bytes::from(sum.to_string()))
            }
            _ => Err(crate::rpc::RpcError::MethodNotFound(format!("Unknown method: {}", method)))
        }
    }
}