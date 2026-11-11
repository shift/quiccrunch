use std::collections::HashMap;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use log::{debug, error, info, trace, warn};
use mio::{Interest, Waker};
use tokio::sync::{mpsc, oneshot};

use tquic::{Config, Connection, Endpoint, PacketInfo, TlsConfig, TransportHandler};

use crate::error::TransportError;
use crate::identity::{peer_identity_from_chain, PeerIdentity};
use crate::transport::messages::WorkerConfig;

const MAX_BUF_SIZE: usize = 65536;

/// Load PEM trust anchors into a TLS config, refusing a path that does not
/// exist. tQUIC's `set_ca_certs` treats a non-file path as a *directory* and
/// BoringSSL accepts a missing directory with an empty trust store, which would
/// silently turn verification into a failure at handshake time rather than a
/// configuration error. Fail at startup instead.
fn load_trust_anchors(
    tls_config: &mut TlsConfig,
    path: &str,
    description: &str,
) -> Result<(), TransportError> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Err(TransportError::ConfigError(format!(
            "{} does not exist: {}",
            description, path
        )));
    }
    tls_config.set_ca_certs(path).map_err(|e| {
        TransportError::ConfigError(format!(
            "Failed to load {} from {}: {:?}",
            description, path, e
        ))
    })
}

/// Per-worker counters. Cheap enough to update on every batch (not every
/// packet) and intended to make saturation, skew and flow-control starvation
/// visible instead of inferring them from latency alone.
#[derive(Debug, Default)]
pub struct RuntimeMetrics {
    pub commands: AtomicU64,
    pub write_batches: AtomicU64,
    pub writes: AtomicU64,
    pub bytes_written: AtomicU64,
    pub write_errors: AtomicU64,
    pub partial_writes: AtomicU64,
    pub flow_blocked: AtomicU64,
    pub packets_sent: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub packets_received: AtomicU64,
    pub bytes_received: AtomicU64,
    pub poll_calls: AtomicU64,
    pub max_command_backlog: AtomicU64,
    /// Connections where the client certificate chain and SPIFFE SAN were
    /// verified successfully.
    pub mtls_handshakes: AtomicU64,
    /// Connections rejected after (or during) the handshake because the client
    /// certificate was missing, untrusted, expired, or carried an invalid or
    /// out-of-domain SPIFFE SAN.
    pub mtls_rejections: AtomicU64,
    /// True when this runtime was configured as a server with a client CA and
    /// therefore requires/verifies client certificates.
    pub mtls_enabled: std::sync::atomic::AtomicBool,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RuntimeMetricsSnapshot {
    pub commands: u64,
    pub write_batches: u64,
    pub writes: u64,
    pub bytes_written: u64,
    pub write_errors: u64,
    pub partial_writes: u64,
    pub flow_blocked: u64,
    pub packets_sent: u64,
    pub bytes_sent: u64,
    pub packets_received: u64,
    pub bytes_received: u64,
    pub poll_calls: u64,
    pub max_command_backlog: u64,
    pub mtls_handshakes: u64,
    pub mtls_rejections: u64,
    pub mtls_enabled: bool,
}

impl RuntimeMetrics {
    pub fn snapshot(&self) -> RuntimeMetricsSnapshot {
        RuntimeMetricsSnapshot {
            commands: self.commands.load(Ordering::Relaxed),
            write_batches: self.write_batches.load(Ordering::Relaxed),
            writes: self.writes.load(Ordering::Relaxed),
            bytes_written: self.bytes_written.load(Ordering::Relaxed),
            write_errors: self.write_errors.load(Ordering::Relaxed),
            partial_writes: self.partial_writes.load(Ordering::Relaxed),
            flow_blocked: self.flow_blocked.load(Ordering::Relaxed),
            packets_sent: self.packets_sent.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            packets_received: self.packets_received.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            poll_calls: self.poll_calls.load(Ordering::Relaxed),
            max_command_backlog: self.max_command_backlog.load(Ordering::Relaxed),
            mtls_handshakes: self.mtls_handshakes.load(Ordering::Relaxed),
            mtls_rejections: self.mtls_rejections.load(Ordering::Relaxed),
            mtls_enabled: self.mtls_enabled.load(Ordering::Relaxed),
        }
    }
}

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
    metrics: Arc<RuntimeMetrics>,
}

impl QuicSocket {
    pub fn new(local: &SocketAddr, registry: &mio::Registry) -> Result<Self, TransportError> {
        Self::with_metrics(local, registry, Arc::new(RuntimeMetrics::default()))
    }

    pub fn with_metrics(
        local: &SocketAddr,
        registry: &mio::Registry,
        metrics: Arc<RuntimeMetrics>,
    ) -> Result<Self, TransportError> {
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
            metrics,
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
        self.metrics
            .packets_sent
            .fetch_add(count as u64, Ordering::Relaxed);
        self.metrics.bytes_sent.fetch_add(
            pkts.iter().map(|(p, _)| p.len() as u64).sum(),
            Ordering::Relaxed,
        );
        Ok(count)
    }
}

#[derive(Default)]
struct ConnectionState {
    established: bool,
    rejected: bool,
}

/// Per-stream receive state.
///
/// tQUIC invokes `on_stream_readable` exactly when new bytes arrive. If the
/// runtime only reacts to that event by waking a *later* reader, any data that
/// arrives while no reader is parked is never re-advertised. That is the root
/// cause of the benchmark's lost completions and 5 s receive timeouts. We
/// therefore drain the stream into this buffer inside the callback and either
/// hand it to a parked reader immediately or keep it until one arrives.
pub type PushItem = Result<(Vec<u8>, bool), TransportError>;
pub type PushSender = mpsc::UnboundedSender<PushItem>;
pub type PushReceiver = mpsc::UnboundedReceiver<PushItem>;
type StreamWaiter = oneshot::Sender<Result<(Vec<u8>, bool), TransportError>>;
type PendingStreamAccept = oneshot::Sender<Result<(u64, PushReceiver), TransportError>>;

/// Per-stream receive state.
///
/// The transport handler drains tQUIC into `data` inside `on_stream_readable`
/// and pushes it straight to the owning task through `push`. The task never
/// sends a read request, so the common path is one wakeup instead of a command
/// plus oneshot round trip. `waiters` remains for the legacy `stream_read()`
/// command.
#[derive(Default)]
struct StreamBuf {
    data: Vec<u8>,
    fin: bool,
    closed: bool,
    errored: bool,
    push: Option<PushSender>,
    push_terminal: bool,
    waiters: Vec<StreamWaiter>,
}

impl StreamBuf {
    fn deliver(&mut self) {
        let terminal = self.fin || self.closed;
        if self.data.is_empty() && !terminal {
            return;
        }
        if let Some(tx) = &self.push {
            if self.push_terminal {
                return;
            }
            let data = std::mem::take(&mut self.data);
            if tx.send(Ok((data, terminal))).is_err() {
                self.push = None;
            } else if terminal {
                self.push_terminal = true;
            }
            return;
        }

        // Legacy reader path: one waiter per ordered byte range.
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

    fn attach_push(&mut self, tx: PushSender) {
        self.push = Some(tx);
    }

    fn fail(&mut self, msg: &str) {
        debug!("stream read failed: {}", msg);
        self.errored = true;
        self.closed = true;
        if let Some(tx) = self.push.take() {
            let _ = tx.send(Err(TransportError::StreamError(msg.to_string())));
            self.push_terminal = true;
        }
        for waiter in self.waiters.drain(..) {
            let _ = waiter.send(Err(TransportError::StreamError(msg.to_string())));
        }
    }
}

/// Register a push reader for `(conn, stream)` and deliver anything already
/// buffered before returning.
fn make_push(
    stream_bufs: &Arc<Mutex<HashMap<(u64, u64), StreamBuf>>>,
    conn_index: u64,
    stream_id: u64,
) -> PushReceiver {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut bufs = stream_bufs.lock().unwrap();
    let buf = bufs.entry((conn_index, stream_id)).or_default();
    buf.attach_push(tx);
    buf.deliver();
    rx
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
    /// Server-side policy: require a verified client certificate with a SPIFFE
    /// SAN in `trust_domain`. `None` means one-way TLS (mTLS off).
    peer_identity_policy: Option<PeerIdentityPolicy>,
    /// Whether an unauthenticated one-way TLS peer chain may be exposed as an
    /// identity. True only when the local TLS stack verified the peer chain.
    verify_peer_chain: bool,
    metrics: Arc<RuntimeMetrics>,
}

#[derive(Debug, Clone)]
struct PeerIdentityPolicy {
    trust_domain: String,
}

impl TquicHandler {
    fn new(
        event_tx: mpsc::UnboundedSender<TquicEvent>,
        stream_bufs: Arc<Mutex<HashMap<(u64, u64), StreamBuf>>>,
        peer_identity_policy: Option<PeerIdentityPolicy>,
        verify_peer_chain: bool,
        metrics: Arc<RuntimeMetrics>,
    ) -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            stream_bufs,
            event_tx,
            peer_identity_policy,
            verify_peer_chain,
            metrics,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum TquicEvent {
    ConnectionEstablished {
        index: u64,
        remote: SocketAddr,
        identity: Option<PeerIdentity>,
    },
    /// A connection reached `on_conn_established` but was rejected by the
    /// application identity policy (missing/invalid/out-of-domain SPIFFE SAN).
    ConnectionRejected {
        index: u64,
        reason: String,
    },
    ConnectionClosed {
        index: u64,
    },
    StreamCreated {
        index: u64,
        stream_id: u64,
    },
    StreamReadable {
        index: u64,
        stream_id: u64,
    },
    StreamWritable {
        index: u64,
        stream_id: u64,
    },
    StreamClosed {
        index: u64,
        stream_id: u64,
    },
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

        // The certificate chain has already been validated by BoringSSL. Only
        // here do we turn it into a workload identity and apply the SPIFFE
        // policy. A failure closes the connection before it is reported as
        // established to the runtime, so no application data can flow.
        let chain = conn.peer_cert_chain();
        let identity = match (&self.peer_identity_policy, chain.as_deref()) {
            (Some(policy), Some(chain)) if !chain.is_empty() => {
                match peer_identity_from_chain(chain, Some(&policy.trust_domain)) {
                    Ok(identity) => {
                        self.metrics.mtls_handshakes.fetch_add(1, Ordering::Relaxed);
                        info!(
                            "verified mTLS peer {} on connection {}",
                            identity.spiffe_id(),
                            index
                        );
                        Some(identity)
                    }
                    Err(e) => {
                        self.metrics.mtls_rejections.fetch_add(1, Ordering::Relaxed);
                        let reason = format!("peer identity rejected: {e}");
                        warn!("connection {}: {}", index, reason);
                        if let Some(state) = self.connections.lock().unwrap().get_mut(&index) {
                            state.rejected = true;
                        }
                        let _ = conn.close(true, 0x100, reason.as_bytes());
                        let _ = self
                            .event_tx
                            .send(TquicEvent::ConnectionRejected { index, reason });
                        return;
                    }
                }
            }
            (Some(_), _) => {
                // Fail closed. In normal operation the TLS handshake already
                // failed because SSL_VERIFY_FAIL_IF_NO_PEER_CERT is set, but a
                // defence-in-depth check here means a future TLS backend that
                // only requests certificates cannot create an unauthenticated
                // session.
                self.metrics.mtls_rejections.fetch_add(1, Ordering::Relaxed);
                let reason = "client certificate required but not presented".to_string();
                warn!("connection {}: {}", index, reason);
                if let Some(state) = self.connections.lock().unwrap().get_mut(&index) {
                    state.rejected = true;
                }
                let _ = conn.close(true, 0x101, reason.as_bytes());
                let _ = self
                    .event_tx
                    .send(TquicEvent::ConnectionRejected { index, reason });
                return;
            }
            (None, Some(chain)) if !chain.is_empty() && self.verify_peer_chain => {
                // One-way TLS server: a client certificate was not requested,
                // but if one is present try to expose its identity. Parse
                // failures are not fatal because mTLS is explicitly off.
                match peer_identity_from_chain(chain, None) {
                    Ok(identity) => Some(identity),
                    Err(e) => {
                        debug!(
                            "connection {}: peer certificate has no usable SPIFFE SAN: {}",
                            index, e
                        );
                        None
                    }
                }
            }
            _ => None,
        };

        let mut conns = self.connections.lock().unwrap();
        if let Some(state) = conns.get_mut(&index) {
            state.established = true;
        }

        let remote = conn
            .paths_iter()
            .next()
            .map(|p| p.remote)
            .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 0)));

        let _ = self.event_tx.send(TquicEvent::ConnectionEstablished {
            index,
            remote,
            identity,
        });
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
        if self.peer_identity_policy.is_some() {
            if let Some(state) = conns.get(&index) {
                if !state.established && !state.rejected {
                    // The connection ended before a verified client identity
                    // was established: handshake failure, missing client cert,
                    // untrusted/expired cert, or a timeout. Count it as a
                    // rejection once, in addition to the application-level
                    // SAN rejections handled in on_conn_established.
                    self.metrics.mtls_rejections.fetch_add(1, Ordering::Relaxed);
                    debug!(
                        "connection {} closed before mTLS identity was established",
                        index
                    );
                }
            }
        }
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
    metrics: Arc<RuntimeMetrics>,
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
    /// Verify the server certificate chain and name on the client.
    ///
    /// **Defaults to `true`.** This is the security-relevant default for the
    /// real data path. A caller that talks to a self-signed loopback test
    /// certificate must opt out explicitly and visibly, e.g.
    /// `RuntimeOptions { verify_server_cert: false, ..Default::default() }`.
    /// Disabling it emits a warning when the runtime is constructed.
    pub verify_server_cert: bool,
    /// PEM file with the trust anchors used when `verify_server_cert` is set.
    /// When unset, the platform trust store is used.
    pub ca_cert_file: Option<String>,
    /// Client certificate (PEM, leaf + intermediates) presented to the server.
    /// Requires `client_key_file`.
    pub client_cert_file: Option<String>,
    /// Client private key (PEM) matching `client_cert_file`.
    pub client_key_file: Option<String>,
    /// Server-side trust anchors (PEM file or directory) used to verify client
    /// certificates. When set, the server requests a client certificate and
    /// fails the handshake if none is presented: mTLS is required. When unset,
    /// the server uses one-way TLS.
    pub client_ca_file: Option<String>,
    /// SPIFFE trust domain that peer identities must belong to. Required when
    /// `client_ca_file` is set on a server.
    pub trust_domain: Option<String>,
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
            verify_server_cert: true,
            ca_cert_file: None,
            client_cert_file: None,
            client_key_file: None,
            client_ca_file: None,
            trust_domain: None,
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
            verify_server_cert: true,
            ca_cert_file: None,
            client_cert_file: None,
            client_key_file: None,
            client_ca_file: None,
            trust_domain: None,
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

        let client_verify = !config.is_server && options.verify_server_cert && config.verify_cert;
        let peer_identity_policy =
            if config.is_server {
                let cert_file = config.cert_file.as_ref().ok_or_else(|| {
                    TransportError::ConfigError("Server requires cert_file".into())
                })?;
                let key_file = config.key_file.as_ref().ok_or_else(|| {
                    TransportError::ConfigError("Server requires key_file".into())
                })?;

                let mut tls_config = TlsConfig::new_server_config(
                    cert_file,
                    key_file,
                    vec![b"h3".to_vec(), b"http/0.9".to_vec()],
                    false,
                )
                .map_err(|e| TransportError::ConfigError(format!("TLS config failed: {:?}", e)))?;

                let client_ca = options
                    .client_ca_file
                    .as_ref()
                    .or(config.client_ca_file.as_ref());
                let trust_domain = options
                    .trust_domain
                    .as_ref()
                    .or(config.trust_domain.as_ref());

                let peer_identity_policy = if let Some(ca_file) = client_ca {
                    let trust_domain = trust_domain.ok_or_else(|| {
                        TransportError::ConfigError(
                            "mTLS requires RuntimeOptions::trust_domain when client_ca_file is set"
                                .into(),
                        )
                    })?;
                    load_trust_anchors(&mut tls_config, ca_file, "client CA certificates")?;
                    // SSL_VERIFY_PEER | SSL_VERIFY_FAIL_IF_NO_PEER_CERT: request a
                    // client certificate and make a missing/invalid one a fatal
                    // handshake error.
                    tls_config.set_verify_client_cert(true);
                    info!(
                        "tQUIC server mTLS ENABLED: client CA={}, trust domain={}",
                        ca_file, trust_domain
                    );
                    Some(PeerIdentityPolicy {
                        trust_domain: trust_domain.clone(),
                    })
                } else {
                    warn!(
                        "tQUIC server TLS is ONE-WAY: no client_ca_file configured, so client \
                     certificates are neither requested nor verified and mTLS is OFF"
                    );
                    None
                };

                quic_config.set_tls_config(tls_config);

                peer_identity_policy
            } else {
                // Two independent switches can disable verification: the runtime
                // option (used by TquicRuntime clients) and WorkerConfig::verify_cert
                // (used by callers constructing TquicAdapter directly). Both must
                // opt out; either one being false wins, so a caller cannot
                // accidentally re-enable a deliberately insecure test transport.
                let verify = client_verify;
                let ca_file = options
                    .ca_cert_file
                    .as_ref()
                    .or(config.ca_cert_file.as_ref());

                let mut tls_config =
                    TlsConfig::new_client_config(vec![b"h3".to_vec(), b"http/0.9".to_vec()], false)
                        .map_err(|e| {
                            TransportError::ConfigError(format!("TLS config failed: {:?}", e))
                        })?;

                // Present a client certificate when configured. This is what makes
                // the server's mTLS policy usable end-to-end.
                let client_cert = options
                    .client_cert_file
                    .as_ref()
                    .or(config.client_cert_file.as_ref());
                let client_key = options
                    .client_key_file
                    .as_ref()
                    .or(config.client_key_file.as_ref());
                match (client_cert, client_key) {
                    (Some(cert), Some(key)) => {
                        tls_config.set_certificate_file(cert).map_err(|e| {
                            TransportError::ConfigError(format!(
                                "Failed to load client certificate from {}: {:?}",
                                cert, e
                            ))
                        })?;
                        tls_config.set_private_key_file(key).map_err(|e| {
                            TransportError::ConfigError(format!(
                                "Failed to load client private key from {}: {:?}",
                                key, e
                            ))
                        })?;
                    }
                    (Some(_), None) => {
                        return Err(TransportError::ConfigError(
                            "client_cert_file is set but client_key_file is missing".into(),
                        ))
                    }
                    (None, Some(_)) => {
                        return Err(TransportError::ConfigError(
                            "client_key_file is set but client_cert_file is missing".into(),
                        ))
                    }
                    (None, None) => {}
                }

                if verify {
                    tls_config.set_verify(true);
                    // With no explicit CA file the BoringSSL context keeps the
                    // platform trust store loaded in Context::new().
                    if let Some(ca_file) = ca_file {
                        load_trust_anchors(&mut tls_config, ca_file, "CA certificates")?;
                    }
                } else {
                    warn!(
                        "tQUIC client TLS server-certificate verification is DISABLED \
                     (verify_server_cert={}, WorkerConfig::verify_cert={}); \
                     this is only acceptable for loopback self-signed test certificates",
                        options.verify_server_cert, config.verify_cert
                    );
                }

                quic_config.set_tls_config(tls_config);

                None
            };

        let poll = mio::Poll::new()?;
        let registry = poll.registry();
        // Token 1 is reserved for the mio waker that breaks the poll when a
        // command arrives. The socket itself uses token 0.
        let waker = Arc::new(Waker::new(registry, mio::Token(1))?);

        let bind_addr = config.bind_addr;

        let metrics = Arc::new(RuntimeMetrics::default());
        metrics.mtls_enabled.store(
            config.is_server && peer_identity_policy.is_some(),
            Ordering::Relaxed,
        );
        let sock = Rc::new(QuicSocket::with_metrics(
            &bind_addr,
            registry,
            metrics.clone(),
        )?);

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let stream_bufs: Arc<Mutex<HashMap<(u64, u64), StreamBuf>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let verify_peer_chain = if config.is_server {
            peer_identity_policy.is_some()
        } else {
            client_verify
        };
        let handlers = TquicHandler::new(
            event_tx,
            stream_bufs.clone(),
            peer_identity_policy,
            verify_peer_chain,
            metrics.clone(),
        );

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
            metrics,
            running: false,
            poll_events: mio::Events::with_capacity(1024),
        })
    }

    pub fn waker(&self) -> Arc<Waker> {
        self.waker.clone()
    }

    pub fn metrics(&self) -> Arc<RuntimeMetrics> {
        self.metrics.clone()
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
                    self.metrics
                        .packets_received
                        .fetch_add(1, Ordering::Relaxed);
                    self.metrics
                        .bytes_received
                        .fetch_add(len as u64, Ordering::Relaxed);
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
            self.metrics
                .packets_received
                .fetch_add(1, Ordering::Relaxed);
            self.metrics
                .bytes_received
                .fetch_add(len as u64, Ordering::Relaxed);

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

/// A connected/established QUIC connection plus its verified peer identity.
type Connected = (u64, Option<PeerIdentity>);
type ConnectResponse = oneshot::Sender<Result<Connected, TransportError>>;
/// An accepted connection: index, remote address, verified peer identity.
type Accepted = (u64, SocketAddr, Option<PeerIdentity>);
type AcceptResponse = oneshot::Sender<Result<Accepted, TransportError>>;

enum RuntimeCommand {
    Connect {
        server_name: String,
        server_addr: SocketAddr,
        resp: ConnectResponse,
    },
    Accept {
        resp: AcceptResponse,
    },
    AcceptStream {
        conn_index: u64,
        resp: oneshot::Sender<Result<(u64, PushReceiver), TransportError>>,
    },
    OpenStream {
        conn_index: u64,
        resp: oneshot::Sender<Result<(u64, PushReceiver), TransportError>>,
    },
    /// Fire-and-forget batch write. The caller only waits for queue capacity,
    /// never for the runtime thread to execute it. Write errors are pushed to
    /// the stream's reader.
    StreamWriteBatchAsync {
        conn_index: u64,
        stream_id: u64,
        messages: Vec<(bytes::Bytes, bool)>,
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
    cmd_tx: mpsc::Sender<RuntimeCommand>,
    thread_handle: Option<thread::JoinHandle<()>>,
    waker: Arc<Waker>,
    wake_pending: Arc<std::sync::atomic::AtomicBool>,
    metrics: Arc<RuntimeMetrics>,
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
    metrics: &RuntimeMetrics,
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
            metrics.flow_blocked.fetch_add(1, Ordering::Relaxed);
            return Err(TransportError::StreamError(format!(
                "Partial FIN write: {}/{}",
                written, total
            )));
        }
        return Ok(written);
    }

    let mut off = 0usize;
    let mut iterations = 0;
    while off < total {
        iterations += 1;
        let written = conn
            .stream_write(stream_id, data.slice(off..), false)
            .map_err(|e| TransportError::StreamError(format!("Stream write failed: {:?}", e)))?;
        if written == 0 {
            metrics.flow_blocked.fetch_add(1, Ordering::Relaxed);
            return Err(TransportError::StreamError(
                "Stream write made no progress (flow control blocked)".into(),
            ));
        }
        off += written;
    }
    if iterations > 1 {
        metrics.partial_writes.fetch_add(1, Ordering::Relaxed);
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
        // Bounded queue: backpressure instead of unbounded memory growth when
        // producers outrun the runtime thread.
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<RuntimeCommand>(2048);
        let wake_pending = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let wake_pending_thread = wake_pending.clone();
        let (waker_tx, waker_rx) =
            std::sync::mpsc::sync_channel::<(Arc<Waker>, Arc<RuntimeMetrics>)>(1);

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
                ca_cert_file: options.ca_cert_file.clone(),
                client_cert_file: options.client_cert_file.clone(),
                client_key_file: options.client_key_file.clone(),
                client_ca_file: options.client_ca_file.clone(),
                trust_domain: options.trust_domain.clone(),
            };

            let mut adapter =
                TquicAdapter::with_options(config, options).expect("Failed to create TquicAdapter");
            let _ = waker_tx.send((adapter.waker(), adapter.metrics()));

            let stream_bufs = adapter.stream_bufs.clone();
            let mut pending_accepts: Vec<AcceptResponse> = Vec::new();
            let mut incoming_connections: Vec<Accepted> = Vec::new();
            let mut pending_stream_accepts: HashMap<u64, Vec<PendingStreamAccept>> = HashMap::new();
            let mut incoming_streams: HashMap<u64, Vec<u64>> = HashMap::new();
            let mut next_stream_ids: HashMap<u64, u64> = HashMap::new();
            let mut pending_connects: HashMap<u64, (Instant, ConnectResponse)> = HashMap::new();

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
                    adapter.metrics.commands.fetch_add(1, Ordering::Relaxed);
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
                            if let Some((conn_index, remote, identity)) = incoming_connections.pop()
                            {
                                let _ = resp.send(Ok((conn_index, remote, identity)));
                            } else {
                                pending_accepts.push(resp);
                            }
                        }
                        RuntimeCommand::AcceptStream { conn_index, resp } => {
                            if let Some(streams) = incoming_streams.get_mut(&conn_index) {
                                if let Some(stream_id) = streams.pop() {
                                    let rx = make_push(&stream_bufs, conn_index, stream_id);
                                    let _ = resp.send(Ok((stream_id, rx)));
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
                                    let rx = make_push(&stream_bufs, conn_index, current_id);
                                    let _ = resp.send(Ok((current_id, rx)));
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
                                .and_then(|conn| {
                                    write_stream_all(conn, stream_id, data, fin, &adapter.metrics)
                                });
                            let _ = resp.send(result);
                        }
                        RuntimeCommand::StreamWriteBatchAsync {
                            conn_index,
                            stream_id,
                            messages,
                        } => {
                            adapter
                                .metrics
                                .write_batches
                                .fetch_add(1, Ordering::Relaxed);
                            adapter
                                .metrics
                                .writes
                                .fetch_add(messages.len() as u64, Ordering::Relaxed);
                            let bytes: u64 = messages.iter().map(|(d, _)| d.len() as u64).sum();
                            adapter
                                .metrics
                                .bytes_written
                                .fetch_add(bytes, Ordering::Relaxed);
                            let result = adapter
                                .endpoint
                                .conn_get_mut(conn_index)
                                .ok_or_else(|| {
                                    TransportError::ConnectionFailed("Connection not found".into())
                                })
                                .and_then(|conn| {
                                    for (data, fin) in messages {
                                        write_stream_all(
                                            conn,
                                            stream_id,
                                            data,
                                            fin,
                                            &adapter.metrics,
                                        )?;
                                    }
                                    Ok(())
                                });
                            if let Err(e) = result {
                                adapter.metrics.write_errors.fetch_add(1, Ordering::Relaxed);
                                let mut bufs = stream_bufs.lock().unwrap();
                                if let Some(buf) = bufs.get_mut(&(conn_index, stream_id)) {
                                    buf.fail(&format!("write failed: {:?}", e));
                                }
                            }
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
                                        total_written += write_stream_all(
                                            conn,
                                            stream_id,
                                            data,
                                            fin,
                                            &adapter.metrics,
                                        )?;
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
                        Ok(TquicEvent::ConnectionEstablished {
                            index,
                            remote,
                            identity,
                        }) => {
                            if let Some((_start, resp)) = pending_connects.remove(&index) {
                                let _ = resp.send(Ok((index, identity)));
                            } else if let Some(waiter) = pending_accepts.pop() {
                                let _ = waiter.send(Ok((index, remote, identity)));
                            } else {
                                incoming_connections.push((index, remote, identity));
                            }
                        }
                        Ok(TquicEvent::ConnectionRejected { index, reason }) => {
                            warn!("runtime: connection {} rejected: {}", index, reason);
                            if let Some((_start, resp)) = pending_connects.remove(&index) {
                                let _ = resp.send(Err(TransportError::ConnectionFailed(format!(
                                    "connection rejected by peer identity policy: {}",
                                    reason
                                ))));
                            }
                        }
                        Ok(TquicEvent::StreamCreated { index, stream_id }) => {
                            if let Some(waiters) = pending_stream_accepts.get_mut(&index) {
                                if let Some(waiter) = waiters.pop() {
                                    let rx = make_push(&stream_bufs, index, stream_id);
                                    let _ = waiter.send(Ok((stream_id, rx)));
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

                adapter.metrics.poll_calls.fetch_add(1, Ordering::Relaxed);
                let backlog = cmd_rx.len() as u64;
                let prev = adapter.metrics.max_command_backlog.load(Ordering::Relaxed);
                if backlog > prev {
                    adapter
                        .metrics
                        .max_command_backlog
                        .store(backlog, Ordering::Relaxed);
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

        let (waker, metrics) = waker_rx.recv().map_err(|_| {
            TransportError::ConnectionFailed("Runtime thread failed to start".into())
        })?;

        Ok(TquicRuntime {
            cmd_tx,
            thread_handle: Some(thread_handle),
            waker,
            wake_pending,
            metrics,
        })
    }

    /// Enqueue a command and wake the runtime thread at most once per batch.
    /// The bounded queue provides backpressure; `send` awaits capacity rather
    /// than growing memory without bound.
    async fn send_command(&self, cmd: RuntimeCommand) -> Result<(), TransportError> {
        // Wake before waiting for queue capacity: when the queue is full the
        // runtime is the only party that can free a slot, and it may be parked
        // in poll(2) with no socket event pending.
        self.wake();
        self.cmd_tx
            .send(cmd)
            .await
            .map_err(|_| TransportError::ConnectionFailed("Runtime stopped".into()))
    }

    fn wake(&self) {
        if !self
            .wake_pending
            .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            let _ = self.waker.wake();
        }
    }

    /// Per-worker counters for diagnostics. Cheap to clone.
    pub fn metrics(&self) -> Arc<RuntimeMetrics> {
        self.metrics.clone()
    }

    pub async fn connect(
        &self,
        server_name: &str,
        server_addr: SocketAddr,
    ) -> Result<u64, TransportError> {
        Ok(self
            .connect_with_identity(server_name, server_addr)
            .await?
            .0)
    }

    /// Connect and also return the identity extracted from the verified server
    /// certificate, if it has a SPIFFE SAN.
    pub async fn connect_with_identity(
        &self,
        server_name: &str,
        server_addr: SocketAddr,
    ) -> Result<(u64, Option<PeerIdentity>), TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_command(RuntimeCommand::Connect {
            server_name: server_name.to_string(),
            server_addr,
            resp: resp_tx,
        })
        .await?;

        resp_rx
            .await
            .map_err(|_| TransportError::ConnectionFailed("Runtime stopped".into()))?
    }

    pub async fn accept(&self) -> Result<(u64, SocketAddr), TransportError> {
        let (index, remote, _identity) = self.accept_with_identity().await?;
        Ok((index, remote))
    }

    /// Accept a connection and return the verified peer identity (if any).
    pub async fn accept_with_identity(
        &self,
    ) -> Result<(u64, SocketAddr, Option<PeerIdentity>), TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_command(RuntimeCommand::Accept { resp: resp_tx })
            .await?;

        resp_rx
            .await
            .map_err(|_| TransportError::ConnectionFailed("Runtime stopped".into()))?
    }

    pub async fn accept_stream(
        &self,
        conn_index: u64,
    ) -> Result<(u64, PushReceiver), TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_command(RuntimeCommand::AcceptStream {
            conn_index,
            resp: resp_tx,
        })
        .await?;

        resp_rx
            .await
            .map_err(|_| TransportError::StreamError("Runtime stopped".into()))?
    }

    pub async fn open_stream(
        &self,
        conn_index: u64,
    ) -> Result<(u64, PushReceiver), TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_command(RuntimeCommand::OpenStream {
            conn_index,
            resp: resp_tx,
        })
        .await?;

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
        })
        .await?;

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
        })
        .await?;

        resp_rx
            .await
            .map_err(|_| TransportError::StreamError("Runtime stopped".into()))?
    }

    /// Enqueue a batch without waiting for the runtime to execute it. The only
    /// wait is bounded-queue capacity, which is the backpressure signal.
    pub async fn stream_write_batch_async(
        &self,
        conn_index: u64,
        stream_id: u64,
        messages: Vec<(bytes::Bytes, bool)>,
    ) -> Result<(), TransportError> {
        self.send_command(RuntimeCommand::StreamWriteBatchAsync {
            conn_index,
            stream_id,
            messages,
        })
        .await
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
        })
        .await?;

        resp_rx
            .await
            .map_err(|_| TransportError::StreamError("Runtime stopped".into()))?
    }

    pub async fn close_connection(&self, conn_index: u64) -> Result<(), TransportError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_command(RuntimeCommand::CloseConnection {
            conn_index,
            resp: resp_tx,
        })
        .await?;

        resp_rx
            .await
            .map_err(|_| TransportError::ConnectionFailed("Runtime stopped".into()))?
    }

    pub fn shutdown(&mut self) {
        let _ = self.cmd_tx.try_send(RuntimeCommand::Shutdown);
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

        let (conn_index, peer_identity) = self
            .runtime
            .connect_with_identity(&server_name, server_addr)
            .await?;

        Ok(TquicConnection {
            conn_index,
            runtime: self.runtime.clone(),
            peer_identity,
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
        Self::with_options(bind_addr, cert_file, key_file, RuntimeOptions::default()).await
    }

    /// Start a server with explicit mTLS/identity configuration.
    pub async fn with_options(
        bind_addr: SocketAddr,
        cert_file: String,
        key_file: String,
        options: RuntimeOptions,
    ) -> Result<Self, TransportError> {
        let runtime =
            TquicRuntime::with_options(true, bind_addr, Some(cert_file), Some(key_file), options)?;

        Ok(TquicServer {
            runtime: Arc::new(runtime),
            bind_addr,
        })
    }

    /// Runtime counters, including mTLS handshake and rejection counts.
    pub fn metrics(&self) -> Arc<RuntimeMetrics> {
        self.runtime.metrics()
    }

    pub async fn listen(&self, _addr: &str) -> Result<TquicListener, TransportError> {
        Ok(TquicListener {
            runtime: self.runtime.clone(),
            bind_addr: self.bind_addr,
        })
    }

    /// Address the server is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        Ok(self.bind_addr)
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
        let (conn_index, _remote, peer_identity) = self.runtime.accept_with_identity().await?;

        Ok(TquicConnection {
            conn_index,
            runtime: self.runtime.clone(),
            peer_identity,
        })
    }

    /// Accept a connection and expose the verified SPIFFE identity directly.
    pub async fn accept_with_identity(
        &mut self,
    ) -> Result<(TquicConnection, Option<PeerIdentity>), TransportError> {
        let (conn_index, _remote, peer_identity) = self.runtime.accept_with_identity().await?;
        let connection = TquicConnection {
            conn_index,
            runtime: self.runtime.clone(),
            peer_identity: peer_identity.clone(),
        };
        Ok((connection, peer_identity))
    }
}

#[async_trait::async_trait]
impl crate::transport::Listener for TquicListener {
    type Connection = TquicConnection;

    async fn accept(
        &mut self,
    ) -> Result<Self::Connection, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let (conn_index, _remote, peer_identity) = self
            .runtime
            .accept_with_identity()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        Ok(TquicConnection {
            conn_index,
            runtime: self.runtime.clone(),
            peer_identity,
        })
    }
}

pub struct TquicConnection {
    pub conn_index: u64,
    runtime: Arc<TquicRuntime>,
    peer_identity: Option<PeerIdentity>,
}

impl TquicConnection {
    /// The verified peer SPIFFE identity, if the peer certificate carried a
    /// valid `spiffe://` URI SAN.
    pub fn peer_identity(&self) -> Option<&PeerIdentity> {
        self.peer_identity.as_ref()
    }

    /// The peer SPIFFE ID as a string, if present.
    pub fn peer_spiffe_id(&self) -> Option<String> {
        self.peer_identity
            .as_ref()
            .map(|id| id.spiffe_id().to_string())
    }

    pub async fn open_stream(&self) -> Result<TquicStream, TransportError> {
        let (stream_id, read_rx) = self.runtime.open_stream(self.conn_index).await?;
        Ok(TquicStream::with_reader(
            self.conn_index,
            stream_id,
            self.runtime.clone(),
            read_rx,
        ))
    }

    pub async fn accept_stream(&self) -> Result<TquicStream, TransportError> {
        let (stream_id, read_rx) = self.runtime.accept_stream(self.conn_index).await?;
        Ok(TquicStream::with_reader(
            self.conn_index,
            stream_id,
            self.runtime.clone(),
            read_rx,
        ))
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
    /// Push-based reader. `None` falls back to the legacy request/oneshot
    /// `stream_read` command (kept for manually constructed streams).
    read_rx: tokio::sync::Mutex<Option<PushReceiver>>,
}

impl TquicStream {
    pub fn new(conn_index: u64, stream_id: u64, runtime: Arc<TquicRuntime>) -> Self {
        Self {
            conn_index,
            stream_id,
            runtime,
            read_rx: tokio::sync::Mutex::new(None),
        }
    }

    fn with_reader(
        conn_index: u64,
        stream_id: u64,
        runtime: Arc<TquicRuntime>,
        read_rx: PushReceiver,
    ) -> Self {
        Self {
            conn_index,
            stream_id,
            runtime,
            read_rx: tokio::sync::Mutex::new(Some(read_rx)),
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

    /// Send data with zero-copy using bytes::Bytes.
    ///
    /// This uses the fire-and-forget path: it waits only for bounded-queue
    /// capacity, not for the runtime thread to execute the write. A write
    /// failure is delivered to this stream's reader as an error.
    pub async fn send_bytes(&self, data: bytes::Bytes, fin: bool) -> Result<(), TransportError> {
        self.runtime
            .stream_write_batch_async(self.conn_index, self.stream_id, vec![(data, fin)])
            .await
    }

    /// Batch send multiple messages in a single queue operation.
    pub async fn send_batch(
        &self,
        messages: Vec<(bytes::Bytes, bool)>,
    ) -> Result<(), TransportError> {
        self.runtime
            .stream_write_batch_async(self.conn_index, self.stream_id, messages)
            .await
    }

    pub async fn receive(&self, _max_length: usize) -> Result<Vec<u8>, TransportError> {
        self.recv().await
    }

    pub async fn close(&self) -> Result<(), TransportError> {
        self.runtime
            .stream_write(self.conn_index, self.stream_id, bytes::Bytes::new(), true)
            .await?;
        Ok(())
    }

    pub async fn recv(&self) -> Result<Vec<u8>, TransportError> {
        let mut guard = self.read_rx.lock().await;
        match guard.as_mut() {
            Some(rx) => match rx.recv().await {
                Some(Ok((data, _fin))) => Ok(data),
                Some(Err(e)) => Err(e),
                None => Err(TransportError::StreamError(
                    "stream reader closed".to_string(),
                )),
            },
            None => {
                // Legacy fallback for streams not created via the runtime.
                let (data, _fin) = self
                    .runtime
                    .stream_read(self.conn_index, self.stream_id)
                    .await?;
                Ok(data)
            }
        }
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
        // The push receiver is single-consumer; clones use the legacy command
        // path rather than silently stealing bytes from the original reader.
        Self {
            conn_index: self.conn_index,
            stream_id: self.stream_id,
            runtime: self.runtime.clone(),
            read_rx: tokio::sync::Mutex::new(None),
        }
    }
}

impl Unpin for TquicStream {}

#[cfg(test)]
mod tls_verification_tests {
    //! The TLS default is security-relevant: a regression to `false` would
    //! silently make the real data path accept any certificate. These tests
    //! pin the default and prove that a self-signed loopback server is
    //! rejected with it (and only accepted when a caller opts out by name).

    use super::*;
    use rcgen::{Certificate, CertificateParams, DistinguishedName, DnType};
    use std::time::Duration;

    #[test]
    fn runtime_options_verify_server_cert_by_default() {
        assert!(
            RuntimeOptions::default().verify_server_cert,
            "RuntimeOptions::default() must verify the server certificate"
        );
        assert!(
            RuntimeOptions::benchmark(0).verify_server_cert,
            "benchmark() must preserve certificate verification"
        );
        assert!(
            RuntimeOptions::benchmark_client().verify_server_cert,
            "benchmark_client() must preserve certificate verification"
        );
        assert!(
            crate::transport::messages::WorkerConfig::default().verify_cert,
            "WorkerConfig::default() must verify the server certificate"
        );
    }

    fn self_signed_server() -> (tempfile::TempDir, SocketAddr) {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut params = CertificateParams::new(vec!["127.0.0.1".to_string()]);
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, "loopback-self-signed-test");
        let cert = Certificate::from_params(params).expect("generate cert");
        std::fs::write(dir.path().join("cert.pem"), cert.serialize_pem().unwrap()).unwrap();
        std::fs::write(dir.path().join("key.pem"), cert.serialize_private_key_pem()).unwrap();

        // Pick a free UDP port, then let the server bind it.
        let probe = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = probe.local_addr().unwrap();
        drop(probe);
        (dir, addr)
    }

    #[tokio::test]
    async fn self_signed_server_is_rejected_with_default_options() {
        let (dir, addr) = self_signed_server();
        let _server = TquicServer::new(
            addr,
            dir.path().join("cert.pem").to_string_lossy().into_owned(),
            dir.path().join("key.pem").to_string_lossy().into_owned(),
            Some(1),
        )
        .await
        .expect("start server");

        let client = TquicClient::new(None).await.expect("client");
        let result =
            tokio::time::timeout(Duration::from_secs(30), client.connect(&addr.to_string()))
                .await
                .expect("handshake attempt must not hang");
        assert!(
            result.is_err(),
            "a self-signed server must be rejected when verify_server_cert is true"
        );
    }

    #[tokio::test]
    async fn explicit_opt_out_accepts_self_signed_loopback_server() {
        let (dir, addr) = self_signed_server();
        let _server = TquicServer::new(
            addr,
            dir.path().join("cert.pem").to_string_lossy().into_owned(),
            dir.path().join("key.pem").to_string_lossy().into_owned(),
            Some(1),
        )
        .await
        .expect("start server");

        let client = TquicClient::with_options(RuntimeOptions {
            // Loopback self-signed test certificate: the only reason this
            // opt-out is legitimate.
            verify_server_cert: false,
            ..RuntimeOptions::default()
        })
        .await
        .expect("client");
        let result =
            tokio::time::timeout(Duration::from_secs(30), client.connect(&addr.to_string())).await;
        assert!(
            matches!(result, Ok(Ok(_))),
            "explicit opt-out must still connect to the loopback test server"
        );
    }
}
