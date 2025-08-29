//! Security error types and result definitions

use thiserror::Error;

/// Security framework error types
#[derive(Error, Debug)]
pub enum SecurityError {
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Authorization denied: {0}")]
    AuthorizationDenied(String),

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Monitoring error: {0}")]
    MonitoringError(String),

    #[error("Logging error: {0}")]
    LoggingError(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Compliance error: {0}")]
    ComplianceError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

#[cfg(feature = "jwt")]
impl From<jsonwebtoken::errors::Error> for SecurityError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        SecurityError::InvalidToken(err.to_string())
    }
}

impl From<serde_json::Error> for SecurityError {
    fn from(err: serde_json::Error) -> Self {
        SecurityError::SerializationError(err.to_string())
    }
}

impl From<reqwest::Error> for SecurityError {
    fn from(err: reqwest::Error) -> Self {
        SecurityError::NetworkError(err.to_string())
    }
}

#[cfg(feature = "compliance")]
impl From<sqlx::Error> for SecurityError {
    fn from(err: sqlx::Error) -> Self {
        SecurityError::DatabaseError(err.to_string())
    }
}

/// Result type for security operations
pub type SecurityResult<T> = Result<T, SecurityError>;