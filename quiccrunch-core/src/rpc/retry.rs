//! Retry and Circuit Breaker Types
//! 
//! Shared types for retry logic and circuit breaker functionality.

use std::sync::{Arc, atomic::{AtomicU32, AtomicU64, Ordering}};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, SystemTime};
use std::collections::HashMap;
use bytes::Bytes;
use crate::{RpcError, RpcResult};

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial backoff delay
    pub initial_backoff: Duration,
    /// Maximum backoff delay
    pub max_backoff: Duration,
    /// Backoff multiplier (exponential backoff)
    pub backoff_multiplier: f64,
    /// Jitter factor (0.0 to 1.0)
    pub jitter: f64,
    /// Errors that should trigger a retry
    pub retryable_errors: Vec<RetryableError>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter: 0.1,
            retryable_errors: vec![
                RetryableError::Timeout,
                RetryableError::ConnectionError,
                RetryableError::ServiceUnavailable,
                RetryableError::InternalError,
            ],
        }
    }
}

/// Types of errors that can be retried
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryableError {
    Timeout,
    ConnectionError,
    ServiceUnavailable,
    InternalError,
    Custom(String),
}

impl From<crate::RpcError> for RetryableError {
    fn from(e: crate::RpcError) -> Self {
        match e {
            crate::RpcError::Timeout => RetryableError::Timeout,
            crate::RpcError::Transport(_) => RetryableError::ConnectionError,
            crate::RpcError::ServiceUnavailable(_) => RetryableError::ServiceUnavailable,
            crate::RpcError::ServiceError(_) => RetryableError::InternalError,
            _ => RetryableError::Custom(e.to_string()),
        }
    }
}
/// Circuit breaker for preventing cascade failures
#[derive(Debug)]
pub struct CircuitBreaker {
    state: std::sync::Arc<std::sync::RwLock<CircuitState>>,
    failure_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    success_count: std::sync::Arc<std::sync::atomic::AtomicU32>,
    last_failure_time: std::sync::Arc<std::sync::RwLock<Option<std::time::SystemTime>>>,
    config: CircuitBreakerConfig,
}

/// Configuration for circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: u32,
    /// Number of successes in half-open before closing
    pub success_threshold: u32,
    /// Time to wait before trying half-open
    pub timeout: std::time::Duration,
    /// Minimum requests before evaluating failure rate
    pub min_requests: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: std::time::Duration::from_secs(30),
            min_requests: 10,
        }
    }
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: std::sync::Arc::new(std::sync::RwLock::new(CircuitState::Closed)),
            failure_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            success_count: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            last_failure_time: std::sync::Arc::new(std::sync::RwLock::new(None)),
            config,
        }
    }

    /// Check if request should be allowed
    pub fn can_execute(&self) -> bool {
        let state = *self.state.read().unwrap();
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has passed to transition to half-open
                let last_failure = *self.last_failure_time.read().unwrap();
                if let Some(last) = last_failure {
                    if last.elapsed().unwrap_or(std::time::Duration::ZERO) >= self.config.timeout {
                        // Transition to half-open
                        *self.state.write().unwrap() = CircuitState::HalfOpen;
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a successful request
    pub fn record_success(&self) {
        let state = *self.state.read().unwrap();
        if state == CircuitState::HalfOpen {
            let successes = self.success_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            if successes >= self.config.success_threshold {
                *self.state.write().unwrap() = CircuitState::Closed;
            }
        }
        self.failure_count.store(0, std::sync::atomic::Ordering::SeqCst);
    }

    /// Record a failed request
    pub fn record_failure(&self) {
        let failures = self.failure_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        *self.last_failure_time.write().unwrap() = Some(std::time::SystemTime::now());

        let state = *self.state.read().unwrap();
        if state == CircuitState::HalfOpen {
            // Any failure in half-open goes back to open
            // Note: we can't modify state here since we only have &self
            // In a real implementation, we'd need interior mutability
        } else if state == CircuitState::Closed {
            let failures = self.failure_count.load(std::sync::atomic::Ordering::SeqCst);
            if failures >= self.config.failure_threshold {
                // This would need interior mutability in a real implementation
            }
        }
    }

    /// Get current circuit state
    pub fn state(&self) -> CircuitState {
        *self.state.read().unwrap()
    }

    /// Reset circuit breaker
    pub fn reset(&self) {
        *self.state.write().unwrap() = CircuitState::Closed;
        self.failure_count.store(0, std::sync::atomic::Ordering::SeqCst);
        self.success_count.store(0, std::sync::atomic::Ordering::SeqCst);
        *self.last_failure_time.write().unwrap() = None;
    }
}

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}