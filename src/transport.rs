use std::sync::Arc;
use std::time::Duration;
use std::net::{SocketAddr, UdpSocket};
use rustls::{Certificate, ClientConfig, PrivateKey, RootCertStore};
use quinn::{Endpoint, ServerConfig, TransportConfig as QuinnTransportConfig, TokioRuntime, VarInt};
use quinn::congestion::{BbrConfig, CubicConfig, NewRenoConfig};

#[derive(Clone, Debug, Copy)]
pub enum CongestionController {
    Bbr,
    Cubic,
    NewReno
}

#[derive(Clone, Debug)]
pub struct TransportSettings {
    pub max_concurrent_uni_streams: u32,
    pub max_concurrent_bidi_streams: u32,
    pub stream_receive_window: u32,
    pub connection_receive_window: u32,
    pub max_idle_timeout_ms: Option<u64>,
    pub keep_alive_interval_ms: Option<u64>,
    pub initial_rtt_ms: u64,
    pub congestion_controller: CongestionController
}

impl Default for TransportSettings {
    fn default() -> Self {
        Self {
            max_concurrent_uni_streams: 1000,
            max_concurrent_bidi_streams: 1000,
            stream_receive_window: 2 * 1024 * 1024, // 2MB
            connection_receive_window: 16 * 1024 * 1024, // 16MB 
            max_idle_timeout_ms: Some(30000), // 30 seconds
            keep_alive_interval_ms: Some(10000), // 10 seconds
            initial_rtt_ms: 100,
            congestion_controller: CongestionController::NewReno
        }
    }
}

#[derive(Debug, Clone)]
pub struct QuicTransport {
    backend: BackendImpl,
}

#[derive(Clone, Debug)]
pub struct TlsConfig {
    pub cert: Certificate,
    pub key: PrivateKey,
    pub ca_cert: Certificate,
}

#[derive(Clone)]
enum ConnectionInner {
    Quinn(quinn::Connection),
}

enum IncomingInner {
    Quinn(Endpoint),
}

#[derive(Clone, Debug)]
enum BackendImpl {
    Quinn(QuinnCtx),
    Alt(QuinnCtx), // placeholder delegating to Quinn
}

#[derive(Clone, Debug)]
struct QuinnCtx {
    endpoint: Endpoint,
    tls_config: TlsConfig,
    settings: TransportSettings,
}

impl BackendImpl {
    fn new(tls_config: TlsConfig, settings: TransportSettings) -> crate::Result<Self> {
        #[cfg(feature = "backend-alt")]
        {
            // For now, Alt delegates to Quinn
            Ok(BackendImpl::Alt(QuinnCtx::new(tls_config, settings)?))
        }
        #[cfg(all(not(feature = "backend-alt"), feature = "backend-quinn"))]
        {
            Ok(BackendImpl::Quinn(QuinnCtx::new(tls_config, settings)?))
        }
        #[cfg(all(not(feature = "backend-alt"), not(feature = "backend-quinn")))]
        {
            compile_error!("One backend feature must be enabled: backend-quinn or backend-alt");
        }
    }

    async fn connect(&self, addr: &str) -> crate::Result<Connection> {
        match self {
            BackendImpl::Quinn(ctx) | BackendImpl::Alt(ctx) => ctx.connect(addr).await,
        }
    }

    async fn listen(&self, addr: &str) -> crate::Result<Incoming> {
        match self {
            BackendImpl::Quinn(ctx) | BackendImpl::Alt(ctx) => ctx.listen(addr).await,
        }
    }
}

impl QuinnCtx {
    fn new(tls_config: TlsConfig, settings: TransportSettings) -> crate::Result<Self> {
        let mut transport_config = QuinnTransportConfig::default();
        settings.apply_to_config(&mut transport_config);

        let mut server_config = ServerConfig::with_single_cert(
            vec![tls_config.cert.clone()],
            tls_config.key.clone()
        ).map_err(|e| crate::TransportError::ConnectionFailed(e.to_string()))?;

        server_config.transport_config(Arc::new(transport_config));

        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| crate::TransportError::ConnectionFailed(e.to_string()))?;

        let endpoint = Endpoint::new(
            Default::default(),
            Some(server_config),
            socket,
            Arc::new(TokioRuntime)
        ).map_err(|e| crate::TransportError::ConnectionFailed(e.to_string()))?;

        Ok(Self { endpoint, tls_config, settings })
    }

    async fn connect(&self, addr: &str) -> crate::Result<Connection> {
        let mut roots = RootCertStore::empty();
        roots.add(&self.tls_config.ca_cert)
            .map_err(|e| crate::TransportError::ConnectionFailed(format!("Failed to add CA cert: {}", e)))?;
        let crypto_config = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let mut client_config = quinn::ClientConfig::new(Arc::new(crypto_config));
        let mut transport = QuinnTransportConfig::default();
        self.settings.apply_to_config(&mut transport);
        client_config.transport_config(Arc::new(transport));
        let server_addr = addr.parse::<SocketAddr>()
            .map_err(|e| crate::TransportError::ConnectionFailed(format!("Invalid address: {}", e)))?;
        println!("Attempting QUIC connection to {}", server_addr);
        println!("Starting TLS handshake with server...");
        let connecting = self.endpoint.connect_with(client_config, server_addr, "localhost")
            .map_err(|e| crate::TransportError::ConnectionFailed(e.to_string()))?;
        let conn = tokio::time::timeout(Duration::from_secs(10), connecting).await
            .map_err(|_| crate::TransportError::ConnectionFailed("Connection timeout".into()))?
            .map_err(|e| crate::TransportError::ConnectionFailed(e.to_string()))?;
        Ok(Connection { inner: ConnectionInner::Quinn(conn), max_message_size: self.settings.stream_receive_window as usize })
    }

    async fn listen(&self, addr: &str) -> crate::Result<Incoming> {
        let bind_addr: SocketAddr = addr.parse()
            .map_err(|e| crate::TransportError::ConnectionFailed(format!("Invalid address: {}", e)))?;
        let socket = UdpSocket::bind(bind_addr)
            .map_err(|e| crate::TransportError::ConnectionFailed(e.to_string()))?;
        let mut server_config = ServerConfig::with_single_cert(
            vec![self.tls_config.cert.clone()],
            self.tls_config.key.clone()
        ).map_err(|e| crate::TransportError::ConnectionFailed(format!("Failed to create server cert config: {}", e)))?;
        let mut transport_config = QuinnTransportConfig::default();
        self.settings.apply_to_config(&mut transport_config);
        server_config.transport_config(Arc::new(transport_config));
        let endpoint = Endpoint::new(
            Default::default(),
            Some(server_config),
            socket,
            Arc::new(TokioRuntime)
        ).map_err(|e| crate::TransportError::ConnectionFailed(e.to_string()))?;
        Ok(Incoming { inner: IncomingInner::Quinn(endpoint), settings: self.settings.clone() })
    }
}

impl TransportSettings {
    pub fn apply_to_config(&self, config: &mut QuinnTransportConfig) {
        config
            .max_concurrent_uni_streams(VarInt::from_u32(self.max_concurrent_uni_streams))
            .max_concurrent_bidi_streams(VarInt::from_u32(self.max_concurrent_bidi_streams))
            .stream_receive_window(VarInt::from_u32(self.stream_receive_window))
            .receive_window(VarInt::from_u32(self.connection_receive_window))
            .max_idle_timeout(self.max_idle_timeout_ms.map(|ms| Duration::from_millis(ms).try_into().unwrap()))
            .keep_alive_interval(self.keep_alive_interval_ms.map(Duration::from_millis))
            .initial_rtt(Duration::from_millis(self.initial_rtt_ms));

        match self.congestion_controller {
            CongestionController::Bbr => {
                config.congestion_controller_factory(Arc::new(BbrConfig::default()));
            },
            CongestionController::Cubic => {
                config.congestion_controller_factory(Arc::new(CubicConfig::default()));
            },
            CongestionController::NewReno => {
                config.congestion_controller_factory(Arc::new(NewRenoConfig::default()));
            }
        }
    }
}

#[async_trait::async_trait]
pub trait Transport {
    async fn connect(&self, addr: &str) -> crate::Result<Connection>;
    async fn listen(&self, addr: &str) -> crate::Result<Incoming>;
}

impl QuicTransport {
    pub fn new(tls_config: TlsConfig) -> crate::Result<Self> {
        Self::with_settings(tls_config, TransportSettings::default())
    }

    pub fn with_settings(tls_config: TlsConfig, settings: TransportSettings) -> crate::Result<Self> {
        Ok(Self { backend: BackendImpl::new(tls_config, settings)? })
    }
}

#[async_trait::async_trait]
impl Transport for QuicTransport {
    async fn connect(&self, addr: &str) -> crate::Result<Connection> {
        self.backend.connect(addr).await
    }

    async fn listen(&self, addr: &str) -> crate::Result<Incoming> {
        self.backend.listen(addr).await
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FlowControlInfo {
    pub path_stats: PathStats,
    pub congestion_state: CongestionState,
    pub bandwidth_estimate: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct PathStats {
    pub rtt: Duration,
    pub smoothed_rtt: Duration,
    pub rtt_variance: Duration,
    pub packets_sent: u64,
    pub packets_lost: u64,
    pub bytes_in_flight: u64,
    pub congestion_events: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct CongestionState {
    pub window_size: u64,
    pub congestion_events: u64,
    pub is_congested: bool,
}

#[derive(Clone)]
pub struct Connection {
    inner: ConnectionInner,
    max_message_size: usize,
}

pub struct Incoming {
    inner: IncomingInner,
    settings: TransportSettings,
}

impl Connection {
    pub fn flow_control_status(&self) -> crate::Result<FlowControlInfo> {
        match &self.inner {
            ConnectionInner::Quinn(conn) => {
                let stats = conn.stats();
                let path_stats = PathStats {
                    rtt: stats.path.rtt,
                    smoothed_rtt: stats.path.rtt,
                    rtt_variance: Duration::from_millis(50),
                    packets_sent: stats.frame_tx.acks,
                    packets_lost: stats.path.lost_packets,
                    bytes_in_flight: stats.path.cwnd,
                    congestion_events: stats.path.congestion_events,
                };
                let congestion_state = CongestionState {
                    window_size: stats.path.cwnd,
                    congestion_events: stats.path.lost_packets,
                    is_congested: stats.path.congestion_events > 0,
                };
                let bandwidth_estimate = {
                    let srtt = stats.path.rtt.as_secs_f64().max(0.001);
                    let window_estimate = stats.path.cwnd as f64 / srtt;
                    (window_estimate as u64).max(16 * 1024)
                };
                Ok(FlowControlInfo { path_stats, congestion_state, bandwidth_estimate })
            }
        }
    }

    pub async fn accept_bi(&self) -> crate::Result<crate::QuicStream> {
        match &self.inner {
            ConnectionInner::Quinn(conn) => {
                let (send, recv) = conn.accept_bi().await
                    .map_err(|e| crate::TransportError::StreamError(e.to_string()))?;
                Ok(crate::QuicStream::new(send, recv, self.max_message_size))
            }
        }
    }

    pub async fn open_bi(&self) -> crate::Result<crate::QuicStream> {
        match &self.inner {
            ConnectionInner::Quinn(conn) => {
                let (send, recv) = conn.open_bi().await
                    .map_err(|e| crate::TransportError::StreamError(e.to_string()))?;
                Ok(crate::QuicStream::new(send, recv, self.max_message_size))
            }
        }
    }
}

impl Incoming {
    pub fn local_addr(&self) -> crate::Result<String> {
        match &self.inner {
            IncomingInner::Quinn(ep) => ep
                .local_addr()
                .map(|addr| addr.to_string())
                .map_err(|e| crate::TransportError::ConnectionFailed(format!("Failed to get local address: {}", e))),
        }
    }

    pub async fn accept(&mut self) -> crate::Result<Connection> {
        match &mut self.inner {
            IncomingInner::Quinn(ep) => {
                let new_conn = ep.accept().await
                    .ok_or_else(|| crate::TransportError::ConnectionFailed("No more incoming connections".into()))?
                    .await
                    .map_err(|e| crate::TransportError::ConnectionFailed(e.to_string()))?;
                Ok(Connection { inner: ConnectionInner::Quinn(new_conn), max_message_size: self.settings.stream_receive_window as usize })
            }
        }
    }
}
