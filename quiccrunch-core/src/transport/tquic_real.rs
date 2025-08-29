use std::collections::HashMap;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, oneshot};
use bytes::Bytes;
use rustls::{Certificate, PrivateKey};

use tquic::{
    Config as TquicConfig, Endpoint as TquicEndpoint, Connection as TquicConnection,
    TlsConfig as TquicTlsConfig, PacketSendHandler, TransportHandler, PacketInfo, FourTuple,
};

use crate::lockfree::{ConnectionHandle, StreamBackend};

/// Real tQUIC implementation using the actual tquic library
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

/// Connection handle for real tQUIC connections
#[derive(Clone, Debug)]
pub struct ConnHandle {
    pub connection_id: u64,
    pub inner: Arc<Mutex<Option<TquicConnection>>>,
}

/// Incoming connection handle for real tQUIC
pub struct IncomingHandle {
    pub conn_rx: tokio::sync::Mutex<mpsc::Receiver<ConnHandle>>,
    pub local_addr: String,
}

/// Main tQUIC context using real tquic library
#[derive(Clone)]
pub struct TquicCtx {
    tls_config: TlsConfig,
    settings: crate::config::QuicTransportSettings,
    endpoint: Arc<Mutex<Option<TquicEndpoint>>>,
    socket: Arc<UdpSocket>,
    connections: Arc<Mutex<HashMap<u64, Arc<Mutex<TquicConnection>>>>>,
    next_conn_id: Arc<std::sync::atomic::AtomicU64>,
}

impl std::fmt::Debug for TquicCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TquicCtx")
            .field("tls_config", &"TlsConfig")
            .field("settings", &self.settings)
            .finish()
    }
}

/// Packet sender implementation for tQUIC
struct PacketSender {
    socket: Arc<UdpSocket>,
}

impl PacketSendHandler for PacketSender {
    fn send_packet(&self, buf: &[u8], info: &PacketInfo) -> tquic::Result<()> {
        let socket = self.socket.clone();
        let buf = buf.to_vec();
        let dst = info.dst;
        
        // Use tokio spawn to handle async UDP send in sync context
        tokio::spawn(async move {
            if let Err(e) = socket.send_to(&buf, dst).await {
                eprintln!("Failed to send packet: {}", e);
            }
        });
        
        Ok(())
    }
}

/// Transport handler implementation for tQUIC callbacks  
struct QuicTransportHandler {
    conn_tx: mpsc::Sender<ConnHandle>,
    connections: Arc<Mutex<HashMap<u64, Arc<Mutex<TquicConnection>>>>>,
    next_conn_id: Arc<std::sync::atomic::AtomicU64>,
}

impl TransportHandler for QuicTransportHandler {
    fn on_new_connection(&mut self, conn: TquicConnection) -> u64 {
        let conn_id = self.next_conn_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let conn_arc = Arc::new(Mutex::new(conn));
        
        self.connections.lock().unwrap().insert(conn_id, conn_arc.clone());
        
        let handle = ConnHandle {
            connection_id: conn_id,
            inner: Arc::new(Mutex::new(None)), // Will be set later
        };
        
        // Try to send the new connection to the server
        if let Err(_) = self.conn_tx.try_send(handle) {
            eprintln!("Failed to notify about new connection");
        }
        
        conn_id
    }
}

impl TquicCtx {
    pub async fn new(tls_config: TlsConfig, settings: crate::config::QuicTransportSettings) -> crate::Result<Self> {
        // Create UDP socket for tQUIC endpoint
        let socket = UdpSocket::bind("0.0.0.0:0").await
            .map_err(|e| crate::TransportError::ConnectionFailed(format!("UDP bind failed: {}", e)))?;
        
        Ok(Self {
            tls_config,
            settings,
            endpoint: Arc::new(Mutex::new(None)),
            socket: Arc::new(socket),
            connections: Arc::new(Mutex::new(HashMap::new())),
            next_conn_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        })
    }

    pub async fn connect(&self, addr: &str) -> crate::Result<crate::transport::Connection> {
        let server_addr: SocketAddr = addr.parse()
            .map_err(|e| crate::TransportError::ConnectionFailed(format!("Invalid address: {}", e)))?;

        // Create tQUIC configuration
        let mut config = Box::new(TquicConfig::new()?);
        config.set_application_protos(&[b"quic-bench"])?;
        config.set_max_idle_timeout(Some(std::time::Duration::from_millis(self.settings.max_idle_timeout_ms as u64)))?;
        
        // Create TLS config for tQUIC
        let tls_config = TquicTlsConfig::new_client_config(
            Some(&self.tls_config.ca_cert_pem),
            None, // No client cert for now
            None, // No client key for now
        )?;
        
        // Create packet sender
        let sender = Rc::new(PacketSender {
            socket: self.socket.clone(),
        });
        
        // Create transport handler (dummy for client)
        let (conn_tx, _conn_rx) = mpsc::channel(10);
        let handler = Box::new(QuicTransportHandler {
            conn_tx,
            connections: self.connections.clone(),
            next_conn_id: self.next_conn_id.clone(),
        });
        
        // Create tQUIC endpoint
        let mut endpoint = TquicEndpoint::new(config, false, handler, sender);
        
        // Get local address for connection
        let local_addr = self.socket.local_addr()
            .map_err(|e| crate::TransportError::ConnectionFailed(format!("Failed to get local addr: {}", e)))?;
        
        // Connect to server
        let conn_id = endpoint.connect(
            local_addr,
            server_addr,
            Some("localhost"), // server name
            None, // no session resumption
            None, // no token
            None, // use default config
        )?;
        
        // Store the endpoint
        *self.endpoint.lock().unwrap() = Some(endpoint);
        
        // Create our connection handle
        let connection_id = self.next_conn_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let (connection_handle, channels) = ConnectionHandle::new(
            connection_id,
            1000,  // write_capacity
            1000,  // read_capacity
            100,   // close_capacity
        );

        let handle = ConnHandle {
            connection_id,
            inner: Arc::new(Mutex::new(None)),
        };

        // Create stream backend for real tQUIC
        let backend = TquicStreamBackend::new(conn_id, self.connections.clone());
        
        let processor = crate::lockfree::StreamProcessor::new(
            connection_id,
            channels,
            Box::new(backend),
        );
        
        // Spawn the processor to handle stream operations
        tokio::spawn(async move {
            if let Err(e) = processor.run().await {
                eprintln!("Stream processor error: {:?}", e);
            }
        });

        Ok(crate::transport::Connection { 
            inner: crate::transport::ConnectionInner::Tquic(handle), 
            max_message_size: self.settings.initial_max_stream_data_bidi_remote as usize,
        })
    }

    pub async fn listen(&self, addr: &str) -> crate::Result<crate::transport::Incoming> {
        let bind_addr: SocketAddr = addr.parse()
            .map_err(|e| crate::TransportError::ConnectionFailed(format!("Invalid address: {}", e)))?;

        // Create tQUIC configuration for server
        let mut config = Box::new(TquicConfig::new()?);
        config.set_application_protos(&[b"quic-bench"])?;
        config.set_max_idle_timeout(Some(std::time::Duration::from_millis(self.settings.max_idle_timeout_ms as u64)))?;
        
        // Create TLS config for server
        let tls_config = TquicTlsConfig::new_server_config(
            &self.tls_config.cert_pem,
            &self.tls_config.key_pem,
            None, // no OCSP
            None, // no SCT list
        )?;
        
        // Create packet sender
        let sender = Rc::new(PacketSender {
            socket: self.socket.clone(),
        });
        
        // Create transport handler for incoming connections
        let (conn_tx, conn_rx) = mpsc::channel(100);
        let handler = Box::new(QuicTransportHandler {
            conn_tx,
            connections: self.connections.clone(),
            next_conn_id: self.next_conn_id.clone(),
        });
        
        // Create tQUIC endpoint for server
        let endpoint = TquicEndpoint::new(config, true, handler, sender);
        
        // Store the endpoint
        *self.endpoint.lock().unwrap() = Some(endpoint);
        
        // Start UDP packet processing loop
        let endpoint_ref = self.endpoint.clone();
        let socket_ref = self.socket.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65535];
            loop {
                match socket_ref.recv_from(&mut buf).await {
                    Ok((len, src)) => {
                        let packet_info = PacketInfo {
                            src,
                            dst: socket_ref.local_addr().unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap()),
                            ecn: None,
                        };
                        
                        if let Some(ref mut endpoint) = *endpoint_ref.lock().unwrap() {
                            if let Err(e) = endpoint.recv(&mut buf[..len], &packet_info) {
                                eprintln!("Failed to process packet: {:?}", e);
                            }
                            
                            // Process any pending work
                            if let Err(e) = endpoint.process_connections() {
                                eprintln!("Failed to process connections: {:?}", e);
                            }
                            
                            // Handle timeouts
                            if let Some(timeout) = endpoint.timeout() {
                                let now = Instant::now();
                                if timeout.is_zero() {
                                    endpoint.on_timeout(now);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("UDP recv error: {}", e);
                        break;
                    }
                }
            }
        });

        let handle = IncomingHandle {
            conn_rx: tokio::sync::Mutex::new(conn_rx),
            local_addr: bind_addr.to_string(),
        };

        Ok(crate::transport::Incoming {
            inner: crate::transport::IncomingInner::Tquic(handle),
            settings: crate::transport::TransportSettings {
                max_concurrent_uni_streams: 1000,
                max_concurrent_bidi_streams: 1000,
                stream_receive_window: self.settings.initial_max_stream_data_bidi_remote as u32,
                connection_receive_window: self.settings.initial_max_data as u32,
                max_idle_timeout_ms: self.settings.max_idle_timeout_ms,
                keep_alive_interval_ms: self.settings.keep_alive_interval_ms,
                initial_rtt_ms: 100,
                congestion_controller: crate::transport::CongestionController::NewReno,
            },
        })
    }
}

/// Stream backend implementation using real tQUIC
struct TquicStreamBackend {
    tquic_conn_id: u64,
    connections: Arc<Mutex<HashMap<u64, Arc<Mutex<TquicConnection>>>>>,
}

impl TquicStreamBackend {
    fn new(tquic_conn_id: u64, connections: Arc<Mutex<HashMap<u64, Arc<Mutex<TquicConnection>>>>>) -> Self {
        Self {
            tquic_conn_id,
            connections,
        }
    }
}

#[async_trait::async_trait]
impl StreamBackend for TquicStreamBackend {
    async fn write(&mut self, stream_id: u64, data: bytes::Bytes, fin: bool) -> Result<(), crate::TransportError> {
        let connections = self.connections.lock().unwrap();
        if let Some(conn_arc) = connections.get(&self.tquic_conn_id) {
            let mut conn = conn_arc.lock().unwrap();
            
            // Write data to the tQUIC stream
            match conn.stream_write(stream_id, data, fin) {
                Ok(written) => {
                    println!("TquicStreamBackend: wrote {} bytes to stream {}", written, stream_id);
                    Ok(())
                }
                Err(e) => {
                    eprintln!("TquicStreamBackend write error: {:?}", e);
                    Err(crate::TransportError::StreamError(format!("tquic write failed: {:?}", e)))
                }
            }
        } else {
            Err(crate::TransportError::StreamError("Connection not found".into()))
        }
    }

    async fn read(&mut self, stream_id: u64, buffer_size: usize) -> Result<Option<bytes::Bytes>, crate::TransportError> {
        let connections = self.connections.lock().unwrap();
        if let Some(conn_arc) = connections.get(&self.tquic_conn_id) {
            let mut conn = conn_arc.lock().unwrap();
            
            // Read data from the tQUIC stream
            let mut buffer = vec![0u8; buffer_size];
            match conn.stream_read(stream_id, &mut buffer) {
                Ok((bytes_read, _fin)) => {
                    if bytes_read > 0 {
                        buffer.truncate(bytes_read);
                        println!("TquicStreamBackend: read {} bytes from stream {}", bytes_read, stream_id);
                        Ok(Some(bytes::Bytes::from(buffer)))
                    } else {
                        Ok(None)
                    }
                }
                Err(e) => {
                    eprintln!("TquicStreamBackend read error: {:?}", e);
                    Err(crate::TransportError::StreamError(format!("tquic read failed: {:?}", e)))
                }
            }
        } else {
            Err(crate::TransportError::StreamError("Connection not found".into()))
        }
    }

    async fn close(&mut self, stream_id: u64) -> Result<(), crate::TransportError> {
        let connections = self.connections.lock().unwrap();
        if let Some(conn_arc) = connections.get(&self.tquic_conn_id) {
            let mut conn = conn_arc.lock().unwrap();
            
            // Close the tQUIC stream
            match conn.stream_shutdown(stream_id, tquic::Shutdown::Write, 0) {
                Ok(_) => Ok(()),
                Err(e) => {
                    eprintln!("TquicStreamBackend close error: {:?}", e);
                    Err(crate::TransportError::StreamError(format!("tquic close failed: {:?}", e)))
                }
            }
        } else {
            Err(crate::TransportError::StreamError("Connection not found".into()))
        }
    }
}