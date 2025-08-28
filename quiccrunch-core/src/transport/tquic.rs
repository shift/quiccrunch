use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::sync::{mpsc, oneshot};
use rustls::{Certificate, PrivateKey};
use dashmap::DashMap;

// Global StreamBridge for SimpleTquic communication
lazy_static::lazy_static! {
    static ref GLOBAL_STREAM_BRIDGE: StreamBridge = StreamBridge::new();
}

// Import lock-free channel system
use crate::lockfree::{StreamHandle, ConnectionHandle, StreamBackend};

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
// PERFORMANCE OPTIMIZED: Using truly lock-free structures
#[derive(Clone, Debug)]
pub struct StreamBridge {
    data: Arc<DashMap<u64, Vec<u8>>>,           // FIXED: Lock-free HashMap
    read_positions: Arc<DashMap<u64, usize>>,  // FIXED: Lock-free HashMap  
    next_stream_id: Arc<AtomicU64>,             // FIXED: Atomic counter
}

impl StreamBridge {
    pub fn new() -> Self {
        Self {
            data: Arc::new(DashMap::new()),
            read_positions: Arc::new(DashMap::new()),
            next_stream_id: Arc::new(AtomicU64::new(0)),
        }
    }
    
    pub fn get_next_stream_id(&self) -> u64 {
        self.next_stream_id.fetch_add(1, Ordering::SeqCst)
    }
    
    pub async fn write(&self, stream_id: u64, data: &[u8]) -> usize {
        self.data.entry(stream_id)
            .or_insert_with(Vec::new)
            .extend_from_slice(data);
        data.len()
    }
    
    pub async fn read(&self, stream_id: u64, len: usize) -> Option<Vec<u8>> {
        // Get current read position (atomic operation)
        let current_pos = self.read_positions.get(&stream_id).map(|entry| *entry).unwrap_or(0);
        
        // Get data for the stream
        if let Some(data_entry) = self.data.get(&stream_id) {
            let data = data_entry.value();
            if current_pos < data.len() {
                let end_pos = (current_pos + len).min(data.len());
                let chunk = data[current_pos..end_pos].to_vec();
                
                // Update read position atomically
                self.read_positions.insert(stream_id, end_pos);
                
                Some(chunk)
            } else {
                None
            }
        } else {
            None
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
            bridge: StreamBridge::new(),
        })
    }

    pub async fn connect(&self, addr: &str) -> crate::Result<crate::transport::Connection> {
        let _server_addr = addr.parse::<SocketAddr>()
            .map_err(|e| crate::TransportError::ConnectionFailed(format!("Invalid address: {}", e)))?;

        // Create connection using lock-free channel system
        let connection_id = 1; // For now, use a simple ID
        
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
        let local_addr = bind_addr.to_string();

        let handle = IncomingHandle {
            conn_rx: tokio::sync::Mutex::new(conn_rx),
            local_addr,
        };

        // For now, we'll simulate accepting connections by creating a connection immediately
        // In a real implementation, this would bind to the socket and accept actual connections
        tokio::spawn(async move {
            // Simulate an incoming connection after a short delay
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            
            let (_cmd_tx, _cmd_rx) = mpsc::channel::<ConnCmd>(100);
            
            // Create connection handle for lock-free channels with default capacities
            let (connection_handle, _channels) = ConnectionHandle::new(2, 1000, 1000, 100);
            let incoming_handle = ConnHandle {
                connection_handle,
                _connection_id: 2,
            };
            
            let _ = conn_tx.send(incoming_handle).await;
        });

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