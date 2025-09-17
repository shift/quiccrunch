use crate::error::TransportError;
use std::net::SocketAddr;
use tokio::sync::oneshot;

#[derive(Debug)]
pub enum TquicCommand {
    Connect {
        remote_addr: SocketAddr,
        response: oneshot::Sender<Result<ConnectionId, TransportError>>,
    },
    Send {
        connection_id: ConnectionId,
        stream_id: u64,
        data: Vec<u8>,
        fin: bool,
        response: oneshot::Sender<Result<(), TransportError>>,
    },
    Receive {
        connection_id: ConnectionId,
        stream_id: u64,
        max_length: usize,
        response: oneshot::Sender<Result<Vec<u8>, TransportError>>,
    },
    Accept {
        response: oneshot::Sender<Result<(ConnectionId, SocketAddr), TransportError>>,
    },
    OpenStream {
        connection_id: ConnectionId,
        response: oneshot::Sender<Result<u64, TransportError>>,
    },
    CloseStream {
        connection_id: ConnectionId,
        stream_id: u64,
        error_code: u64,
    },
    CloseConnection {
        connection_id: ConnectionId,
        error_code: u64,
        reason: Vec<u8>,
    },
    Shutdown,
}

#[derive(Debug)]
pub enum TquicEvent {
    ConnectionEstablished {
        connection_id: ConnectionId,
        remote_addr: SocketAddr,
    },
    ConnectionClosed {
        connection_id: ConnectionId,
        error_code: u64,
        reason: Vec<u8>,
    },
    StreamCreated {
        connection_id: ConnectionId,
        stream_id: u64,
        is_bidirectional: bool,
    },
    StreamClosed {
        connection_id: ConnectionId,
        stream_id: u64,
        error_code: u64,
    },
    DataReceived {
        connection_id: ConnectionId,
        stream_id: u64,
        data: Vec<u8>,
        fin: bool,
    },
    StreamWritable {
        connection_id: ConnectionId,
        stream_id: u64,
    },
}

pub type ConnectionId = u64;

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub worker_id: usize,
    pub bind_addr: SocketAddr,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    pub is_server: bool,
    pub max_idle_timeout: std::time::Duration,
    pub max_udp_payload_size: usize,
    pub enable_early_data: bool,
    pub application_protocols: Vec<Vec<u8>>,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            worker_id: 0,
            bind_addr: "0.0.0.0:0".parse().unwrap(),
            cert_file: None,
            key_file: None,
            is_server: false,
            max_idle_timeout: std::time::Duration::from_secs(30),
            max_udp_payload_size: 1350,
            enable_early_data: false,
            application_protocols: vec![b"h3".to_vec()],
        }
    }
}

#[derive(Debug)]
pub struct WorkerStats {
    pub worker_id: usize,
    pub active_connections: usize,
    pub total_connections: u64,
    pub total_streams: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub errors: u64,
    pub uptime: std::time::Duration,
}

#[derive(Debug, Clone)]
pub enum WorkerHealth {
    Healthy,
    Degraded { reason: String },
    Failed { error: String },
}

#[derive(Debug)]
pub struct WorkerStatus {
    pub worker_id: usize,
    pub health: WorkerHealth,
    pub stats: WorkerStats,
    pub queue_depth: usize,
}
