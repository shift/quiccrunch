
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

/// Result type for transport operations
pub type Result<T> = std::result::Result<T, TransportError>;