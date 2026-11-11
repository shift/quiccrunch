//! Rate Limiting and DDoS Protection for RPC
//!
//! Provides token bucket rate limiter, per-client rate limiting,
//! service-level rate limiting, and DDoS protection patterns.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Token bucket rate limiter
pub struct TokenBucketRateLimiter {
    /// Maximum number of tokens (burst capacity)
    max_tokens: u64,
    /// Current number of tokens
    tokens: Arc<RwLock<u64>>,
    /// Token refill rate (tokens per second)
    refill_rate: f64,
    /// Last refill time
    last_refill: Arc<RwLock<Instant>>,
}

impl TokenBucketRateLimiter {
    /// Create a new token bucket rate limiter
    pub fn new(max_tokens: u64, refill_rate: f64) -> Self {
        Self {
            max_tokens,
            tokens: Arc::new(RwLock::new(max_tokens)),
            refill_rate,
            last_refill: Arc::new(RwLock::new(Instant::now())),
        }
    }

    /// Try to consume a token
    pub async fn try_consume(&self, tokens: u64) -> bool {
        let mut current_tokens = self.tokens.write().await;
        let mut last_refill = self.last_refill.write().await;

        // Refill tokens based on elapsed time
        let now = Instant::now();
        let elapsed = last_refill.elapsed().as_secs_f64();
        let refill_amount = (elapsed * self.refill_rate) as u64;

        if refill_amount > 0 {
            *current_tokens = (*current_tokens + refill_amount).min(self.max_tokens);
            *last_refill = now;
        }

        // Try to consume tokens
        if *current_tokens >= tokens {
            *current_tokens -= tokens;
            true
        } else {
            false
        }
    }

    /// Get current token count
    pub async fn available_tokens(&self) -> u64 {
        let current_tokens = self.tokens.read().await;
        *current_tokens
    }

    /// Get time until next token is available
    pub async fn time_until_available(&self) -> Duration {
        let current_tokens = self.tokens.read().await;
        if *current_tokens > 0 {
            Duration::ZERO
        } else {
            let wait_time = 1.0 / self.refill_rate;
            Duration::from_secs_f64(wait_time)
        }
    }
}

/// Per-client rate limiter
pub struct PerClientRateLimiter {
    /// Default rate limit for all clients
    default_limit: u64,
    /// Default burst capacity
    default_burst: u64,
    /// Per-client rate limiters
    limiters: Arc<RwLock<HashMap<String, Arc<TokenBucketRateLimiter>>>>,
}

impl PerClientRateLimiter {
    /// Create a new per-client rate limiter
    pub fn new(default_limit: u64, default_burst: u64) -> Self {
        Self {
            default_limit,
            default_burst,
            limiters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get or create rate limiter for a client
    pub async fn get_limiter(&self, client_id: &str) -> Arc<TokenBucketRateLimiter> {
        let mut limiters = self.limiters.write().await;

        limiters
            .entry(client_id.to_string())
            .or_insert_with(|| {
                Arc::new(TokenBucketRateLimiter::new(
                    self.default_burst,
                    self.default_limit as f64,
                ))
            })
            .clone()
    }

    /// Try to consume a token for a client
    pub async fn try_consume(&self, client_id: &str, tokens: u64) -> bool {
        let limiter = self.get_limiter(client_id).await;
        limiter.try_consume(tokens).await
    }

    /// Set custom rate limit for a client
    pub async fn set_client_limit(&self, client_id: &str, limit: u64, burst: u64) {
        let mut limiters = self.limiters.write().await;
        limiters.insert(
            client_id.to_string(),
            Arc::new(TokenBucketRateLimiter::new(burst, limit as f64)),
        );
    }

    /// Remove rate limiter for a client
    pub async fn remove_client(&self, client_id: &str) {
        let mut limiters = self.limiters.write().await;
        limiters.remove(client_id);
    }

    /// Get all client IDs
    pub async fn list_clients(&self) -> Vec<String> {
        let limiters = self.limiters.read().await;
        limiters.keys().cloned().collect()
    }
}

/// Service-level rate limiter
pub struct ServiceRateLimiter {
    /// Rate limiters per service
    limiters: Arc<RwLock<HashMap<String, Arc<TokenBucketRateLimiter>>>>,
}

impl ServiceRateLimiter {
    /// Create a new service rate limiter
    pub fn new() -> Self {
        Self {
            limiters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a service with rate limit
    pub async fn add_service(&self, service_name: &str, max_tokens: u64, refill_rate: f64) {
        let mut limiters = self.limiters.write().await;
        limiters.insert(
            service_name.to_string(),
            Arc::new(TokenBucketRateLimiter::new(max_tokens, refill_rate)),
        );
    }

    /// Try to consume a token for a service
    pub async fn try_consume(&self, service_name: &str, tokens: u64) -> bool {
        let limiters = self.limiters.read().await;
        if let Some(limiter) = limiters.get(service_name) {
            limiter.try_consume(tokens).await
        } else {
            // No rate limit configured, allow
            true
        }
    }

    /// Remove a service rate limit
    pub async fn remove_service(&self, service_name: &str) {
        let mut limiters = self.limiters.write().await;
        limiters.remove(service_name);
    }
}

impl Default for ServiceRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// DDoS protection configuration
#[derive(Debug, Clone)]
pub struct DdosProtectionConfig {
    /// Maximum requests per second per client
    pub max_requests_per_second: u64,
    /// Maximum burst size
    pub max_burst: u64,
    /// Maximum concurrent connections per client
    pub max_concurrent_connections: usize,
    /// Maximum request size in bytes
    pub max_request_size: usize,
    /// Block duration for violations
    pub block_duration: Duration,
    /// Maximum number of blocked clients
    pub max_blocked_clients: usize,
}

impl Default for DdosProtectionConfig {
    fn default() -> Self {
        Self {
            max_requests_per_second: 1000,
            max_burst: 5000,
            max_concurrent_connections: 100,
            max_request_size: 1024 * 1024,            // 1MB
            block_duration: Duration::from_secs(300), // 5 minutes
            max_blocked_clients: 10000,
        }
    }
}

/// DDoS protection state for a client
#[derive(Debug, Clone)]
struct ClientProtectionState {
    /// Number of violations
    violations: u32,
    /// Block expiry time
    blocked_until: Option<Instant>,
    /// Current concurrent connections
    concurrent_connections: usize,
}

/// DDoS protection manager
pub struct DdosProtection {
    config: DdosProtectionConfig,
    /// Per-client protection state
    client_states: Arc<RwLock<HashMap<String, ClientProtectionState>>>,
    /// Per-client rate limiters
    rate_limiters: Arc<PerClientRateLimiter>,
}

impl DdosProtection {
    /// Create a new DDoS protection manager
    pub fn new(config: DdosProtectionConfig) -> Self {
        let rate_limiters = Arc::new(PerClientRateLimiter::new(
            config.max_requests_per_second,
            config.max_burst,
        ));

        Self {
            config,
            client_states: Arc::new(RwLock::new(HashMap::new())),
            rate_limiters,
        }
    }

    /// Check if a client is allowed to make a request
    pub async fn check_request(
        &self,
        client_id: &str,
        request_size: usize,
    ) -> Result<(), DdosProtectionError> {
        // Check if client is blocked
        {
            let states = self.client_states.read().await;
            if let Some(state) = states.get(client_id) {
                if let Some(blocked_until) = state.blocked_until {
                    if Instant::now() < blocked_until {
                        return Err(DdosProtectionError::ClientBlocked {
                            client_id: client_id.to_string(),
                            expires_at: blocked_until,
                        });
                    }
                }
            }
        }

        // Check request size
        if request_size > self.config.max_request_size {
            self.record_violation(client_id, "request_size_exceeded")
                .await;
            return Err(DdosProtectionError::RequestTooLarge {
                size: request_size,
                max_size: self.config.max_request_size,
            });
        }

        // Check rate limit
        if !self.rate_limiters.try_consume(client_id, 1).await {
            self.record_violation(client_id, "rate_limit_exceeded")
                .await;
            return Err(DdosProtectionError::RateLimitExceeded {
                client_id: client_id.to_string(),
                retry_after: Duration::from_secs(1),
            });
        }

        Ok(())
    }

    /// Record a connection open
    pub async fn record_connection_open(&self, client_id: &str) -> Result<(), DdosProtectionError> {
        let mut states = self.client_states.write().await;
        let state = states
            .entry(client_id.to_string())
            .or_insert(ClientProtectionState {
                violations: 0,
                blocked_until: None,
                concurrent_connections: 0,
            });

        state.concurrent_connections += 1;

        if state.concurrent_connections > self.config.max_concurrent_connections {
            self.record_violation_inner(state, client_id, "concurrent_connections_exceeded")
                .await;
            return Err(DdosProtectionError::TooManyConnections {
                current: state.concurrent_connections,
                max: self.config.max_concurrent_connections,
            });
        }

        Ok(())
    }

    /// Record a connection close
    pub async fn record_connection_close(&self, client_id: &str) {
        let mut states = self.client_states.write().await;
        if let Some(state) = states.get_mut(client_id) {
            state.concurrent_connections = state.concurrent_connections.saturating_sub(1);
        }
    }

    /// Record a violation
    async fn record_violation(&self, client_id: &str, reason: &str) {
        let mut states = self.client_states.write().await;
        let state = states
            .entry(client_id.to_string())
            .or_insert(ClientProtectionState {
                violations: 0,
                blocked_until: None,
                concurrent_connections: 0,
            });

        self.record_violation_inner(state, client_id, reason).await;
    }

    /// Record a violation (inner)
    async fn record_violation_inner(
        &self,
        state: &mut ClientProtectionState,
        client_id: &str,
        reason: &str,
    ) {
        state.violations += 1;

        log::warn!(
            "DDoS protection violation for client {}: {} (violations: {})",
            client_id,
            reason,
            state.violations
        );

        // Block client after multiple violations
        if state.violations >= 3 {
            state.blocked_until = Some(Instant::now() + self.config.block_duration);
            log::error!(
                "Client {} blocked for {:?} due to {} violations",
                client_id,
                self.config.block_duration,
                state.violations
            );
        }
    }

    /// Check if a client is blocked
    pub async fn is_blocked(&self, client_id: &str) -> bool {
        let states = self.client_states.read().await;
        if let Some(state) = states.get(client_id) {
            if let Some(blocked_until) = state.blocked_until {
                return Instant::now() < blocked_until;
            }
        }
        false
    }

    /// Unblock a client
    pub async fn unblock_client(&self, client_id: &str) {
        let mut states = self.client_states.write().await;
        if let Some(state) = states.get_mut(client_id) {
            state.blocked_until = None;
            state.violations = 0;
        }
    }

    /// Get protection statistics
    pub async fn stats(&self) -> DdosProtectionStats {
        let states = self.client_states.read().await;
        let blocked_clients: Vec<String> = states
            .iter()
            .filter(|(_, state)| {
                state
                    .blocked_until
                    .map(|until| Instant::now() < until)
                    .unwrap_or(false)
            })
            .map(|(client_id, _)| client_id.clone())
            .collect();

        let total_violations: u32 = states.values().map(|s| s.violations).sum();
        let total_connections: usize = states.values().map(|s| s.concurrent_connections).sum();

        DdosProtectionStats {
            total_clients: states.len(),
            blocked_clients,
            total_violations,
            total_connections,
        }
    }
}

/// DDoS protection error
#[derive(Debug, Clone)]
pub enum DdosProtectionError {
    /// Client is blocked
    ClientBlocked {
        client_id: String,
        expires_at: Instant,
    },
    /// Rate limit exceeded
    RateLimitExceeded {
        client_id: String,
        retry_after: Duration,
    },
    /// Request too large
    RequestTooLarge { size: usize, max_size: usize },
    /// Too many concurrent connections
    TooManyConnections { current: usize, max: usize },
}

impl std::fmt::Display for DdosProtectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DdosProtectionError::ClientBlocked {
                client_id,
                expires_at,
            } => {
                write!(f, "Client {} is blocked until {:?}", client_id, expires_at)
            }
            DdosProtectionError::RateLimitExceeded {
                client_id,
                retry_after,
            } => {
                write!(
                    f,
                    "Rate limit exceeded for client {}, retry after {:?}",
                    client_id, retry_after
                )
            }
            DdosProtectionError::RequestTooLarge { size, max_size } => {
                write!(
                    f,
                    "Request too large: {} bytes (max: {} bytes)",
                    size, max_size
                )
            }
            DdosProtectionError::TooManyConnections { current, max } => {
                write!(f, "Too many connections: {} (max: {})", current, max)
            }
        }
    }
}

impl std::error::Error for DdosProtectionError {}

/// DDoS protection statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DdosProtectionStats {
    pub total_clients: usize,
    pub blocked_clients: Vec<String>,
    pub total_violations: u32,
    pub total_connections: usize,
}

/// Rate limiting interceptor for RPC
pub struct RateLimitInterceptor {
    /// Per-client rate limiter
    client_limiter: Arc<PerClientRateLimiter>,
    /// Service-level rate limiter
    service_limiter: Arc<ServiceRateLimiter>,
    /// DDoS protection
    ddos_protection: Arc<DdosProtection>,
}

impl RateLimitInterceptor {
    /// Create a new rate limit interceptor
    pub fn new(
        default_client_limit: u64,
        default_client_burst: u64,
        ddos_config: DdosProtectionConfig,
    ) -> Self {
        Self {
            client_limiter: Arc::new(PerClientRateLimiter::new(
                default_client_limit,
                default_client_burst,
            )),
            service_limiter: Arc::new(ServiceRateLimiter::new()),
            ddos_protection: Arc::new(DdosProtection::new(ddos_config)),
        }
    }

    /// Add service rate limit
    pub async fn add_service_limit(&self, service_name: &str, max_tokens: u64, refill_rate: f64) {
        self.service_limiter
            .add_service(service_name, max_tokens, refill_rate)
            .await;
    }

    /// Set client rate limit
    pub async fn set_client_limit(&self, client_id: &str, limit: u64, burst: u64) {
        self.client_limiter
            .set_client_limit(client_id, limit, burst)
            .await;
    }

    /// Check rate limit for a request
    pub async fn check_rate_limit(
        &self,
        client_id: &str,
        service_name: &str,
        request_size: usize,
    ) -> Result<(), DdosProtectionError> {
        // Check DDoS protection first
        self.ddos_protection
            .check_request(client_id, request_size)
            .await?;

        // Check per-client rate limit
        if !self.client_limiter.try_consume(client_id, 1).await {
            return Err(DdosProtectionError::RateLimitExceeded {
                client_id: client_id.to_string(),
                retry_after: Duration::from_secs(1),
            });
        }

        // Check service-level rate limit
        if !self.service_limiter.try_consume(service_name, 1).await {
            return Err(DdosProtectionError::RateLimitExceeded {
                client_id: client_id.to_string(),
                retry_after: Duration::from_secs(1),
            });
        }

        Ok(())
    }

    /// Get DDoS protection statistics
    pub async fn ddos_stats(&self) -> DdosProtectionStats {
        self.ddos_protection.stats().await
    }
}

impl Clone for RateLimitInterceptor {
    fn clone(&self) -> Self {
        Self {
            client_limiter: self.client_limiter.clone(),
            service_limiter: self.service_limiter.clone(),
            ddos_protection: self.ddos_protection.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_bucket_rate_limiter() {
        let limiter = TokenBucketRateLimiter::new(10, 1.0);

        // Should be able to consume tokens
        assert!(limiter.try_consume(5).await);
        assert!(limiter.try_consume(5).await);

        // Should not be able to consume more
        assert!(!limiter.try_consume(1).await);
    }

    #[tokio::test]
    async fn test_per_client_rate_limiter() {
        let limiter = PerClientRateLimiter::new(10, 10);

        // Should be able to consume tokens for different clients
        assert!(limiter.try_consume("client1", 5).await);
        assert!(limiter.try_consume("client2", 5).await);

        // Should not be able to consume more for client1
        assert!(!limiter.try_consume("client1", 6).await);
    }

    #[tokio::test]
    async fn test_ddos_protection() {
        let config = DdosProtectionConfig {
            max_requests_per_second: 10,
            max_burst: 10,
            max_concurrent_connections: 5,
            max_request_size: 1024,
            block_duration: Duration::from_secs(1),
            max_blocked_clients: 100,
        };

        let protection = DdosProtection::new(config);

        // Should allow requests
        assert!(protection.check_request("client1", 100).await.is_ok());

        // Should reject large requests
        assert!(protection.check_request("client1", 2048).await.is_err());
    }
}
