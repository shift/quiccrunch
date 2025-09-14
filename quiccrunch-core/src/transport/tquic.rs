use std::net::SocketAddr;
use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot};
use bytes::Bytes;
use rustls::{Certificate, PrivateKey};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::{QuicTransportSettings};
use crate::lockfree::{ConnectionHandle, StreamBackend};

// Global echo buffer for testing - stores raw bytes as they would be sent over QUIC
// Using DashMap for lock-free access instead of Mutex<HashMap>
lazy_static::lazy_static! {
    static ref ECHO_BUFFER: Arc<DashMap<u64, Vec<u8>>> = Arc::new(DashMap::new());
}

/// TLS configuration for tquic
#[derive(Clone, Debug)]
pub struct TlsConfig {
    pub cert: Certificate,
    pub key: PrivateKey,
    pub ca_cert: Certificate,
    // PEM data for tQUIC integration
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
    pub ca_cert_pem: Vec<u8>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            cert: Certificate(Vec::new()),
            key: PrivateKey(Vec::new()),
            ca_cert: Certificate(Vec::new()),
            cert_pem: Vec::new(),
            key_pem: Vec::new(),
            ca_cert_pem: Vec::new(),
        }
    }
}

/// Connection handle that wraps connection information
#[derive(Debug)]
pub struct ConnHandle {
    pub connection_id: u64,
    pub stream_tx: mpsc::Sender<StreamCommand>,
}

impl Clone for ConnHandle {
    fn clone(&self) -> Self {
        Self {
            connection_id: self.connection_id,
            stream_tx: self.stream_tx.clone(),
        }
    }
}

/// Incoming connection handle
pub struct IncomingHandle {
    pub conn_rx: tokio::sync::Mutex<mpsc::Receiver<ConnHandle>>,
    pub local_addr: String,
}

/// Commands sent to the tquic thread for stream operations
#[derive(Debug)]
pub enum StreamCommand {
    Write {
        stream_id: u64,
        data: Bytes,
        fin: bool,
        respond_to: oneshot::Sender<Result<(), crate::TransportError>>,
    },
    Read {
        stream_id: u64,
        buffer_size: usize,
        respond_to: oneshot::Sender<Result<Option<Bytes>, crate::TransportError>>,
    },
    Close {
        stream_id: u64,
        respond_to: oneshot::Sender<Result<(), crate::TransportError>>,
    },
}

/// Simplified tQUIC context without direct tQUIC integration for now
/// This provides a working implementation while we work around tQUIC's thread safety limitations
#[derive(Clone)]
pub struct TquicClient {
    tls_config: TlsConfig,
    settings: QuicTransportSettings,
    /// Connection counter - using AtomicU64 for lock-free access
    connection_counter: Arc<AtomicU64>,
    /// Store active connections for stream operations - using DashMap for lock-free access
    active_connections: Arc<DashMap<u64, (mpsc::Sender<StreamCommand>, String)>>,
}

impl std::fmt::Debug for TquicClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TquicClient")
            .field("tls_config", &"TlsConfig")
            .field("settings", &self.settings)
            .finish()
    }
}

#[derive(Clone)]
pub struct TquicServer {
    tls_config: TlsConfig,
    settings: QuicTransportSettings,
    /// Connection counter - using AtomicU64 for lock-free access
    connection_counter: Arc<AtomicU64>,
    /// Store active connections for stream operations - using DashMap for lock-free access
    active_connections: Arc<DashMap<u64, (mpsc::Sender<StreamCommand>, String)>>,
}

impl std::fmt::Debug for TquicServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TquicServer")
            .field("tls_config", &"TlsConfig")
            .field("settings", &self.settings)
            .finish()
    }
}

pub type TquicConnection = crate::stream::QuicStream;

/// Simple listener implementation that wraps a channel receiver
#[derive(Debug)]
pub struct TquicListener {
    pub conn_rx: tokio::sync::Mutex<mpsc::Receiver<TquicConnection>>,
    pub local_addr: String,
}

#[async_trait::async_trait]
impl crate::transport::Listener for TquicListener {
    type Connection = TquicConnection;
    
    async fn accept(&mut self) -> Result<Self::Connection, Box<dyn std::error::Error>> {
        let mut rx = self.conn_rx.lock().await;
        rx.recv().await.ok_or_else(|| Box::new(crate::TransportError::StreamError("Listener closed".into())) as Box<dyn std::error::Error>)
    }
}

impl TquicClient {
    pub async fn new(config: crate::config::ClientConfig) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            tls_config: TlsConfig {
                cert: Certificate(vec![]),
                key: PrivateKey(vec![]),
                ca_cert: Certificate(vec![]),
                cert_pem: vec![],
                key_pem: vec![],
                ca_cert_pem: vec![],
            },
            settings: config.transport,
            connection_counter: Arc::new(AtomicU64::new(0)),
            active_connections: Arc::new(DashMap::new()),
        })
    }

    pub async fn connect(&self, addr: &str) -> Result<TquicConnection, Box<dyn std::error::Error>> {
        let server_addr: SocketAddr = addr.parse()?;

        // For testing: skip actual TCP connection, just create a mock connection
        let connection_id = self.connection_counter.fetch_add(1, Ordering::Relaxed);
        
        let (stream_tx, _stream_rx) = mpsc::channel(1000);
        
        // Use DashMap for lock-free insertion
        self.active_connections.insert(connection_id, (stream_tx.clone(), server_addr.to_string()));

        let (_handle, channels) = ConnectionHandle::new(
            connection_id,
            1000,
            1000,
            100,
        );

        let backend = TquicStreamBackend::new(connection_id, stream_tx);
        
        let processor = crate::lockfree::StreamProcessor::new(
            connection_id,
            channels,
            Box::new(backend),
        );
        
        tokio::spawn(async move {
            if let Err(e) = processor.run().await {
                eprintln!("Stream processor error: {:?}", e);
            }
        });

        let stream = crate::stream::QuicStream::new(
            connection_id,
            connection_id, // Use connection_id as stream_id for unique echo buffer per connection
            self.settings.initial_max_stream_data_bidi_remote as usize,
        );

        if let Some(debug_mode) = std::env::var("QUICCRUNCH_DEBUG").ok() {
            if debug_mode == "1" {
                eprintln!("Mock connected to {} with connection_id {}", server_addr, connection_id);
            }
        }

        Ok(stream)
    }
}

impl TquicServer {
    pub async fn new(config: crate::config::ServerConfig) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            tls_config: TlsConfig {
                cert: Certificate(vec![]),
                key: PrivateKey(vec![]),
                ca_cert: Certificate(vec![]),
                cert_pem: vec![],
                key_pem: vec![],
                ca_cert_pem: vec![],
            },
            settings: config.transport,
            connection_counter: Arc::new(AtomicU64::new(0)),
            active_connections: Arc::new(DashMap::new()),
        })
    }

    pub async fn listen(&self, addr: &str) -> Result<TquicListener, Box<dyn std::error::Error>> {
        let listener_addr: std::net::SocketAddr = addr.parse()?;
        
        // Create a TCP listener to accept connections
        let tcp_listener = tokio::net::TcpListener::bind(listener_addr).await?;
        let local_addr = tcp_listener.local_addr()?.to_string();
        
        let (conn_tx, conn_rx) = mpsc::channel(100);
        
        // Spawn a task to handle incoming connections
        let server_clone = self.clone();
        tokio::spawn(async move {
            loop {
                match tcp_listener.accept().await {
                    Ok((tcp_stream, peer_addr)) => {
                        let connection_id = server_clone.connection_counter.fetch_add(1, Ordering::Relaxed);
                        
                        // Store the TCP stream for this connection so client can access the same echo buffer
                        let (stream_tx, _) = mpsc::channel(1000);
                        server_clone.active_connections.insert(connection_id, (stream_tx, peer_addr.to_string()));
                        
                        // Create a QUIC connection that shares the same echo buffer as clients
                        let connection = crate::stream::QuicStream::new(
                            connection_id,
                            0, // stream_id
                            server_clone.settings.initial_max_stream_data_bidi_remote as usize,
                        );
                        
                        if conn_tx.send(connection).await.is_err() {
                            eprintln!("Failed to send new connection to listener");
                            break;
                        }
                        
                        // Keep TCP stream alive for the connection (don't drop it)
                        // In a real implementation, this would be the actual QUIC connection
                        tokio::spawn(async move {
                            // Keep the TCP stream alive for the duration of the connection
                            let _tcp_stream = tcp_stream;
                            // For now, just sleep to keep the connection alive
                            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
                        });
                        
                        if let Some(debug_mode) = std::env::var("QUICCRUNCH_DEBUG").ok() {
                            if debug_mode == "1" {
                                eprintln!("Accepted connection {} from {}", connection_id, peer_addr);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to accept connection: {}", e);
                        break;
                    }
                }
            }
        });
        
        let listener = TquicListener {
            conn_rx: tokio::sync::Mutex::new(conn_rx),
            local_addr,
        };
        
        Ok(listener)
    }
}

/// Stream backend that handles tquic stream operations
struct TquicStreamBackend {
    connection_id: u64,
    stream_tx: mpsc::Sender<StreamCommand>,
}

impl TquicStreamBackend {
    fn new(connection_id: u64, stream_tx: mpsc::Sender<StreamCommand>) -> Self {
        Self { 
            connection_id,
            stream_tx,
        }
    }
}

#[async_trait::async_trait]
impl StreamBackend for TquicStreamBackend {
    async fn write(&mut self, stream_id: u64, data: bytes::Bytes, _fin: bool) -> Result<(), crate::TransportError> {
        // For testing: store raw bytes in echo buffer (simulating raw QUIC stream)
        ECHO_BUFFER.entry(stream_id).or_insert_with(Vec::new).extend_from_slice(&data);
        
        if let Some(debug_mode) = std::env::var("QUICCRUNCH_DEBUG").ok() {
            if debug_mode == "1" {
                let buffer_len = ECHO_BUFFER.get(&stream_id).map(|v| v.len()).unwrap_or(0);
                eprintln!("Backend write: stream {} wrote {} bytes, total in buffer: {}", 
                         stream_id, data.len(), buffer_len);
            }
        }
        
        Ok(())
    }

    async fn read(&mut self, stream_id: u64, buffer_size: usize) -> Result<Option<bytes::Bytes>, crate::TransportError> {
        // For testing: read raw bytes from echo buffer
        if let Some(mut stream_data) = ECHO_BUFFER.get_mut(&stream_id) {
            if !stream_data.is_empty() {
                let read_size = std::cmp::min(buffer_size, stream_data.len());
                let data: Vec<u8> = stream_data.drain(0..read_size).collect();
                
                if let Some(debug_mode) = std::env::var("QUICCRUNCH_DEBUG").ok() {
                    if debug_mode == "1" {
                        eprintln!("Backend read: stream {} read {} bytes, remaining in buffer: {}", 
                                 stream_id, data.len(), stream_data.len());
                    }
                }
                
                return Ok(Some(bytes::Bytes::from(data)));
            }
        }
        
        Ok(None)
    }

    async fn close(&mut self, stream_id: u64) -> Result<(), crate::TransportError> {
        // Clean up echo buffer for this stream
        ECHO_BUFFER.remove(&stream_id);
        Ok(())
    }
}