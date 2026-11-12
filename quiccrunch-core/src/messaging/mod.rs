//! Versioned backend messaging contract for the WAF <-> distri-kv link.
//!
//! The wire format is a length-prefixed Cap'n Proto [`Envelope`]. The contract
//! version is explicit and checked before any payload is interpreted. All
//! variable-length fields are bounded locally before encoding and on decode via
//! Cap'n Proto traversal limits.

pub mod client;
pub mod retention;
pub mod server;
pub mod transport;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use capnp::message::ReaderOptions;
use capnp::serialize;

use crate::schema::messaging_capnp::{
    envelope, identity, key_value, policy_watch_event, request, response, Method,
};

/// Contract version carried on every envelope.
pub const CONTRACT_VERSION: u16 = 1;
/// Maximum encoded envelope size (4 MiB).
pub const MAX_ENVELOPE_BYTES: usize = 4 * 1024 * 1024;
/// Maximum policy bundle size (8 MiB).
pub const MAX_BUNDLE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum number of events accepted in one ingest call.
pub const MAX_EVENTS_PER_INGEST: usize = 1024;
/// Maximum metadata entries per message.
pub const MAX_METADATA_ENTRIES: usize = 64;
/// Cap'n Proto traversal limit in words (4 MiB / 8).
const MAX_TRAVERSAL_WORDS: usize = MAX_ENVELOPE_BYTES / 8;

/// Bounded retention for per-policy watch replay.
///
/// Every policy keeps a ring buffer of the last `max_entries` versions, capped
/// at `max_bytes` of bundle payload. When either bound is exceeded the oldest
/// entry is evicted and `watch_evictions_total` increments. A resume request
/// older than the oldest retained entry fails with
/// [`MessagingError::ResumeTooOld`]; the server never silently skips a version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionConfig {
    pub max_entries: usize,
    pub max_bytes: usize,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            max_entries: 1024,
            max_bytes: 4 * 1024 * 1024,
        }
    }
}

/// Bounded queue and admission limits for watch delivery and event ingest.
///
/// Policy: publishers never block. When a per-watch queue is full the server
/// closes that subscription with [`MessagingError::SlowConsumer`], increments
/// `watch_shed_total`, and the client resumes from its last applied version
/// (or falls back to a snapshot if retention has evicted it). `EventIngest`
/// is shed with [`MessagingError::Backpressure`] when the in-flight cap is
/// reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackpressureConfig {
    /// Max queued events per watch.
    pub watch_queue_entries: usize,
    /// Max queued bundle bytes per watch.
    pub watch_queue_bytes: usize,
    /// Max queued bundle bytes across all watches of one subscriber.
    pub subscriber_queue_bytes: usize,
    /// Max concurrent watches per subscriber (connection).
    pub max_watches_per_subscriber: usize,
    /// Max concurrent watch subscriptions server-wide.
    pub max_subscribers: usize,
    /// Max concurrent `EventIngest` calls before shedding.
    pub max_in_flight_ingest: usize,
    /// Advertised retry delay for shed work.
    pub retry_after_ms: u32,
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self {
            watch_queue_entries: 1024,
            watch_queue_bytes: 4 * 1024 * 1024,
            subscriber_queue_bytes: 8 * 1024 * 1024,
            max_watches_per_subscriber: 8,
            max_subscribers: 1024,
            max_in_flight_ingest: 64,
            retry_after_ms: 50,
        }
    }
}

/// Bounded request dedupe for mutations.
///
/// The client reuses the same `request_id` when it retries. The server caches
/// the response for `capacity` requests (oldest first) and replays it instead
/// of applying the mutation twice. Outside this window the contract is
/// at-least-once; mutations carry stable keys (`lease_id`, `event_id`) so the
/// backend can still deduplicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DedupeConfig {
    pub capacity: usize,
}

impl Default for DedupeConfig {
    fn default() -> Self {
        Self { capacity: 4096 }
    }
}

/// Server-side counters for retention, shedding and resume behaviour.
#[derive(Debug, Default)]
pub struct MessagingMetrics {
    pub watch_evictions_total: AtomicU64,
    pub watch_retained_entries: AtomicU64,
    pub watch_retained_bytes: AtomicU64,
    pub watch_shed_total: AtomicU64,
    pub watch_slow_consumer_total: AtomicU64,
    pub watch_queue_depth: AtomicU64,
    pub watch_queue_max_depth: AtomicU64,
    pub resume_too_old_total: AtomicU64,
    pub resume_ahead_total: AtomicU64,
    pub subscribers: AtomicU64,
    pub ingest_shed_total: AtomicU64,
    pub ingest_in_flight: AtomicU64,
    pub ingest_accepted_total: AtomicU64,
    pub dedupe_hits_total: AtomicU64,
}

impl MessagingMetrics {
    pub fn snapshot(&self) -> MessagingMetricsSnapshot {
        MessagingMetricsSnapshot {
            watch_evictions_total: self.watch_evictions_total.load(Ordering::Relaxed),
            watch_retained_entries: self.watch_retained_entries.load(Ordering::Relaxed),
            watch_retained_bytes: self.watch_retained_bytes.load(Ordering::Relaxed),
            watch_shed_total: self.watch_shed_total.load(Ordering::Relaxed),
            watch_slow_consumer_total: self.watch_slow_consumer_total.load(Ordering::Relaxed),
            watch_queue_depth: self.watch_queue_depth.load(Ordering::Relaxed),
            watch_queue_max_depth: self.watch_queue_max_depth.load(Ordering::Relaxed),
            resume_too_old_total: self.resume_too_old_total.load(Ordering::Relaxed),
            resume_ahead_total: self.resume_ahead_total.load(Ordering::Relaxed),
            subscribers: self.subscribers.load(Ordering::Relaxed),
            ingest_shed_total: self.ingest_shed_total.load(Ordering::Relaxed),
            ingest_in_flight: self.ingest_in_flight.load(Ordering::Relaxed),
            ingest_accepted_total: self.ingest_accepted_total.load(Ordering::Relaxed),
            dedupe_hits_total: self.dedupe_hits_total.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn retained(&self, delta_entries: i64, delta_bytes: i64) {
        apply_delta(&self.watch_retained_entries, delta_entries);
        apply_delta(&self.watch_retained_bytes, delta_bytes);
    }
}

fn apply_delta(counter: &AtomicU64, delta: i64) {
    if delta >= 0 {
        counter.fetch_add(delta as u64, Ordering::Relaxed);
    } else {
        counter.fetch_sub(delta.unsigned_abs(), Ordering::Relaxed);
    }
}

/// Point-in-time metrics, suitable for a Prometheus/OTel exporter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MessagingMetricsSnapshot {
    pub watch_evictions_total: u64,
    pub watch_retained_entries: u64,
    pub watch_retained_bytes: u64,
    pub watch_shed_total: u64,
    pub watch_slow_consumer_total: u64,
    pub watch_queue_depth: u64,
    pub watch_queue_max_depth: u64,
    pub resume_too_old_total: u64,
    pub resume_ahead_total: u64,
    pub subscribers: u64,
    pub ingest_shed_total: u64,
    pub ingest_in_flight: u64,
    pub ingest_accepted_total: u64,
    pub dedupe_hits_total: u64,
}

/// Byte size charged against retention/queue caps for one event.
pub fn event_cost(event: &PolicyWatchEventDto) -> usize {
    event.bundle.len() + event.policy_id.len() + event.checksum.len() + 32
}

/// Shared metrics handle.
pub type MetricsHandle = Arc<MessagingMetrics>;

#[derive(Debug, thiserror::Error)]
pub enum MessagingError {
    #[error("unsupported contract version: {0} (supported: {})", CONTRACT_VERSION)]
    UnsupportedVersion(u16),
    #[error("frame too large: {size} > {max}")]
    FrameTooLarge { size: usize, max: usize },
    #[error("malformed frame: {0}")]
    Codec(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("rate limited: {0}")]
    RateLimited(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("service error: {0}")]
    Service(String),
    #[error("transport error: {0}")]
    Transport(String),
    /// The requested resume point has been evicted from the bounded history.
    /// The client must fetch a full snapshot and restart from its version.
    #[error("resume too old: earliest available version is {earliest_available}")]
    ResumeTooOld { earliest_available: u64 },
    /// The client claims a version newer than the server (e.g. after server
    /// state loss). A full snapshot is required.
    #[error("resume ahead: requested {requested}, server latest {server_latest}")]
    ResumeAhead { requested: u64, server_latest: u64 },
    /// Admission control rejected the work; the caller may retry after the
    /// advertised delay. Nothing was applied.
    #[error("backpressure on {resource}: retry after {retry_after_ms}ms")]
    Backpressure {
        resource: String,
        retry_after_ms: u32,
    },
    /// A request whose outcome is unknown failed mid-flight. Retrying with the
    /// same request id is safe for mutations (server-side dedupe window).
    #[error("retryable failure: {message} (retry after {retry_after_ms}ms)")]
    Retryable {
        message: String,
        retry_after_ms: u32,
    },
    /// The subscriber could not keep up; the server closed the watch. The
    /// client resumes from its last applied version.
    #[error("slow consumer: {0}")]
    SlowConsumer(String),
    /// Per-subscriber watch limits or the global subscriber cap was reached.
    #[error("watch limit exceeded: {0}")]
    WatchLimitExceeded(String),
}

impl MessagingError {
    /// Whether a caller can safely retry the operation. Mutations are
    /// idempotent within the server dedupe window (see `DedupeConfig`).
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Transport(_)
                | Self::Backpressure { .. }
                | Self::Retryable { .. }
                | Self::SlowConsumer(_)
                | Self::Service(_)
        )
    }

    pub fn retry_after_ms(&self) -> Option<u32> {
        match self {
            Self::Backpressure { retry_after_ms, .. } | Self::Retryable { retry_after_ms, .. } => {
                Some(*retry_after_ms)
            }
            _ => None,
        }
    }

    pub fn error_code(&self) -> u16 {
        match self {
            Self::UnsupportedVersion(_) => 1,
            Self::FrameTooLarge { .. } => 2,
            Self::Codec(_) => 3,
            Self::Unauthorized(_) => 6,
            Self::PermissionDenied(_) => 7,
            Self::RateLimited(_) => 8,
            Self::InvalidRequest(_) => 9,
            Self::Service(_) => 10,
            Self::Transport(_) => 11,
            Self::ResumeTooOld { .. } => 12,
            Self::ResumeAhead { .. } => 13,
            Self::Backpressure { .. } => 14,
            Self::SlowConsumer(_) => 15,
            Self::WatchLimitExceeded(_) => 16,
            Self::Retryable { .. } => 17,
        }
    }
}

pub type Result<T> = std::result::Result<T, MessagingError>;

/// Peer identity established by the transport (mTLS / SPIFFE binding).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerIdentity {
    pub spiffe_id: String,
    pub node_id: String,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeaseAcquire {
    pub lease_id: String,
    pub resource: String,
    pub ttl_ms: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeaseAcquired {
    pub granted: bool,
    pub fencing_token: u64,
    pub expires_at_unix_ms: u64,
    pub holder: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeaseRelease {
    pub lease_id: String,
    pub fencing_token: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LeaseReleased {
    pub released: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolicyGet {
    pub policy_id: String,
    pub known_version: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolicyBundle {
    pub policy_id: String,
    pub version: u64,
    pub bundle: Vec<u8>,
    pub checksum: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolicyWatch {
    pub policy_id: String,
    pub since_version: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolicyWatchEventDto {
    pub policy_id: String,
    pub version: u64,
    pub bundle: Vec<u8>,
    pub checksum: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Event {
    pub event_id: String,
    pub kind: String,
    pub occurred_at_unix_ms: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventIngest {
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventIngested {
    pub accepted: u32,
    pub duplicate: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Heartbeat {
    pub node_id: String,
    pub epoch: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeartbeatAck {
    pub server_time_unix_ms: u64,
    pub epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestPayload {
    LeaseAcquire(LeaseAcquire),
    LeaseRelease(LeaseRelease),
    PolicyGet(PolicyGet),
    PolicyWatch(PolicyWatch),
    EventIngest(EventIngest),
    Heartbeat(Heartbeat),
}

impl RequestPayload {
    pub fn method(&self) -> Method {
        match self {
            Self::LeaseAcquire(_) => Method::LeaseAcquire,
            Self::LeaseRelease(_) => Method::LeaseRelease,
            Self::PolicyGet(_) => Method::PolicyGet,
            Self::PolicyWatch(_) => Method::PolicyWatch,
            Self::EventIngest(_) => Method::EventIngest,
            Self::Heartbeat(_) => Method::Heartbeat,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponsePayload {
    LeaseAcquire(LeaseAcquired),
    LeaseRelease(LeaseReleased),
    PolicyGet(PolicyBundle),
    EventIngest(EventIngested),
    Heartbeat(HeartbeatAck),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Request {
        identity: PeerIdentity,
        metadata: HashMap<String, String>,
        payload: RequestPayload,
    },
    Response(ResponsePayload),
    WatchEvent(PolicyWatchEventDto),
    StreamEnd(String),
    Error {
        code: u16,
        message: String,
        retry_after_ms: u32,
        earliest_available: u64,
        server_latest: u64,
        retryable: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub contract_version: u16,
    pub message_id: u64,
    pub request_id: u64,
    pub timestamp_ms: u64,
    pub body: Body,
}

impl Message {
    pub fn request(message_id: u64, request_id: u64, body: Body) -> Self {
        Self {
            contract_version: CONTRACT_VERSION,
            message_id,
            request_id,
            timestamp_ms: 0,
            body,
        }
    }
}

fn codec_err<E: std::fmt::Display>(err: E) -> MessagingError {
    MessagingError::Codec(err.to_string())
}

fn read_text(reader: capnp::text::Reader<'_>) -> Result<String> {
    reader.to_string().map_err(codec_err)
}

fn check_len(name: &str, len: usize, max: usize) -> Result<()> {
    if len > max {
        return Err(MessagingError::FrameTooLarge { size: len, max });
    }
    let _ = name;
    Ok(())
}

fn write_identity(mut builder: identity::Builder<'_>, id: &PeerIdentity) -> Result<()> {
    builder.set_spiffe_id(id.spiffe_id.as_str());
    builder.set_node_id(id.node_id.as_str());
    let mut roles = builder.reborrow().init_roles(id.roles.len() as u32);
    for (i, role) in id.roles.iter().enumerate() {
        roles.reborrow().set(i as u32, role.as_str());
    }
    Ok(())
}

fn write_metadata(
    mut builder: capnp::struct_list::Builder<'_, key_value::Owned>,
    metadata: &HashMap<String, String>,
) -> Result<()> {
    for (i, (key, value)) in metadata.iter().enumerate() {
        let mut entry = builder.reborrow().get(i as u32);
        entry.set_key(key.as_str());
        entry.set_value(value.as_str());
    }
    Ok(())
}

fn encode_request_payload(mut req: request::Builder<'_>, payload: &RequestPayload) -> Result<()> {
    match payload {
        RequestPayload::LeaseAcquire(p) => {
            let mut b = req.reborrow().init_lease_acquire();
            b.set_lease_id(p.lease_id.as_str());
            b.set_resource(p.resource.as_str());
            b.set_ttl_ms(p.ttl_ms);
        }
        RequestPayload::LeaseRelease(p) => {
            let mut b = req.reborrow().init_lease_release();
            b.set_lease_id(p.lease_id.as_str());
            b.set_fencing_token(p.fencing_token);
        }
        RequestPayload::PolicyGet(p) => {
            let mut b = req.reborrow().init_policy_get();
            b.set_policy_id(p.policy_id.as_str());
            b.set_known_version(p.known_version);
        }
        RequestPayload::PolicyWatch(p) => {
            let mut b = req.reborrow().init_policy_watch();
            b.set_policy_id(p.policy_id.as_str());
            b.set_since_version(p.since_version);
        }
        RequestPayload::EventIngest(p) => {
            check_len("events", p.events.len(), MAX_EVENTS_PER_INGEST)?;
            let mut b = req.reborrow().init_event_ingest();
            let mut events = b.reborrow().init_events(p.events.len() as u32);
            for (i, event) in p.events.iter().enumerate() {
                let mut e = events.reborrow().get(i as u32);
                e.set_event_id(event.event_id.as_str());
                e.set_kind(event.kind.as_str());
                e.set_occurred_at_unix_ms(event.occurred_at_unix_ms);
                e.set_payload(&event.payload);
            }
        }
        RequestPayload::Heartbeat(p) => {
            let mut b = req.reborrow().init_heartbeat();
            b.set_node_id(p.node_id.as_str());
            b.set_epoch(p.epoch);
        }
    }
    Ok(())
}

fn encode_response_payload(
    mut resp: response::Builder<'_>,
    payload: &ResponsePayload,
) -> Result<()> {
    match payload {
        ResponsePayload::LeaseAcquire(p) => {
            let mut b = resp.reborrow().init_lease_acquire();
            b.set_granted(p.granted);
            b.set_fencing_token(p.fencing_token);
            b.set_expires_at_unix_ms(p.expires_at_unix_ms);
            b.set_holder(p.holder.as_str());
        }
        ResponsePayload::LeaseRelease(p) => {
            let mut b = resp.reborrow().init_lease_release();
            b.set_released(p.released);
        }
        ResponsePayload::PolicyGet(p) => {
            check_len("bundle", p.bundle.len(), MAX_BUNDLE_BYTES)?;
            let mut b = resp.reborrow().init_policy_get();
            b.set_policy_id(p.policy_id.as_str());
            b.set_version(p.version);
            b.set_bundle(&p.bundle);
            b.set_checksum(p.checksum.as_str());
        }
        ResponsePayload::EventIngest(p) => {
            let mut b = resp.reborrow().init_event_ingest();
            b.set_accepted(p.accepted);
            b.set_duplicate(p.duplicate);
        }
        ResponsePayload::Heartbeat(p) => {
            let mut b = resp.reborrow().init_heartbeat();
            b.set_server_time_unix_ms(p.server_time_unix_ms);
            b.set_epoch(p.epoch);
        }
    }
    Ok(())
}

fn response_method(payload: &ResponsePayload) -> Method {
    match payload {
        ResponsePayload::LeaseAcquire(_) => Method::LeaseAcquire,
        ResponsePayload::LeaseRelease(_) => Method::LeaseRelease,
        ResponsePayload::PolicyGet(_) => Method::PolicyGet,
        ResponsePayload::EventIngest(_) => Method::EventIngest,
        ResponsePayload::Heartbeat(_) => Method::Heartbeat,
    }
}

fn encode_watch_event(
    envelope: &mut envelope::Builder<'_>,
    event: &PolicyWatchEventDto,
) -> Result<()> {
    check_len("bundle", event.bundle.len(), MAX_BUNDLE_BYTES)?;
    let mut b = envelope.reborrow().init_watch_event();
    b.set_policy_id(event.policy_id.as_str());
    b.set_version(event.version);
    b.set_bundle(&event.bundle);
    b.set_checksum(event.checksum.as_str());
    Ok(())
}

/// Encode a message as a length-prefixed Cap'n Proto frame.
pub fn encode(message: &Message) -> Result<Vec<u8>> {
    if message.contract_version != CONTRACT_VERSION {
        return Err(MessagingError::UnsupportedVersion(message.contract_version));
    }

    let mut builder = capnp::message::Builder::new_default();
    let mut env = builder.init_root::<envelope::Builder>();
    env.set_contract_version(message.contract_version);
    env.set_message_id(message.message_id);
    env.set_request_id(message.request_id);
    env.set_timestamp_ms(message.timestamp_ms);

    match &message.body {
        Body::Request {
            identity: peer,
            metadata,
            payload,
        } => {
            check_len("metadata", metadata.len(), MAX_METADATA_ENTRIES)?;
            env.set_method(payload.method());
            let mut req = env.reborrow().init_request();
            write_identity(req.reborrow().init_identity(), peer)?;
            write_metadata(
                req.reborrow().init_metadata(metadata.len() as u32),
                metadata,
            )?;
            encode_request_payload(req, payload)?;
        }
        Body::Response(payload) => {
            env.set_method(response_method(payload));
            let mut resp = env.reborrow().init_response();
            encode_response_payload(resp.reborrow(), payload)?;
            // Silence the unused-mut warning while keeping the builder API.
            let _ = &mut resp;
        }
        Body::WatchEvent(event) => {
            env.set_method(Method::PolicyWatch);
            encode_watch_event(&mut env, event)?;
        }
        Body::StreamEnd(reason) => {
            let mut b = env.reborrow().init_stream_end();
            b.set_reason(reason.as_str());
        }
        Body::Error {
            code,
            message: text,
            retry_after_ms,
            earliest_available,
            server_latest,
            retryable,
        } => {
            let mut b = env.reborrow().init_error();
            b.set_code(*code);
            b.set_message(text.as_str());
            b.set_retry_after_ms(*retry_after_ms);
            b.set_earliest_available(*earliest_available);
            b.set_server_latest(*server_latest);
            b.set_retryable(*retryable);
        }
    }

    let mut body = serialize::write_message_to_words(&builder);
    // Cap'n Proto requires 8-byte aligned segments; pad the flat frame.
    let padding = (8 - (body.len() % 8)) % 8;
    body.resize(body.len() + padding, 0);
    let len = body.len();
    if len > MAX_ENVELOPE_BYTES {
        return Err(MessagingError::FrameTooLarge {
            size: len,
            max: MAX_ENVELOPE_BYTES,
        });
    }
    let mut frame = Vec::with_capacity(4 + len);
    frame.extend_from_slice(&(len as u32).to_be_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

fn decode_identity(reader: identity::Reader<'_>) -> Result<PeerIdentity> {
    let spiffe_id = read_text(reader.get_spiffe_id().map_err(codec_err)?)?;
    let node_id = read_text(reader.get_node_id().map_err(codec_err)?)?;
    let roles_reader = reader.get_roles().map_err(codec_err)?;
    let mut roles = Vec::with_capacity(roles_reader.len() as usize);
    for role in roles_reader.iter() {
        roles.push(read_text(role.map_err(codec_err)?)?);
    }
    Ok(PeerIdentity {
        spiffe_id,
        node_id,
        roles,
    })
}

fn decode_metadata(
    reader: capnp::struct_list::Reader<'_, key_value::Owned>,
) -> Result<HashMap<String, String>> {
    check_len("metadata", reader.len() as usize, MAX_METADATA_ENTRIES)?;
    let mut metadata = HashMap::new();
    for entry in reader.iter() {
        let key = read_text(entry.get_key().map_err(codec_err)?)?;
        let value = read_text(entry.get_value().map_err(codec_err)?)?;
        metadata.insert(key, value);
    }
    Ok(metadata)
}

fn decode_request_payload(reader: request::Reader<'_>, method: Method) -> Result<RequestPayload> {
    Ok(match method {
        Method::LeaseAcquire => {
            let b = reader.get_lease_acquire().map_err(codec_err)?;
            RequestPayload::LeaseAcquire(LeaseAcquire {
                lease_id: read_text(b.get_lease_id().map_err(codec_err)?)?,
                resource: read_text(b.get_resource().map_err(codec_err)?)?,
                ttl_ms: b.get_ttl_ms(),
            })
        }
        Method::LeaseRelease => {
            let b = reader.get_lease_release().map_err(codec_err)?;
            RequestPayload::LeaseRelease(LeaseRelease {
                lease_id: read_text(b.get_lease_id().map_err(codec_err)?)?,
                fencing_token: b.get_fencing_token(),
            })
        }
        Method::PolicyGet => {
            let b = reader.get_policy_get().map_err(codec_err)?;
            RequestPayload::PolicyGet(PolicyGet {
                policy_id: read_text(b.get_policy_id().map_err(codec_err)?)?,
                known_version: b.get_known_version(),
            })
        }
        Method::PolicyWatch => {
            let b = reader.get_policy_watch().map_err(codec_err)?;
            RequestPayload::PolicyWatch(PolicyWatch {
                policy_id: read_text(b.get_policy_id().map_err(codec_err)?)?,
                since_version: b.get_since_version(),
            })
        }
        Method::EventIngest => {
            let b = reader.get_event_ingest().map_err(codec_err)?;
            let events_reader = b.get_events().map_err(codec_err)?;
            check_len(
                "events",
                events_reader.len() as usize,
                MAX_EVENTS_PER_INGEST,
            )?;
            let mut events = Vec::with_capacity(events_reader.len() as usize);
            for e in events_reader.iter() {
                events.push(Event {
                    event_id: read_text(e.get_event_id().map_err(codec_err)?)?,
                    kind: read_text(e.get_kind().map_err(codec_err)?)?,
                    occurred_at_unix_ms: e.get_occurred_at_unix_ms(),
                    payload: e.get_payload().map_err(codec_err)?.to_vec(),
                });
            }
            RequestPayload::EventIngest(EventIngest { events })
        }
        Method::Heartbeat => {
            let b = reader.get_heartbeat().map_err(codec_err)?;
            RequestPayload::Heartbeat(Heartbeat {
                node_id: read_text(b.get_node_id().map_err(codec_err)?)?,
                epoch: b.get_epoch(),
            })
        }
    })
}

fn decode_response_payload(
    reader: response::Reader<'_>,
    method: Method,
) -> Result<ResponsePayload> {
    Ok(match method {
        Method::LeaseAcquire => {
            let b = reader.get_lease_acquire().map_err(codec_err)?;
            ResponsePayload::LeaseAcquire(LeaseAcquired {
                granted: b.get_granted(),
                fencing_token: b.get_fencing_token(),
                expires_at_unix_ms: b.get_expires_at_unix_ms(),
                holder: read_text(b.get_holder().map_err(codec_err)?)?,
            })
        }
        Method::LeaseRelease => {
            let b = reader.get_lease_release().map_err(codec_err)?;
            ResponsePayload::LeaseRelease(LeaseReleased {
                released: b.get_released(),
            })
        }
        Method::PolicyGet => {
            let b = reader.get_policy_get().map_err(codec_err)?;
            let bundle = b.get_bundle().map_err(codec_err)?.to_vec();
            check_len("bundle", bundle.len(), MAX_BUNDLE_BYTES)?;
            ResponsePayload::PolicyGet(PolicyBundle {
                policy_id: read_text(b.get_policy_id().map_err(codec_err)?)?,
                version: b.get_version(),
                bundle,
                checksum: read_text(b.get_checksum().map_err(codec_err)?)?,
            })
        }
        Method::EventIngest => {
            let b = reader.get_event_ingest().map_err(codec_err)?;
            ResponsePayload::EventIngest(EventIngested {
                accepted: b.get_accepted(),
                duplicate: b.get_duplicate(),
            })
        }
        Method::Heartbeat => {
            let b = reader.get_heartbeat().map_err(codec_err)?;
            ResponsePayload::Heartbeat(HeartbeatAck {
                server_time_unix_ms: b.get_server_time_unix_ms(),
                epoch: b.get_epoch(),
            })
        }
        Method::PolicyWatch => {
            return Err(MessagingError::InvalidRequest(
                "PolicyWatch has no unary response payload".into(),
            ))
        }
    })
}

fn decode_watch_event(reader: policy_watch_event::Reader<'_>) -> Result<PolicyWatchEventDto> {
    let bundle = reader.get_bundle().map_err(codec_err)?.to_vec();
    check_len("bundle", bundle.len(), MAX_BUNDLE_BYTES)?;
    Ok(PolicyWatchEventDto {
        policy_id: read_text(reader.get_policy_id().map_err(codec_err)?)?,
        version: reader.get_version(),
        bundle,
        checksum: read_text(reader.get_checksum().map_err(codec_err)?)?,
    })
}

/// Decode one length-prefixed frame, enforcing size and traversal limits.
pub fn decode(frame: &[u8]) -> Result<Message> {
    if frame.len() < 4 {
        return Err(MessagingError::Codec("frame shorter than prefix".into()));
    }
    let len = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    if len > MAX_ENVELOPE_BYTES {
        return Err(MessagingError::FrameTooLarge {
            size: len,
            max: MAX_ENVELOPE_BYTES,
        });
    }
    if frame.len() != 4 + len {
        return Err(MessagingError::Codec(format!(
            "length prefix {} does not match frame body {}",
            len,
            frame.len() - 4
        )));
    }

    let mut options = ReaderOptions::new();
    options.traversal_limit_in_words(Some(MAX_TRAVERSAL_WORDS));
    options.nesting_limit(32);

    let mut slice = &frame[4..];
    let reader = serialize::read_message_from_flat_slice(&mut slice, options).map_err(codec_err)?;
    let env = reader.get_root::<envelope::Reader>().map_err(codec_err)?;

    let contract_version = env.get_contract_version();
    if contract_version != CONTRACT_VERSION {
        return Err(MessagingError::UnsupportedVersion(contract_version));
    }

    let method = env.get_method().map_err(codec_err)?;
    let body = match env.which().map_err(codec_err)? {
        envelope::Which::Request(req) => {
            let req = req.map_err(codec_err)?;
            Body::Request {
                identity: decode_identity(req.get_identity().map_err(codec_err)?)?,
                metadata: decode_metadata(req.get_metadata().map_err(codec_err)?)?,
                payload: decode_request_payload(req, method)?,
            }
        }
        envelope::Which::Response(resp) => {
            let resp = resp.map_err(codec_err)?;
            Body::Response(decode_response_payload(resp, method)?)
        }
        envelope::Which::WatchEvent(event) => {
            Body::WatchEvent(decode_watch_event(event.map_err(codec_err)?)?)
        }
        envelope::Which::StreamEnd(end) => {
            let end = end.map_err(codec_err)?;
            Body::StreamEnd(read_text(end.get_reason().map_err(codec_err)?)?)
        }
        envelope::Which::Error(err) => {
            let err = err.map_err(codec_err)?;
            Body::Error {
                code: err.get_code(),
                message: read_text(err.get_message().map_err(codec_err)?)?,
                retry_after_ms: err.get_retry_after_ms(),
                earliest_available: err.get_earliest_available(),
                server_latest: err.get_server_latest(),
                retryable: err.get_retryable(),
            }
        }
    };

    Ok(Message {
        contract_version,
        message_id: env.get_message_id(),
        request_id: env.get_request_id(),
        timestamp_ms: env.get_timestamp_ms(),
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> PeerIdentity {
        PeerIdentity {
            spiffe_id: "spiffe://waf/node-a".into(),
            node_id: "node-a".into(),
            roles: vec!["waf".into()],
        }
    }

    #[test]
    fn request_round_trip_preserves_metadata_and_payload() {
        let mut metadata = HashMap::new();
        metadata.insert("authorization".into(), "Bearer t".into());
        let message = Message::request(
            7,
            9,
            Body::Request {
                identity: identity(),
                metadata,
                payload: RequestPayload::LeaseAcquire(LeaseAcquire {
                    lease_id: "lease-1".into(),
                    resource: "policy/default".into(),
                    ttl_ms: 5_000,
                }),
            },
        );

        let frame = encode(&message).unwrap();
        let decoded = decode(&frame).unwrap();
        assert_eq!(decoded.contract_version, CONTRACT_VERSION);
        match decoded.body {
            Body::Request {
                identity: id,
                metadata,
                payload,
            } => {
                assert_eq!(id, identity());
                assert_eq!(metadata.get("authorization").unwrap(), "Bearer t");
                assert!(matches!(
                    payload,
                    RequestPayload::LeaseAcquire(LeaseAcquire { ttl_ms: 5_000, .. })
                ));
            }
            other => panic!("unexpected body: {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_contract_version() {
        let mut message = Message::request(
            1,
            1,
            Body::Response(ResponsePayload::Heartbeat(HeartbeatAck {
                server_time_unix_ms: 1,
                epoch: 1,
            })),
        );
        message.contract_version = 99;
        assert!(matches!(
            encode(&message),
            Err(MessagingError::UnsupportedVersion(99))
        ));
    }

    #[test]
    fn rejects_oversized_frame_prefix() {
        let mut frame = (MAX_ENVELOPE_BYTES as u32 + 1).to_be_bytes().to_vec();
        frame.extend_from_slice(&[0u8; 8]);
        assert!(matches!(
            decode(&frame),
            Err(MessagingError::FrameTooLarge { .. })
        ));
    }
}
