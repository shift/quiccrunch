//! Bounded per-policy watch retention and per-subscriber delivery queues.
//!
//! # Retention
//!
//! Each policy keeps a ring buffer of the most recent versions. The buffer is
//! bounded by entry count and bundle bytes ([`RetentionConfig`]). Publishing a
//! version appends it and evicts the oldest entries until both bounds hold;
//! every eviction increments `watch_evictions_total`. An entry larger than the
//! byte budget is delivered to live subscribers but not retained, because
//! keeping it would break the memory bound.
//!
//! A reconnecting client sends `sinceVersion`. The server replays every
//! retained version greater than that value. If the first version after
//! `sinceVersion` is older than the oldest retained entry, the server returns
//! [`MessagingError::ResumeTooOld`] with `earliest_available`; it never skips a
//! version. The client must then fetch a full snapshot.
//!
//! # Backpressure
//!
//! Publishing never blocks on a slow subscriber. Every subscription has a
//! bounded queue ([`BackpressureConfig::watch_queue_entries`] and
//! `watch_queue_bytes`), and every subscriber (connection) has a byte budget
//! shared by all of its watches. If either bound is exceeded, the subscription
//! is closed with [`MessagingError::SlowConsumer`] and
//! `watch_shed_total` increments. The client reconnects with its last applied
//! version, so no update is silently lost.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

use tokio::sync::Notify;

use super::{
    event_cost, BackpressureConfig, MessagingError, MessagingMetrics, PolicyWatchEventDto, Result,
    RetentionConfig,
};

/// Identifies one transport connection / client for budget accounting.
pub type SubscriberId = u64;

/// Byte and watch-count budget shared by all subscriptions of a subscriber.
pub struct SubscriberBudget {
    queued_bytes: AtomicUsize,
    max_bytes: usize,
    watches: AtomicUsize,
    max_watches: usize,
}

impl SubscriberBudget {
    pub fn new(max_bytes: usize, max_watches: usize) -> Self {
        Self {
            queued_bytes: AtomicUsize::new(0),
            max_bytes,
            watches: AtomicUsize::new(0),
            max_watches,
        }
    }

    pub fn queued_bytes(&self) -> usize {
        self.queued_bytes.load(Ordering::Relaxed)
    }

    pub fn watches(&self) -> usize {
        self.watches.load(Ordering::Relaxed)
    }

    fn try_add_watch(&self) -> bool {
        let mut current = self.watches.load(Ordering::Relaxed);
        loop {
            if current >= self.max_watches {
                return false;
            }
            match self.watches.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }

    fn remove_watch(&self) {
        self.watches.fetch_sub(1, Ordering::AcqRel);
    }

    fn try_reserve(&self, bytes: usize) -> bool {
        let mut current = self.queued_bytes.load(Ordering::Relaxed);
        loop {
            let next = match current.checked_add(bytes) {
                Some(next) if next <= self.max_bytes => next,
                _ => return false,
            };
            match self.queued_bytes.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(actual) => current = actual,
            }
        }
    }

    fn release(&self, bytes: usize) {
        self.queued_bytes.fetch_sub(bytes, Ordering::AcqRel);
    }
}

#[derive(Default)]
struct QueueInner {
    events: VecDeque<PolicyWatchEventDto>,
    bytes: usize,
}

/// Bounded queue for one watch subscription.
pub struct WatchQueue {
    inner: Mutex<QueueInner>,
    notify: Notify,
    closed: AtomicBool,
    lagged: AtomicBool,
    max_entries: usize,
    max_bytes: usize,
    budget: Arc<SubscriberBudget>,
    metrics: Arc<MessagingMetrics>,
}

impl WatchQueue {
    fn new(
        max_entries: usize,
        max_bytes: usize,
        budget: Arc<SubscriberBudget>,
        metrics: Arc<MessagingMetrics>,
    ) -> Self {
        Self {
            inner: Mutex::new(QueueInner::default()),
            notify: Notify::new(),
            closed: AtomicBool::new(false),
            lagged: AtomicBool::new(false),
            max_entries,
            max_bytes,
            budget,
            metrics,
        }
    }

    /// Enqueue without blocking. Returns `false` when the subscription must be
    /// closed because the subscriber cannot keep up.
    fn try_push(&self, event: PolicyWatchEventDto) -> bool {
        if self.closed.load(Ordering::Acquire) || self.lagged.load(Ordering::Acquire) {
            return false;
        }
        let cost = event_cost(&event);
        if !self.budget.try_reserve(cost) {
            return false;
        }
        let mut inner = self.inner.lock().expect("watch queue poisoned");
        if inner.events.len() >= self.max_entries || inner.bytes + cost > self.max_bytes {
            drop(inner);
            self.budget.release(cost);
            return false;
        }
        inner.events.push_back(event);
        inner.bytes += cost;
        let depth = self
            .metrics
            .watch_queue_depth
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        self.metrics
            .watch_queue_max_depth
            .fetch_max(depth, Ordering::Relaxed);
        drop(inner);
        self.notify.notify_one();
        true
    }

    async fn pop(&self) -> Option<Result<PolicyWatchEventDto>> {
        loop {
            if let Some(event) = {
                let mut inner = self.inner.lock().expect("watch queue poisoned");
                if let Some(event) = inner.events.pop_front() {
                    inner.bytes -= event_cost(&event);
                    Some(event)
                } else {
                    None
                }
            } {
                self.budget.release(event_cost(&event));
                self.metrics
                    .watch_queue_depth
                    .fetch_sub(1, Ordering::Relaxed);
                return Some(Ok(event));
            }

            if self.closed.load(Ordering::Acquire) {
                if self.lagged.load(Ordering::Acquire) {
                    return Some(Err(MessagingError::SlowConsumer(
                        "subscriber queue exceeded its bound".into(),
                    )));
                }
                return None;
            }

            self.notify.notified().await;
        }
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.notify.notify_one();
    }

    fn lag(&self) {
        self.lagged.store(true, Ordering::Release);
        self.close();
    }

    pub fn is_lagged(&self) -> bool {
        self.lagged.load(Ordering::Acquire)
    }
}

#[derive(Default)]
struct PolicyState {
    latest: Option<PolicyWatchEventDto>,
    history: VecDeque<PolicyWatchEventDto>,
    history_bytes: usize,
    subscribers: HashMap<u64, Arc<WatchQueue>>,
}

impl PolicyState {
    fn latest_version(&self) -> u64 {
        self.latest.as_ref().map(|event| event.version).unwrap_or(0)
    }

    fn earliest_available(&self) -> u64 {
        match self.history.front() {
            Some(event) => event.version,
            None => self.latest_version().saturating_add(1),
        }
    }
}

/// Server-side authoritative policy versions plus bounded replay history.
pub struct PolicyStore {
    policies: Mutex<HashMap<String, PolicyState>>,
    retention: RetentionConfig,
    metrics: Arc<MessagingMetrics>,
    next_subscription: AtomicU64,
    subscribers: AtomicUsize,
    max_subscribers: usize,
}

impl PolicyStore {
    pub fn new(retention: RetentionConfig, metrics: Arc<MessagingMetrics>) -> Self {
        Self {
            policies: Mutex::new(HashMap::new()),
            retention,
            metrics,
            next_subscription: AtomicU64::new(1),
            subscribers: AtomicUsize::new(0),
            max_subscribers: BackpressureConfig::default().max_subscribers,
        }
    }

    pub fn with_max_subscribers(mut self, max_subscribers: usize) -> Self {
        self.max_subscribers = max_subscribers.max(1);
        self
    }

    pub fn retention(&self) -> RetentionConfig {
        self.retention
    }

    pub fn metrics(&self) -> Arc<MessagingMetrics> {
        self.metrics.clone()
    }

    /// Publish the next version of a policy. Versions are strictly increasing
    /// per policy. Live subscribers are notified without blocking; a
    /// subscriber whose queue is full is closed as a slow consumer.
    pub fn publish(
        &self,
        policy_id: impl Into<String>,
        bundle: Vec<u8>,
        checksum: impl Into<String>,
    ) -> PolicyWatchEventDto {
        let policy_id = policy_id.into();
        let checksum = checksum.into();
        let mut policies = self.policies.lock().expect("policy store poisoned");
        let state = policies.entry(policy_id.clone()).or_default();
        let version = state.latest_version() + 1;
        let event = PolicyWatchEventDto {
            policy_id,
            version,
            bundle,
            checksum,
        };

        state.latest = Some(event.clone());
        let cost = event_cost(&event);
        let retain = self.retention.max_entries > 0 && cost <= self.retention.max_bytes;
        if retain {
            state.history.push_back(event.clone());
            state.history_bytes += cost;
            let mut evicted_entries = 0u64;
            let mut evicted_bytes = 0u64;
            while state.history.len() > self.retention.max_entries
                || state.history_bytes > self.retention.max_bytes
            {
                let Some(old) = state.history.pop_front() else {
                    break;
                };
                let old_cost = event_cost(&old);
                state.history_bytes -= old_cost;
                evicted_entries += 1;
                evicted_bytes += old_cost as u64;
            }
            if evicted_entries > 0 {
                self.metrics
                    .watch_evictions_total
                    .fetch_add(evicted_entries, Ordering::Relaxed);
                self.metrics
                    .retained(-(evicted_entries as i64), -(evicted_bytes as i64));
            }
            self.metrics.retained(1, cost as i64);
        } else {
            // Delivered live, but never retained: keeping it would break the
            // byte bound. Drop the older history too, so a resume across this
            // version cannot replay a gap. It falls back to a snapshot.
            let dropped = state.history.len() as u64;
            let dropped_bytes: usize = state.history.iter().map(event_cost).sum();
            state.history.clear();
            state.history_bytes = 0;
            self.metrics
                .watch_evictions_total
                .fetch_add(dropped + 1, Ordering::Relaxed);
            if dropped > 0 {
                self.metrics
                    .retained(-(dropped as i64), -(dropped_bytes as i64));
            }
        }

        let mut lagged = Vec::new();
        for (id, queue) in &state.subscribers {
            if !queue.try_push(event.clone()) {
                lagged.push(*id);
            }
        }
        for id in lagged {
            if let Some(queue) = state.subscribers.remove(&id) {
                queue.lag();
                self.metrics
                    .watch_shed_total
                    .fetch_add(1, Ordering::Relaxed);
                self.metrics
                    .watch_slow_consumer_total
                    .fetch_add(1, Ordering::Relaxed);
                self.subscribers.fetch_sub(1, Ordering::AcqRel);
                self.metrics.subscribers.fetch_sub(1, Ordering::Relaxed);
            }
        }
        event
    }

    pub fn latest(&self, policy_id: &str) -> Option<PolicyWatchEventDto> {
        self.policies
            .lock()
            .expect("policy store poisoned")
            .get(policy_id)
            .and_then(|state| state.latest.clone())
    }

    pub fn latest_version(&self, policy_id: &str) -> u64 {
        self.policies
            .lock()
            .expect("policy store poisoned")
            .get(policy_id)
            .map(PolicyState::latest_version)
            .unwrap_or(0)
    }

    pub fn earliest_available(&self, policy_id: &str) -> u64 {
        self.policies
            .lock()
            .expect("policy store poisoned")
            .get(policy_id)
            .map(PolicyState::earliest_available)
            .unwrap_or(0)
    }

    pub fn retained_entries(&self, policy_id: &str) -> usize {
        self.policies
            .lock()
            .expect("policy store poisoned")
            .get(policy_id)
            .map(|state| state.history.len())
            .unwrap_or(0)
    }

    pub fn retained_bytes(&self, policy_id: &str) -> usize {
        self.policies
            .lock()
            .expect("policy store poisoned")
            .get(policy_id)
            .map(|state| state.history_bytes)
            .unwrap_or(0)
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers.load(Ordering::Relaxed)
    }

    /// Register a watch and compute the exact replay set under the same lock
    /// as the subscriber registration, so a concurrent publish is either in
    /// the replay set or in the live queue — never lost and never duplicated.
    pub fn subscribe(
        self: &Arc<Self>,
        subscriber: SubscriberId,
        policy_id: &str,
        since_version: u64,
        config: &BackpressureConfig,
        budget: Arc<SubscriberBudget>,
    ) -> Result<WatchSubscription> {
        if !budget.try_add_watch() {
            return Err(MessagingError::WatchLimitExceeded(format!(
                "subscriber {subscriber} already has {} watches",
                budget.watches()
            )));
        }
        if self.subscribers.load(Ordering::Relaxed) >= self.max_subscribers {
            budget.remove_watch();
            return Err(MessagingError::WatchLimitExceeded(
                "server watch capacity exhausted".into(),
            ));
        }

        let mut policies = self.policies.lock().expect("policy store poisoned");
        let state = policies.entry(policy_id.to_string()).or_default();
        let latest = state.latest_version();
        if since_version > latest {
            self.metrics
                .resume_ahead_total
                .fetch_add(1, Ordering::Relaxed);
            budget.remove_watch();
            return Err(MessagingError::ResumeAhead {
                requested: since_version,
                server_latest: latest,
            });
        }

        let replay = if since_version < latest {
            let earliest = state.earliest_available();
            if since_version + 1 < earliest {
                self.metrics
                    .resume_too_old_total
                    .fetch_add(1, Ordering::Relaxed);
                budget.remove_watch();
                return Err(MessagingError::ResumeTooOld {
                    earliest_available: earliest,
                });
            }
            state
                .history
                .iter()
                .filter(|event| event.version > since_version)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };

        let id = self.next_subscription.fetch_add(1, Ordering::Relaxed);
        let queue = Arc::new(WatchQueue::new(
            config.watch_queue_entries,
            config.watch_queue_bytes,
            budget,
            self.metrics.clone(),
        ));
        state.subscribers.insert(id, queue.clone());
        self.subscribers.fetch_add(1, Ordering::AcqRel);
        self.metrics.subscribers.fetch_add(1, Ordering::Relaxed);
        drop(policies);

        Ok(WatchSubscription {
            queue,
            store: Arc::downgrade(self),
            policy_id: policy_id.to_string(),
            subscription_id: id,
            replay: replay.into(),
            finished: false,
        })
    }

    fn unsubscribe(&self, policy_id: &str, subscription_id: u64) {
        let mut policies = self.policies.lock().expect("policy store poisoned");
        if let Some(state) = policies.get_mut(policy_id) {
            if state.subscribers.remove(&subscription_id).is_some() {
                self.subscribers.fetch_sub(1, Ordering::AcqRel);
                self.metrics.subscribers.fetch_sub(1, Ordering::Relaxed);
            }
        }
    }
}

/// A live watch: buffered replay first, then queued live updates.
pub struct WatchSubscription {
    queue: Arc<WatchQueue>,
    store: Weak<PolicyStore>,
    policy_id: String,
    subscription_id: u64,
    replay: VecDeque<PolicyWatchEventDto>,
    finished: bool,
}

impl WatchSubscription {
    pub fn policy_id(&self) -> &str {
        &self.policy_id
    }

    pub fn is_lagged(&self) -> bool {
        self.queue.is_lagged()
    }

    /// Next update. `None` means the server closed the watch normally; an
    /// error means the client must resume (slow consumer) or re-sync.
    pub async fn next(&mut self) -> Option<Result<PolicyWatchEventDto>> {
        if self.finished {
            return None;
        }
        if let Some(event) = self.replay.pop_front() {
            return Some(Ok(event));
        }
        let next = self.queue.pop().await;
        if !matches!(next, Some(Ok(_))) {
            self.finished = true;
        }
        next
    }
}

impl Drop for WatchQueue {
    fn drop(&mut self) {
        let inner = self.inner.lock().expect("watch queue poisoned");
        let remaining = inner.events.len() as u64;
        drop(inner);
        if remaining > 0 {
            self.metrics
                .watch_queue_depth
                .fetch_sub(remaining, Ordering::Relaxed);
        }
    }
}

impl std::fmt::Debug for WatchSubscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchSubscription")
            .field("policy_id", &self.policy_id)
            .field("subscription_id", &self.subscription_id)
            .field("replay_len", &self.replay.len())
            .field("finished", &self.finished)
            .finish()
    }
}

impl Drop for WatchSubscription {
    fn drop(&mut self) {
        self.queue.close();
        if let Some(store) = self.store.upgrade() {
            store.unsubscribe(&self.policy_id, self.subscription_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(retention: RetentionConfig) -> Arc<PolicyStore> {
        Arc::new(PolicyStore::new(
            retention,
            Arc::new(MessagingMetrics::default()),
        ))
    }

    fn budget() -> Arc<SubscriberBudget> {
        Arc::new(SubscriberBudget::new(8 * 1024 * 1024, 8))
    }

    #[tokio::test]
    async fn resume_replays_exactly_after_since_version() {
        let store = store(RetentionConfig::default());
        for i in 0..10 {
            store.publish("p", vec![i], "sum");
        }
        let mut sub = store
            .subscribe(1, "p", 3, &BackpressureConfig::default(), budget())
            .unwrap();
        let mut versions = Vec::new();
        while let Ok(Some(Ok(event))) =
            tokio::time::timeout(std::time::Duration::from_millis(50), sub.next()).await
        {
            versions.push(event.version);
        }
        assert_eq!(versions, vec![4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn eviction_reports_earliest_available() {
        let store = store(RetentionConfig {
            max_entries: 3,
            max_bytes: 1024,
        });
        for i in 0..10 {
            store.publish("p", vec![i], "sum");
        }
        assert_eq!(store.earliest_available("p"), 8);
        let err = store
            .subscribe(1, "p", 2, &BackpressureConfig::default(), budget())
            .unwrap_err();
        match err {
            MessagingError::ResumeTooOld { earliest_available } => {
                assert_eq!(earliest_available, 8)
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn byte_cap_evicts_oldest() {
        let store = store(RetentionConfig {
            max_entries: 100,
            max_bytes: 80,
        });
        for _ in 0..10 {
            store.publish("p", vec![0u8; 16], "sum");
        }
        let retained = store.retained_entries("p");
        assert!(retained < 10, "expected eviction, retained {retained}");
        assert!(store.retained_bytes("p") <= 80);
    }

    #[tokio::test]
    async fn slow_subscriber_is_closed_and_counted() {
        let metrics = Arc::new(MessagingMetrics::default());
        let store = Arc::new(PolicyStore::new(
            RetentionConfig::default(),
            metrics.clone(),
        ));
        let config = BackpressureConfig {
            watch_queue_entries: 2,
            watch_queue_bytes: 1024,
            subscriber_queue_bytes: 1024,
            ..Default::default()
        };
        let mut sub = store
            .subscribe(1, "p", 0, &config, Arc::new(SubscriberBudget::new(1024, 8)))
            .unwrap();
        for _ in 0..10 {
            store.publish("p", vec![0u8; 8], "sum");
        }
        assert!(sub.is_lagged());
        assert_eq!(metrics.snapshot().watch_shed_total, 1);
        // Drain the two queued events, then observe the explicit close.
        let _ = sub.next().await;
        let _ = sub.next().await;
        assert!(matches!(
            sub.next().await,
            Some(Err(MessagingError::SlowConsumer(_)))
        ));
    }
}
