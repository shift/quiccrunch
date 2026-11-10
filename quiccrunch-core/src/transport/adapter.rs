use std::collections::HashMap;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use log::{debug, error, info, trace};
use mio::{Interest, Waker};
use tokio::sync::{mpsc, oneshot};

use tquic::{Config, Connection, Endpoint, PacketInfo, TlsConfig, TransportHandler};

use crate::error::TransportError;
use crate::transport::messages::WorkerConfig;

const MAX_BUF_SIZE: usize = 65536;

/// Pre-allocated buffer pool for common message sizes
/// Reduces allocation overhead by reusing buffers
pub struct BufferPool {
    small: Vec<Vec<u8>>,  // 64-256 bytes
    medium: Vec<Vec<u8>>, // 256-1024 bytes
    large: Vec<Vec<u8>>,  // 1024-4096 bytes
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferPool {
    pub fn new() -> Self {
        Self {
            small: Vec::new(),
            medium: Vec::new(),
            large: Vec::new(),
        }
    }

    /// Get a buffer of at least the specified size
    pub fn get(&mut self, min_size: usize) -> Vec<u8> {
        let size_class = match min_size {
            0..=256 => 0,
            257..=1024 => 1,
            1025..=4096 => 2,
            _ => return vec![0u8; min_size],
        };

        let pool = match size_class {
            0 => &mut self.small,
            1 => &mut self.medium,
            _ => &mut self.large,
        };

        if let Some(mut buf) = pool.pop() {
            buf.clear();
            buf.resize(min_size, 0);
            buf
        } else {
            vec![0u8; min_size]
        }
    }

    /// Return a buffer to the pool for reuse
    pub fn put(&mut self, buf: Vec<u8>) {
        let cap = buf.capacity();
        let pool = match cap {
            0..=256 => &mut self.small,
            257..=1024 => &mut self.medium,
            1025..=4096 => &mut self.large,
            _ => return, // Don't pool very large buffers
        };

        // Limit pool size to prevent memory bloat
        if pool.len() < 1024 {
            pool.push(buf);
        }
    }
}

pub struct QuicSocket {
    socket: Arc<Mutex<mio::net::UdpSocket>>,
    local_addr: SocketAddr,
}

impl QuicSocket {
    pub fn new(local: &SocketAddr, registry: &mio::Registry) -> Result<Self, TransportError> {
        // Create socket with SO_REUSEPORT for multi-runtime support
        let socket = socket2::Socket::new(
            socket2::Domain::IPV4,
            socket2::Type::DGRAM,
            Some(socket2::Protocol::UDP),
        )?;

        socket.set_reuse_address(true)?;
        #[cfg(unix)]
        socket.set_reuse_port(true)?;
        socket.set_nonblocking(true)?;

        // Large socket buffers absorb loopback bursts. The kernel doubles these
        // values, so 4 MiB requested is ~8 MiB effective on Linux.
        const SOCKET_BUFFER_BYTES: usize = 4 * 1024 * 1024;
        if let Err(e) = socket.set_recv_buffer_size(SOCKET_BUFFER_BYTES) {
            debug!("SO_RCVBUF tuning failed: {:?}", e);
        }
        if let Err(e) = socket.set_send_buffer_size(SOCKET_BUFFER_BYTES) {
            debug!("SO_SNDBUF tuning failed: {:?}", e);
        }

        // Bind using socket2's bind method
        socket.bind(&socket2::SockAddr::from(*local))?;

        let std_socket: std::net::UdpSocket = socket.into();
        let local_addr = std_socket.local_addr()?;

        // Convert to mio socket
        let mut socket = mio::net::UdpSocket::from_std(std_socket);

        registry.register(&mut socket, mio::Token(0), Interest::READABLE)?;

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
        self.socket.lock().unwrap().send_to(buf, dst)
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
            trace!("written {} bytes", pkt.len());
            count += 1;
        }
        Ok(count)
    }
}

#[derive(Default)]
struct ConnectionState {
    established: bool,
}

/// Per-stream receive state.
///
/// tQUIC invokes `on_stream_readable` exactly when new bytes arrive. If the
/// runtime only reacts to that event by waking a *later* reader, any data that
/// arrives while no reader is parked is never re-advertised. That is the root
/// cause of the benchmark's lost completions and 5 s receive timeouts. We
/// therefore drain the stream into this buffer inside the callback and either
/// hand it to a parked reader immediately or keep it until one arrives.
type StreamWaiter = oneshot::Sender<Result<(Vec<u8>, bool), TransportError>>;

#[derive(Default)]
struct StreamBuf {
    data: Vec<u8>,
    fin: bool,
    closed: bool,
    errored: bool,
    waiters: Vec<StreamWaiter>,
}

impl StreamBuf {
    /// Deliver buffered bytes to parked readers. Reads are stream-ordered, so
    /// each reader receives a distinct byte range. Once the stream is terminal,
    /// any remaining readers observe EOF instead of hanging.
    fn deliver(&mut self) {
        loop {
            let has_payload = !self.data.is_empty();
            let terminal = self.fin || self.closed;
            if !has_payload && !terminal {
                return;
            }
            let waiter = match self.waiters.pop() {
                Some(w) => w,
                None => return,
            };
            let data = std::mem::take(&mut self.data);
            let _ = waiter.send(Ok((data, terminal)));
            if !terminal {
                return;
            }
        }
    }

    fn fail(&mut self, msg: &str) {
        debug!("stream read failed: {}", msg);
        self.errored = true;
        self.closed = true;
        for waiter in self.waiters.drain(..) {
            let _ = waiter.send(Err(TransportError::StreamError(msg.to_string())));
        }
    }
}

fn read_stream_available(conn: &mut tquic::Connection, stream_id: u64, buf: &mut StreamBuf) {
    let mut scratch = [0u8; 16 * 1024];
    loop {
        if buf.closed || buf.errored || buf.fin {
            break;
        }
        match conn.stream_read(stream_id, &mut scratch) {
            Ok((0, fin)) => {
                if fin {
                    buf.fin = true;
                }
                break;
            }
            Ok((n, fin)) => {
                buf.data.extend_from_slice(&scratch[..n]);
                if fin {
                    buf.fin = true;
                    break;
                }
            }
            Err(tquic::Error::Done) => break,
            Err(e) => {
                // StreamStateError is expected while a peer-initiated stream is
                // still being opened; anything else is a real read failure.
                if matches!(e, tquic::Error::StreamStateError) {
                    break;
                }
                buf.fail(&format!("stream_read failed: {:?}", e));
                break;
            }
        }
    }
}

struct TquicHandler {
    connections: Arc<Mutex<HashMap<u64, ConnectionState>>>,
    stream_bufs: Arc<Mutex<HashMap<(u64, u64), StreamBuf>>>,
    event_tx: mpsc::UnboundedSender<TquicEvent>,
}

impl TquicHandler {
    fn new(
        event_tx: mpsc::UnboundedSender<TquicEvent>,
        stream_bufs: Arc<Mutex<HashMap<(u64, u64), StreamBuf>>>,
    ) -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            stream_bufs,
            event_tx,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum TquicEvent {
    ConnectionEstablished { index: u64, remote: SocketAddr },
    ConnectionClosed { index: u64 },
    StreamCreated { index: u64, stream_id: u64 },
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

        let remote = conn
            .paths_iter()
            .next()
            .map(|p| p.remote)
            .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 0)));

        let _ = self
            .event_tx
            .send(TquicEvent::ConnectionEstablished { index, remote });
    }

    fn on_conn_closed(&mut self, conn: &mut Connection) {
        debug!(
            "{} connection closed (index {:?}) local_error={:?} peer_error={:?}",
            conn.trace_id(),
            conn.index(),
            conn.local_error(),
            conn.peer_error()
        );
        let index = conn.index().unwrap();

        let mut conns = self.connections.lock().unwrap();
        conns.remove(&index);
        drop(conns);

        // Fail or finish every parked reader on this connection so tasks never
        // block forever after a peer goes away.
        {
            let mut bufs = self.stream_bufs.lock().unwrap();
            for ((conn_index, _), buf) in bufs.iter_mut() {
                if *conn_index == index {
                    buf.closed = true;
                    buf.deliver();
                }
            }
        }

        let _ = self.event_tx.send(TquicEvent::ConnectionClosed { index });
    }

    fn on_stream_created(&mut self, conn: &mut Connection, stream_id: u64) {
        debug!("{} stream {} created", conn.trace_id(), stream_id);
        let index = conn.index().unwrap();
        let _ = self
            .event_tx
            .send(TquicEvent::StreamCreated { index, stream_id });
    }

    fn on_stream_readable(&mut self, conn: &mut Connection, stream_id: u64) {
        let index = match conn.index() {
            Some(index) => index,
            None => return,
        };

        // Drain inside the callback. Blocking here is correct: the endpoint
        // thread owns the connection and must consume all readable bytes before
        // returning, otherwise tQUIC will not re-advertise the stream.
        let mut bufs = self.stream_bufs.lock().unwrap();
        let buf = bufs.entry((index, stream_id)).or_default();
        read_stream_available(conn, stream_id, buf);
        buf.deliver();

        // Only notify the runtime when a reader still needs to be woken or the
        // stream reached a terminal state.
        if !buf.waiters.is_empty() || buf.fin || buf.closed {
            let _ = self
                .event_tx
                .send(TquicEvent::StreamReadable { index, stream_id });
        }
    }

    fn on_stream_writable(&mut self, conn: &mut Connection, stream_id: u64) {
        let index = conn.index().unwrap();
        let _ = self
            .event_tx
            .send(TquicEvent::StreamWritable { index, stream_id });
    }

    fn on_stream_closed(&mut self, conn: &mut Connection, stream_id: u64) {
        debug!("{} stream {} closed", conn.trace_id(), stream_id);
        let index = match conn.index() {
            Some(index) => index,
            None => return,
        };
        {
            let mut bufs = self.stream_bufs.lock().unwrap();
            let buf = bufs.entry((index, stream_id)).or_default();
            // One last drain attempt in case data and FIN arrived together and
            // readable was not delivered separately.
            read_stream_available(conn, stream_id, buf);
            debug!(
                "stream {}/{} closed: buffered={} fin={} closed={}",
                index,
                stream_id,
                buf.data.len(),
                buf.fin,
                buf.closed
            );
            buf.closed = true;
            buf.deliver();
        }
        let _ = self
            .event_tx
            .send(TquicEvent::StreamClosed { index, stream_id });
    }

    fn on_new_token(&mut self, _conn: &mut Connection, _token: Vec<u8>) {}
}

pub struct TquicAdapter {
    endpoint: Endpoint,
    poll: mio::Poll,
    sock: Rc<QuicSocket>,
    recv_buf: Vec<u8>,
    event_rx: mpsc::UnboundedReceiver<TquicEvent>,
    stream_bufs: Arc<Mutex<HashMap<(u64, u64), StreamBuf>>>,
    waker: Arc<Waker>,
    running: bool,
    poll_events: mio::Events,
}

/// Transport tuning knobs that are orthogonal to `WorkerConfig`.
///
/// Defaults preserve the historical behaviour except for the flow-control
/// windows, which were smaller than the benchmark's advertised pipeline and
/// caused silent stalls at depth > 2048.
#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    /// Worker identity, used for CPU pinning and diagnostics.
    pub worker_id: usize,
    /// BBR pacing. Great on real networks, but it quantises sends to the pacer
    /// granularity (1 ms) and dominates loopback tail latency. Disable for
    /// same-host benchmarks.
    pub enable_pacing: bool,
    /// Pin the runtime thread to `worker_id % num_cpus`.
    pub pin_to_cpu: bool,
    pub initial_congestion_window: u64,
    pub max_connection_window: u64,
    pub max_stream_window: u64,
    pub initial_max_data: u64,
    pub initial_max_stream_data: u64,
    /// ACK every N ack-eliciting packets; 1 minimises RTT at the cost of ACKs.
    pub ack_eliciting_threshold: u64,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            worker_id: 0,
            enable_pacing: true,
            pin_to_cpu: false,
            initial_congestion_window: 32,
            max_connection_window: 64 * 1024 * 1024,
            max_stream_window: 16 * 1024 * 1024,
            initial_max_data: 32 * 1024 * 1024,
            initial_max_stream_data: 8 * 1024 * 1024,
            ack_eliciting_threshold: 2,
        }
    }
}

impl RuntimeOptions {
    /// Options tuned for same-host throughput benchmarking: pacing off, deep
    /// flow-control windows and a large initial congestion window.
    pub fn benchmark(worker_id: usize) -> Self {
        Self {
            worker_id,
            enable_pacing: false,
            pin_to_cpu: true,
            initial_congestion_window: 128,
            max_connection_window: 128 * 1024 * 1024,
            max_stream_window: 32 * 1024 * 1024,
            initial_max_data: 64 * 1024 * 1024,
            initial_max_stream_data: 16 * 1024 * 1024,
            ack_eliciting_threshold: 1,
        }
    }

    /// Benchmark client tuning. The client is left unpinned so a runner can
    /// place it on the cores not used by the server workers.
    pub fn benchmark_client() -> Self {
        Self {
            pin_to_cpu: false,
            ..Self::benchmark(0)
        }
    }
}

impl TquicAdapter {
    pub fn new(config: WorkerConfig) -> Result<Self, TransportError> {
        Self::with_options(config, RuntimeOptions::default())
    }

    pub fn with_options(
        config: WorkerConfig,
        options: RuntimeOptions,
    ) -> Result<Self, TransportError> {
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
        quic_config.set_max_connection_window(options.max_connection_window);
        quic_config.set_max_stream_window(options.max_stream_window);
        quic_config.set_initial_max_data(options.initial_max_data);
        quic_config.set_initial_max_stream_data_bidi_local(options.initial_max_stream_data);
        quic_config.set_initial_max_stream_data_bidi_remote(options.initial_max_stream_data);
        quic_config.set_initial_congestion_window(options.initial_congestion_window);
        quic_config.set_min_congestion_window(4);
        quic_config.enable_pacing(options.enable_pacing);
        quic_config.set_ack_eliciting_threshold(options.ack_eliciting_threshold);
        quic_config.set_send_batch_size(64);

        if config.is_server {
            let cert_file = config
                .cert_file
                .as_ref()
                .ok_or_else(|| TransportError::ConfigError("Server requires cert_file".into()))?;
            let key_file = config
                .key_file
                .as_ref()
                .ok_or_else(|| TransportError::ConfigError("Server requires key_file".into()))?;

            let tls_config = TlsConfig::new_server_config(
                cert_file,
                key_file,
                vec![b"h3".to_vec(), b"http/0.9".to_vec()],
                true,
            )
            .map_err(|e| TransportError::ConfigError(format!("TLS config failed: {:?}", e)))?;

            quic_config.set_tls_config(tls_config);
        } else {
            let tls_config =
                TlsConfig::new_client_config(vec![b"h3".to_vec(), b"http/0.9".to_vec()], false)
                    .map_err(|e| {
                        TransportError::ConfigError(format!("TLS config failed: {:?}", e))
                    })?;

            quic_config.set_tls_config(tls_config);
        }

        let poll = mio::Poll::new()?;
        let registry = poll.registry();
        // Token 1 is reserved for the mio waker that breaks the poll when a
        // command arrives. The socket itself uses token 0.
        let waker = Arc::new(Waker::new(registry, mio::Token(1))?);

        let bind_addr = config.bind_addr;

        let sock = Rc::new(QuicSocket::new(&bind_addr, registry)?);

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let stream_bufs: Arc<Mutex<HashMap<(u64, u64), StreamBuf>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let handlers = TquicHandler::new(event_tx, stream_bufs.clone());

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
            stream_bufs,
            waker,
            running: false,
            poll_events: mio::Events::with_capacity(1024),
        })
    }

    pub fn waker(&self) -> Arc<Waker> {
        self.waker.clone()
    }

    fn recv_packets(&mut self) {
        loop {
            match self
                .sock
                .socket
                .lock()
                .unwrap()
                .recv_from(&mut self.recv_buf)
            {
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
            self.poll.poll(&mut events, timeout)?;

            for event in events.iter() {
                if event.token() == mio::Token(0) && event.is_readable() {
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
            let (len, local, remote) = match self.sock.recv_from(&mut self.recv_buf, event.token())
            {
                Ok(v) => v,
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        debug!("socket recv would block");
                        break;
                    }
                    return Err(e.into());
                }
            };
            trace!("socket recv {} bytes from {:?}", len, remote);

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

    pub fn connect(
        &mut self,
        remote_addr: SocketAddr,
        server_name: &str,
    ) -> Result<u64, TransportError> {
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

    pub fn stream_read(
        &mut self,
        conn_index: u64,
        stream_id: u64,
        buf: &mut [u8],
    ) -> Result<(usize, bool), TransportError> {
        let conn = self
            .endpoint
            .conn_get_mut(conn_index)
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
        data: bytes::Bytes,
        fin: bool,
        resp: oneshot::Sender<Result<usize, TransportError>>,
    },
    StreamWriteBatch {
        conn_index: u64,
        stream_id: u64,
        messages: Vec<(bytes::Bytes, bool)>,
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
    waker: Arc<Waker>,
    wake_pending: Arc<std::sync::atomic::AtomicBool>,
}

impl std::fmt::Debug for TquicRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TquicRuntime").finish()
    }
}

#[cfg(target_os = "linux")]
fn pin_current_thread_to_cpu(cpu: usize) {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        libc::CPU_SET(cpu % num_cpus::get(), &mut set);
        let rc = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
        if rc != 0 {
            debug!(
                "sched_setaffinity(cpu={}) failed: {:?}",
                cpu,
                std::io::Error::last_os_error()
            );
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn pin_current_thread_to_cpu(_cpu: usize) {}

/// Write an entire `Bytes` buffer to a stream, respecting tQUIC's partial write
/// semantics. FIN is never split across writes because that would close the
/// send half before the remaining bytes are queued.
fn write_stream_all(
    conn: &mut tquic::Connection,
    stream_id: u64,
    data: bytes::Bytes,
    fin: bool,
) -> Result<usize, TransportError> {
    if fin {
        trace!(
            "write_stream_all: FIN on stream {} ({} bytes)",
            stream_id,
            data.len()
        );
    }
    let total = data.len();
    if total == 0 {
        conn.stream_write(stream_id, data, fin)
            .map_err(|e| TransportError::StreamError(format!("Stream write failed: {:?}", e)))?;
        return Ok(0);
    }
    if fin {
        let written = conn
            .stream_write(stream_id, data, true)
            .map_err(|e| TransportError::StreamError(format!("Stream write failed: {:?}", e)))?;
        if written != total {
            return Err(TransportError::StreamError(format!(
                "Partial FIN write: {}/{}",
                written, total
            )));
        }
        return Ok(written);
    }

    let mut off = 0usize;
    while off < total {
        let written = conn
            .stream_write(stream_id, data.slice(off..), false)
            .map_err(|e| TransportError::StreamError(format!("Stream write failed: {:?}", e)))?;
        if written == 0 {
            return Err(TransportError::StreamError(
                "Stream write made no progress (flow control blocked)".into(),
            ));
        }
        off += written;
    }
    Ok(off)
}

impl TquicRuntime {
    pub fn new(
        is_server: bool,
        bind_addr: SocketAddr,
        cert_file: Option<String>,
        key_file: Option<String>,
    ) -> Result<Self, TransportError> {
        Self::with_options(
            is_server,
            bind_addr,
            cert_file,
            key_file,
            RuntimeOptions::default(),
        )
    }

    pub fn with_options(
        is_server: bool,
        bind_addr: SocketAddr,
        cert_file: Option<String>,
        key_file: Option<String>,
        options: RuntimeOptions,
    ) -> Result<Self, TransportError> {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<RuntimeCommand>();
        let wake_pending = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let wake_pending_thread = wake_pending.clone();
        let (waker_tx, waker_rx) = std::sync::mpsc::sync_channel::<Arc<Waker>>(1);

        let thread_handle = thread::spawn(move || {
            pin_current_thread_to_cpu(options.worker_id);

            let worker_id = options.worker_id;
            let config = WorkerConfig {
                worker_id,
                bind_addr,
                is_server,
                cert_file,
                key_file,
                max_idle_timeout: std::time::Duration::from_secs(30),
                max_udp_payload_size: 65527,
                enable_early_data: false,
                application_protocols: vec![b"h3".to_vec(), b"http/0.9".to_vec()],
                verify_cert: true,
                ca_cert_file: None,
            };

            let mut adapter =
                TquicAdapter::with_options(config, options).expect("Failed to create TquicAdapter");
            let _ = waker_tx.send(adapter.waker());

            let stream_bufs = adapter.stream_bufs.clone();
            let mut pending_accepts: Vec<
                oneshot::Sender<Result<(u64, SocketAddr), TransportError>>,
            > = Vec::new();
            let mut incoming_connections: Vec<(u64, SocketAddr)> = Vec::new();
            let mut pending_stream_accepts: HashMap<
                u64,
                Vec<oneshot::Sender<Result<u64, TransportError>>>,
            > = HashMap::new();
            let mut incoming_streams: HashMap<u64, Vec<u64>> = HashMap::new();
            let mut next_stream_ids: HashMap<u64, u64> = HashMap::new();
            let mut pending_connects: HashMap<
                u64,
                (Instant, oneshot::Sender<Result<u64, TransportError>>),
            > = HashMap::new();

            info!(
                "TquicRuntime thread started on {} (worker {})",
                bind_addr, worker_id
            );

            adapter.running = true;

            loop {
                // Order matters: clear the wake flag *before* draining commands
                // so a command enqueued during the drain always causes a wake.
                wake_pending_thread.store(false, std::sync::atomic::Ordering::Release);

                while let Ok(cmd) = cmd_rx.try_recv() {
                    match cmd {
                        RuntimeCommand::Connect {
                            server_name,
                            server_addr,
                            resp,
                        } => match adapter.connect(server_addr, &server_name) {
                            Ok(conn_index) => {
                                pending_connects.insert(conn_index, (Instant::now(), resp));
                            }
                            Err(e) => {
                                let _ = resp.send(Err(e));
                            }
                        },
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
                                    pending_stream_accepts
                                        .entry(conn_index)
                                        .or_default()
                                        .push(resp);
                                }
                            } else {
                                pending_stream_accepts
                                    .entry(conn_index)
                                    .or_default()
                                    .push(resp);
                            }
                        }
                        RuntimeCommand::OpenStream { conn_index, resp } => {
                            let stream_id = next_stream_ids.entry(conn_index).or_insert(0);
                            let conn = adapter.endpoint.conn_get_mut(conn_index);
                            if conn.is_none() {
                                let _ = resp.send(Err(TransportError::StreamError(
                                    "Connection not found".into(),
                                )));
                                continue;
                            }
                            let conn = conn.unwrap();
                            match conn.stream_new(*stream_id, 0, true) {
                                Ok(()) => {
                                    let current_id = *stream_id;
                                    *stream_id += 4;
                                    let _ = resp.send(Ok(current_id));
                                }
                                Err(e) => {
                                    let _ = resp.send(Err(TransportError::StreamError(format!(
                                        "Stream creation failed: {:?}",
                                        e
                                    ))));
                                }
                            }
                        }
                        RuntimeCommand::StreamWrite {
                            conn_index,
                            stream_id,
                            data,
                            fin,
                            resp,
                        } => {
                            let result = adapter
                                .endpoint
                                .conn_get_mut(conn_index)
                                .ok_or_else(|| {
                                    TransportError::ConnectionFailed("Connection not found".into())
                                })
                                .and_then(|conn| write_stream_all(conn, stream_id, data, fin));
                            let _ = resp.send(result);
                        }
                        RuntimeCommand::StreamWriteBatch {
                            conn_index,
                            stream_id,
                            messages,
                            resp,
                        } => {
                            let result = adapter
                                .endpoint
                                .conn_get_mut(conn_index)
                                .ok_or_else(|| {
                                    TransportError::ConnectionFailed("Connection not found".into())
                                })
                                .and_then(|conn| {
                                    let mut total_written = 0;
                                    for (data, fin) in messages {
                                        total_written +=
                                            write_stream_all(conn, stream_id, data, fin)?;
                                    }
                                    Ok(total_written)
                                });
                            let _ = resp.send(result);
                        }
                        RuntimeCommand::StreamRead {
                            conn_index,
                            stream_id,
                            resp,
                        } => {
                            // Data is drained into `stream_bufs` by the transport
                            // handler, so a read simply parks or consumes. No
                            // extra round-trip to the connection is needed.
                            let mut bufs = stream_bufs.lock().unwrap();
                            let buf = bufs.entry((conn_index, stream_id)).or_default();
                            buf.waiters.push(resp);
                            buf.deliver();
                        }
                        RuntimeCommand::CloseConnection { conn_index, resp } => {
                            if let Some(conn) = adapter.endpoint.conn_get_mut(conn_index) {
                                let _ = conn.close(true, 0, b"Application close");
                                let _ = resp.send(Ok(()));
                            } else {
                                let _ = resp.send(Err(TransportError::ConnectionFailed(
                                    "Connection not found".into(),
                                )));
                            }
                        }
                        RuntimeCommand::Shutdown => {
                            adapter.close();
                            return;
                        }
                    }
                }

                if let Err(e) = adapter.endpoint.process_connections() {
                    error!("process connections error: {:?}", e);
                }
                adapter.endpoint.on_timeout(Instant::now());

                // Drain every event, not an arbitrary batch of 64. A backlog
                // here directly becomes receiver latency.
                loop {
                    match adapter.event_rx.try_recv() {
                        Ok(TquicEvent::ConnectionEstablished { index, remote }) => {
                            if let Some((_start, resp)) = pending_connects.remove(&index) {
                                let _ = resp.send(Ok(index));
                            } else if let Some(waiter) = pending_accepts.pop() {
                                let _ = waiter.send(Ok((index, remote)));
                            } else {
                                incoming_connections.push((index, remote));
                            }
                        }
                        Ok(TquicEvent::StreamCreated { index, stream_id }) => {
                            if let Some(waiters) = pending_stream_accepts.get_mut(&index) {
                                if let Some(waiter) = waiters.pop() {
                                    let _ = waiter.send(Ok(stream_id));
                                } else {
                                    incoming_streams.entry(index).or_default().push(stream_id);
                                }
                            } else {
                                incoming_streams.entry(index).or_default().push(stream_id);
                            }
                        }
                        Ok(TquicEvent::StreamReadable { index, stream_id })
                        | Ok(TquicEvent::StreamClosed { index, stream_id }) => {
                            let mut bufs = stream_bufs.lock().unwrap();
                            if let Some(buf) = bufs.get_mut(&(index, stream_id)) {
                                buf.deliver();
                            }
                        }
                        Ok(TquicEvent::ConnectionClosed { index }) => {
                            debug!("runtime: connection {} closed event", index);
                            let mut bufs = stream_bufs.lock().unwrap();
                            for ((conn_index, _), buf) in bufs.iter_mut() {
                                if *conn_index == index {
                                    buf.closed = true;
                                    buf.deliver();
                                }
                            }
                            bufs.retain(|(conn_index, _), _| *conn_index != index);
                            incoming_streams.remove(&index);
                            next_stream_ids.remove(&index);
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }

                let now = Instant::now();
                let connection_timeout = Duration::from_secs(10);
                let mut timed_out = Vec::new();
                for (conn_index, (start_time, _)) in pending_connects.iter() {
                    if now.duration_since(*start_time) > connection_timeout {
                        timed_out.push(*conn_index);
                    }
                }
                for conn_index in timed_out {
                    if let Some((_start, resp)) = pending_connects.remove(&conn_index) {
                        let _ = resp.send(Err(TransportError::ConnectionFailed(
                            "Connection timeout".into(),
                        )));
                        if let Some(conn) = adapter.endpoint.conn_get_mut(conn_index) {
                            let _ = conn.close(true, 0, b"Connection timeout");
                        }
                    }
                }

                if !adapter.running {
                    break;
                }

                // Sleep in the kernel until a packet arrives or a command
                // wakes us. This replaces the previous busy-poll loop plus a
                // 1 ms condvar nap, which alone imposed a ~1-2 ms latency floor.
                let poll_timeout = adapter
                    .endpoint
                    .timeout()
                    .map(|t| t.min(Duration::from_millis(10)))
                    .unwrap_or(Duration::from_millis(10));
                let poll_result = adapter
                    .poll
                    .poll(&mut adapter.poll_events, Some(poll_timeout));

                match poll_result {
                    Ok(()) => {
                        let mut readable = false;
                        for event in adapter.poll_events.iter() {
                            if event.token() == mio::Token(0) && event.is_readable() {
                                readable = true;
                            }
                            // Token 1 is the waker: no payload, loop restarts.
                        }
                        if readable {
                            adapter.recv_packets();
                        }
                    }
                    Err(e) => {
                        error!("Poll error: {:?}", e);
                    }
                }
            }
        });

        let waker = waker_rx.recv().map_err(|_| {
            TransportError::ConnectionFailed("Runtime thread failed to start".into())
        })?;

        Ok(TquicRuntime {
            cmd_tx,
            thread_handle: Some(thread_handle),
            waker,
            wake_pending,
        })
    }

    /// Enqueue a command and wake the runtime thread at most once per batch.
    fn send_command(&self, cmd: RuntimeCommand) -> Result<(), TransportError> {
        self.cmd_tx
            .send(cmd)
            .map_err(|_| TransportError::ConnectionFailed("Runtime stopped".into()))?;
        if !self
            .wake_pending
            .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            let _ = self.waker.wake();
        }
        Ok(())
    }

    pub async fn connect(
        &self,
        server_name: &str,
        server_addr: SocketAddr,
    ) -> Result<u64, TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_command(RuntimeCommand::Connect {
            server_name: server_name.to_string(),
            server_addr,
            resp: resp_tx,
        })?;

        resp_rx
            .await
            .map_err(|_| TransportError::ConnectionFailed("Runtime stopped".into()))?
    }

    pub async fn accept(&self) -> Result<(u64, SocketAddr), TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_command(RuntimeCommand::Accept { resp: resp_tx })?;

        resp_rx
            .await
            .map_err(|_| TransportError::ConnectionFailed("Runtime stopped".into()))?
    }

    pub async fn accept_stream(&self, conn_index: u64) -> Result<u64, TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_command(RuntimeCommand::AcceptStream {
            conn_index,
            resp: resp_tx,
        })?;

        resp_rx
            .await
            .map_err(|_| TransportError::StreamError("Runtime stopped".into()))?
    }

    pub async fn open_stream(&self, conn_index: u64) -> Result<u64, TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_command(RuntimeCommand::OpenStream {
            conn_index,
            resp: resp_tx,
        })?;

        resp_rx
            .await
            .map_err(|_| TransportError::StreamError("Runtime stopped".into()))?
    }

    pub async fn stream_write(
        &self,
        conn_index: u64,
        stream_id: u64,
        data: bytes::Bytes,
        fin: bool,
    ) -> Result<usize, TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_command(RuntimeCommand::StreamWrite {
            conn_index,
            stream_id,
            data,
            fin,
            resp: resp_tx,
        })?;

        resp_rx
            .await
            .map_err(|_| TransportError::StreamError("Runtime stopped".into()))?
    }

    /// Batch write multiple messages in a single channel command
    /// This reduces context switches from 3N to 3 for N messages
    pub async fn stream_write_batch(
        &self,
        conn_index: u64,
        stream_id: u64,
        messages: Vec<(bytes::Bytes, bool)>,
    ) -> Result<usize, TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_command(RuntimeCommand::StreamWriteBatch {
            conn_index,
            stream_id,
            messages,
            resp: resp_tx,
        })?;

        resp_rx
            .await
            .map_err(|_| TransportError::StreamError("Runtime stopped".into()))?
    }

    pub async fn stream_read(
        &self,
        conn_index: u64,
        stream_id: u64,
    ) -> Result<(Vec<u8>, bool), TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_command(RuntimeCommand::StreamRead {
            conn_index,
            stream_id,
            resp: resp_tx,
        })?;

        resp_rx
            .await
            .map_err(|_| TransportError::StreamError("Runtime stopped".into()))?
    }

    pub async fn close_connection(&self, conn_index: u64) -> Result<(), TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_command(RuntimeCommand::CloseConnection {
            conn_index,
            resp: resp_tx,
        })?;

        resp_rx
            .await
            .map_err(|_| TransportError::ConnectionFailed("Runtime stopped".into()))?
    }

    pub fn shutdown(&mut self) {
        let _ = self.send_command(RuntimeCommand::Shutdown);
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

    /// Create a client with explicit transport tuning. Used by benchmarks to
    /// disable pacing and raise flow-control windows.
    pub async fn with_options(options: RuntimeOptions) -> Result<Self, TransportError> {
        let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
        let runtime = TquicRuntime::with_options(false, bind_addr, None, None, options)?;
        Ok(TquicClient {
            runtime: Arc::new(runtime),
        })
    }

    pub async fn connect(&self, addr: &str) -> Result<TquicConnection, TransportError> {
        let server_addr: SocketAddr = addr
            .parse()
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
        _worker_count: Option<usize>,
    ) -> Result<Self, TransportError> {
        let runtime = TquicRuntime::new(true, bind_addr, Some(cert_file), Some(key_file))?;

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
    pub runtime: Arc<TquicRuntime>,
    pub bind_addr: SocketAddr,
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
        let (conn_index, _remote) = self
            .runtime
            .accept()
            .await
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

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
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

#[allow(dead_code)]
pub struct TquicConnectionReader {
    conn_index: u64,
}

#[allow(dead_code)]
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

    pub async fn send(&self, data: &[u8], fin: bool) -> Result<(), TransportError> {
        // Convert &[u8] to Bytes (this copies, but is necessary for backward compatibility)
        // For zero-copy, use send_bytes() with bytes::Bytes directly
        self.runtime
            .stream_write(
                self.conn_index,
                self.stream_id,
                bytes::Bytes::copy_from_slice(data),
                fin,
            )
            .await?;
        Ok(())
    }

    /// Send data with zero-copy using bytes::Bytes
    /// This avoids the copy that happens with send() when you already have Bytes
    pub async fn send_bytes(&self, data: bytes::Bytes, fin: bool) -> Result<(), TransportError> {
        self.runtime
            .stream_write(self.conn_index, self.stream_id, data, fin)
            .await?;
        Ok(())
    }

    /// Batch send multiple messages in a single channel command
    /// This reduces context switches from 3N to 3 for N messages
    pub async fn send_batch(
        &self,
        messages: Vec<(bytes::Bytes, bool)>,
    ) -> Result<(), TransportError> {
        self.runtime
            .stream_write_batch(self.conn_index, self.stream_id, messages)
            .await?;
        Ok(())
    }

    pub async fn receive(&self, _max_length: usize) -> Result<Vec<u8>, TransportError> {
        let (data, _fin) = self
            .runtime
            .stream_read(self.conn_index, self.stream_id)
            .await?;
        Ok(data)
    }

    pub async fn close(&self) -> Result<(), TransportError> {
        self.runtime
            .stream_write(self.conn_index, self.stream_id, bytes::Bytes::new(), true)
            .await?;
        Ok(())
    }

    pub async fn recv(&self) -> Result<Vec<u8>, TransportError> {
        let (data, _fin) = self
            .runtime
            .stream_read(self.conn_index, self.stream_id)
            .await?;
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

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

impl Clone for TquicStream {
    fn clone(&self) -> Self {
        Self {
            conn_index: self.conn_index,
            stream_id: self.stream_id,
            runtime: self.runtime.clone(),
        }
    }
}

impl Unpin for TquicStream {}
