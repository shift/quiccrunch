// Re-export the new adapter types
pub use super::adapter::{
    TquicAdapter, TquicClient, TquicConnection, TquicListener, TquicServer, TquicStream,
};

// Re-export tQUIC types for compatibility - using proper v1.6.0 APIs
pub use tquic::{Config, Connection, ConnectionId, Endpoint, TlsConfig};
