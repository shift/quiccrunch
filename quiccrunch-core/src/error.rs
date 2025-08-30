
use thiserror::Error;

/// Zero-allocation error codes for hot paths
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ErrorCode {
    // Connection errors
    ConnectionFailed,
    ConnectionTimeout,
    ConnectionRefused,
    ConnectionReset,
    TooManyConnections,

    // Stream errors
    StreamError,
    StreamClosed,
    StreamTimeout,
    InvalidStreamId,

    // Protocol errors
    ProtocolError,
    InvalidMessage,
    MessageTooLarge,
    HeaderCorrupted,

    // Resource errors
    OutOfMemory,
    BufferOverflow,
    ResourceExhausted,

    // Configuration errors
    InvalidConfig,
    FeatureNotSupported,
}

/// Zero-allocation error type for hot paths
#[derive(Clone, Copy, Debug)]
pub struct FastError {
    pub code: ErrorCode,
    pub line: u32,
}

impl FastError {
    #[inline(always)]
    pub const fn new(code: ErrorCode) -> Self {
        Self { code, line: line!() }
    }

    #[inline(always)]
    pub fn connection_failed() -> Self {
        Self::new(ErrorCode::ConnectionFailed)
    }

    #[inline(always)]
    pub fn stream_error() -> Self {
        Self::new(ErrorCode::StreamError)
    }

    #[inline(always)]
    pub fn invalid_message() -> Self {
        Self::new(ErrorCode::InvalidMessage)
    }

    #[inline(always)]
    pub fn buffer_overflow() -> Self {
        Self::new(ErrorCode::BufferOverflow)
    }
}

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

    // Zero-allocation fast error variant
    #[error("Fast error: {:?}", .0)]
    Fast(FastError),
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

#[cfg(feature = "backend-tquic")]
impl From<tquic::Error> for TransportError {
    fn from(err: tquic::Error) -> Self {
        TransportError::ConnectionFailed(format!("tQUIC error: {:?}", err))
    }
}

/// Result type for transport operations
pub type Result<T> = std::result::Result<T, TransportError>;