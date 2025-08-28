
use thiserror::Error;

/// Error types for the transport layer
#[derive(Error, Debug)]
pub enum TransportError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    
    #[error("Stream error: {0}")]
    StreamError(String),
    
    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Certificate error: {0}")]
    CertificateError(#[from] Box<dyn std::error::Error + Send + Sync>),

    #[error("Timeout error: {0}")]
    TimeoutError(#[from] tokio::time::error::Elapsed),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Invalid message: {0}")]
    InvalidMessage(String),
}

impl From<Box<dyn std::error::Error>> for TransportError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        TransportError::ConfigError(err.to_string())
    }
}

impl From<serde_json::Error> for TransportError {
    fn from(err: serde_json::Error) -> Self {
        TransportError::ProtocolError(format!("JSON error: {}", err))
    }
}

impl From<capnp::Error> for TransportError {
    fn from(err: capnp::Error) -> Self {
        TransportError::SerializationError(format!("Cap'n Proto error: {}", err))
    }
}

impl From<capnp::NotInSchema> for TransportError {
    fn from(err: capnp::NotInSchema) -> Self {
        TransportError::SerializationError(format!("Cap'n Proto schema error: {}", err))
    }
}

/// Result type for transport operations
pub type Result<T> = std::result::Result<T, TransportError>;