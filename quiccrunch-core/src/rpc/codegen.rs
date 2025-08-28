//! Code Generation for QuicCrunch RPC Services
//! 
//! Provides macros and utilities for generating RPC client and server code from service definitions.

use paste::paste;

/// Macro to define an RPC service with methods
/// 
/// # Example
/// 
/// ```rust
/// use quiccrunch::rpc_service;
/// use bytes::Bytes;
/// 
/// rpc_service! {
///     UserService {
///         async fn get_user(user_id: u64) -> User;
///         async fn create_user(user: User) -> u64;
///         async fn delete_user(user_id: u64) -> bool;
///     }
/// }
/// ```
#[macro_export]
macro_rules! rpc_service {
    (
        $service_name:ident {
            $(async fn $method_name:ident($($param_name:ident: $param_type:ty),*) -> $return_type:ty;)*
        }
    ) => {
        // Generate service trait
        #[async_trait::async_trait]
        pub trait $service_name: Send + Sync {
            $(
                async fn $method_name(&self, $($param_name: $param_type),*) -> crate::rpc::RpcResult<$return_type>;
            )*
        }
        
        paste::paste! {
            // Generate service wrapper that implements RpcService
            pub struct [<$service_name Wrapper>]<T: $service_name> {
                service: T,
            }
            
            impl<T: $service_name> [<$service_name Wrapper>]<T> {
                pub fn new(service: T) -> Self {
                    Self { service }
                }
                
                pub fn methods(&self) -> Vec<String> {
                    vec![
                        $(stringify!($method_name).to_string(),)*
                    ]
                }
            }
            
            #[async_trait::async_trait]
            impl<T: $service_name> crate::rpc::RpcService for [<$service_name Wrapper>]<T> {
                async fn call(&self, context: &crate::rpc::RpcContext, method: &str, params: bytes::Bytes) -> crate::rpc::RpcResult<bytes::Bytes> {
                    match method {
                        $(
                            stringify!($method_name) => {
                                // Deserialize parameters from JSON
                                let params_json: serde_json::Value = serde_json::from_slice(&params)
                                    .map_err(|e| crate::rpc::RpcError::InvalidParameters(e.to_string()))?;
                                
                                // Extract parameters - this is a simplified version that expects a JSON object
                                // with parameter names as keys
                                $crate::rpc_service_extract_params!(params_json, $($param_name: $param_type),*);
                                
                                // Call the service method
                                let result = self.service.$method_name($($param_name),*).await?;
                                
                                // Serialize result
                                let result_json = serde_json::to_vec(&result)
                                    .map_err(|e| crate::rpc::RpcError::SerializationError(e.to_string()))?;
                                
                                Ok(bytes::Bytes::from(result_json))
                            }
                        )*
                        _ => Err(crate::rpc::RpcError::MethodNotFound(method.to_string())),
                    }
                }
            }
            
            // Generate client wrapper
            pub struct [<$service_name Client>] {
                client: crate::rpc::PooledRpcClient,
            }
            
            impl [<$service_name Client>] {
                pub fn new(pool: std::sync::Arc<crate::rpc::RpcConnectionPool>) -> Self {
                    Self {
                        client: crate::rpc::PooledRpcClient::new(pool),
                    }
                }
                
                $(
                    pub async fn $method_name(&self, $($param_name: $param_type),*) -> crate::rpc::RpcResult<$return_type> {
                        // Serialize parameters
                        let params = serde_json::json!({
                            $(stringify!($param_name): $param_name,)*
                        });
                        let params_bytes = bytes::Bytes::from(params.to_string());
                        
                        // Make RPC call
                        let result_bytes = self.client.call(
                            stringify!($service_name),
                            stringify!($method_name),
                            params_bytes
                        ).await?;
                        
                        // Deserialize result
                        let result: $return_type = serde_json::from_slice(&result_bytes)
                            .map_err(|e| crate::rpc::RpcError::SerializationError(e.to_string()))?;
                        
                        Ok(result)
                    }
                )*
            }
        }
    };
}

/// Helper macro for extracting parameters from JSON
#[macro_export]
macro_rules! rpc_service_extract_params {
    ($params_json:expr,) => {};
    ($params_json:expr, $param_name:ident: $param_type:ty) => {
        let $param_name: $param_type = $params_json
            .get(stringify!($param_name))
            .ok_or_else(|| crate::rpc::RpcError::InvalidParameters(format!("Missing parameter: {}", stringify!($param_name))))?
            .clone();
        let $param_name: $param_type = serde_json::from_value($param_name)
            .map_err(|e| crate::rpc::RpcError::InvalidParameters(format!("Invalid parameter type for {}: {}", stringify!($param_name), e)))?;
    };
    ($params_json:expr, $param_name:ident: $param_type:ty, $($rest_param_name:ident: $rest_param_type:ty),*) => {
        let $param_name: $param_type = $params_json
            .get(stringify!($param_name))
            .ok_or_else(|| crate::rpc::RpcError::InvalidParameters(format!("Missing parameter: {}", stringify!($param_name))))?
            .clone();
        let $param_name: $param_type = serde_json::from_value($param_name)
            .map_err(|e| crate::rpc::RpcError::InvalidParameters(format!("Invalid parameter type for {}: {}", stringify!($param_name), e)))?;
        
        $crate::rpc_service_extract_params!($params_json, $($rest_param_name: $rest_param_type),*);
    };
}

/// Macro to define an RPC client for an existing service
#[macro_export]
macro_rules! rpc_client {
    ($service_name:ident, $endpoint:expr) => {
        [<$service_name Client>]::new($endpoint.to_string())
    };
}

/// Macro to register an RPC service with a server
#[macro_export]
macro_rules! register_service {
    ($server:expr, $service:expr) => {
        $server.register_service(std::sync::Arc::new(
            [<$service_name Wrapper>]::new($service)
        )).await
    };
}

/// Attribute macro for service implementations (future enhancement)
/// This would be implemented as a procedural macro in a separate crate
// TODO: Implement procedural macros in separate crate
// pub use quiccrunch_codegen::*;

/// Helper trait for serializable types in RPC
pub trait RpcSerializable: serde::Serialize + for<'de> serde::Deserialize<'de> + Send + Sync {}

impl<T> RpcSerializable for T
where
    T: serde::Serialize + for<'de> serde::Deserialize<'de> + Send + Sync
{}

/// Helper for generating service stubs from Cap'n Proto schemas
pub struct ServiceGenerator {
    schema_path: std::path::PathBuf,
    output_path: std::path::PathBuf,
}

impl ServiceGenerator {
    pub fn new(schema_path: impl Into<std::path::PathBuf>, output_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            schema_path: schema_path.into(),
            output_path: output_path.into(),
        }
    }
    
    /// Generate Rust code from Cap'n Proto schema
    pub fn generate(&self) -> std::io::Result<()> {
        // This would parse the Cap'n Proto schema and generate corresponding Rust code
        // For now, this is a placeholder for future implementation
        println!("Generating service code from {:?} to {:?}", self.schema_path, self.output_path);
        Ok(())
    }
}

/// Convenience macro for creating typed RPC parameters
#[macro_export]
macro_rules! rpc_params {
    ($($key:ident: $value:expr),* $(,)?) => {
        {
            let mut params = std::collections::HashMap::new();
            $(
                params.insert(stringify!($key).to_string(), serde_json::to_value($value).unwrap());
            )*
            bytes::Bytes::from(serde_json::to_string(&params).unwrap())
        }
    };
}

/// Convenience macro for extracting typed RPC results
#[macro_export]
macro_rules! rpc_result {
    ($result:expr, $type:ty) => {
        {
            let value: serde_json::Value = serde_json::from_slice(&$result)?;
            serde_json::from_value::<$type>(value)
                .map_err(|e| crate::rpc::RpcError::SerializationError(e.to_string()))?
        }
    };
}