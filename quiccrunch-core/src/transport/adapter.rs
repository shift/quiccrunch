use std::collections::HashMap;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use std::thread;

use log::{debug, error, info};
use mio::Interest;
use tokio::sync::{mpsc, oneshot};

use tquic::{Config, Connection, Endpoint, PacketInfo, TlsConfig, TransportHandler};

use crate::error::TransportError;
use crate::transport::messages::WorkerConfig;

const MAX_BUF_SIZE: usize = 65536;

pub struct QuicSocket {
    socket: Arc<Mutex<mio::net::UdpSocket>>,
    local_addr: SocketAddr,
}

impl QuicSocket {
    pub fn new(local: &SocketAddr, registry: &mio::Registry) -> Result<Self, TransportError> {
        let mut socket = mio::net::UdpSocket::bind(*local)
            .map_err(|e| TransportError::Io(e))?;
        let local_addr = socket.local_addr()
            .map_err(|e| TransportError::Io(e))?;

        registry
            .register(&mut socket, mio::Token(0), Interest::READABLE)
            .map_err(|e| TransportError::Io(e))?;

        Ok(Self {
            socket: Arc::new(Mutex::new(socket)),
            local_addr,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn recv_from(
        &self,
        buf: &mut [u8],
        _token: mio::Token,
    ) -> std::io::Result<(usize, SocketAddr, SocketAddr)> {
        match self.socket.lock().unwrap().recv_from(buf) {
            Ok((len, remote)) => Ok((len, self.local_addr, remote)),
            Err(e) => Err(e),
        }
    }

    pub fn send_to(&self, buf: &[u8], _src: SocketAddr, dst: SocketAddr) -> std::io::Result<usize> {
        Ok(self.socket.lock().unwrap().send_to(buf, dst)?)
    }
}

impl tquic::PacketSendHandler for QuicSocket {
    fn on_packets_send(&self, pkts: &[(Vec<u8>, PacketInfo)]) -> tquic::Result<usize> {
        let mut count = 0;
        for (pkt, info) in pkts {
            if let Err(e) = self.send_to(pkt, info.src, info.dst) {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    debug!("socket send would block");
                    return Ok(count);
                }
                return Err(tquic::Error::InvalidOperation(format!(
                    "socket send_to(): {:?}",
                    e
                )));
            }
            debug!("written {} bytes", pkt.len());
            count += 1;
        }
        Ok(count)
    }
}

#[derive(Default)]
struct ConnectionState {
    established: bool,
    readable_streams: Vec<u64>,
    writable_streams: Vec<u64>,
}

struct TquicHandler {
    connections: Arc<Mutex<HashMap<u64, ConnectionState>>>,
    event_tx: mpsc::UnboundedSender<TquicEvent>,
}

impl TquicHandler {
    fn new(event_tx: mpsc::UnboundedSender<TquicEvent>) -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }
}

#[derive(Debug, Clone)]
enum TquicEvent {
    ConnectionEstablished { index: u64, remote: SocketAddr },
    ConnectionClosed { index: u64 },
    StreamReadable { index: u64, stream_id: u64 },
    StreamWritable { index: u64, stream_id: u64 },
    StreamClosed { index: u64, stream_id: u64 },
}

impl TransportHandler for TquicHandler {
    fn on_conn_created(&mut self, conn: &mut Connection) {
        debug!("{} connection created", conn.trace_id());
        let index = conn.index().unwrap();
        let mut conns = self.connections.lock().unwrap();
        conns.insert(index, ConnectionState::default());
    }

    fn on_conn_established(&mut self, conn: &mut Connection) {
        debug!("{} connection established", conn.trace_id());
        let index = conn.index().unwrap();
        
        let mut conns = self.connections.lock().unwrap();
        if let Some(state) = conns.get_mut(&index) {
            state.established = true;
        }

        let remote = conn.paths_iter().next().map(|p| p.remote).unwrap_or_else(|| {
            SocketAddr::from(([0, 0, 0, 0], 0))
        });

        let _ = self.event_tx.send(TquicEvent::ConnectionEstablished { index, remote });
    }

    fn on_conn_closed(&mut self, conn: &mut Connection) {
        debug!("{} connection closed", conn.trace_id());
        let index = conn.index().unwrap();
        
        let mut conns = self.connections.lock().unwrap();
        conns.remove(&index);
        
        let _ = self.event_tx.send(TquicEvent::ConnectionClosed { index });
    }

    fn on_stream_created(&mut self, conn: &mut Connection, stream_id: u64) {
        debug!("{} stream {} created", conn.trace_id(), stream_id);
    }

    fn on_stream_readable(&mut self, conn: &mut Connection, stream_id: u64) {
        let index = conn.index().unwrap();
        let _ = self.event_tx.send(TquicEvent::StreamReadable { index, stream_id });
    }

    fn on_stream_writable(&mut self, conn: &mut Connection, stream_id: u64) {
        let index = conn.index().unwrap();
        let _ = self.event_tx.send(TquicEvent::StreamWritable { index, stream_id });
    }

    fn on_stream_closed(&mut self, conn: &mut Connection, stream_id: u64) {
        debug!("{} stream {} closed", conn.trace_id(), stream_id);
        let index = conn.index().unwrap();
        let _ = self.event_tx.send(TquicEvent::StreamClosed { index, stream_id });
    }

    fn on_new_token(&mut self, _conn: &mut Connection, _token: Vec<u8>) {}
}

pub struct TquicAdapter {
    endpoint: Endpoint,
    poll: mio::Poll,
    sock: Rc<QuicSocket>,
    recv_buf: Vec<u8>,
    event_rx: mpsc::UnboundedReceiver<TquicEvent>,
    running: bool,
    poll_events: mio::Events,
}

impl TquicAdapter {
    pub fn new(config: WorkerConfig) -> Result<Self, TransportError> {
        let mut quic_config = Config::new()
            .map_err(|e| TransportError::ConfigError(format!("Config creation failed: {:?}", e)))?;

        quic_config.set_recv_udp_payload_size(65527);
        quic_config.set_send_udp_payload_size(1200);
        quic_config.set_max_idle_timeout(30000);
        quic_config.set_max_handshake_timeout(10000);
        quic_config.set_initial_rtt(333);
        quic_config.set_cid_len(8);
        quic_config.enable_stateless_reset(true);
        quic_config.set_congestion_control_algorithm(tquic::CongestionControlAlgorithm::Bbr);
        quic_config.set_initial_congestion_window(32);
        quic_config.set_min_congestion_window(4);

        if config.is_server {
            let cert_file = config.cert_file.as_ref()
                .ok_or_else(|| TransportError::ConfigError("Server requires cert_file".into()))?;
            let key_file = config.key_file.as_ref()
                .ok_or_else(|| TransportError::ConfigError("Server requires key_file".into()))?;

            let tls_config = TlsConfig::new_server_config(
                cert_file,
                key_file,
                vec![b"h3".to_vec(), b"http/0.9".to_vec()],
                true,
            ).map_err(|e| TransportError::ConfigError(format!("TLS config failed: {:?}", e)))?;

            quic_config.set_tls_config(tls_config);
        } else {
            let tls_config = TlsConfig::new_client_config(
                vec![b"h3".to_vec(), b"http/0.9".to_vec()],
                false,
            ).map_err(|e| TransportError::ConfigError(format!("TLS config failed: {:?}", e)))?;

            quic_config.set_tls_config(tls_config);
        }

        let poll = mio::Poll::new()
            .map_err(|e| TransportError::Io(e))?;
        let registry = poll.registry();

        let bind_addr = config.bind_addr;

        let sock = Rc::new(QuicSocket::new(&bind_addr, registry)?);

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let handlers = TquicHandler::new(event_tx);

        let endpoint = Endpoint::new(
            Box::new(quic_config),
            config.is_server,
            Box::new(handlers),
            sock.clone(),
        );

        Ok(Self {
            endpoint,
            poll,
            sock,
            recv_buf: vec![0u8; MAX_BUF_SIZE],
            event_rx,
            running: false,
            poll_events: mio::Events::with_capacity(1024),
        })
    }
    
    fn recv_packets(&mut self) {
        loop {
            match self.sock.socket.lock().unwrap().recv_from(&mut self.recv_buf) {
                Ok((len, from)) => {
                    let pkt_buf = &mut self.recv_buf[..len];
                    let pkt_info = PacketInfo {
                        src: from,
                        dst: self.sock.local_addr,
                        time: Instant::now(),
                    };
                    
                    if let Err(e) = self.endpoint.recv(pkt_buf, &pkt_info) {
                        debug!("recv error: {:?}", e);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    break;
                }
                Err(e) => {
                    error!("Socket recv error: {:?}", e);
                    break;
                }
            }
        }
    }

    pub fn run_event_loop(&mut self) -> Result<(), TransportError> {
        self.running = true;
        let mut events = mio::Events::with_capacity(1024);

        while self.running {
            if let Err(e) = self.endpoint.process_connections() {
                error!("process connections error: {:?}", e);
            }

            let timeout = self.endpoint.timeout();
            self.poll.poll(&mut events, timeout)
                .map_err(|e| TransportError::Io(e))?;

            for event in events.iter() {
                if event.is_readable() {
                    self.process_read_event(event)?;
                }
            }

            self.endpoint.on_timeout(Instant::now());

            while let Ok(event) = self.event_rx.try_recv() {
                debug!("Received event: {:?}", event);
            }
        }

        Ok(())
    }

    fn process_read_event(&mut self, event: &mio::event::Event) -> Result<(), TransportError> {
        loop {
            let (len, local, remote) = match self.sock.recv_from(&mut self.recv_buf, event.token()) {
                Ok(v) => v,
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        debug!("socket recv would block");
                        break;
                    }
                    return Err(TransportError::Io(e));
                }
            };
            debug!("socket recv {} bytes from {:?}", len, remote);

            let pkt_buf = &mut self.recv_buf[..len];
            let pkt_info = PacketInfo {
                src: remote,
                dst: local,
                time: Instant::now(),
            };

            match self.endpoint.recv(pkt_buf, &pkt_info) {
                Ok(_) => {}
                Err(e) => {
                    error!("recv failed: {:?}", e);
                    continue;
                }
            };
        }

        Ok(())
    }

    pub fn connect(&mut self, remote_addr: SocketAddr, server_name: &str) -> Result<u64, TransportError> {
        self.endpoint
            .connect(
                self.sock.local_addr(),
                remote_addr,
                Some(server_name),
                None,
                None,
                None,
            )
            .map_err(|e| TransportError::ConnectionFailed(format!("Connect failed: {:?}", e)))
    }

    pub fn stream_write(&mut self, conn_index: u64, stream_id: u64, data: &[u8], fin: bool) -> Result<usize, TransportError> {
        let conn = self.endpoint.conn_get_mut(conn_index)
            .ok_or_else(|| TransportError::ConnectionFailed("Connection not found".into()))?;

        conn.stream_write(stream_id, bytes::Bytes::copy_from_slice(data), fin)
            .map_err(|e| TransportError::StreamError(format!("Stream write failed: {:?}", e)))
    }

    pub fn stream_read(&mut self, conn_index: u64, stream_id: u64, buf: &mut [u8]) -> Result<(usize, bool), TransportError> {
        let conn = self.endpoint.conn_get_mut(conn_index)
            .ok_or_else(|| TransportError::ConnectionFailed("Connection not found".into()))?;

        conn.stream_read(stream_id, buf)
            .map_err(|e| TransportError::StreamError(format!("Stream read failed: {:?}", e)))
    }

    pub fn close(&mut self) {
        self.running = false;
        self.endpoint.close(false);
    }
}

enum RuntimeCommand {
    Connect {
        server_name: String,
        server_addr: SocketAddr,
        resp: oneshot::Sender<Result<u64, TransportError>>,
    },
    Accept {
        resp: oneshot::Sender<Result<(u64, SocketAddr), TransportError>>,
    },
    AcceptStream {
        conn_index: u64,
        resp: oneshot::Sender<Result<u64, TransportError>>,
    },
    OpenStream {
        conn_index: u64,
        resp: oneshot::Sender<Result<u64, TransportError>>,
    },
    StreamWrite {
        conn_index: u64,
        stream_id: u64,
        data: Vec<u8>,
        fin: bool,
        resp: oneshot::Sender<Result<usize, TransportError>>,
    },
    StreamRead {
        conn_index: u64,
        stream_id: u64,
        resp: oneshot::Sender<Result<(Vec<u8>, bool), TransportError>>,
    },
    CloseConnection {
        conn_index: u64,
        resp: oneshot::Sender<Result<(), TransportError>>,
    },
    Shutdown,
}

pub struct TquicRuntime {
    cmd_tx: mpsc::UnboundedSender<RuntimeCommand>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl std::fmt::Debug for TquicRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TquicRuntime").finish()
    }
}

impl TquicRuntime {
    pub fn new(is_server: bool, bind_addr: SocketAddr, cert_file: Option<String>, key_file: Option<String>) -> Result<Self, TransportError> {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<RuntimeCommand>();
        
        let thread_handle = thread::spawn(move || {
            let config = WorkerConfig {
                worker_id: 0,
                bind_addr,
                is_server,
                cert_file,
                key_file,
                max_idle_timeout: std::time::Duration::from_secs(30),
                max_udp_payload_size: 65527,
                enable_early_data: false,
                application_protocols: vec![b"h3".to_vec(), b"http/0.9".to_vec()],
            };
            
            let mut adapter = TquicAdapter::new(config)
                .expect("Failed to create TquicAdapter");
            
            let mut stream_readers: HashMap<(u64, u64), Vec<oneshot::Sender<Result<(Vec<u8>, bool), TransportError>>>> = HashMap::new();
            let mut pending_accepts: Vec<oneshot::Sender<Result<(u64, SocketAddr), TransportError>>> = Vec::new();
            let mut incoming_connections: Vec<(u64, SocketAddr)> = Vec::new();
            let mut pending_stream_accepts: HashMap<u64, Vec<oneshot::Sender<Result<u64, TransportError>>>> = HashMap::new();
            let mut incoming_streams: HashMap<u64, Vec<u64>> = HashMap::new();
            let mut next_stream_ids: HashMap<u64, u64> = HashMap::new();
            let mut pending_connects: HashMap<u64, (Instant, oneshot::Sender<Result<u64, TransportError>>)> = HashMap::new();
            
            info!("TquicRuntime thread started on {}", bind_addr);
            
            loop {
                while let Ok(cmd) = cmd_rx.try_recv() {
                    match cmd {
                        RuntimeCommand::Connect { server_name, server_addr, resp } => {
                            match adapter.connect(server_addr, &server_name) {
                                Ok(conn_index) => {
                                    pending_connects.insert(conn_index, (Instant::now(), resp));
                                }
                                Err(e) => {
                                    let _ = resp.send(Err(e));
                                }
                            }
                        }
                        RuntimeCommand::Accept { resp } => {
                            if let Some((conn_index, remote)) = incoming_connections.pop() {
                                let _ = resp.send(Ok((conn_index, remote)));
                            } else {
                                pending_accepts.push(resp);
                            }
                        }
                        RuntimeCommand::AcceptStream { conn_index, resp } => {
                            if let Some(streams) = incoming_streams.get_mut(&conn_index) {
                                if let Some(stream_id) = streams.pop() {
                                    let _ = resp.send(Ok(stream_id));
                                } else {
                                    pending_stream_accepts.entry(conn_index)
                                        .or_insert_with(Vec::new)
                                        .push(resp);
                                }
                            } else {
                                pending_stream_accepts.entry(conn_index)
                                    .or_insert_with(Vec::new)
                                    .push(resp);
                            }
                        }
                        RuntimeCommand::OpenStream { conn_index, resp } => {
                            let stream_id = next_stream_ids.entry(conn_index).or_insert(0);
                            let result = adapter.endpoint.conn_get_mut(conn_index)
                                .and_then(|conn| conn.stream_new(*stream_id, 0, true).ok())
                                .ok_or_else(|| TransportError::StreamError("Failed to open stream".into()));
                            
                            if result.is_ok() {
                                let current_id = *stream_id;
                                *stream_id += 4;
                                let _ = resp.send(Ok(current_id));
                            } else {
                                let _ = resp.send(result.map(|_| 0));
                            }
                        }
                        RuntimeCommand::StreamWrite { conn_index, stream_id, data, fin, resp } => {
                            let result = adapter.stream_write(conn_index, stream_id, &data, fin);
                            let _ = resp.send(result);
                        }
                        RuntimeCommand::StreamRead { conn_index, stream_id, resp } => {
                            stream_readers.entry((conn_index, stream_id))
                                .or_insert_with(Vec::new)
                                .push(resp);
                        }
                        RuntimeCommand::CloseConnection { conn_index, resp } => {
                            if let Some(conn) = adapter.endpoint.conn_get_mut(conn_index) {
                                conn.close(true, 0, b"Application close");
                                let _ = resp.send(Ok(()));
                            } else {
                                let _ = resp.send(Err(TransportError::ConnectionFailed("Connection not found".into())));
                            }
                        }
                        RuntimeCommand::Shutdown => {
                            adapter.close();
                            return;
                        }
                    }
                }
                
                adapter.endpoint.process_connections();
                
                let tquic_timeout = adapter.endpoint.timeout();
                if let Some(_timeout) = tquic_timeout {
                    adapter.endpoint.on_timeout(Instant::now());
                }
                
                let poll_timeout = tquic_timeout.unwrap_or(std::time::Duration::from_millis(100));
                match adapter.poll.poll(&mut adapter.poll_events, Some(poll_timeout)) {
                    Ok(_) => {
                        let has_readable = adapter.poll_events.iter().any(|e| e.is_readable());
                        if has_readable {
                            adapter.recv_packets();
                        }
                    }
                    Err(e) => {
                        error!("Poll error: {:?}", e);
                    }
                }
                
                while let Ok(event) = adapter.event_rx.try_recv() {
                    match event {
                        TquicEvent::ConnectionEstablished { index, remote } => {
                            if let Some((_start, resp)) = pending_connects.remove(&index) {
                                let _ = resp.send(Ok(index));
                            } else if let Some(waiter) = pending_accepts.pop() {
                                let _ = waiter.send(Ok((index, remote)));
                            } else {
                                incoming_connections.push((index, remote));
                            }
                        }
                        TquicEvent::StreamReadable { index, stream_id } => {
                            if let Some(waiters) = pending_stream_accepts.get_mut(&index) {
                                if let Some(waiter) = waiters.pop() {
                                    let _ = waiter.send(Ok(stream_id));
                                } else {
                                    incoming_streams.entry(index)
                                        .or_insert_with(Vec::new)
                                        .push(stream_id);
                                }
                            } else {
                                incoming_streams.entry(index)
                                    .or_insert_with(Vec::new)
                                    .push(stream_id);
                            }
                        }
                        _ => {}
                    }
                }
                
                let now = Instant::now();
                let connection_timeout = std::time::Duration::from_secs(10);
                let mut timed_out = Vec::new();
                for (conn_index, (start_time, _)) in pending_connects.iter() {
                    if now.duration_since(*start_time) > connection_timeout {
                        timed_out.push(*conn_index);
                    }
                }
                for conn_index in timed_out {
                    if let Some((_start, resp)) = pending_connects.remove(&conn_index) {
                        let _ = resp.send(Err(TransportError::ConnectionFailed("Connection timeout".into())));
                        if let Some(conn) = adapter.endpoint.conn_get_mut(conn_index) {
                            let _ = conn.close(true, 0, b"Connection timeout");
                        }
                    }
                }
                
                for ((conn_idx, stream_id), waiters) in stream_readers.drain() {
                    let mut buf = vec![0u8; 65536];
                    match adapter.stream_read(conn_idx, stream_id, &mut buf) {
                        Ok((n, fin)) => {
                            buf.truncate(n);
                            for waiter in waiters {
                                let _ = waiter.send(Ok((buf.clone(), fin)));
                            }
                        }
                        Err(e) => {
                            for waiter in waiters {
                                let _ = waiter.send(Err(TransportError::StreamError(format!("{:?}", e))));
                            }
                        }
                    }
                }
                
                if !adapter.running {
                    break;
                }
            }
            
            info!("TquicRuntime thread stopped");
        });
        
        Ok(TquicRuntime {
            cmd_tx,
            thread_handle: Some(thread_handle),
        })
    }
    
    pub async fn connect(&self, server_name: &str, server_addr: SocketAddr) -> Result<u64, TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.cmd_tx.send(RuntimeCommand::Connect {
            server_name: server_name.to_string(),
            server_addr,
            resp: resp_tx,
        }).map_err(|_| TransportError::ConnectionFailed("Runtime stopped".into()))?;
        
        resp_rx.await
            .map_err(|_| TransportError::ConnectionFailed("Runtime stopped".into()))?
    }
    
    pub async fn accept(&self) -> Result<(u64, SocketAddr), TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.cmd_tx.send(RuntimeCommand::Accept {
            resp: resp_tx,
        }).map_err(|_| TransportError::ConnectionFailed("Runtime stopped".into()))?;
        
        resp_rx.await
            .map_err(|_| TransportError::ConnectionFailed("Runtime stopped".into()))?
    }
    
    pub async fn accept_stream(&self, conn_index: u64) -> Result<u64, TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.cmd_tx.send(RuntimeCommand::AcceptStream {
            conn_index,
            resp: resp_tx,
        }).map_err(|_| TransportError::StreamError("Runtime stopped".into()))?;
        
        resp_rx.await
            .map_err(|_| TransportError::StreamError("Runtime stopped".into()))?
    }
    
    pub async fn open_stream(&self, conn_index: u64) -> Result<u64, TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.cmd_tx.send(RuntimeCommand::OpenStream {
            conn_index,
            resp: resp_tx,
        }).map_err(|_| TransportError::StreamError("Runtime stopped".into()))?;
        
        resp_rx.await
            .map_err(|_| TransportError::StreamError("Runtime stopped".into()))?
    }
    
    pub async fn stream_write(&self, conn_index: u64, stream_id: u64, data: Vec<u8>, fin: bool) -> Result<usize, TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.cmd_tx.send(RuntimeCommand::StreamWrite {
            conn_index,
            stream_id,
            data,
            fin,
            resp: resp_tx,
        }).map_err(|_| TransportError::StreamError("Runtime stopped".into()))?;
        
        resp_rx.await
            .map_err(|_| TransportError::StreamError("Runtime stopped".into()))?
    }
    
    pub async fn stream_read(&self, conn_index: u64, stream_id: u64) -> Result<(Vec<u8>, bool), TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.cmd_tx.send(RuntimeCommand::StreamRead {
            conn_index,
            stream_id,
            resp: resp_tx,
        }).map_err(|_| TransportError::StreamError("Runtime stopped".into()))?;
        
        resp_rx.await
            .map_err(|_| TransportError::StreamError("Runtime stopped".into()))?
    }
    
    pub async fn close_connection(&self, conn_index: u64) -> Result<(), TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.cmd_tx.send(RuntimeCommand::CloseConnection {
            conn_index,
            resp: resp_tx,
        }).map_err(|_| TransportError::ConnectionFailed("Runtime stopped".into()))?;
        
        resp_rx.await
            .map_err(|_| TransportError::ConnectionFailed("Runtime stopped".into()))?
    }
    
    pub fn shutdown(&mut self) {
        let _ = self.cmd_tx.send(RuntimeCommand::Shutdown);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for TquicRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Clone, Debug)]
pub struct TquicClient {
    runtime: Arc<TquicRuntime>,
}

impl TquicClient {
    pub async fn new(_worker_count: Option<usize>) -> Result<Self, TransportError> {
        let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
        
        let runtime = TquicRuntime::new(false, bind_addr, None, None)?;
        
        Ok(TquicClient {
            runtime: Arc::new(runtime),
        })
    }

    pub async fn connect(&self, addr: &str) -> Result<TquicConnection, TransportError> {
        let server_addr: SocketAddr = addr.parse()
            .map_err(|e| TransportError::ConfigError(format!("Invalid address: {}", e)))?;
        
        let server_name = server_addr.ip().to_string();
        
        let conn_index = self.runtime.connect(&server_name, server_addr).await?;
        
        Ok(TquicConnection {
            conn_index,
            runtime: self.runtime.clone(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct TquicServer {
    runtime: Arc<TquicRuntime>,
    bind_addr: SocketAddr,
}

impl TquicServer {
    pub async fn new(
        bind_addr: SocketAddr,
        cert_file: String,
        key_file: String,
        _worker_count: Option<usize>
    ) -> Result<Self, TransportError> {
        let runtime = TquicRuntime::new(
            true,
            bind_addr,
            Some(cert_file),
            Some(key_file)
        )?;
        
        Ok(TquicServer {
            runtime: Arc::new(runtime),
            bind_addr,
        })
    }

    pub async fn listen(&self, _addr: &str) -> Result<TquicListener, TransportError> {
        Ok(TquicListener {
            runtime: self.runtime.clone(),
            bind_addr: self.bind_addr,
        })
    }
}

pub struct TquicListener {
    runtime: Arc<TquicRuntime>,
    bind_addr: SocketAddr,
}

impl TquicListener {
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        Ok(self.bind_addr)
    }

    pub async fn accept(&mut self) -> Result<TquicConnection, TransportError> {
        let (conn_index, _remote) = self.runtime.accept().await?;
        
        Ok(TquicConnection {
            conn_index,
            runtime: self.runtime.clone(),
        })
    }
}

#[async_trait::async_trait]
impl crate::transport::Listener for TquicListener {
    type Connection = TquicConnection;

    async fn accept(
        &mut self,
    ) -> Result<Self::Connection, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let (conn_index, _remote) = self.runtime.accept().await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        
        Ok(TquicConnection {
            conn_index,
            runtime: self.runtime.clone(),
        })
    }
}

pub struct TquicConnection {
    pub conn_index: u64,
    runtime: Arc<TquicRuntime>,
}

impl TquicConnection {
    pub async fn open_stream(&self) -> Result<TquicStream, TransportError> {
        let stream_id = self.runtime.open_stream(self.conn_index).await?;
        
        Ok(TquicStream {
            conn_index: self.conn_index,
            stream_id,
            runtime: self.runtime.clone(),
        })
    }

    pub async fn accept_stream(&self) -> Result<TquicStream, TransportError> {
        let stream_id = self.runtime.accept_stream(self.conn_index).await?;
        
        Ok(TquicStream {
            conn_index: self.conn_index,
            stream_id,
            runtime: self.runtime.clone(),
        })
    }

    pub async fn close(&self) -> Result<(), TransportError> {
        self.runtime.close_connection(self.conn_index).await
    }
}

impl Unpin for TquicConnection {}

impl tokio::io::AsyncRead for TquicConnection {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Pending
    }
}

impl tokio::io::AsyncWrite for TquicConnection {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(Ok(0))
    }

    fn poll_flush(self: std::pin::Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: std::pin::Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

impl crate::transport::Splittable for TquicConnection {
    type Reader = TquicConnectionReader;
    type Writer = TquicConnectionWriter;
    
    fn split(self) -> (Self::Reader, Self::Writer) {
        let reader = TquicConnectionReader {
            conn_index: self.conn_index,
        };
        let writer = TquicConnectionWriter {
            conn_index: self.conn_index,
        };
        (reader, writer)
    }
}

pub struct TquicConnectionReader {
    conn_index: u64,
}

pub struct TquicConnectionWriter {
    conn_index: u64,
}

pub struct TquicStream {
    pub conn_index: u64,
    pub stream_id: u64,
    runtime: Arc<TquicRuntime>,
}

impl TquicStream {
    pub fn new(conn_index: u64, stream_id: u64, runtime: Arc<TquicRuntime>) -> Self {
        Self {
            conn_index,
            stream_id,
            runtime,
        }
    }

    pub async fn send(&mut self, data: &[u8], fin: bool) -> Result<(), TransportError> {
        self.runtime.stream_write(self.conn_index, self.stream_id, data.to_vec(), fin).await?;
        Ok(())
    }

    pub async fn receive(&mut self, _max_length: usize) -> Result<Vec<u8>, TransportError> {
        let (data, _fin) = self.runtime.stream_read(self.conn_index, self.stream_id).await?;
        Ok(data)
    }

    pub async fn close(&self) -> Result<(), TransportError> {
        self.runtime.stream_write(self.conn_index, self.stream_id, vec![], true).await?;
        Ok(())
    }
    
    pub async fn recv(&mut self) -> Result<Vec<u8>, TransportError> {
        let (data, _fin) = self.runtime.stream_read(self.conn_index, self.stream_id).await?;
        Ok(data)
    }
}

impl tokio::io::AsyncRead for TquicStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Pending
    }
}

impl tokio::io::AsyncWrite for TquicStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::task::Poll::Ready(Ok(0))
    }

    fn poll_flush(self: std::pin::Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: std::pin::Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

impl Unpin for TquicStream {}
