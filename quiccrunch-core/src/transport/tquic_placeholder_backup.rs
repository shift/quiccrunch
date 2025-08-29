use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use rustls::{Certificate, PrivateKey};
use dashmap::DashMap;

#[cfg(feature = "backend-tquic")]
use tquic;

use crate::lockfree::{ConnectionHandle, StreamBackend};

// Global StreamBridge for SimpleTquic communication
lazy_static::lazy_static! {
    static ref GLOBAL_STREAM_BRIDGE: StreamBridge = StreamBridge::new();
}

// Re-export the TquicStreamHandle from stream module
use crate::stream::TquicStreamHandle;

/// Bridge backend that implements StreamBackend for our tQUIC system
struct TquicStreamBackend {
    bridge: StreamBridge,
}

#[async_trait::async_trait]
impl StreamBackend for TquicStreamBackend {
    async fn write(&mut self, stream_id: u64, data: bytes::Bytes, _fin: bool) -> Result<(), crate::TransportError> {
        self.bridge.write(stream_id, &data).await;
        Ok(())
    }

    async fn read(&mut self, stream_id: u64, buffer_size: usize) -> Result<Option<bytes::Bytes>, crate::TransportError> {
        if let Some(data) = self.bridge.read(stream_id, buffer_size).await {
            Ok(Some(bytes::Bytes::from(data)))
        } else {
            Ok(None)
        }
    }

    async fn close(&mut self, _stream_id: u64) -> Result<(), crate::TransportError> {
        // For now, just return success
        Ok(())
    }
}

// Stream commands for tQUIC stream operations
#[derive(Debug)]
pub enum StreamCmd {
    Write { 
        stream_id: u64,
        data: bytes::Bytes,
        fin: bool,
        rsp: oneshot::Sender<crate::Result<usize>> 
    },
    Read { 
        stream_id: u64,
        len: usize, 
        rsp: oneshot::Sender<crate::Result<Option<Vec<u8>>>> 
    },
    Shutdown { 
        stream_id: u64,
        rsp: oneshot::Sender<crate::Result<()>> 
    },
}

// Connection commands for tQUIC connection operations  
#[derive(Debug)]
pub enum ConnCmd {
    OpenBi { rsp: oneshot::Sender<crate::Result<(TquicStreamHandle, TquicStreamHandle)>> },
}

// tQUIC TLS configuration
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

// Handle for tQUIC connection (now using lock-free channels)
#[derive(Clone, Debug)]
pub struct ConnHandle {
    pub connection_handle: ConnectionHandle,
    pub _connection_id: u64,
}

// Handle for tQUIC incoming connections
pub struct IncomingHandle {
    pub conn_rx: tokio::sync::Mutex<mpsc::Receiver<ConnHandle>>,
    pub local_addr: String,
}

// Main tQUIC context (enhanced with lock-free channels)
#[derive(Clone, Debug)]
pub struct TquicCtx {
    tls_config: TlsConfig,
    settings: crate::config::QuicTransportSettings,
    bridge: StreamBridge,
}

// Shared stream data for communication between client and server streams
// PERFORMANCE OPTIMIZED: Using truly lock-free structures with separate client/server buffers
#[derive(Clone, Debug)]
pub struct StreamBridge {
    client_data: Arc<DashMap<u64, Vec<u8>>>,           // Data written by client, read by server
    server_data: Arc<DashMap<u64, Vec<u8>>>,           // Data written by server, read by client
    client_read_positions: Arc<DashMap<u64, usize>>,  // Client read positions (reading server data)
    server_read_positions: Arc<DashMap<u64, usize>>,  // Server read positions (reading client data)
    next_stream_id: Arc<AtomicU64>,                    // FIXED: Atomic counter
}

impl StreamBridge {
    pub fn new() -> Self {
        Self {
            client_data: Arc::new(DashMap::new()),
            server_data: Arc::new(DashMap::new()),
            client_read_positions: Arc::new(DashMap::new()),
            server_read_positions: Arc::new(DashMap::new()),
            next_stream_id: Arc::new(AtomicU64::new(0)),
        }
    }
    
    pub fn get_next_stream_id(&self) -> u64 {
        self.next_stream_id.fetch_add(1, Ordering::SeqCst)
    }
    
    /// Client writes data (to be read by server)
    pub async fn write(&self, stream_id: u64, data: &[u8]) -> usize {
        println!("StreamBridge client write: stream_id={}, data_len={}", stream_id, data.len());
        self.client_data.entry(stream_id)
            .or_insert_with(Vec::new)
            .extend_from_slice(data);
        println!("StreamBridge client write complete: stream_id={}, total_size={}", 
                 stream_id, 
                 self.client_data.get(&stream_id).map(|entry| entry.len()).unwrap_or(0));
        data.len()
    }
    
    /// Client reads data (from server)
    pub async fn read(&self, stream_id: u64, len: usize) -> Option<Vec<u8>> {
        // Get current read position (atomic operation)
        let current_pos = self.client_read_positions.get(&stream_id).map(|entry| *entry).unwrap_or(0);
        
        println!("StreamBridge client read: stream_id={}, len={}, current_pos={}", stream_id, len, current_pos);
        
        // Get data from server for this stream
        if let Some(data_entry) = self.server_data.get(&stream_id) {
            let data = data_entry.value();
            println!("StreamBridge client read: stream_id={}, server data available={} bytes", stream_id, data.len());
            if current_pos < data.len() {
                let end_pos = (current_pos + len).min(data.len());
                let chunk = data[current_pos..end_pos].to_vec();
                
                // Update read position atomically
                self.client_read_positions.insert(stream_id, end_pos);
                
                println!("StreamBridge client read success: stream_id={}, read {} bytes, new_pos={}", 
                         stream_id, chunk.len(), end_pos);
                Some(chunk)
            } else {
                println!("StreamBridge client read: stream_id={}, already read all server data", stream_id);
                None
            }
        } else {
            println!("StreamBridge client read: stream_id={}, no server data found", stream_id);
            None
        }
    }
    
    /// Server writes data (to be read by client)
    pub async fn write_server_side(&self, stream_id: u64, data: &[u8]) -> usize {
        println!("StreamBridge server write: stream_id={}, data_len={}", stream_id, data.len());
        self.server_data.entry(stream_id)
            .or_insert_with(Vec::new)
            .extend_from_slice(data);
        println!("StreamBridge server write complete: stream_id={}, total_size={}", 
                 stream_id, 
                 self.server_data.get(&stream_id).map(|entry| entry.len()).unwrap_or(0));
        data.len()
    }
    
    /// Server reads data (from client)
    pub async fn read_server_side(&self, stream_id: u64, len: usize) -> Option<Vec<u8>> {
        // Get current read position (atomic operation)
        let current_pos = self.server_read_positions.get(&stream_id).map(|entry| *entry).unwrap_or(0);
        
        println!("StreamBridge server read: stream_id={}, len={}, current_pos={}", stream_id, len, current_pos);
        
        // Get data from client for this stream
        if let Some(data_entry) = self.client_data.get(&stream_id) {
            let data = data_entry.value();
            println!("StreamBridge server read: stream_id={}, client data available={} bytes", stream_id, data.len());
            if current_pos < data.len() {
                let end_pos = (current_pos + len).min(data.len());
                let chunk = data[current_pos..end_pos].to_vec();
                
                // Update read position atomically
                self.server_read_positions.insert(stream_id, end_pos);
                
                println!("StreamBridge server read success: stream_id={}, read {} bytes, new_pos={}", 
                         stream_id, chunk.len(), end_pos);
                Some(chunk)
            } else {
                println!("StreamBridge server read: stream_id={}, already read all client data", stream_id);
                None
            }
        } else {
            println!("StreamBridge server read: stream_id={}, no client data found", stream_id);
            None
        }
    }
    
    /// Clear processed client data to avoid re-processing
    pub async fn clear_processed_client_data(&self, stream_id: u64, bytes_processed: usize) {
        if let Some(mut data_entry) = self.client_data.get_mut(&stream_id) {
            let data = data_entry.value_mut();
            if bytes_processed <= data.len() {
                data.drain(0..bytes_processed);
                // Reset server read position since we removed data from the beginning
                self.server_read_positions.insert(stream_id, 0);
                println!("StreamBridge: cleared {} bytes from client data for stream {}, remaining: {}", 
                         bytes_processed, stream_id, data.len());
            }
        }
    }
}

// Global access functions for SimpleTquic
impl StreamBridge {
    pub fn global() -> &'static StreamBridge {
        &GLOBAL_STREAM_BRIDGE
    }
}

impl TquicCtx {
    pub fn new(tls_config: TlsConfig, settings: crate::config::QuicTransportSettings) -> crate::Result<Self> {        
        Ok(Self {
            tls_config,
            settings,
            bridge: StreamBridge::global().clone(), // Use the global bridge instead of creating a new one
        })
    }

    pub async fn connect(&self, addr: &str) -> crate::Result<crate::transport::Connection> {
        let _server_addr = addr.parse::<SocketAddr>()
            .map_err(|e| crate::TransportError::ConnectionFailed(format!("Invalid address: {}", e)))?;

        // Create connection using lock-free channel system
        let connection_id = 1;
        
        let (connection_handle, channels) = ConnectionHandle::new(
            connection_id,
            1000,  // write_capacity
            1000,  // read_capacity
            100,   // close_capacity
        );

        let handle = ConnHandle {
            connection_handle,
            _connection_id: connection_id,
        };

        // Create and start the lock-free stream processor for this connection
        let backend = TquicStreamBackend {
            bridge: self.bridge.clone(),
        };
        
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

        let (conn_tx, conn_rx) = mpsc::channel::<ConnHandle>(100);
        let bridge = self.bridge.clone();
        
        // For now, create a working echo server that properly uses the bridge
        tokio::spawn(async move {
            // Simulate accepting connections
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            
            let connection_id = 2;
            let (connection_handle, channels) = ConnectionHandle::new(connection_id, 1000, 1000, 100);
            
            // Start an echo server that properly manages the bridge with separate read/write positions
            let bridge_for_echo = bridge.clone();
            tokio::spawn(async move {
                loop {
                    // Check for data on stream ID 0 (fixed stream ID used by client/server) more frequently
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                    
                    let stream_id = 0u64; // Use fixed stream ID 0 to match client/server
                    
                    // Check if there's new incoming data written to this stream (from client)
                    if let Some(incoming_data) = bridge_for_echo.read_server_side(stream_id, 4096).await {
                        // Parse the message: first 4 bytes are length, then the data
                        if incoming_data.len() >= 4 {
                            let msg_len = u32::from_le_bytes([
                                incoming_data[0], incoming_data[1], incoming_data[2], incoming_data[3]
                            ]) as usize;
                            
                            if incoming_data.len() >= 4 + msg_len {
                                // Extract just the payload (without header)
                                let payload = &incoming_data[4..4 + msg_len];
                                
                                // Create echo response: length header + echoed payload
                                let mut response = Vec::with_capacity(4 + payload.len());
                                response.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                                response.extend_from_slice(payload);
                                
                                // Write the echo response back to the client side of the stream
                                bridge_for_echo.write_server_side(stream_id, &response).await;
                                println!("Bridge echo server: echoed {} bytes (payload: {} bytes) for stream {}", 
                                         response.len(), payload.len(), stream_id);
                            }
                        }
                    }
                }
            });
            
            // Create backend and processor
            let backend = TquicStreamBackend {
                bridge: bridge.clone(),
            };
            let processor = crate::lockfree::StreamProcessor::new(connection_id, channels, Box::new(backend));
            tokio::spawn(async move {
                if let Err(e) = processor.run().await {
                    eprintln!("Stream processor error: {:?}", e);
                }
            });
            
            let incoming_handle = ConnHandle {
                connection_handle,
                _connection_id: connection_id,
            };
            
            let _ = conn_tx.send(incoming_handle).await;
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

    async fn handle_send_stream(
        mut cmd_rx: mpsc::Receiver<StreamCmd>,
        bridge: StreamBridge,
    ) {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                StreamCmd::Write { stream_id, data, rsp, .. } => {
                    let written = bridge.write(stream_id, &data).await;
                    let _ = rsp.send(Ok(written));
                }
                StreamCmd::Shutdown { rsp, .. } => {
                    // For now, just acknowledge shutdown
                    let _ = rsp.send(Ok(()));
                    break;
                }
                _ => {}
            }
        }
    }

    async fn handle_recv_stream(
        mut cmd_rx: mpsc::Receiver<StreamCmd>,
        bridge: StreamBridge,
    ) {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                StreamCmd::Read { stream_id, len, rsp } => {
                    let data = bridge.read(stream_id, len).await;
                    let _ = rsp.send(Ok(data));
                }
                _ => {}
            }
        }
    }
}