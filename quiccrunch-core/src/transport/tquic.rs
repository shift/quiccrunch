// Re-export the new adapter types 
pub use super::adapter::{TquicClient, TquicServer, TquicListener, TquicConnection, TquicStream, TquicAdapter};

// Re-export tQUIC types for compatibility - using proper v1.6.0 APIs
pub use tquic::{Config, TlsConfig, Connection, ConnectionId, Endpoint};