use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};
use std::rc::Rc;

use mio::{Events, Token, Interest};
use mio::net::UdpSocket;
use tokio::sync::mpsc;

use tquic::{Config, TlsConfig, Endpoint, TransportHandler, PacketSendHandler, Connection};
use crate::transport::messages::{
    TquicCommand, TquicEvent, WorkerConfig, WorkerStats, WorkerHealth, WorkerStatus, ConnectionId as ConnId
};
use crate::error::{TransportError, Result};

const SOCKET_TOKEN: Token = Token(0);
const COMMAND_TOKEN: Token = Token(1);

pub struct TquicWorker {
    worker_id: usize,
    config: WorkerConfig,
    poll: mio::Poll,
    events: Events,
    socket: UdpSocket,
    endpoint: Option<Endpoint>,
    command_rx: mpsc::Receiver<TquicCommand>,
    event_tx: Option<mpsc::Sender<TquicEvent>>,
    connections: HashMap<ConnId, Box<Connection>>,
    running: Arc<AtomicBool>,
    stats: WorkerStats,
    start_time: Instant,
    next_connection_id: ConnId,
}

// Transport handler that doesn't use Rc<RefCell<T>>
struct WorkerTransportHandler {
    worker_id: usize,
    event_tx: Option<mpsc::Sender<TquicEvent>>,
    connections: *mut HashMap<ConnId, Box<Connection>>, // Raw pointer to avoid Rc issues
}

// SAFETY: WorkerTransportHandler is only used within a single thread (the worker thread)
unsafe impl Send for WorkerTransportHandler {}

impl WorkerTransportHandler {
    fn new(worker_id: usize, event_tx: Option<mpsc::Sender<TquicEvent>>) -> Self {
        Self {
            worker_id,
            event_tx,
            connections: std::ptr::null_mut(),
        }
    }

    fn set_connections(&mut self, connections: &mut HashMap<ConnId, Box<Connection>>) {
        self.connections = connections as *mut HashMap<ConnId, Box<Connection>>;
    }
}

impl TransportHandler for WorkerTransportHandler {
    fn on_conn_created(&mut self, conn: &mut Connection) {
        tracing::debug!(worker_id = self.worker_id, "Connection created");
    }

    fn on_conn_established(&mut self, conn: &mut Connection) {
        tracing::debug!(worker_id = self.worker_id, "Connection established");
        
        // Get connection ID and remote address
        let conn_id = conn.trace_id();
        let remote_addr = conn.peer_addr().unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());
        
        if let Some(ref tx) = self.event_tx {
            let _ = tx.try_send(TquicEvent::ConnectionEstablished {
                connection_id: conn_id.as_bytes()[0] as u64, // Simplified ID extraction
                remote_addr,
            });
        }
    }

    fn on_conn_closed(&mut self, conn: &mut Connection) {
        tracing::debug!(worker_id = self.worker_id, "Connection closed");
        
        let conn_id = conn.trace_id();
        if let Some(ref tx) = self.event_tx {
            let _ = tx.try_send(TquicEvent::ConnectionClosed {
                connection_id: conn_id.as_bytes()[0] as u64,
                error_code: 0,
                reason: Vec::new(),
            });
        }
    }

    fn on_stream_created(&mut self, _conn: &mut Connection, stream_id: u64) {
        tracing::debug!(worker_id = self.worker_id, stream_id = stream_id, "Stream created");
    }

    fn on_stream_readable(&mut self, _conn: &mut Connection, stream_id: u64) {
        tracing::debug!(worker_id = self.worker_id, stream_id = stream_id, "Stream readable");
    }

    fn on_stream_writable(&mut self, _conn: &mut Connection, stream_id: u64) {
        tracing::debug!(worker_id = self.worker_id, stream_id = stream_id, "Stream writable");
    }

    fn on_stream_closed(&mut self, _conn: &mut Connection, stream_id: u64) {
        tracing::debug!(worker_id = self.worker_id, stream_id = stream_id, "Stream closed");
    }

    fn on_new_token(&mut self, _conn: &mut Connection, _token: Vec<u8>) {
        // Handle new token if needed
    }
}

// Packet send handler
struct WorkerPacketSendHandler {
    socket: Rc<UdpSocket>,
    stats: *mut WorkerStats, // Raw pointer to avoid Rc issues
}

unsafe impl Send for WorkerPacketSendHandler {}

impl WorkerPacketSendHandler {
    fn new(socket: Rc<UdpSocket>) -> Self {
        Self {
            socket,
            stats: std::ptr::null_mut(),
        }
    }

    fn set_stats(&mut self, stats: &mut WorkerStats) {
        self.stats = stats as *mut WorkerStats;
    }
}

impl PacketSendHandler for WorkerPacketSendHandler {
    fn on_packets_send(&self, pkts: &[(Vec<u8>, tquic::PacketInfo)]) -> tquic::Result<usize> {
        let mut sent = 0;
        for (buf, packet) in pkts {
            match self.socket.send_to(buf, packet.dst) {
                Ok(_) => {
                    sent += 1;
                    // Update stats if available
                    unsafe {
                        if !self.stats.is_null() {
                            (*self.stats).bytes_sent += buf.len() as u64;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Send error: {}", e);
                    unsafe {
                        if !self.stats.is_null() {
                            (*self.stats).errors += 1;
                        }
                    }
                    return Err(tquic::Error::Done);
                }
            }
        }
        Ok(sent)
    }
}

impl TquicWorker {
    pub fn new(
        config: WorkerConfig,
        command_rx: mpsc::Receiver<TquicCommand>,
        event_tx: Option<mpsc::Sender<TquicEvent>>,
    ) -> Result<Self> {
        let poll = mio::Poll::new().map_err(|e| TransportError::Io(e))?;
        let events = Events::with_capacity(1024);

        // Create UDP socket
        let socket = std::net::UdpSocket::bind(config.bind_addr)
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
        socket.set_nonblocking(true).map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
        
        // Configure SO_REUSEPORT if supported (Linux/macOS)
        #[cfg(unix)]
        {
            use socket2::{Socket};
            let socket2 = Socket::from(socket);
            if let Err(e) = socket2.set_reuse_port(true) {
                tracing::warn!("Failed to set SO_REUSEPORT: {}", e);
            }
        }

        let socket = UdpSocket::from_std(socket);

        let stats = WorkerStats {
            worker_id: config.worker_id,
            active_connections: 0,
            total_connections: 0,
            total_streams: 0,
            bytes_sent: 0,
            bytes_received: 0,
            errors: 0,
            uptime: Duration::new(0, 0),
        };

        let start_time = Instant::now();
        let running = Arc::new(AtomicBool::new(true));

        Ok(TquicWorker {
            worker_id: config.worker_id,
            config,
            poll,
            events,
            socket,
            endpoint: None,
            command_rx,
            event_tx,
            connections: HashMap::new(),
            running,
            stats,
            start_time,
            next_connection_id: 1,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        tracing::info!("Starting tQUIC worker {}", self.worker_id);

        // Initialize endpoint
        self.initialize_endpoint()?;

        // Register socket with poll
        self.poll.registry()
            .register(&mut self.socket, SOCKET_TOKEN, Interest::READABLE)
            .map_err(|e| TransportError::Io(e))?;

        // Main event loop
        while self.running.load(Ordering::Relaxed) {
            // Poll for events with a short timeout
            self.poll.poll(&mut self.events, Some(Duration::from_millis(10)))
                .map_err(|e| TransportError::Io(e))?;

            // Process poll events
            for event in self.events.iter() {
                match event.token() {
                    SOCKET_TOKEN => {
                        if event.is_readable() {
                            self.handle_socket_readable()?;
                        }
                    }
                    _ => {}
                }
            }

            // Process commands
            self.process_commands()?;

            // Process tQUIC endpoint
            if let Some(ref mut endpoint) = self.endpoint {
                endpoint.process_connections();
                endpoint.send_packets();
            }

            // Update stats
            self.update_stats();
        }

        tracing::info!("tQUIC worker {} stopped", self.worker_id);
        Ok(())
    }

    fn initialize_endpoint(&mut self) -> Result<()> {
        // Create tQUIC config
        let mut config = Config::new()
            .map_err(|e| TransportError::Other(format!("Failed to create config: {:?}", e)))?;
        
        config.set_max_idle_timeout(self.config.max_idle_timeout.as_millis() as u64);
        config.set_recv_udp_payload_size(self.config.max_udp_payload_size as u16);
        config.set_send_udp_payload_size(self.config.max_udp_payload_size);
        config.enable_early_data(self.config.enable_early_data);
        config.set_application_protos(&self.config.application_protocols)
            .map_err(|e| TransportError::Other(format!("Failed to set application protocols: {:?}", e)))?;

        // Create TLS config
        let tls_config = if self.config.is_server {
            if let (Some(cert_file), Some(key_file)) = (&self.config.cert_file, &self.config.key_file) {
                TlsConfig::new_server_config(
                    cert_file,
                    key_file,
                    self.config.application_protocols.clone(),
                    self.config.enable_early_data,
                ).map_err(|e| TransportError::Other(format!("Failed to create server TLS config: {:?}", e)))?
            } else {
                return Err(TransportError::Other("Server mode requires cert_file and key_file".into()));
            }
        } else {
            TlsConfig::new_client_config(
                self.config.application_protocols.clone(),
                self.config.enable_early_data,
            ).map_err(|e| TransportError::Other(format!("Failed to create client TLS config: {:?}", e)))?
        };

        // Create handlers
        let mut transport_handler = WorkerTransportHandler::new(self.worker_id, self.event_tx.clone());
        transport_handler.set_connections(&mut self.connections);

        let socket_rc = Rc::new(self.socket.try_clone().map_err(|e| TransportError::Io(e))?);
        let mut packet_handler = WorkerPacketSendHandler::new(socket_rc);
        packet_handler.set_stats(&mut self.stats);

        // Create endpoint
        let endpoint = Endpoint::new(
            Box::new(config),
            self.config.is_server,
            Box::new(transport_handler),
            Rc::new(packet_handler),
        );

        self.endpoint = Some(endpoint);
        Ok(())
    }

    fn handle_socket_readable(&mut self) -> Result<()> {
        let mut buf = [0u8; 65536];
        
        while let Ok((len, src_addr)) = self.socket.recv_from(&mut buf) {
            if let Some(ref mut endpoint) = self.endpoint {
                let pkt_info = tquic::PacketInfo {
                    src: src_addr,
                    dst: self.socket.local_addr().map_err(|e| TransportError::Io(e))?,
                    time: Instant::now(),
                };
                
                if let Err(e) = endpoint.recv(&buf[..len], &pkt_info) {
                    match e {
                        tquic::Error::Done => break, // No more packets to process
                        _ => {
                            tracing::error!("Error processing packet: {:?}", e);
                            self.stats.errors += 1;
                        }
                    }
                }
                
                self.stats.bytes_received += len as u64;
            }
        }
        
        Ok(())
    }

    fn process_commands(&mut self) -> Result<()> {
        while let Ok(cmd) = self.command_rx.try_recv() {
            match cmd {
                TquicCommand::Connect { remote_addr, response } => {
                    let result = self.handle_connect(remote_addr);
                    let _ = response.send(result);
                }
                TquicCommand::Send { connection_id, stream_id, data, fin, response } => {
                    let result = self.handle_send(connection_id, stream_id, data, fin);
                    let _ = response.send(result);
                }
                TquicCommand::Receive { connection_id, stream_id, max_length, response } => {
                    let result = self.handle_receive(connection_id, stream_id, max_length);
                    let _ = response.send(result);
                }
                TquicCommand::Accept { response } => {
                    let result = self.handle_accept();
                    let _ = response.send(result);
                }
                TquicCommand::OpenStream { connection_id, response } => {
                    let result = self.handle_open_stream(connection_id);
                    let _ = response.send(result);
                }
                TquicCommand::CloseStream { connection_id, stream_id, error_code } => {
                    self.handle_close_stream(connection_id, stream_id, error_code);
                }
                TquicCommand::CloseConnection { connection_id, error_code, reason } => {
                    self.handle_close_connection(connection_id, error_code, reason);
                }
                TquicCommand::Shutdown => {
                    self.running.store(false, Ordering::Relaxed);
                    break;
                }
            }
        }
        Ok(())
    }

    fn handle_connect(&mut self, remote_addr: SocketAddr) -> Result<ConnId> {
        if let Some(ref mut endpoint) = self.endpoint {
            let conn_id = self.next_connection_id;
            self.next_connection_id += 1;

            match endpoint.connect(remote_addr, &[]) {
                Ok(conn) => {
                    self.connections.insert(conn_id, conn);
                    self.stats.total_connections += 1;
                    self.stats.active_connections = self.connections.len();
                    Ok(conn_id)
                }
                Err(e) => Err(TransportError::Other(format!("Connection failed: {:?}", e)))
            }
        } else {
            Err(TransportError::Other("Endpoint not initialized".into()))
        }
    }

    fn handle_send(&mut self, connection_id: ConnId, stream_id: u64, data: Vec<u8>, fin: bool) -> Result<()> {
        if let Some(conn) = self.connections.get_mut(&connection_id) {
            match conn.stream_send(stream_id, &data, fin) {
                Ok(_) => Ok(()),
                Err(e) => Err(TransportError::Other(format!("Send failed: {:?}", e)))
            }
        } else {
            Err(TransportError::Other("Connection not found".into()))
        }
    }

    fn handle_receive(&mut self, connection_id: ConnId, stream_id: u64, max_length: usize) -> Result<Vec<u8>> {
        if let Some(conn) = self.connections.get_mut(&connection_id) {
            let mut buf = vec![0u8; max_length];
            match conn.stream_recv(stream_id, &mut buf) {
                Ok((len, _fin)) => {
                    buf.truncate(len);
                    self.stats.bytes_received += len as u64;
                    Ok(buf)
                }
                Err(e) => Err(TransportError::Other(format!("Receive failed: {:?}", e)))
            }
        } else {
            Err(TransportError::Other("Connection not found".into()))
        }
    }

    fn handle_accept(&mut self) -> Result<(ConnId, SocketAddr)> {
        // For accepting connections, we need to wait for the transport handler to notify us
        // This is a simplified implementation - in reality we'd need a queue of pending connections
        Err(TransportError::Other("Accept not yet implemented".into()))
    }

    fn handle_open_stream(&mut self, connection_id: ConnId) -> Result<u64> {
        if let Some(conn) = self.connections.get_mut(&connection_id) {
            match conn.stream_new(true) {
                Ok(stream_id) => {
                    self.stats.total_streams += 1;
                    Ok(stream_id)
                }
                Err(e) => Err(TransportError::Other(format!("Stream creation failed: {:?}", e)))
            }
        } else {
            Err(TransportError::Other("Connection not found".into()))
        }
    }

    fn handle_close_stream(&mut self, connection_id: ConnId, stream_id: u64, error_code: u64) {
        if let Some(conn) = self.connections.get_mut(&connection_id) {
            let _ = conn.stream_shutdown(stream_id, tquic::Shutdown::Write, error_code);
        }
    }

    fn handle_close_connection(&mut self, connection_id: ConnId, error_code: u64, reason: Vec<u8>) {
        if let Some(mut conn) = self.connections.remove(&connection_id) {
            let _ = conn.close(true, error_code, &reason);
            self.stats.active_connections = self.connections.len();
        }
    }

    fn update_stats(&mut self) {
        self.stats.uptime = self.start_time.elapsed();
        self.stats.active_connections = self.connections.len();
    }

    pub fn health_status(&self) -> WorkerHealth {
        if self.stats.errors > 100 {
            WorkerHealth::Degraded {
                reason: format!("High error count: {}", self.stats.errors)
            }
        } else if !self.running.load(Ordering::Relaxed) {
            WorkerHealth::Failed {
                error: "Worker stopped".into()
            }
        } else {
            WorkerHealth::Healthy
        }
    }

    pub fn get_status(&self) -> WorkerStatus {
        WorkerStatus {
            worker_id: self.worker_id,
            health: self.health_status(),
            stats: WorkerStats {
                worker_id: self.stats.worker_id,
                active_connections: self.stats.active_connections,
                total_connections: self.stats.total_connections,
                total_streams: self.stats.total_streams,
                bytes_sent: self.stats.bytes_sent,
                bytes_received: self.stats.bytes_received,
                errors: self.stats.errors,
                uptime: self.stats.uptime,
            },
            queue_depth: 0, // Would need to track command queue depth
        }
    }

    pub fn shutdown(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}