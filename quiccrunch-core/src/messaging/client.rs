//! Rust client for the versioned backend messaging contract.
//!
//! The client provides three layers:
//!
//! * [`MessagingClient`] — one framed stream (unary and watch calls).
//! * [`MessagingPool`] — endpoint-keyed, bounded connection pool with
//!   reconnect/backoff, in-flight limits and idle eviction.
//! * [`PolicyWatcher`] — a self-healing watch that resumes from its last
//!   applied version and falls back to a full snapshot on `ResumeTooOld`.
//!
//! # Retry / idempotency contract
//!
//! Every request carries a client-generated `request_id`. Retries reuse that
//! id. The server deduplicates mutations (`LeaseAcquire`, `LeaseRelease`,
//! `EventIngest`) for a bounded window and replays the original response, so a
//! retry after an ambiguous failure is applied at most once within the window.
//! Outside the window the contract is **at-least-once**; mutations carry
//! stable keys (`lease_id`, `event_id`) so the backend can deduplicate.
//! Read-only calls (`PolicyGet`, `Heartbeat`) are always safe to retry.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use rand::Rng;
use tokio::sync::{mpsc, Mutex as AsyncMutex, RwLock, Semaphore};

use super::{
    encode, Body, Message, MessagingError, PeerIdentity, PolicyBundle, PolicyWatchEventDto,
    RequestPayload, ResponsePayload, Result,
};

/// Bidirectional frame transport. Implemented for tQUIC streams and for the
/// in-memory loopback used by tests.
#[async_trait]
pub trait FrameIo: Send {
    async fn send_frame(&mut self, frame: &[u8]) -> Result<()>;
    async fn recv_frame(&mut self) -> Result<Vec<u8>>;
}

/// In-memory duplex frame channel used by the loopback tests.
pub struct LoopbackIo {
    tx: mpsc::Sender<Vec<u8>>,
    rx: mpsc::Receiver<Vec<u8>>,
}

impl LoopbackIo {
    pub fn pair() -> (Self, Self) {
        Self::pair_with_capacity(64)
    }

    pub fn pair_with_capacity(capacity: usize) -> (Self, Self) {
        let (client_tx, server_rx) = mpsc::channel(capacity);
        let (server_tx, client_rx) = mpsc::channel(capacity);
        (
            Self {
                tx: client_tx,
                rx: client_rx,
            },
            Self {
                tx: server_tx,
                rx: server_rx,
            },
        )
    }
}

#[async_trait]
impl FrameIo for LoopbackIo {
    async fn send_frame(&mut self, frame: &[u8]) -> Result<()> {
        self.tx
            .send(frame.to_vec())
            .await
            .map_err(|e| MessagingError::Transport(e.to_string()))
    }

    async fn recv_frame(&mut self) -> Result<Vec<u8>> {
        self.rx
            .recv()
            .await
            .ok_or_else(|| MessagingError::Transport("loopback closed".into()))
    }
}

/// Client-side request/response and server-streaming driver for one stream.
pub struct MessagingClient {
    io: Box<dyn FrameIo>,
    identity: PeerIdentity,
    metadata: HashMap<String, String>,
    next_request_id: AtomicU64,
    rpc_timeout: Duration,
}

impl MessagingClient {
    pub fn new(io: Box<dyn FrameIo>, identity: PeerIdentity) -> Self {
        Self {
            io,
            identity,
            metadata: HashMap::new(),
            next_request_id: AtomicU64::new(1),
            rpc_timeout: Duration::from_secs(30),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.rpc_timeout = timeout;
        self
    }

    pub fn next_request_id(&self) -> u64 {
        self.next_request_id.fetch_add(1, Ordering::Relaxed)
    }

    fn build_request(&self, request_id: u64, payload: RequestPayload) -> Message {
        Message::request(
            request_id,
            request_id,
            Body::Request {
                identity: self.identity.clone(),
                metadata: self.metadata.clone(),
                payload,
            },
        )
    }

    async fn send_request_with_id(
        &mut self,
        payload: RequestPayload,
        request_id: u64,
    ) -> Result<u64> {
        let message = self.build_request(request_id, payload);
        let frame = encode(&message)?;
        self.io.send_frame(&frame).await?;
        Ok(request_id)
    }

    /// Send a request with an explicit id. Reusing the id across attempts is
    /// what makes mutation retries safe (server-side dedupe).
    pub async fn call_with_request_id(
        &mut self,
        payload: RequestPayload,
        request_id: u64,
    ) -> Result<ResponsePayload> {
        self.send_request_with_id(payload, request_id).await?;
        let message = self.recv_message(request_id).await?;
        match message.body {
            Body::Response(payload) => Ok(payload),
            Body::Error {
                code,
                message,
                retry_after_ms,
                earliest_available,
                server_latest,
                ..
            } => Err(map_error(
                code,
                message,
                retry_after_ms,
                earliest_available,
                server_latest,
            )),
            other => Err(MessagingError::Codec(format!(
                "unexpected response body: {other:?}"
            ))),
        }
    }

    async fn recv_message(&mut self, request_id: u64) -> Result<Message> {
        let message = tokio::time::timeout(self.rpc_timeout, self.recv_next())
            .await
            .map_err(|_| MessagingError::Retryable {
                message: format!("request {request_id} exceeded {:?}", self.rpc_timeout),
                retry_after_ms: 0,
            })??;
        if message.request_id != request_id {
            return Err(MessagingError::Codec(format!(
                "response request_id {} does not match {}",
                message.request_id, request_id
            )));
        }
        Ok(message)
    }

    /// Read the next message on the stream without request-id correlation.
    /// Used by live watches.
    pub async fn recv_next(&mut self) -> Result<Message> {
        let frame = self.io.recv_frame().await?;
        super::decode(&frame)
    }

    /// Perform a unary request/response call.
    pub async fn call(&mut self, payload: RequestPayload) -> Result<ResponsePayload> {
        let request_id = self.next_request_id();
        self.call_with_request_id(payload, request_id).await
    }

    /// Catch-up watch: read events until `StreamEnd`. This is the finite form
    /// used by loopback tests and by clients that poll.
    pub async fn watch(&mut self, payload: RequestPayload) -> Result<Vec<PolicyWatchEventDto>> {
        let request_id = self.next_request_id();
        self.send_request_with_id(payload, request_id).await?;
        let mut events = Vec::new();
        loop {
            let message = self.recv_message(request_id).await?;
            match message.body {
                Body::WatchEvent(event) => events.push(event),
                Body::StreamEnd(_) => return Ok(events),
                Body::Error {
                    code,
                    message,
                    retry_after_ms,
                    earliest_available,
                    server_latest,
                    ..
                } => {
                    return Err(map_error(
                        code,
                        message,
                        retry_after_ms,
                        earliest_available,
                        server_latest,
                    ))
                }
                other => {
                    return Err(MessagingError::Codec(format!(
                        "unexpected watch body: {other:?}"
                    )))
                }
            }
        }
    }

    /// Start a live watch and call `on_event` for each update. Returning
    /// `false` from the handler stops the client and closes the stream.
    pub async fn watch_until<F>(&mut self, payload: RequestPayload, mut on_event: F) -> Result<()>
    where
        F: FnMut(&PolicyWatchEventDto) -> bool,
    {
        let request_id = self.next_request_id();
        self.send_request_with_id(payload, request_id).await?;
        loop {
            let message = self.recv_message(request_id).await?;
            match message.body {
                Body::WatchEvent(event) => {
                    if !on_event(&event) {
                        return Ok(());
                    }
                }
                Body::StreamEnd(_) => return Ok(()),
                Body::Error {
                    code,
                    message,
                    retry_after_ms,
                    earliest_available,
                    server_latest,
                    ..
                } => {
                    return Err(map_error(
                        code,
                        message,
                        retry_after_ms,
                        earliest_available,
                        server_latest,
                    ))
                }
                other => {
                    return Err(MessagingError::Codec(format!(
                        "unexpected watch body: {other:?}"
                    )))
                }
            }
        }
    }
}

fn map_error(
    code: u16,
    message: String,
    retry_after_ms: u32,
    earliest_available: u64,
    server_latest: u64,
) -> MessagingError {
    match code {
        1 => MessagingError::UnsupportedVersion(0),
        2 => MessagingError::FrameTooLarge { size: 0, max: 0 },
        3 => MessagingError::Codec(message),
        6 => MessagingError::Unauthorized(message),
        7 => MessagingError::PermissionDenied(message),
        8 => MessagingError::RateLimited(message),
        9 => MessagingError::InvalidRequest(message),
        10 => MessagingError::Service(message),
        12 => MessagingError::ResumeTooOld { earliest_available },
        13 => MessagingError::ResumeAhead {
            requested: earliest_available,
            server_latest,
        },
        14 => MessagingError::Backpressure {
            resource: message,
            retry_after_ms,
        },
        15 => MessagingError::SlowConsumer(message),
        16 => MessagingError::WatchLimitExceeded(message),
        17 => MessagingError::Retryable {
            message,
            retry_after_ms,
        },
        _ => MessagingError::Transport(message),
    }
}

/// Drive an in-memory loopback server task for `MessagingServer::dispatch`
/// style testing without opening a socket.
pub fn spawn_loopback_server(
    mut server_io: LoopbackIo,
    server: Arc<super::server::MessagingServer>,
    peer: Option<PeerIdentity>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let budget = server.new_subscriber_budget();
        let subscriber = 1;
        while let Some(frame) = server_io.rx.recv().await {
            match server
                .dispatch_with_budget(&frame, peer.as_ref(), subscriber, budget.clone())
                .await
            {
                super::server::Dispatch::Frames(frames) => {
                    for response in frames {
                        if server_io.tx.send(response).await.is_err() {
                            return;
                        }
                    }
                }
                super::server::Dispatch::Watch(mut watch) => {
                    while let Some(frame) = watch.next().await {
                        if server_io.tx.send(frame).await.is_err() {
                            return;
                        }
                    }
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Connection pool
// ---------------------------------------------------------------------------

/// A pooled transport connection. Implementations must be safe to share.
#[async_trait]
pub trait PooledConnection: Send + Sync {
    async fn open_stream(&self) -> Result<Box<dyn FrameIo>>;
    async fn close(&self);
}

/// Opens transport connections for the pool. The tQUIC implementation lives in
/// [`super::transport::TquicConnector`].
#[async_trait]
pub trait Connector: Send + Sync {
    async fn connect(
        &self,
        address: &str,
        server_name: Option<&str>,
    ) -> Result<Box<dyn PooledConnection>>;
}

#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum distinct endpoints retained by the pool.
    pub max_connections: usize,
    /// Connections idle longer than this are closed and removed.
    pub idle_timeout: Duration,
    pub connect_timeout: Duration,
    pub rpc_timeout: Duration,
    /// How long a request waits for an in-flight slot before shedding.
    pub queue_wait: Duration,
    pub max_in_flight_per_connection: usize,
    pub max_connect_attempts: u32,
    pub backoff_base: Duration,
    pub backoff_max: Duration,
    /// Automatic retries for retryable failures (same request id).
    pub max_retries: u32,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 8,
            idle_timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(2),
            rpc_timeout: Duration::from_secs(10),
            queue_wait: Duration::from_millis(50),
            max_in_flight_per_connection: 64,
            max_connect_attempts: 5,
            backoff_base: Duration::from_millis(10),
            backoff_max: Duration::from_secs(1),
            max_retries: 3,
        }
    }
}

#[derive(Debug, Default)]
pub struct ClientMetrics {
    pub connections_opened: AtomicU64,
    pub connections_closed: AtomicU64,
    pub reconnects_total: AtomicU64,
    pub pool_evictions_total: AtomicU64,
    pub idle_evictions_total: AtomicU64,
    pub pool_size: AtomicU64,
    pub retries_total: AtomicU64,
    pub retryable_failures_total: AtomicU64,
    pub watch_resumes_total: AtomicU64,
    pub snapshot_fallbacks_total: AtomicU64,
}

impl ClientMetrics {
    pub fn snapshot(&self) -> ClientMetricsSnapshot {
        ClientMetricsSnapshot {
            connections_opened: self.connections_opened.load(Ordering::Relaxed),
            connections_closed: self.connections_closed.load(Ordering::Relaxed),
            reconnects_total: self.reconnects_total.load(Ordering::Relaxed),
            pool_evictions_total: self.pool_evictions_total.load(Ordering::Relaxed),
            idle_evictions_total: self.idle_evictions_total.load(Ordering::Relaxed),
            pool_size: self.pool_size.load(Ordering::Relaxed),
            retries_total: self.retries_total.load(Ordering::Relaxed),
            retryable_failures_total: self.retryable_failures_total.load(Ordering::Relaxed),
            watch_resumes_total: self.watch_resumes_total.load(Ordering::Relaxed),
            snapshot_fallbacks_total: self.snapshot_fallbacks_total.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClientMetricsSnapshot {
    pub connections_opened: u64,
    pub connections_closed: u64,
    pub reconnects_total: u64,
    pub pool_evictions_total: u64,
    pub idle_evictions_total: u64,
    pub pool_size: u64,
    pub retries_total: u64,
    pub retryable_failures_total: u64,
    pub watch_resumes_total: u64,
    pub snapshot_fallbacks_total: u64,
}

struct Endpoint {
    address: String,
    server_name: Option<String>,
    connection: AsyncMutex<Option<Arc<dyn PooledConnection>>>,
    in_flight: Semaphore,
    last_used: Mutex<Instant>,
    active: AtomicUsize,
}

impl Endpoint {
    fn new(address: String, server_name: Option<String>, max_in_flight: usize) -> Self {
        Self {
            address,
            server_name,
            connection: AsyncMutex::new(None),
            in_flight: Semaphore::new(max_in_flight.max(1)),
            last_used: Mutex::new(Instant::now()),
            active: AtomicUsize::new(0),
        }
    }

    fn touch(&self) {
        *self.last_used.lock().expect("last_used poisoned") = Instant::now();
    }

    fn idle_for(&self) -> Duration {
        self.last_used.lock().expect("last_used poisoned").elapsed()
    }
}

fn endpoint_key(address: &str, server_name: Option<&str>) -> String {
    match server_name {
        Some(name) => format!("{address}|{name}"),
        None => address.to_string(),
    }
}

fn backoff_delay(base: Duration, max: Duration, attempt: u32) -> Duration {
    let shift = attempt.saturating_sub(1).min(10);
    let exp = base.saturating_mul(1u32 << shift);
    let capped = exp.min(max);
    let jitter_ms = if capped.as_millis() == 0 {
        0
    } else {
        rand::thread_rng().gen_range(0..=capped.as_millis() as u64)
    };
    capped + Duration::from_millis(jitter_ms)
}

/// Endpoint-keyed, bounded connection pool with reconnect and backoff.
pub struct MessagingPool {
    connector: Arc<dyn Connector>,
    config: PoolConfig,
    endpoints: RwLock<HashMap<String, Arc<Endpoint>>>,
    metrics: Arc<ClientMetrics>,
    next_request_id: AtomicU64,
}

impl MessagingPool {
    pub fn new(connector: Arc<dyn Connector>, config: PoolConfig) -> Self {
        Self {
            connector,
            config,
            endpoints: RwLock::new(HashMap::new()),
            metrics: Arc::new(ClientMetrics::default()),
            next_request_id: AtomicU64::new(1),
        }
    }

    pub fn config(&self) -> &PoolConfig {
        &self.config
    }

    pub fn metrics(&self) -> Arc<ClientMetrics> {
        self.metrics.clone()
    }

    pub fn metrics_snapshot(&self) -> ClientMetricsSnapshot {
        self.metrics.snapshot()
    }

    pub async fn pool_size(&self) -> usize {
        self.endpoints.read().await.len()
    }

    fn next_request(&self) -> u64 {
        self.next_request_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Close and remove endpoints idle longer than `idle_timeout`.
    pub async fn reap_idle(&self) -> usize {
        let stale: Vec<(String, Arc<Endpoint>)> = {
            let endpoints = self.endpoints.read().await;
            endpoints
                .iter()
                .filter(|(_, endpoint)| endpoint.idle_for() >= self.config.idle_timeout)
                .map(|(key, endpoint)| (key.clone(), endpoint.clone()))
                .collect()
        };
        if stale.is_empty() {
            return 0;
        }
        let mut endpoints = self.endpoints.write().await;
        let mut removed = 0;
        for (key, endpoint) in stale {
            // Re-check under the write lock.
            if endpoints
                .get(&key)
                .is_some_and(|current| current.idle_for() >= self.config.idle_timeout)
            {
                endpoints.remove(&key);
                removed += 1;
                self.close_endpoint(&endpoint).await;
                self.metrics
                    .idle_evictions_total
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        self.metrics
            .pool_size
            .store(endpoints.len() as u64, Ordering::Relaxed);
        removed
    }

    async fn close_endpoint(&self, endpoint: &Endpoint) {
        let connection = endpoint.connection.lock().await.take();
        if let Some(connection) = connection {
            connection.close().await;
            self.metrics
                .connections_closed
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    async fn endpoint(&self, address: &str, server_name: Option<&str>) -> Result<Arc<Endpoint>> {
        let key = endpoint_key(address, server_name);
        if let Some(endpoint) = self.endpoints.read().await.get(&key) {
            return Ok(endpoint.clone());
        }

        let mut endpoints = self.endpoints.write().await;
        if let Some(endpoint) = endpoints.get(&key) {
            return Ok(endpoint.clone());
        }
        if endpoints.len() >= self.config.max_connections {
            // Evict the least-recently-used endpoint that is not active. If
            // every slot is busy, shed the new endpoint request instead of
            // silently closing a live connection.
            let victim = endpoints
                .iter()
                .filter(|(_, endpoint)| endpoint.active.load(Ordering::Relaxed) == 0)
                .min_by_key(|(_, endpoint)| *endpoint.last_used.lock().expect("last_used"))
                .map(|(key, _)| key.clone());
            match victim {
                Some(victim) => {
                    let evicted = endpoints.remove(&victim);
                    drop(endpoints);
                    if let Some(endpoint) = evicted {
                        self.close_endpoint(&endpoint).await;
                        self.metrics
                            .pool_evictions_total
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    endpoints = self.endpoints.write().await;
                }
                None => {
                    return Err(MessagingError::Backpressure {
                        resource: "connection_pool".into(),
                        retry_after_ms: self.config.queue_wait.as_millis() as u32,
                    });
                }
            }
        }
        let endpoint = Arc::new(Endpoint::new(
            address.to_string(),
            server_name.map(str::to_string),
            self.config.max_in_flight_per_connection,
        ));
        endpoints.insert(key, endpoint.clone());
        self.metrics
            .pool_size
            .store(endpoints.len() as u64, Ordering::Relaxed);
        Ok(endpoint)
    }

    async fn connection(&self, endpoint: &Arc<Endpoint>) -> Result<Arc<dyn PooledConnection>> {
        if let Some(connection) = endpoint.connection.lock().await.clone() {
            return Ok(connection);
        }

        // One handshake at a time per endpoint; callers queue on the mutex
        // instead of stampeding a restarting peer.
        let mut slot = endpoint.connection.lock().await;
        if let Some(connection) = slot.clone() {
            return Ok(connection);
        }
        let mut attempt = 0u32;
        loop {
            let result = tokio::time::timeout(
                self.config.connect_timeout,
                self.connector
                    .connect(&endpoint.address, endpoint.server_name.as_deref()),
            )
            .await;
            match result {
                Ok(Ok(connection)) => {
                    let connection: Arc<dyn PooledConnection> = Arc::from(connection);
                    *slot = Some(connection.clone());
                    self.metrics
                        .connections_opened
                        .fetch_add(1, Ordering::Relaxed);
                    self.metrics
                        .reconnects_total
                        .fetch_add(1, Ordering::Relaxed);
                    return Ok(connection);
                }
                Ok(Err(err)) => {
                    attempt += 1;
                    if attempt >= self.config.max_connect_attempts {
                        self.metrics
                            .retryable_failures_total
                            .fetch_add(1, Ordering::Relaxed);
                        return Err(MessagingError::Retryable {
                            message: format!(
                                "connect to {} failed after {attempt} attempts: {err}",
                                endpoint.address
                            ),
                            retry_after_ms: self.config.backoff_max.as_millis() as u32,
                        });
                    }
                }
                Err(_) => {
                    attempt += 1;
                    if attempt >= self.config.max_connect_attempts {
                        self.metrics
                            .retryable_failures_total
                            .fetch_add(1, Ordering::Relaxed);
                        return Err(MessagingError::Retryable {
                            message: format!(
                                "connect to {} timed out after {attempt} attempts",
                                endpoint.address
                            ),
                            retry_after_ms: self.config.backoff_max.as_millis() as u32,
                        });
                    }
                }
            }
            tokio::time::sleep(backoff_delay(
                self.config.backoff_base,
                self.config.backoff_max,
                attempt,
            ))
            .await;
        }
    }

    async fn invalidate(&self, endpoint: &Arc<Endpoint>) {
        let connection = endpoint.connection.lock().await.take();
        if let Some(connection) = connection {
            connection.close().await;
            self.metrics
                .connections_closed
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    async fn open_stream(&self, endpoint: &Arc<Endpoint>) -> Result<Box<dyn FrameIo>> {
        let connection = self.connection(endpoint).await?;
        match connection.open_stream().await {
            Ok(stream) => Ok(stream),
            Err(err) => {
                self.invalidate(endpoint).await;
                Err(MessagingError::Retryable {
                    message: format!("open stream failed: {err}"),
                    retry_after_ms: self.config.backoff_base.as_millis() as u32,
                })
            }
        }
    }

    /// Perform a unary call through the pool, reusing the pooled connection and
    /// retrying retryable failures with the same request id.
    pub async fn call(
        &self,
        address: &str,
        server_name: Option<&str>,
        identity: &PeerIdentity,
        metadata: &HashMap<String, String>,
        payload: RequestPayload,
    ) -> Result<ResponsePayload> {
        let request_id = self.next_request();
        self.call_with_request_id(
            address,
            server_name,
            identity,
            metadata,
            payload,
            request_id,
        )
        .await
    }

    /// Call with an explicit request id. Retries reuse it so server-side
    /// dedupe applies the mutation at most once.
    pub async fn call_with_request_id(
        &self,
        address: &str,
        server_name: Option<&str>,
        identity: &PeerIdentity,
        metadata: &HashMap<String, String>,
        payload: RequestPayload,
        request_id: u64,
    ) -> Result<ResponsePayload> {
        let endpoint = self.endpoint(address, server_name).await?;
        let mut attempt = 0u32;
        loop {
            let permit =
                match tokio::time::timeout(self.config.queue_wait, endpoint.in_flight.acquire())
                    .await
                {
                    Ok(Ok(permit)) => permit,
                    Ok(Err(_)) => {
                        return Err(MessagingError::Retryable {
                            message: "connection closed".into(),
                            retry_after_ms: 0,
                        })
                    }
                    Err(_) => {
                        return Err(MessagingError::Backpressure {
                            resource: "in_flight".into(),
                            retry_after_ms: self.config.queue_wait.as_millis() as u32,
                        })
                    }
                };
            endpoint.active.fetch_add(1, Ordering::Relaxed);
            endpoint.touch();
            let outcome = async {
                let stream = self.open_stream(&endpoint).await?;
                let mut client = MessagingClient::new(stream, identity.clone())
                    .with_timeout(self.config.rpc_timeout);
                for (key, value) in metadata {
                    client = client.with_metadata(key.clone(), value.clone());
                }
                client
                    .call_with_request_id(payload.clone(), request_id)
                    .await
            }
            .await;
            endpoint.active.fetch_sub(1, Ordering::Relaxed);
            drop(permit);

            match outcome {
                Ok(response) => return Ok(response),
                Err(err) if err.is_retryable() && attempt < self.config.max_retries => {
                    attempt += 1;
                    self.metrics.retries_total.fetch_add(1, Ordering::Relaxed);
                    if matches!(
                        err,
                        MessagingError::Transport(_) | MessagingError::Retryable { .. }
                    ) {
                        self.invalidate(&endpoint).await;
                    }
                    let delay = match err.retry_after_ms() {
                        Some(ms) if ms > 0 => Duration::from_millis(ms as u64),
                        _ => backoff_delay(
                            self.config.backoff_base,
                            self.config.backoff_max,
                            attempt,
                        ),
                    };
                    tokio::time::sleep(delay).await;
                }
                Err(err) => {
                    if err.is_retryable() {
                        self.metrics
                            .retryable_failures_total
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    return Err(err);
                }
            }
        }
    }

    /// Establish one watch stream. Returns the connected client and the
    /// stream's request id. The caller owns reconnection policy.
    pub async fn open_watch(
        &self,
        address: &str,
        server_name: Option<&str>,
        identity: &PeerIdentity,
        metadata: &HashMap<String, String>,
        payload: RequestPayload,
    ) -> Result<(MessagingClient, u64)> {
        let endpoint = self.endpoint(address, server_name).await?;
        let mut attempt = 0u32;
        loop {
            endpoint.touch();
            match self.open_stream(&endpoint).await {
                Ok(stream) => {
                    let mut client = MessagingClient::new(stream, identity.clone())
                        .with_timeout(self.config.rpc_timeout);
                    for (key, value) in metadata {
                        client = client.with_metadata(key.clone(), value.clone());
                    }
                    let request_id = client.next_request_id();
                    client
                        .send_request_with_id(payload.clone(), request_id)
                        .await?;
                    return Ok((client, request_id));
                }
                Err(err) if err.is_retryable() && attempt < self.config.max_connect_attempts => {
                    attempt += 1;
                    self.metrics
                        .watch_resumes_total
                        .fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(backoff_delay(
                        self.config.backoff_base,
                        self.config.backoff_max,
                        attempt,
                    ))
                    .await;
                }
                Err(err) => return Err(err),
            }
        }
    }

    async fn snapshot(
        &self,
        address: &str,
        server_name: Option<&str>,
        identity: &PeerIdentity,
        metadata: &HashMap<String, String>,
        policy_id: &str,
    ) -> Result<PolicyBundle> {
        match self
            .call(
                address,
                server_name,
                identity,
                metadata,
                RequestPayload::PolicyGet(super::PolicyGet {
                    policy_id: policy_id.to_string(),
                    known_version: 0,
                }),
            )
            .await?
        {
            ResponsePayload::PolicyGet(bundle) => Ok(bundle),
            other => Err(MessagingError::Codec(format!(
                "unexpected snapshot response: {other:?}"
            ))),
        }
    }
}

/// A self-healing policy watch.
///
/// `next_event()` transparently:
/// * resumes from the last applied version after a disconnect,
/// * falls back to a full `PolicyGet` snapshot on `ResumeTooOld` or when the
///   server is behind the client (server state loss),
/// * skips duplicate versions and treats a version gap as a snapshot trigger,
/// * backs off exponentially with jitter between attempts.
pub struct PolicyWatcher {
    pool: Arc<MessagingPool>,
    address: String,
    server_name: Option<String>,
    identity: PeerIdentity,
    metadata: HashMap<String, String>,
    policy_id: String,
    last_applied: u64,
    stream: Option<MessagingClient>,
    request_id: u64,
    attempt: u32,
    last_snapshot: Option<PolicyBundle>,
}

impl PolicyWatcher {
    pub fn new(
        pool: Arc<MessagingPool>,
        address: impl Into<String>,
        server_name: Option<String>,
        identity: PeerIdentity,
        metadata: HashMap<String, String>,
        policy_id: impl Into<String>,
        since_version: u64,
    ) -> Self {
        Self {
            pool,
            address: address.into(),
            server_name,
            identity,
            metadata,
            policy_id: policy_id.into(),
            last_applied: since_version,
            stream: None,
            request_id: 0,
            attempt: 0,
            last_snapshot: None,
        }
    }

    pub fn last_applied(&self) -> u64 {
        self.last_applied
    }

    pub fn last_snapshot(&self) -> Option<&PolicyBundle> {
        self.last_snapshot.as_ref()
    }

    pub fn policy_id(&self) -> &str {
        &self.policy_id
    }

    async fn refresh_snapshot(&mut self) -> Result<()> {
        let bundle = self
            .pool
            .snapshot(
                &self.address,
                self.server_name.as_deref(),
                &self.identity,
                &self.metadata,
                &self.policy_id,
            )
            .await?;
        // A snapshot is authoritative. If the server lost state and its
        // version moved backwards, follow it and document the reset.
        self.last_applied = bundle.version;
        self.last_snapshot = Some(bundle);
        self.pool
            .metrics
            .snapshot_fallbacks_total
            .fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn connect(&mut self) -> Result<()> {
        let payload = RequestPayload::PolicyWatch(super::PolicyWatch {
            policy_id: self.policy_id.clone(),
            since_version: self.last_applied,
        });
        let (client, request_id) = self
            .pool
            .open_watch(
                &self.address,
                self.server_name.as_deref(),
                &self.identity,
                &self.metadata,
                payload,
            )
            .await?;
        self.stream = Some(client);
        self.request_id = request_id;
        Ok(())
    }

    /// Wait for the next update. Returns an error only for non-retryable
    /// failures (authorization, malformed data); transport and resume failures
    /// are handled internally.
    pub async fn next_event(&mut self) -> Result<PolicyWatchEventDto> {
        loop {
            if self.stream.is_none() {
                if let Err(err) = self.connect().await {
                    if !err.is_retryable() {
                        return Err(err);
                    }
                    self.attempt += 1;
                    self.pool
                        .metrics
                        .watch_resumes_total
                        .fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(backoff_delay(
                        self.pool.config.backoff_base,
                        self.pool.config.backoff_max,
                        self.attempt,
                    ))
                    .await;
                    continue;
                }
                self.attempt = 0;
            }

            let message = match self
                .stream
                .as_mut()
                .expect("stream established")
                .recv_next()
                .await
            {
                Ok(message) => message,
                Err(err) if err.is_retryable() => {
                    self.stream = None;
                    self.attempt += 1;
                    self.pool
                        .metrics
                        .watch_resumes_total
                        .fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(backoff_delay(
                        self.pool.config.backoff_base,
                        self.pool.config.backoff_max,
                        self.attempt,
                    ))
                    .await;
                    continue;
                }
                Err(err) => return Err(err),
            };

            match message.body {
                Body::WatchEvent(event) => {
                    if event.version <= self.last_applied {
                        // Duplicate replay: already applied.
                        continue;
                    }
                    if event.version > self.last_applied + 1 {
                        // A gap means retention or ordering was violated.
                        // Re-sync instead of applying a partial sequence.
                        self.stream = None;
                        self.refresh_snapshot().await?;
                        continue;
                    }
                    self.last_applied = event.version;
                    return Ok(event);
                }
                Body::StreamEnd(_) => {
                    self.stream = None;
                    continue;
                }
                Body::Error {
                    code,
                    message,
                    retry_after_ms,
                    earliest_available,
                    server_latest,
                    ..
                } => {
                    let err = map_error(
                        code,
                        message,
                        retry_after_ms,
                        earliest_available,
                        server_latest,
                    );
                    self.stream = None;
                    match err {
                        MessagingError::ResumeTooOld { .. }
                        | MessagingError::ResumeAhead { .. } => {
                            self.refresh_snapshot().await?;
                            continue;
                        }
                        MessagingError::SlowConsumer(_)
                        | MessagingError::Transport(_)
                        | MessagingError::Retryable { .. } => {
                            self.attempt += 1;
                            self.pool
                                .metrics
                                .watch_resumes_total
                                .fetch_add(1, Ordering::Relaxed);
                            tokio::time::sleep(backoff_delay(
                                self.pool.config.backoff_base,
                                self.pool.config.backoff_max,
                                self.attempt,
                            ))
                            .await;
                            continue;
                        }
                        other => return Err(other),
                    }
                }
                other => {
                    return Err(MessagingError::Codec(format!(
                        "unexpected watch body: {other:?}"
                    )))
                }
            }
        }
    }
}
