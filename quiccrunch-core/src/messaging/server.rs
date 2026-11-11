//! Server-side dispatch for the versioned backend messaging contract.
//!
//! `MessagingServer` owns the bounded watch retention store
//! ([`PolicyStore`](super::retention::PolicyStore)), request dedupe and
//! admission control. Business logic lives behind [`MessagingService`].

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;

use crate::interceptor::InterceptorChain;
use crate::rpc::RpcContext;

use super::retention::{PolicyStore, SubscriberBudget, SubscriberId, WatchSubscription};
use super::{
    encode, BackpressureConfig, Body, DedupeConfig, EventIngest, EventIngested, Heartbeat,
    HeartbeatAck, LeaseAcquire, LeaseAcquired, LeaseRelease, LeaseReleased, Message,
    MessagingError, MessagingMetrics, PeerIdentity, PolicyBundle, PolicyGet, PolicyWatchEventDto,
    RequestPayload, ResponsePayload, Result, CONTRACT_VERSION,
};

/// Business logic for the WAF <-> distri-kv messaging surface.
#[async_trait]
pub trait MessagingService: Send + Sync {
    async fn lease_acquire(&self, ctx: &RequestContext, req: LeaseAcquire)
        -> Result<LeaseAcquired>;
    async fn lease_release(&self, ctx: &RequestContext, req: LeaseRelease)
        -> Result<LeaseReleased>;
    async fn policy_get(&self, ctx: &RequestContext, req: PolicyGet) -> Result<PolicyBundle>;
    async fn event_ingest(&self, ctx: &RequestContext, req: EventIngest) -> Result<EventIngested>;
    async fn heartbeat(&self, ctx: &RequestContext, req: Heartbeat) -> Result<HeartbeatAck>;

    /// Store backing `PolicyGet`. When provided, [`MessagingServer::new`] uses
    /// it for watch retention so snapshots and replay agree. Backends that do
    /// not expose a store must use [`MessagingServer::with_policy_store`].
    fn policy_store(&self) -> Option<Arc<PolicyStore>> {
        None
    }
}

#[derive(Debug, Clone)]
pub struct RequestContext {
    pub identity: PeerIdentity,
    pub metadata: HashMap<String, String>,
    pub request_id: u64,
}

/// Per-method RBAC plus identity requirements.
#[derive(Debug, Clone, Default)]
pub struct AuthorizationPolicy {
    /// Require a transport-established peer identity.
    pub require_identity: bool,
    /// Accept the identity asserted inside the envelope when the transport
    /// cannot supply one. Development only; leaves identity spoofable.
    pub allow_untrusted_identity: bool,
    /// Member-facing method name -> roles, any one of which is sufficient.
    pub required_roles: HashMap<String, Vec<String>>,
}

impl AuthorizationPolicy {
    pub fn allow_role(mut self, method: &str, role: &str) -> Self {
        self.required_roles
            .entry(method.to_string())
            .or_default()
            .push(role.to_string());
        self
    }

    fn authorize(&self, method: &str, identity: &PeerIdentity) -> Result<()> {
        let Some(roles) = self.required_roles.get(method) else {
            return Ok(());
        };
        if roles.is_empty() {
            return Ok(());
        }
        if roles
            .iter()
            .any(|role| identity.roles.iter().any(|r| r == role))
        {
            Ok(())
        } else {
            Err(MessagingError::PermissionDenied(format!(
                "method {method} requires one of {roles:?}"
            )))
        }
    }
}

/// Result of dispatching one request frame.
pub enum Dispatch {
    /// Zero or more complete frames (unary calls).
    Frames(Vec<Vec<u8>>),
    /// A live watch. The caller writes frames until [`WatchStream::next`]
    /// returns `None`.
    Watch(WatchStream),
}

/// A live watch stream produced by the server.
pub struct WatchStream {
    subscription: WatchSubscription,
    request_id: u64,
    next_message_id: Arc<AtomicU64>,
    finished: bool,
}

impl WatchStream {
    /// Next encoded frame, or `None` when the watch is finished (a
    /// `StreamEnd`/terminal error frame has already been returned).
    pub async fn next(&mut self) -> Option<Vec<u8>> {
        if self.finished {
            return None;
        }
        let id = self.next_message_id.fetch_add(1, Ordering::Relaxed);
        match self.subscription.next().await {
            Some(Ok(event)) => encode(&Message::request(
                id,
                self.request_id,
                Body::WatchEvent(event),
            ))
            .ok(),
            Some(Err(err)) => {
                self.finished = true;
                encode(&Message::request(id, self.request_id, error_body(&err))).ok()
            }
            None => {
                self.finished = true;
                encode(&Message::request(
                    id,
                    self.request_id,
                    Body::StreamEnd("complete".into()),
                ))
                .ok()
            }
        }
    }

    pub fn is_lagged(&self) -> bool {
        self.subscription.is_lagged()
    }
}

struct DedupeCache {
    capacity: usize,
    order: VecDeque<(String, u64)>,
    entries: HashMap<(String, u64), ResponsePayload>,
}

impl DedupeCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            order: VecDeque::new(),
            entries: HashMap::new(),
        }
    }

    fn get(&self, key: &(String, u64)) -> Option<ResponsePayload> {
        self.entries.get(key).cloned()
    }

    fn insert(&mut self, key: (String, u64), response: ResponsePayload) {
        if self.entries.contains_key(&key) {
            return;
        }
        while self.entries.len() >= self.capacity {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, response);
    }
}

/// Server configuration and dispatch engine.
pub struct MessagingServer {
    service: Arc<dyn MessagingService>,
    chain: Arc<InterceptorChain>,
    authz: AuthorizationPolicy,
    next_message_id: Arc<AtomicU64>,
    store: Arc<PolicyStore>,
    metrics: Arc<MessagingMetrics>,
    backpressure: BackpressureConfig,
    dedupe: Mutex<DedupeCache>,
    ingest_in_flight: AtomicUsize,
}

impl MessagingServer {
    pub fn new(service: Arc<dyn MessagingService>) -> Self {
        let store = service.policy_store().unwrap_or_else(|| {
            Arc::new(PolicyStore::new(
                Default::default(),
                Arc::new(MessagingMetrics::default()),
            ))
        });
        let metrics = store.metrics();
        Self {
            service,
            chain: Arc::new(InterceptorChain::new()),
            authz: AuthorizationPolicy::default(),
            next_message_id: Arc::new(AtomicU64::new(1)),
            store,
            metrics,
            backpressure: BackpressureConfig::default(),
            dedupe: Mutex::new(DedupeCache::new(DedupeConfig::default().capacity)),
            ingest_in_flight: AtomicUsize::new(0),
        }
    }

    /// Use a store shared with the backing service (and, in tests, with other
    /// nodes) so `PolicyGet` snapshots and watch replay agree.
    pub fn with_policy_store(
        service: Arc<dyn MessagingService>,
        store: Arc<PolicyStore>,
        metrics: Arc<MessagingMetrics>,
    ) -> Self {
        Self {
            service,
            chain: Arc::new(InterceptorChain::new()),
            authz: AuthorizationPolicy::default(),
            next_message_id: Arc::new(AtomicU64::new(1)),
            store,
            metrics,
            backpressure: BackpressureConfig::default(),
            dedupe: Mutex::new(DedupeCache::new(DedupeConfig::default().capacity)),
            ingest_in_flight: AtomicUsize::new(0),
        }
    }

    pub fn with_interceptors(mut self, chain: InterceptorChain) -> Self {
        self.chain = Arc::new(chain);
        self
    }

    pub fn with_authorization(mut self, authz: AuthorizationPolicy) -> Self {
        self.authz = authz;
        self
    }

    pub fn with_backpressure(mut self, backpressure: BackpressureConfig) -> Self {
        self.backpressure = backpressure;
        self
    }

    pub fn with_dedupe(mut self, config: DedupeConfig) -> Self {
        self.dedupe = Mutex::new(DedupeCache::new(config.capacity));
        self
    }

    pub fn interceptors(&self) -> &Arc<InterceptorChain> {
        &self.chain
    }

    pub fn policy_store(&self) -> &Arc<PolicyStore> {
        &self.store
    }

    pub fn metrics(&self) -> Arc<MessagingMetrics> {
        self.metrics.clone()
    }

    pub fn backpressure(&self) -> &BackpressureConfig {
        &self.backpressure
    }

    /// Budget matching this server's configured limits, for transports that
    /// want to create one budget per connection.
    pub fn new_subscriber_budget(&self) -> Arc<SubscriberBudget> {
        Arc::new(SubscriberBudget::new(
            self.backpressure.subscriber_queue_bytes,
            self.backpressure.max_watches_per_subscriber,
        ))
    }

    /// Server-side control-plane publish hook. The backing service must use
    /// the same store for `PolicyGet`.
    pub fn publish(
        &self,
        policy_id: impl Into<String>,
        bundle: Vec<u8>,
        checksum: impl Into<String>,
    ) -> PolicyWatchEventDto {
        self.store.publish(policy_id, bundle, checksum)
    }

    fn next_id(&self) -> u64 {
        self.next_message_id.fetch_add(1, Ordering::Relaxed)
    }

    async fn dispatch_one(
        &self,
        frame: &[u8],
        peer: Option<&PeerIdentity>,
        subscriber: SubscriberId,
        budget: Arc<SubscriberBudget>,
    ) -> Dispatch {
        match self.handle(frame, peer, subscriber, budget).await {
            Ok(dispatch) => dispatch,
            Err((request_id, err)) => {
                let message = Message::request(self.next_id(), request_id, error_body(&err));
                Dispatch::Frames(encode(&message).ok().into_iter().collect())
            }
        }
    }

    async fn handle(
        &self,
        frame: &[u8],
        peer: Option<&PeerIdentity>,
        subscriber: SubscriberId,
        budget: Arc<SubscriberBudget>,
    ) -> std::result::Result<Dispatch, (u64, MessagingError)> {
        let message = super::decode(frame).map_err(|e| (0, e))?;
        let request_id = message.request_id;

        let (asserted_identity, metadata, payload) = match message.body {
            Body::Request {
                identity,
                metadata,
                payload,
            } => (identity, metadata, payload),
            _ => {
                return Err((
                    request_id,
                    MessagingError::InvalidRequest("expected a request envelope".into()),
                ))
            }
        };

        let identity = match peer {
            Some(identity) => identity.clone(),
            None if self.authz.allow_untrusted_identity => asserted_identity,
            None if self.authz.require_identity => {
                return Err((
                    request_id,
                    MessagingError::Unauthorized(
                        "peer identity not established by transport".into(),
                    ),
                ))
            }
            None => PeerIdentity::default(),
        };

        let method = method_name(&payload).to_string();
        let mut context = RpcContext {
            service: "quiccrunch.messaging".into(),
            method: method.clone(),
            request_id,
            metadata: metadata.clone(),
            start_time: SystemTime::now(),
            timeout: None,
        };
        let mut params = Bytes::new();

        self.chain
            .before_request(&mut context, &mut params)
            .await
            .map_err(|e| (request_id, interceptor_error(e)))?;

        if let Err(err) = self.authz.authorize(&method, &identity) {
            let _ = self
                .chain
                .on_error(
                    &mut context,
                    &crate::RpcError::PermissionDenied(err.to_string()),
                )
                .await;
            return Err((request_id, err));
        }

        let ctx = RequestContext {
            identity: identity.clone(),
            metadata: metadata.clone(),
            request_id,
        };

        if let RequestPayload::PolicyWatch(req) = payload {
            let subscription = self
                .store
                .subscribe(
                    subscriber,
                    &req.policy_id,
                    req.since_version,
                    &self.backpressure,
                    budget,
                )
                .map_err(|e| (request_id, e))?;
            let mut body = Bytes::new();
            if let Err(e) = self.chain.after_response(&mut context, &mut body).await {
                let _ = self.chain.on_error(&mut context, &e).await;
                return Err((request_id, MessagingError::Service(e.to_string())));
            }
            return Ok(Dispatch::Watch(WatchStream {
                subscription,
                request_id,
                next_message_id: self.next_message_id.clone(),
                finished: false,
            }));
        }

        // Mutations are deduplicated by (peer, request_id) so a retry after an
        // ambiguous failure cannot apply twice inside the dedupe window.
        let dedupe_key = if is_mutation(&payload) {
            Some(identity_key(&ctx.identity, request_id))
        } else {
            None
        };
        if let Some(key) = &dedupe_key {
            if let Some(response) = self.dedupe.lock().expect("dedupe poisoned").get(key) {
                self.metrics
                    .dedupe_hits_total
                    .fetch_add(1, Ordering::Relaxed);
                return Ok(Dispatch::Frames(
                    encode(&Message::request(
                        self.next_id(),
                        request_id,
                        Body::Response(response),
                    ))
                    .ok()
                    .into_iter()
                    .collect(),
                ));
            }
        }

        let ingest_guard = if matches!(payload, RequestPayload::EventIngest(_)) {
            match self.enter_ingest() {
                Ok(guard) => Some(guard),
                Err(err) => return Err((request_id, err)),
            }
        } else {
            None
        };

        let result = match payload {
            RequestPayload::LeaseAcquire(req) => self
                .service
                .lease_acquire(&ctx, req)
                .await
                .map(ResponsePayload::LeaseAcquire),
            RequestPayload::LeaseRelease(req) => self
                .service
                .lease_release(&ctx, req)
                .await
                .map(ResponsePayload::LeaseRelease),
            RequestPayload::PolicyGet(req) => self
                .service
                .policy_get(&ctx, req)
                .await
                .map(ResponsePayload::PolicyGet),
            RequestPayload::EventIngest(req) => {
                let result = self
                    .service
                    .event_ingest(&ctx, req)
                    .await
                    .map(ResponsePayload::EventIngest);
                drop(ingest_guard);
                result
            }
            RequestPayload::Heartbeat(req) => self
                .service
                .heartbeat(&ctx, req)
                .await
                .map(ResponsePayload::Heartbeat),
            RequestPayload::PolicyWatch(_) => unreachable!("handled above"),
        };

        match result {
            Ok(payload) => {
                let mut body = Bytes::new();
                if let Err(e) = self.chain.after_response(&mut context, &mut body).await {
                    let _ = self.chain.on_error(&mut context, &e).await;
                    return Err((request_id, MessagingError::Service(e.to_string())));
                }
                if let Some(key) = dedupe_key {
                    self.dedupe
                        .lock()
                        .expect("dedupe poisoned")
                        .insert(key, payload.clone());
                }
                Ok(Dispatch::Frames(vec![encode(&Message::request(
                    self.next_id(),
                    request_id,
                    Body::Response(payload),
                ))
                .map_err(|e| (request_id, e))?]))
            }
            Err(err) => {
                let _ = self
                    .chain
                    .on_error(
                        &mut context,
                        &crate::RpcError::ServiceUnavailable(err.to_string()),
                    )
                    .await;
                Err((request_id, err))
            }
        }
    }

    fn enter_ingest(&self) -> Result<IngestGuard<'_>> {
        let in_flight = self.ingest_in_flight.fetch_add(1, Ordering::AcqRel) + 1;
        if in_flight > self.backpressure.max_in_flight_ingest.max(1) {
            self.ingest_in_flight.fetch_sub(1, Ordering::AcqRel);
            self.metrics
                .ingest_shed_total
                .fetch_add(1, Ordering::Relaxed);
            return Err(MessagingError::Backpressure {
                resource: "EventIngest".into(),
                retry_after_ms: self.backpressure.retry_after_ms,
            });
        }
        self.metrics
            .ingest_in_flight
            .store(in_flight as u64, Ordering::Relaxed);
        Ok(IngestGuard { server: self })
    }

    /// Dispatch a wire frame and return zero or more response frames. Each
    /// call gets its own subscriber budget; transports should call
    /// [`Self::dispatch_with_budget`] with one budget per connection.
    pub async fn dispatch(&self, frame: &[u8], peer: Option<&PeerIdentity>) -> Dispatch {
        let subscriber = self.next_id();
        let budget = Arc::new(SubscriberBudget::new(
            self.backpressure.subscriber_queue_bytes,
            self.backpressure.max_watches_per_subscriber,
        ));
        self.dispatch_one(frame, peer, subscriber, budget).await
    }

    /// Dispatch with a connection-scoped subscriber id and budget.
    pub async fn dispatch_with_budget(
        &self,
        frame: &[u8],
        peer: Option<&PeerIdentity>,
        subscriber: SubscriberId,
        budget: Arc<SubscriberBudget>,
    ) -> Dispatch {
        self.dispatch_one(frame, peer, subscriber, budget).await
    }

    /// Current contract version understood by this server.
    pub fn contract_version(&self) -> u16 {
        CONTRACT_VERSION
    }
}

struct IngestGuard<'a> {
    server: &'a MessagingServer,
}

impl Drop for IngestGuard<'_> {
    fn drop(&mut self) {
        let remaining = self
            .server
            .ingest_in_flight
            .fetch_sub(1, Ordering::AcqRel)
            .saturating_sub(1);
        self.server
            .metrics
            .ingest_in_flight
            .store(remaining as u64, Ordering::Relaxed);
    }
}

fn interceptor_error(err: crate::RpcError) -> MessagingError {
    match err {
        crate::RpcError::RateLimited(message) => MessagingError::RateLimited(message),
        crate::RpcError::PermissionDenied(message) => MessagingError::PermissionDenied(message),
        crate::RpcError::Timeout => MessagingError::RateLimited("request timeout".into()),
        other => MessagingError::Unauthorized(other.to_string()),
    }
}

fn identity_key(identity: &PeerIdentity, request_id: u64) -> (String, u64) {
    let node = if !identity.node_id.is_empty() {
        identity.node_id.clone()
    } else {
        identity.spiffe_id.clone()
    };
    (node, request_id)
}

fn is_mutation(payload: &RequestPayload) -> bool {
    matches!(
        payload,
        RequestPayload::LeaseAcquire(_)
            | RequestPayload::LeaseRelease(_)
            | RequestPayload::EventIngest(_)
    )
}

fn error_body(err: &MessagingError) -> Body {
    let (earliest_available, server_latest) = match err {
        MessagingError::ResumeTooOld { earliest_available } => (*earliest_available, 0),
        MessagingError::ResumeAhead {
            requested,
            server_latest,
        } => (*requested, *server_latest),
        _ => (0, 0),
    };
    Body::Error {
        code: err.error_code(),
        message: err.to_string(),
        retry_after_ms: err.retry_after_ms().unwrap_or(0),
        earliest_available,
        server_latest,
        retryable: err.is_retryable(),
    }
}

fn method_name(payload: &RequestPayload) -> &'static str {
    match payload {
        RequestPayload::LeaseAcquire(_) => "LeaseAcquire",
        RequestPayload::LeaseRelease(_) => "LeaseRelease",
        RequestPayload::PolicyGet(_) => "PolicyGet",
        RequestPayload::PolicyWatch(_) => "PolicyWatch",
        RequestPayload::EventIngest(_) => "EventIngest",
        RequestPayload::Heartbeat(_) => "Heartbeat",
    }
}

/// Simple reference implementation used by tests and as a starting point.
pub struct InMemoryService {
    pub leases: Mutex<HashMap<String, (u64, u64)>>,
    pub next_fencing_token: AtomicU64,
    pub store: Arc<PolicyStore>,
    pub ingested: AtomicU64,
    /// Number of mutations applied (after idempotency filtering).
    pub applied: AtomicU64,
    seen_events: Mutex<SeenEvents>,
}

struct SeenEvents {
    order: VecDeque<String>,
    set: HashSet<String>,
    capacity: usize,
}

impl Default for InMemoryService {
    fn default() -> Self {
        let metrics = Arc::new(MessagingMetrics::default());
        Self::with_store(Arc::new(PolicyStore::new(Default::default(), metrics)))
    }
}

impl InMemoryService {
    pub fn with_store(store: Arc<PolicyStore>) -> Self {
        Self {
            leases: Mutex::new(HashMap::new()),
            next_fencing_token: AtomicU64::new(0),
            store,
            ingested: AtomicU64::new(0),
            applied: AtomicU64::new(0),
            seen_events: Mutex::new(SeenEvents {
                order: VecDeque::new(),
                set: HashSet::new(),
                capacity: 4096,
            }),
        }
    }

    pub fn store(&self) -> &Arc<PolicyStore> {
        &self.store
    }
    /// Publish a policy version through the shared store.
    pub fn publish(
        &self,
        policy_id: impl Into<String>,
        bundle: Vec<u8>,
        checksum: impl Into<String>,
    ) -> PolicyWatchEventDto {
        self.store.publish(policy_id, bundle, checksum)
    }

    fn mark_event(&self, event_id: &str) -> bool {
        let mut seen = self.seen_events.lock().expect("seen events poisoned");
        if seen.set.contains(event_id) {
            return false;
        }
        while seen.set.len() >= seen.capacity {
            if let Some(old) = seen.order.pop_front() {
                seen.set.remove(&old);
            }
        }
        seen.order.push_back(event_id.to_string());
        seen.set.insert(event_id.to_string());
        true
    }
}

#[async_trait]
impl MessagingService for InMemoryService {
    async fn lease_acquire(
        &self,
        _ctx: &RequestContext,
        req: LeaseAcquire,
    ) -> Result<LeaseAcquired> {
        if req.ttl_ms == 0 {
            return Err(MessagingError::InvalidRequest("ttlMs must be > 0".into()));
        }
        let token = self.next_fencing_token.fetch_add(1, Ordering::SeqCst) + 1;
        let expires = now_ms() + req.ttl_ms as u64;
        self.leases
            .lock()
            .unwrap()
            .insert(req.lease_id.clone(), (token, expires));
        self.applied.fetch_add(1, Ordering::Relaxed);
        Ok(LeaseAcquired {
            granted: true,
            fencing_token: token,
            expires_at_unix_ms: expires,
            holder: req.resource,
        })
    }

    async fn lease_release(
        &self,
        _ctx: &RequestContext,
        req: LeaseRelease,
    ) -> Result<LeaseReleased> {
        let mut leases = self.leases.lock().unwrap();
        let released = match leases.get(&req.lease_id) {
            Some((token, _)) if *token == req.fencing_token => {
                leases.remove(&req.lease_id);
                true
            }
            _ => false,
        };
        self.applied.fetch_add(1, Ordering::Relaxed);
        Ok(LeaseReleased { released })
    }

    async fn policy_get(&self, _ctx: &RequestContext, req: PolicyGet) -> Result<PolicyBundle> {
        match self.store.latest(&req.policy_id) {
            Some(event) => Ok(PolicyBundle {
                policy_id: event.policy_id,
                version: event.version,
                bundle: event.bundle,
                checksum: event.checksum,
            }),
            None => Ok(PolicyBundle {
                policy_id: req.policy_id,
                version: 0,
                bundle: Vec::new(),
                checksum: String::new(),
            }),
        }
    }

    async fn event_ingest(&self, _ctx: &RequestContext, req: EventIngest) -> Result<EventIngested> {
        if req.events.len() > super::MAX_EVENTS_PER_INGEST {
            return Err(MessagingError::InvalidRequest("too many events".into()));
        }
        let mut accepted = 0u32;
        let mut duplicate = 0u32;
        for event in &req.events {
            if self.mark_event(&event.event_id) {
                accepted += 1;
            } else {
                duplicate += 1;
            }
        }
        self.ingested.fetch_add(accepted as u64, Ordering::Relaxed);
        self.applied.fetch_add(accepted as u64, Ordering::Relaxed);
        Ok(EventIngested {
            accepted,
            duplicate,
        })
    }

    async fn heartbeat(&self, _ctx: &RequestContext, req: Heartbeat) -> Result<HeartbeatAck> {
        Ok(HeartbeatAck {
            server_time_unix_ms: now_ms(),
            epoch: req.epoch,
        })
    }

    fn policy_store(&self) -> Option<Arc<PolicyStore>> {
        Some(self.store.clone())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
