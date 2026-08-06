//! RPC Framework Error Types
//!
//! Comprehensive error handling for QuicCrunch RPC operations.

use thiserror::Error;
use crate::TransportError;

/// Result type for RPC operations
pub type RpcResult<T> = Result<T, RpcError>;

/// Comprehensive RPC error types
#[derive(Error, Debug)]
pub enum RpcError {
    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("Method '{method}' not found in service '{service}'")]
    MethodNotFound { service: String, method: String },

    #[error("Service '{service}' not found")]
    ServiceNotFound { service: String },

    #[error("Invalid request: {reason}")]
    InvalidRequest { reason: String },

    #[error("Request timeout after {timeout_ms}ms")]
    Timeout { timeout_ms: u32 },

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Unauthenticated")]
    Unauthenticated,

    #[error("Permission denied: {reason}")]
    PermissionDenied { reason: String },

    #[error("Resource exhausted: {resource}")]
    ResourceExhausted { resource: String },

    #[error("Service unavailable: {reason}")]
    Unavailable { reason: String },

    #[error("Internal error: {message}")]
    InternalError { message: String },

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Connection pool exhausted")]
    ConnectionPoolExhausted,

    #[error("Invalid service definition")]
    InvalidServiceDefinition,

    #[error("Schema mismatch: expected {expected}, got {actual}")]
    SchemaMismatch { expected: String, actual: String },
}

impl RpcError {
    /// Convert to Cap'n Proto error code
    pub fn to_error_code(&self) -> u16 {
        match self {
            RpcError::Transport(_) => 10,
            RpcError::MethodNotFound { .. } => 2,
            RpcError::ServiceNotFound { .. } => 3,
            RpcError::InvalidRequest { .. } => 1,
            RpcError::Timeout { .. } => 4,
            RpcError::Cancelled => 5,
            RpcError::Unauthenticated => 6,
            RpcError::PermissionDenied { .. } => 7,
            RpcError::ResourceExhausted { .. } => 8,
            RpcError::Unavailable { .. } => 9,
            RpcError::InternalError { .. } => 10,
            RpcError::Serialization(_) => 1,
            RpcError::Deserialization(_) => 1,
            RpcError::ConnectionPoolExhausted => 8,
            RpcError::InvalidServiceDefinition => 1,
            RpcError::SchemaMismatch { .. } => 1,
        }
    }

    /// Create from error code and message
    pub fn from_error_code(code: u16, message: String) -> Self {
        match code {
            1 => RpcError::InvalidRequest { reason: message },
            2 => RpcError::MethodNotFound { 
                service: "unknown".to_string(), 
                method: message 
            },
            3 => RpcError::ServiceNotFound { service: message },
            4 => RpcError::Timeout { timeout_ms: 30000 },
            5 => RpcError::Cancelled,
            6 => RpcError::Unauthenticated,
            7 => RpcError::PermissionDenied { reason: message },
            8 => RpcError::ResourceExhausted { resource: message },
            9 => RpcError::Unavailable { reason: message },
            10 => RpcError::InternalError { message },
            _ => RpcError::InternalError { message: format!("Unknown error code: {}", code) },
        }
    }
}