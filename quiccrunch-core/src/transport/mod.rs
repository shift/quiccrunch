
pub mod tquic;

use async_trait::async_trait;
use std::error::Error;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::config::{ClientConfig, ServerConfig};
use crate::transport::tquic::TlsConfig;

#[derive(Debug, Clone)]
pub struct QuicTransportConfig {
    pub client: ClientConfig,
    pub server: ServerConfig,
}

impl From<TlsConfig> for QuicTransportConfig {
    fn from(_tls_config: TlsConfig) -> Self {
        // Create a basic config using the TLS configuration
        let client_config = ClientConfig::default();
        let server_config = ServerConfig::default();
        
        QuicTransportConfig {
            client: client_config,
            server: server_config,
        }
    }
}

pub trait Splittable {
    type Reader;
    type Writer;
    fn split(self) -> (Self::Reader, Self::Writer);
}

#[async_trait]
pub trait Transport {
    type Connection: AsyncRead + AsyncWrite + Unpin + Send + Splittable;
    type Listener: Listener<Connection = Self::Connection>;
    type Config;

    async fn new(config: Self::Config) -> Result<Self, Box<dyn Error>>
    where
        Self: Sized;
    async fn connect(&self, addr: &str) -> Result<Self::Connection, Box<dyn Error>>;
    async fn listen(&self, addr: &str) -> Result<Self::Listener, Box<dyn Error>>;
}

#[async_trait]
pub trait Listener {
    type Connection: AsyncRead + AsyncWrite + Unpin + Send;
    async fn accept(&mut self) -> Result<Self::Connection, Box<dyn Error>>;
}

#[derive(Debug, Clone)]
pub struct QuicTransport {
    pub client: tquic::TquicClient,
    pub server: tquic::TquicServer,
}

#[async_trait]
impl Transport for QuicTransport {
    type Connection = tquic::TquicConnection;
    type Listener = tquic::TquicListener;
    type Config = QuicTransportConfig;

    async fn new(config: Self::Config) -> Result<Self, Box<dyn Error>> {
        let client = tquic::TquicClient::new(config.client).await?;
        let server = tquic::TquicServer::new(config.server).await?;
        Ok(QuicTransport { client, server })
    }

    async fn connect(&self, addr: &str) -> Result<Self::Connection, Box<dyn Error>> {
        self.client.connect(addr).await
    }

    async fn listen(&self, addr: &str) -> Result<Self::Listener, Box<dyn Error>> {
        self.server.listen(addr).await
    }
}
