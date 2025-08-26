
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
}

/// Result type for transport operations
pub type Result<T> = std::result::Result<T, TransportError>;