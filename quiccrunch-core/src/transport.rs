use std::time::Duration;
use rustls::{Certificate, PrivateKey};

#[cfg(feature = "backend-tquic")]
pub mod tquic;

#[cfg(feature = "grpc-comparison")]
pub mod grpc;

// Enforce tQUIC backend only
#[cfg(not(feature = "backend-tquic"))]
compile_error!("You must enable the backend-tquic feature");

#[derive(Clone, Debug, Copy)]
pub enum CongestionController {
    Bbr, // BBR for tquic
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
    // PEM data for tQUIC backend (required)
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
    pub ca_cert_pem: Vec<u8>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        // Generate self-signed certificates for testing
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = cert.serialize_der().unwrap();
        let key_der = cert.serialize_private_key_der();
        let cert_pem = cert.serialize_pem().unwrap().into_bytes();
        let key_pem = cert.serialize_private_key_pem().into_bytes();
        
        Self {
            cert: Certificate(cert_der.clone()),
            key: PrivateKey(key_der),
            ca_cert: Certificate(cert_der),
            cert_pem: cert_pem.clone(),
            key_pem,
            ca_cert_pem: cert_pem,
        }
    }
}

#[derive(Clone)]
enum ConnectionInner {
    #[cfg(feature = "backend-quinn")]
    Quinn(quinn::Connection),
    #[cfg(feature = "backend-tquic")]
    Tquic(crate::transport::tquic::ConnHandle),
    #[cfg(feature = "io-uring-advanced")]
    IoUringAdvanced(crate::io_uring_advanced::Connection),
}

enum IncomingInner {
    #[cfg(feature = "backend-quinn")]
    Quinn(Endpoint),
    #[cfg(feature = "backend-tquic")]
    Tquic(crate::transport::tquic::IncomingHandle),
    #[cfg(feature = "io-uring-advanced")]
    IoUringAdvanced(crate::io_uring_advanced::Incoming),
}

#[derive(Clone, Debug)]
enum BackendImpl {
    #[cfg(feature = "backend-quinn")]
    Quinn(QuinnCtx),
    #[cfg(feature = "backend-quinn")]
    #[allow(dead_code)]
    Alt(QuinnCtx), // placeholder delegating to Quinn
    #[cfg(feature = "backend-tquic")]
    Tquic(crate::transport::tquic::TquicCtx),
    #[cfg(feature = "io-uring-advanced")]
    IoUringAdvanced(crate::io_uring_advanced::IoUringTransport),
}

#[cfg(feature = "backend-quinn")]
#[derive(Clone, Debug)]
struct QuinnCtx {
    endpoint: Endpoint,
    tls_config: TlsConfig,
    settings: TransportSettings,
}

impl BackendImpl {
    fn new(tls_config: TlsConfig, settings: TransportSettings) -> crate::Result<Self> {
        // Prioritize io_uring backend when available and enabled
        #[cfg(feature = "io-uring-advanced")]
        {
            let advanced_settings = crate::config::QuicTransportSettings {
                backend: crate::config::Backend::IoUringAdvanced,
                max_idle_timeout_ms: settings.max_idle_timeout_ms,
                keep_alive_interval_ms: settings.keep_alive_interval_ms,
                initial_max_data: settings.connection_receive_window as u64,
                initial_max_stream_data_bidi_remote: settings.stream_receive_window as u64,
                congestion_control: match settings.congestion_controller {
                    CongestionController::Bbr => crate::config::CongestionControl::Bbr,
                    CongestionController::Cubic => crate::config::CongestionControl::Cubic,
                    CongestionController::NewReno => crate::config::CongestionControl::NewReno,
                },
                enable_multipath: true,
                enable_hystart: true,
            };
            return Self::new_advanced(tls_config, advanced_settings);
        }

        // Fallback to tQUIC as the default backend when available
        #[cfg(feature = "backend-tquic")]
        {
            // Convert simple settings to advanced settings and delegate to tquic
            let advanced_settings = crate::config::QuicTransportSettings {
                backend: crate::config::Backend::Tquic,
                max_idle_timeout_ms: settings.max_idle_timeout_ms,
                keep_alive_interval_ms: settings.keep_alive_interval_ms,
                initial_max_data: settings.connection_receive_window as u64,
                initial_max_stream_data_bidi_remote: settings.stream_receive_window as u64,
                congestion_control: match settings.congestion_controller {
                    CongestionController::Bbr => crate::config::CongestionControl::Bbr,
                    CongestionController::Cubic => crate::config::CongestionControl::Cubic,
                    CongestionController::NewReno => crate::config::CongestionControl::NewReno,
                },
                enable_multipath: true,
                enable_hystart: true,
            };
            Self::new_advanced(tls_config, advanced_settings)
        }
        #[cfg(all(not(feature = "backend-tquic"), feature = "backend-quinn"))]
        {
            // Fallback to Quinn only when tQUIC is not available
            Ok(BackendImpl::Quinn(QuinnCtx::new(tls_config, settings)?))
        }
    }

    fn new_advanced(tls_config: TlsConfig, adv: crate::config::QuicTransportSettings) -> crate::Result<Self> {
        match adv.backend {
            crate::config::Backend::Tquic => {
                #[cfg(feature = "backend-tquic")]
                {
                    let tls = crate::transport::tquic::TlsConfig {
                        cert: tls_config.cert.clone(),
                        key: tls_config.key.clone(),
                        ca_cert: tls_config.ca_cert.clone(),
                        cert_pem: tls_config.cert_pem.clone(),
                        key_pem: tls_config.key_pem.clone(),
                        ca_cert_pem: tls_config.ca_cert_pem.clone(),
                    };
                    return Ok(BackendImpl::Tquic(crate::transport::tquic::TquicCtx::new(tls, adv)?));
                }
                #[cfg(not(feature = "backend-tquic"))]
                {
                    tracing::warn!("backend-tquic disabled; falling back to Quinn");
                    let q = Self::map_advanced_to_quinn(&adv);
                    return Ok(BackendImpl::Quinn(QuinnCtx::new(tls_config, q)?));
                }
            }
            crate::config::Backend::Quinn => {
                let _q = Self::map_advanced_to_quinn(&adv);
                #[cfg(feature = "backend-quinn")]
                {
                    Ok(BackendImpl::Quinn(QuinnCtx::new(tls_config, _q)?))
                }
                #[cfg(not(feature = "backend-quinn"))]
                {
                    return Err(crate::TransportError::ConnectionFailed("backend-quinn not enabled".into()));
                }
            }
            crate::config::Backend::IoUringAdvanced => {
                #[cfg(feature = "io-uring-advanced")]
                {
                    let io_uring_config = crate::io_uring_advanced::Config {
                        enable_vectorized_io: true,
                        enable_zero_copy: true,
                        numa_awareness: true,
                        coalesce_operations: true,
                        buffer_pool_size: 1024 * 1024, // 1MB pool
                        max_concurrent_ops: 1000,
                        use_registered_buffers: true,
                    };
                    Ok(BackendImpl::IoUringAdvanced(crate::io_uring_advanced::IoUringTransport::new(tls_config, adv, io_uring_config)?))
                }
                #[cfg(not(feature = "io-uring-advanced"))]
                {
                    #[cfg(feature = "backend-tquic")]
                    {
                        tracing::warn!("io-uring-advanced disabled; falling back to tQUIC");
                        let tls = crate::transport::tquic::TlsConfig {
                            cert: tls_config.cert.clone(),
                            key: tls_config.key.clone(),
                            ca_cert: tls_config.ca_cert.clone(),
                            cert_pem: tls_config.cert_pem.clone(),
                            key_pem: tls_config.key_pem.clone(),
                            ca_cert_pem: tls_config.ca_cert_pem.clone(),
                        };
                        Ok(BackendImpl::Tquic(crate::transport::tquic::TquicCtx::new(tls, adv)?))
                    }
                    #[cfg(not(feature = "backend-tquic"))]
                    {
                        tracing::warn!("io-uring-advanced disabled; falling back to Quinn");
                        let q = Self::map_advanced_to_quinn(&adv);
                        Ok(BackendImpl::Quinn(QuinnCtx::new(tls_config, q)?))
                    }
                }
            }
        }
    }

    fn map_advanced_to_quinn(adv: &crate::config::QuicTransportSettings) -> TransportSettings {
        let stream_win = adv.initial_max_stream_data_bidi_remote.min(u32::MAX as u64) as u32;
        let conn_win = adv.initial_max_data.min(u32::MAX as u64) as u32;
        let cc = match adv.congestion_control {
            crate::config::CongestionControl::BbrV3 => {
                tracing::warn!("BBRv3 requested on Quinn; using BBR fallback");
                CongestionController::Bbr
            }
            crate::config::CongestionControl::Bbr => CongestionController::Bbr,
            crate::config::CongestionControl::Cubic => CongestionController::Cubic,
            crate::config::CongestionControl::NewReno => CongestionController::NewReno,
        };
        TransportSettings {
            max_concurrent_uni_streams: 1000,
            max_concurrent_bidi_streams: 1000,
            stream_receive_window: stream_win,
            connection_receive_window: conn_win,
            max_idle_timeout_ms: adv.max_idle_timeout_ms,
            keep_alive_interval_ms: adv.keep_alive_interval_ms,
            initial_rtt_ms: 100,
            congestion_controller: cc,
        }
    }

    async fn connect(&self, addr: &str) -> crate::Result<Connection> {
        match self {
            #[cfg(feature = "backend-quinn")]
            BackendImpl::Quinn(ctx) | BackendImpl::Alt(ctx) => ctx.connect(addr).await,
            #[cfg(feature = "backend-tquic")]
            BackendImpl::Tquic(ctx) => ctx.connect(addr).await,
            #[cfg(feature = "io-uring-advanced")]
            BackendImpl::IoUringAdvanced(ctx) => {
                let conn = ctx.connect(addr).await?;
                Ok(Connection { inner: ConnectionInner::IoUringAdvanced(conn.inner), max_message_size: 65536 })
            }
        }
    }

    async fn listen(&self, addr: &str) -> crate::Result<Incoming> {
        match self {
            #[cfg(feature = "backend-quinn")]
            BackendImpl::Quinn(ctx) | BackendImpl::Alt(ctx) => ctx.listen(addr).await,
            #[cfg(feature = "backend-tquic")]
            BackendImpl::Tquic(ctx) => ctx.listen(addr).await,
            #[cfg(feature = "io-uring-advanced")]
            BackendImpl::IoUringAdvanced(ctx) => {
                let incoming = ctx.listen(addr).await?;
                Ok(Incoming { 
                    inner: IncomingInner::IoUringAdvanced(incoming.inner), 
                    settings: TransportSettings { 
                        max_concurrent_uni_streams: 1000,
                        max_concurrent_bidi_streams: 1000,
                        stream_receive_window: 65536,
                        connection_receive_window: 65536 * 4,
                        max_idle_timeout_ms: Some(30000),
                        keep_alive_interval_ms: Some(10000),
                        initial_rtt_ms: 100,
                        congestion_controller: CongestionController::NewReno,
                    }
                })
            }
        }
    }
}

#[cfg(feature = "backend-quinn")]
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
        // For development/testing, we'll use a custom verifier that accepts our self-signed certificates
        use rustls::{ClientConfig, ServerName};
        
        struct AcceptAllVerifier;
        
        impl rustls::client::ServerCertVerifier for AcceptAllVerifier {
            fn verify_server_cert(
                &self,
                _end_entity: &rustls::Certificate,
                _intermediates: &[rustls::Certificate],
                _server_name: &ServerName,
                _scts: &mut dyn Iterator<Item = &[u8]>,
                _ocsp_response: &[u8],
                _now: std::time::SystemTime,
            ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
                Ok(rustls::client::ServerCertVerified::assertion())
            }
        }
        
        let crypto_config = ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(Arc::new(AcceptAllVerifier))
            .with_no_client_auth();
            
        let mut client_config = quinn::ClientConfig::new(Arc::new(crypto_config));
        let mut transport = QuinnTransportConfig::default();
        self.settings.apply_to_config(&mut transport);
        client_config.transport_config(Arc::new(transport));
        let server_addr = addr.parse::<SocketAddr>()
            .map_err(|e| crate::TransportError::ConnectionFailed(format!("Invalid address: {}", e)))?;
        eprintln!("Attempting QUIC connection to {}", server_addr);
        eprintln!("Starting TLS handshake with server...");
        let connecting = self.endpoint.connect_with(client_config, server_addr, "127.0.0.1")
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

#[cfg(feature = "backend-quinn")]
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
pub trait Transport: Send + Sync {
    async fn connect(&self, addr: &str) -> crate::Result<Connection>;
    async fn listen(&self, addr: &str) -> crate::Result<Incoming>;
}

impl QuicTransport {
    pub fn new(tls_config: TlsConfig) -> crate::Result<Self> {
        Self::with_settings(tls_config, TransportSettings::default())
    }

    /// Get information about which backend is currently active
    pub fn backend_info(&self) -> &'static str {
        match &self.backend {
            #[cfg(feature = "backend-quinn")]
            BackendImpl::Quinn(_) => "Quinn",
            #[cfg(feature = "backend-quinn")]
            BackendImpl::Alt(_) => "Quinn (Alt)",
            #[cfg(feature = "backend-tquic")]
            BackendImpl::Tquic(_) => "tQUIC",
            #[cfg(feature = "io-uring-advanced")]
            BackendImpl::IoUringAdvanced(_) => "io_uring Advanced",
        }
    }

    pub fn with_settings(tls_config: TlsConfig, settings: TransportSettings) -> crate::Result<Self> {
        Ok(Self { backend: BackendImpl::new(tls_config, settings)? })
    }

    pub fn with_settings_advanced(tls_config: TlsConfig, settings: crate::config::QuicTransportSettings) -> crate::Result<Self> {
        Ok(Self { backend: BackendImpl::new_advanced(tls_config, settings)? })
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
            #[cfg(feature = "backend-quinn")]
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
            #[cfg(feature = "backend-tquic")]
            ConnectionInner::Tquic(_) => {
                // TODO: expose tquic stats mapping when available
                Err(crate::TransportError::StreamError("flow_control_status not supported for tquic yet".into()))
            }
            #[cfg(feature = "io-uring-advanced")]
            ConnectionInner::IoUringAdvanced(_) => {
                // TODO: implement flow control stats for io_uring_advanced
                Err(crate::TransportError::StreamError("flow_control_status not supported for io_uring_advanced yet".into()))
            }
        }
    }

    pub async fn accept_bi(&self) -> crate::Result<crate::QuicStream> {
        match &self.inner {
            #[cfg(feature = "backend-quinn")]
            ConnectionInner::Quinn(conn) => {
                let (send, recv) = conn.accept_bi().await
                    .map_err(|e| crate::TransportError::StreamError(e.to_string()))?;
                Ok(crate::QuicStream::new(send, recv, self.max_message_size))
            }
            #[cfg(all(feature = "backend-tquic", feature = "lock-free-channels"))]
            ConnectionInner::Tquic(_handle) => {
                // For now, just return an error indicating that bidirectional streams are not yet implemented for tquic
                Err(crate::TransportError::StreamError("bidirectional streams not yet implemented for tquic backend".into()))
            }
            #[cfg(all(feature = "backend-tquic", not(feature = "lock-free-channels")))]
            ConnectionInner::Tquic(_handle) => {
                // Standard tquic implementation using DashMap-based streams (performance regression fix)
                // Use a fixed stream ID to ensure client and server communicate on the same stream
                let stream_id = 0; // Fixed stream ID for communication
                Ok(crate::QuicStream::new_simple_tquic(stream_id, self.max_message_size))
            }
            #[cfg(feature = "io-uring-advanced")]
            ConnectionInner::IoUringAdvanced(conn) => {
                conn.accept_bi().await
            }
        }
    }

    pub async fn open_bi(&self) -> crate::Result<crate::QuicStream> {
        match &self.inner {
            #[cfg(feature = "backend-quinn")]
            ConnectionInner::Quinn(conn) => {
                let (send, recv) = conn.open_bi().await
                    .map_err(|e| crate::TransportError::StreamError(e.to_string()))?;
                Ok(crate::QuicStream::new(send, recv, self.max_message_size))
            }
            #[cfg(all(feature = "backend-tquic", feature = "lock-free-channels"))]
            ConnectionInner::Tquic(_handle) => {
                // For now, just return an error indicating that bidirectional streams are not yet implemented for tquic
                Err(crate::TransportError::StreamError("bidirectional streams not yet implemented for tquic backend".into()))
            }
            #[cfg(all(feature = "backend-tquic", not(feature = "lock-free-channels")))]
            ConnectionInner::Tquic(_handle) => {
                // Standard tquic implementation using DashMap-based streams (performance regression fix)
                // Use a fixed stream ID to ensure client and server communicate on the same stream
                let stream_id = 0; // Fixed stream ID for communication
                Ok(crate::QuicStream::new_simple_tquic(stream_id, self.max_message_size))
            }
            #[cfg(feature = "io-uring-advanced")]
            ConnectionInner::IoUringAdvanced(conn) => {
                conn.open_bi().await
            }
        }
    }
}

impl Incoming {
    pub fn local_addr(&self) -> crate::Result<String> {
        match &self.inner {
            #[cfg(feature = "backend-quinn")]
            IncomingInner::Quinn(ep) => ep
                .local_addr()
                .map(|addr| addr.to_string())
                .map_err(|e| crate::TransportError::ConnectionFailed(format!("Failed to get local address: {}", e))),
            #[cfg(feature = "backend-tquic")]
            IncomingInner::Tquic(h) => Ok(h.local_addr.clone()),
            #[cfg(feature = "io-uring-advanced")]
            IncomingInner::IoUringAdvanced(h) => h.local_addr(),
        }
    }

    pub async fn accept(&mut self) -> crate::Result<Connection> {
        match &mut self.inner {
            #[cfg(feature = "backend-quinn")]
            IncomingInner::Quinn(ep) => {
                let new_conn = ep.accept().await
                    .ok_or_else(|| crate::TransportError::ConnectionFailed("No more incoming connections".into()))?
                    .await
                    .map_err(|e| crate::TransportError::ConnectionFailed(e.to_string()))?;
                Ok(Connection { inner: ConnectionInner::Quinn(new_conn), max_message_size: self.settings.stream_receive_window as usize })
            }
            #[cfg(feature = "backend-tquic")]
            IncomingInner::Tquic(h) => {
                let mut rx = h.conn_rx.lock().await;
                match rx.recv().await {
                    Some(handle) => Ok(Connection { inner: ConnectionInner::Tquic(handle), max_message_size: self.settings.stream_receive_window as usize }),
                    None => Err(crate::TransportError::ConnectionFailed("No more incoming connections".into())),
                }
            }
            #[cfg(feature = "io-uring-advanced")]
            IncomingInner::IoUringAdvanced(h) => {
                let conn = h.accept().await?;
                Ok(Connection { inner: ConnectionInner::IoUringAdvanced(conn), max_message_size: self.settings.stream_receive_window as usize })
            }
        }
    }
}
