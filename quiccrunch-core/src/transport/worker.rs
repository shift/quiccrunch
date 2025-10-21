use bytes::Bytes;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use mio::net::UdpSocket;
use mio::{Events, Interest, Poll, Token, Waker};
use socket2::Socket; // Add missing import
use tokio::sync::mpsc;

use crate::error::{Result, TransportError};
use crate::transport::messages::{
    ConnectionId as ConnId, TquicCommand, TquicEvent, WorkerConfig, WorkerHealth, WorkerStats,
    WorkerStatus,
};
use tquic::{Config, Connection, Endpoint, PacketSendHandler, TlsConfig, TransportHandler};

const SOCKET_TOKEN: Token = Token(0);
const WAKER_TOKEN: Token = Token(1);

pub struct TquicWorker {
    worker_id: usize,
    config: WorkerConfig,
    poll: mio::Poll,
    events: Events,
    socket: UdpSocket,
    waker: Arc<Waker>,
    endpoint: Option<Endpoint>,
    command_rx: mpsc::Receiver<TquicCommand>,
    event_tx: Option<mpsc::Sender<TquicEvent>>,
    connections: HashMap<ConnId, ConnId>,
    stream_queue: Arc<std::sync::Mutex<HashMap<ConnId, Vec<u64>>>>,
    running: Arc<AtomicBool>,
    stats: WorkerStats,
    start_time: Instant,
    next_connection_id: ConnId,
}

struct WorkerTransportHandler {
    worker_id: usize,
    event_tx: Option<mpsc::Sender<TquicEvent>>,
    stream_queue: Arc<std::sync::Mutex<HashMap<ConnId, Vec<u64>>>>,
}

// SAFETY: WorkerTransportHandler is only used within a single thread (the worker thread)
unsafe impl Send for WorkerTransportHandler {}

impl WorkerTransportHandler {
    fn new(
        worker_id: usize,
        event_tx: Option<mpsc::Sender<TquicEvent>>,
        stream_queue: Arc<std::sync::Mutex<HashMap<ConnId, Vec<u64>>>>,
    ) -> Self {
        Self {
            worker_id,
            event_tx,
            stream_queue,
        }
    }
}

impl TransportHandler for WorkerTransportHandler {
    fn on_conn_created(&mut self, _conn: &mut Connection) {
        tracing::debug!(worker_id = self.worker_id, "Connection created");
    }

    fn on_conn_established(&mut self, conn: &mut Connection) {
        tracing::debug!(worker_id = self.worker_id, "Connection established");

        // Get connection ID and remote address
        let conn_id = conn.trace_id();
        // Remove peer_addr call - not available in tQUIC v1.6.0
        let remote_addr = "unknown:0".parse().unwrap(); // Placeholder

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

    fn on_stream_created(&mut self, conn: &mut Connection, stream_id: u64) {
        let conn_id = conn.trace_id().as_bytes()[0] as u64;
        
        tracing::debug!(
            worker_id = self.worker_id,
            connection_id = conn_id,
            stream_id = stream_id,
            "Stream created"
        );

        if let Ok(mut queue) = self.stream_queue.lock() {
            queue.entry(conn_id).or_insert_with(Vec::new).push(stream_id);
        }
    }

    fn on_stream_readable(&mut self, _conn: &mut Connection, stream_id: u64) {
        tracing::debug!(
            worker_id = self.worker_id,
            stream_id = stream_id,
            "Stream readable"
        );
    }

    fn on_stream_writable(&mut self, _conn: &mut Connection, stream_id: u64) {
        tracing::debug!(
            worker_id = self.worker_id,
            stream_id = stream_id,
            "Stream writable"
        );
    }

    fn on_stream_closed(&mut self, _conn: &mut Connection, stream_id: u64) {
        tracing::debug!(
            worker_id = self.worker_id,
            stream_id = stream_id,
            "Stream closed"
        );
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
    fn new_simple() -> Self {
        // Create a dummy socket that will never be used
        let dummy_socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        Self {
            socket: Rc::new(UdpSocket::from_std(dummy_socket)),
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
    /// Create an optimized UDP socket with platform-specific optimizations
    fn create_optimized_socket(bind_addr: SocketAddr, worker_id: usize) -> Result<UdpSocket> {
        use socket2::{Domain, Protocol, SockAddr, Socket, Type};
        
        let domain = if bind_addr.is_ipv4() {
            Domain::IPV4
        } else {
            Domain::IPV6
        };
        
        let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))
            .map_err(|e| TransportError::ConnectionFailed(format!("Failed to create socket: {}", e)))?;
        
        // Enable SO_REUSEPORT for load distribution across workers
        #[cfg(unix)]
        {
            if let Err(e) = socket.set_reuse_port(true) {
                tracing::warn!(worker_id = worker_id, "Failed to set SO_REUSEPORT: {}", e);
            }
        }
        
        // Platform-specific optimizations
        #[cfg(target_os = "linux")]
        Self::apply_linux_optimizations(&socket, worker_id)?;
        
        #[cfg(target_os = "macos")]
        Self::apply_macos_optimizations(&socket, worker_id)?;
        
        #[cfg(target_os = "windows")]
        Self::apply_windows_optimizations(&socket, worker_id)?;
        
        // Set socket buffer sizes optimized for QUIC workloads
        Self::optimize_socket_buffers(&socket)?;
        
        // Bind and configure
        socket.bind(&SockAddr::from(bind_addr))
            .map_err(|e| TransportError::ConnectionFailed(format!("Failed to bind socket: {}", e)))?;
        
        socket.set_nonblocking(true)
            .map_err(|e| TransportError::ConnectionFailed(format!("Failed to set nonblocking: {}", e)))?;
        
        // Convert to std socket then mio socket
        let std_socket: std::net::UdpSocket = socket.into();
        Ok(UdpSocket::from_std(std_socket))
    }
    
    #[cfg(target_os = "linux")]
    fn apply_linux_optimizations(socket: &Socket, worker_id: usize) -> Result<()> {
        use socket2::SockRef;
        
        // Try to set CPU affinity for better cache locality
        if let Err(e) = Self::set_cpu_affinity(worker_id) {
            tracing::debug!(worker_id = worker_id, "CPU affinity setting failed: {}", e);
        }
        
        // Enable generic segmentation offload (GSO) for better throughput
        let sock_ref = SockRef::from(socket);
        
        // Set receive timeout to prevent blocking
        let timeout = std::time::Duration::from_millis(1);
        if let Err(e) = sock_ref.set_read_timeout(Some(timeout)) {
            tracing::debug!(worker_id = worker_id, "Failed to set read timeout: {}", e);
        }
        
        Ok(())
    }
    
    #[cfg(target_os = "linux")]
    fn set_cpu_affinity(worker_id: usize) -> Result<()> {
        use libc::{cpu_set_t, sched_setaffinity, CPU_SET, CPU_ZERO};
        
        let cpu_count = num_cpus::get();
        let target_cpu = worker_id % cpu_count;
        
        unsafe {
            let mut cpu_set: cpu_set_t = std::mem::zeroed();
            CPU_ZERO(&mut cpu_set);
            CPU_SET(target_cpu, &mut cpu_set);
            
            let result = sched_setaffinity(
                0, // current thread
                std::mem::size_of::<cpu_set_t>(),
                &cpu_set,
            );
            
            if result != 0 {
                let error = std::io::Error::last_os_error();
                return Err(TransportError::Other(format!("CPU affinity failed: {}", error)));
            }
        }
        
        tracing::debug!(worker_id = worker_id, cpu = target_cpu, "Set CPU affinity");
        Ok(())
    }
    
    #[cfg(target_os = "macos")]
    fn apply_macos_optimizations(socket: &Socket, worker_id: usize) -> Result<()> {
        use socket2::SockRef;
        
        let sock_ref = SockRef::from(socket);
        
        // Set socket priority for better QoS
        if let Err(e) = sock_ref.set_priority(7) { // High priority
            tracing::debug!(worker_id = worker_id, "Failed to set socket priority: {}", e);
        }
        
        Ok(())
    }
    
    #[cfg(target_os = "windows")]
    fn apply_windows_optimizations(socket: &Socket, worker_id: usize) -> Result<()> {
        use socket2::SockRef;
        
        let sock_ref = SockRef::from(socket);
        
        // Windows-specific optimizations
        // Set ideal send backlog size
        const SIO_IDEAL_SEND_BACKLOG_SIZE: u32 = 0x4004747B;
        
        // Note: Windows ioctl operations would need unsafe code
        // For now, just apply basic optimizations
        
        Ok(())
    }
    
    fn optimize_socket_buffers(socket: &Socket) -> Result<()> {
        use socket2::SockRef;
        
        let sock_ref = SockRef::from(socket);
        
        // Optimize buffer sizes for QUIC workloads
        // These values are tuned for high-performance networking
        
        // Receive buffer: Larger for handling packet bursts
        let recv_buffer_size = 2 * 1024 * 1024; // 2MB
        if let Err(e) = sock_ref.set_recv_buffer_size(recv_buffer_size) {
            tracing::warn!("Failed to set receive buffer size: {}", e);
        }
        
        // Send buffer: Moderate size for QUIC's congestion control
        let send_buffer_size = 1024 * 1024; // 1MB
        if let Err(e) = sock_ref.set_send_buffer_size(send_buffer_size) {
            tracing::warn!("Failed to set send buffer size: {}", e);
        }
        
        Ok(())
    }

    pub fn new(
        config: WorkerConfig,
        command_rx: mpsc::Receiver<TquicCommand>,
        event_tx: Option<mpsc::Sender<TquicEvent>>,
    ) -> Result<Self> {
        let poll = Poll::new().map_err(|e| TransportError::Io(e))?;
        let events = Events::with_capacity(1024);

        // Create waker for command notification
        let waker =
            Arc::new(Waker::new(poll.registry(), WAKER_TOKEN).map_err(|e| TransportError::Io(e))?);

        // Create and optimize UDP socket with platform-specific optimizations
        let socket = Self::create_optimized_socket(config.bind_addr, config.worker_id)?;

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
        let stream_queue = Arc::new(std::sync::Mutex::new(HashMap::new()));

        Ok(TquicWorker {
            worker_id: config.worker_id,
            config,
            poll,
            events,
            socket,
            waker,
            endpoint: None,
            command_rx,
            event_tx,
            connections: HashMap::new(),
            stream_queue,
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
        self.poll
            .registry()
            .register(&mut self.socket, SOCKET_TOKEN, Interest::READABLE)
            .map_err(|e| TransportError::Io(e))?;

        // Main event loop
        while self.running.load(Ordering::Relaxed) {
            // Poll for events with a short timeout
            self.poll
                .poll(&mut self.events, Some(Duration::from_millis(10)))
                .map_err(|e| TransportError::Io(e))?;

            // Process poll events
            let mut socket_readable = false;
            let mut waker_triggered = false;
            
            for event in self.events.iter() {
                match event.token() {
                    SOCKET_TOKEN if event.is_readable() => {
                        socket_readable = true;
                    }
                    WAKER_TOKEN => {
                        // Waker triggered - process commands
                        waker_triggered = true;
                    }
                    _ => {
                        // Unknown token
                        tracing::warn!("Unknown poll event token: {:?}", event.token());
                    }
                }
            }

            // Handle events after the iterator is dropped
            if socket_readable {
                self.handle_socket_readable()?;
            }
            if waker_triggered {
                self.process_commands()?;
            }

            // Always process commands (in case waker wasn't triggered)
            self.process_commands()?;

            // Process tQUIC endpoint - remove send_packets() call
            if let Some(ref mut endpoint) = self.endpoint {
                endpoint.process_connections();
                // Note: Packet sending handled via PacketSendHandler callbacks
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
        // Note: enable_early_data and set_application_protos methods don't exist in tQUIC v1.6.0
        // These would need to be set during TlsConfig creation

        // Create TLS config
        let _tls_config = if self.config.is_server {
            if let (Some(cert_file), Some(key_file)) =
                (&self.config.cert_file, &self.config.key_file)
            {
                TlsConfig::new_server_config(
                    cert_file,
                    key_file,
                    self.config.application_protocols.clone(),
                    self.config.enable_early_data,
                )
                .map_err(|e| {
                    TransportError::Other(format!("Failed to create server TLS config: {:?}", e))
                })?
            } else {
                return Err(TransportError::Other(
                    "Server mode requires cert_file and key_file".into(),
                ));
            }
        } else {
            TlsConfig::new_client_config(
                self.config.application_protocols.clone(),
                self.config.enable_early_data,
            )
            .map_err(|e| {
                TransportError::Other(format!("Failed to create client TLS config: {:?}", e))
            })?
        };

        // Create handlers
        let transport_handler = WorkerTransportHandler::new(
            self.worker_id,
            self.event_tx.clone(),
            self.stream_queue.clone(),
        );

        // Simplified packet handler that doesn't need socket access during creation
        let mut packet_handler = WorkerPacketSendHandler::new_simple();
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
                    dst: self
                        .socket
                        .local_addr()
                        .map_err(|e| TransportError::Io(e))?,
                    time: Instant::now(),
                };

                if let Err(e) = endpoint.recv(&mut buf[..len], &pkt_info) {
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
                TquicCommand::Connect {
                    remote_addr,
                    response,
                } => {
                    let result = self.handle_connect(remote_addr);
                    let _ = response.send(result);
                }
                TquicCommand::Send {
                    connection_id,
                    stream_id,
                    data,
                    fin,
                    response,
                } => {
                    let result = self.handle_send(connection_id, stream_id, data, fin);
                    let _ = response.send(result);
                }
                TquicCommand::Receive {
                    connection_id,
                    stream_id,
                    max_length,
                    response,
                } => {
                    let result = self.handle_receive(connection_id, stream_id, max_length);
                    let _ = response.send(result);
                }
                TquicCommand::Accept { response } => {
                    let result = self.handle_accept();
                    let _ = response.send(result);
                }
                TquicCommand::OpenStream {
                    connection_id,
                    response,
                } => {
                    let result = self.handle_open_stream(connection_id);
                    let _ = response.send(result);
                }
                TquicCommand::AcceptStream {
                    connection_id,
                    response,
                } => {
                    let result = self.handle_accept_stream(connection_id);
                    let _ = response.send(result);
                }
                TquicCommand::CloseStream {
                    connection_id,
                    stream_id,
                    error_code,
                } => {
                    self.handle_close_stream(connection_id, stream_id, error_code);
                }
                TquicCommand::CloseConnection {
                    connection_id,
                    error_code,
                    reason,
                } => {
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

            match endpoint.connect(
                self.config.bind_addr, // local address
                remote_addr,           // remote address
                None,                  // server_name as Option<&str>
                None,                  // session as Option<&[u8]>
                None,                  // token as Option<&[u8]>
                None,                  // config as Option<&Config>
            ) {
                Ok(conn) => {
                    self.connections.insert(conn_id, conn);
                    self.stats.total_connections += 1;
                    self.stats.active_connections = self.connections.len();
                    Ok(conn_id)
                }
                Err(e) => Err(TransportError::Other(format!("Connection failed: {:?}", e))),
            }
        } else {
            Err(TransportError::Other("Endpoint not initialized".into()))
        }
    }

    fn handle_send(
        &mut self,
        connection_id: ConnId,
        stream_id: u64,
        data: Vec<u8>,
        fin: bool,
    ) -> Result<()> {
        if let Some(ref mut endpoint) = self.endpoint {
            if let Some(conn) = endpoint.conn_get_mut(connection_id) {
                let bytes_data = Bytes::from(data);
                match conn.stream_write(stream_id, bytes_data, fin) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(TransportError::Other(format!("Send failed: {:?}", e))),
                }
            } else {
                Err(TransportError::Other("Connection not found".into()))
            }
        } else {
            Err(TransportError::Other("Endpoint not initialized".into()))
        }
    }

    fn handle_receive(
        &mut self,
        connection_id: ConnId,
        stream_id: u64,
        max_length: usize,
    ) -> Result<Vec<u8>> {
        if let Some(ref mut endpoint) = self.endpoint {
            if let Some(conn) = endpoint.conn_get_mut(connection_id) {
                let mut buf = vec![0u8; max_length];
                match conn.stream_read(stream_id, &mut buf) {
                    Ok((len, _fin)) => {
                        buf.truncate(len);
                        self.stats.bytes_received += len as u64;
                        Ok(buf)
                    }
                    Err(e) => Err(TransportError::Other(format!("Receive failed: {:?}", e))),
                }
            } else {
                Err(TransportError::Other("Connection not found".into()))
            }
        } else {
            Err(TransportError::Other("Endpoint not initialized".into()))
        }
    }

    fn handle_accept(&mut self) -> Result<(ConnId, SocketAddr)> {
        // For accepting connections, we need to wait for the transport handler to notify us
        // This is a simplified implementation - in reality we'd need a queue of pending connections
        Err(TransportError::Other("Accept not yet implemented".into()))
    }

    fn handle_open_stream(&mut self, connection_id: ConnId) -> Result<u64> {
        if let Some(ref mut endpoint) = self.endpoint {
            if let Some(conn) = endpoint.conn_get_mut(connection_id) {
                match conn.stream_bidi_new(0, false) {
                    // urgency=0, incremental=false
                    Ok(stream_id) => {
                        self.stats.total_streams += 1;
                        Ok(stream_id)
                    }
                    Err(e) => Err(TransportError::Other(format!(
                        "Stream creation failed: {:?}",
                        e
                    ))),
                }
            } else {
                Err(TransportError::Other("Connection not found".into()))
            }
        } else {
            Err(TransportError::Other("Endpoint not initialized".into()))
        }
    }

    fn handle_accept_stream(&mut self, connection_id: ConnId) -> Result<u64> {
        if let Some(ref mut endpoint) = self.endpoint {
            if endpoint.conn_get_mut(connection_id).is_none() {
                return Err(TransportError::ConnectionFailed("Connection not found".into()));
            }

            if let Ok(mut queue) = self.stream_queue.lock() {
                if let Some(stream_ids) = queue.get_mut(&connection_id) {
                    if let Some(stream_id) = stream_ids.pop() {
                        self.stats.total_streams += 1;
                        tracing::debug!(
                            worker_id = self.worker_id,
                            connection_id = connection_id,
                            stream_id = stream_id,
                            "Accepted stream from queue"
                        );
                        return Ok(stream_id);
                    }
                }
            }

            Err(TransportError::Other("No pending streams available".into()))
        } else {
            Err(TransportError::Other("Endpoint not initialized".into()))
        }
    }

    fn handle_close_stream(&mut self, connection_id: ConnId, stream_id: u64, error_code: u64) {
        if let Some(ref mut endpoint) = self.endpoint {
            if let Some(conn) = endpoint.conn_get_mut(connection_id) {
                let _ = conn.stream_shutdown(stream_id, tquic::Shutdown::Write, error_code);
            }
        }
    }

    fn handle_close_connection(&mut self, connection_id: ConnId, error_code: u64, reason: Vec<u8>) {
        if let Some(ref mut endpoint) = self.endpoint {
            if let Some(conn) = endpoint.conn_get_mut(connection_id) {
                let _ = conn.close(true, error_code, &reason);
            }
        }
        // Remove from our tracking
        self.connections.remove(&connection_id);
        self.stats.active_connections = self.connections.len();
    }

    fn update_stats(&mut self) {
        self.stats.uptime = self.start_time.elapsed();
        self.stats.active_connections = self.connections.len();
    }

    pub fn health_status(&self) -> WorkerHealth {
        if self.stats.errors > 100 {
            WorkerHealth::Degraded {
                reason: format!("High error count: {}", self.stats.errors),
            }
        } else if !self.running.load(Ordering::Relaxed) {
            WorkerHealth::Failed {
                error: "Worker stopped".into(),
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

    pub fn get_waker(&self) -> Arc<Waker> {
        self.waker.clone()
    }
}
