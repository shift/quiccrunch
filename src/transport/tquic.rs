use std::collections::HashMap;
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::sync::{mpsc, oneshot, Mutex};
use rustls::{Certificate, PrivateKey};

// Re-export the TquicStreamHandle from stream module
use crate::stream::TquicStreamHandle;

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

// Handle for tQUIC connection
#[derive(Clone, Debug)]
pub struct ConnHandle {
    pub cmd_tx: mpsc::Sender<ConnCmd>,
    pub _connection_id: u64,
}

// Handle for tQUIC incoming connections
pub struct IncomingHandle {
    pub conn_rx: tokio::sync::Mutex<mpsc::Receiver<ConnHandle>>,
    pub local_addr: String,
}

// Main tQUIC context
#[derive(Clone, Debug)]
pub struct TquicCtx {
    tls_config: TlsConfig,
    settings: crate::config::QuicTransportSettings,
    bridge: StreamBridge,
}

// Shared stream data for communication between client and server streams
#[derive(Clone, Debug)]
struct StreamBridge {
    data: Arc<Mutex<HashMap<u64, Vec<u8>>>>,
    read_positions: Arc<Mutex<HashMap<u64, usize>>>,
    next_stream_id: Arc<Mutex<u64>>,
}

impl StreamBridge {
    fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(HashMap::new())),
            read_positions: Arc::new(Mutex::new(HashMap::new())),
            next_stream_id: Arc::new(Mutex::new(0)),
        }
    }
    
    async fn get_next_stream_id(&self) -> u64 {
        let mut id = self.next_stream_id.lock().await;
        *id += 1;
        *id
    }
    
    async fn write(&self, stream_id: u64, data: &[u8]) -> usize {
        let mut stream_data = self.data.lock().await;
        stream_data.entry(stream_id).or_insert_with(Vec::new).extend_from_slice(data);
        data.len()
    }
    
    async fn read(&self, stream_id: u64, len: usize) -> Option<Vec<u8>> {
        let stream_data = self.data.lock().await;
        let mut read_pos = self.read_positions.lock().await;
        
        if let Some(data) = stream_data.get(&stream_id) {
            let current_pos = read_pos.get(&stream_id).copied().unwrap_or(0);
            if current_pos < data.len() {
                let end_pos = (current_pos + len).min(data.len());
                let chunk = data[current_pos..end_pos].to_vec();
                read_pos.insert(stream_id, end_pos);
                Some(chunk)
            } else {
                None
            }
        } else {
            None
        }
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

        // Create command channels for the connection
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<ConnCmd>(100);
        let connection_id = 1; // For now, use a simple ID

        let handle = ConnHandle {
            cmd_tx,
            _connection_id: connection_id,
        };

        let bridge = self.bridge.clone();
        
        // Spawn connection task to handle commands - keep it running indefinitely
        tokio::spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    ConnCmd::OpenBi { rsp } => {
                        let stream_id = bridge.get_next_stream_id().await;
                        
                        let (send_cmd_tx, send_cmd_rx) = mpsc::channel::<StreamCmd>(100);
                        let (recv_cmd_tx, recv_cmd_rx) = mpsc::channel::<StreamCmd>(100);
                        
                        let send_stream = TquicStreamHandle::new(stream_id, send_cmd_tx);
                        let recv_stream = TquicStreamHandle::new(stream_id, recv_cmd_tx);

                        // Spawn stream handlers
                        let send_bridge = bridge.clone();
                        tokio::spawn(Self::handle_send_stream(send_cmd_rx, send_bridge));
                        
                        let recv_bridge = bridge.clone();
                        tokio::spawn(Self::handle_recv_stream(recv_cmd_rx, recv_bridge));
                        
                        let _ = rsp.send(Ok((send_stream, recv_stream)));
                    }
                }
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
            
            let (cmd_tx, _cmd_rx) = mpsc::channel::<ConnCmd>(100);
            let incoming_handle = ConnHandle {
                cmd_tx,
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