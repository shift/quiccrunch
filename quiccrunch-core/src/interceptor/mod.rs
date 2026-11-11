//! Interceptor/Middleware Framework for RPC
//!
//! Provides a flexible middleware/interceptor system for RPC requests/responses.

use crate::rpc::retry::{CircuitBreaker, RetryableError};
use async_trait::async_trait;

/// Interceptor trait for request/response processing
///
/// Interceptors can intercept and modify requests before they are sent,
/// and responses before they are returned. They can also handle errors.
///
/// Interceptors are executed in order of registration for requests,
/// and in reverse order for responses (like middleware stack).
#[async_trait]
#[allow(unused_variables)]
pub trait Interceptor: Send + Sync {
    /// Called before sending a request
    /// Returns Ok(()) to continue, or Err to short-circuit and return an error
    async fn before_request(
        &self,
        context: &mut crate::rpc::RpcContext,
        params: &mut bytes::Bytes,
    ) -> crate::RpcResult<()> {
        Ok(())
    }

    /// Called after receiving a successful response
    /// Can modify the response before returning to caller
    async fn after_response(
        &self,
        context: &mut crate::rpc::RpcContext,
        response: &mut bytes::Bytes,
    ) -> crate::RpcResult<()> {
        Ok(())
    }

    /// Called when an error occurs
    /// Can transform the error or handle it (e.g., logging, metrics)
    async fn on_error(
        &self,
        context: &mut crate::rpc::RpcContext,
        error: &crate::RpcError,
    ) -> crate::RpcResult<()> {
        Ok(())
    }

    /// Called when a request times out
    async fn on_timeout(&self, context: &mut crate::rpc::RpcContext) -> crate::RpcResult<()> {
        Ok(())
    }

    /// Called when a request is retried
    async fn on_retry(
        &self,
        context: &mut crate::rpc::RpcContext,
        attempt: u32,
        error: &crate::RpcError,
    ) -> crate::RpcResult<()> {
        Ok(())
    }
}

/// Interceptor chain for managing multiple interceptors
///
/// Interceptors are executed in order of registration for requests,
/// and in reverse order for responses (like middleware stack).
pub struct InterceptorChain {
    interceptors: Vec<Box<dyn Interceptor>>,
}

impl InterceptorChain {
    pub fn new() -> Self {
        Self {
            interceptors: Vec::new(),
        }
    }

    /// Add an interceptor to the chain
    pub fn add<I: Interceptor + 'static>(&mut self, interceptor: I) {
        self.interceptors.push(Box::new(interceptor));
    }

    /// Execute all before_request interceptors
    pub async fn before_request(
        &self,
        context: &mut crate::rpc::RpcContext,
        params: &mut bytes::Bytes,
    ) -> crate::RpcResult<()> {
        for interceptor in &self.interceptors {
            interceptor.before_request(context, params).await?;
        }
        Ok(())
    }

    /// Execute all after_response interceptors in reverse order
    pub async fn after_response(
        &self,
        context: &mut crate::rpc::RpcContext,
        response: &mut bytes::Bytes,
    ) -> crate::RpcResult<()> {
        for interceptor in self.interceptors.iter().rev() {
            interceptor.after_response(context, response).await?;
        }
        Ok(())
    }

    /// Execute all on_error interceptors in reverse order
    pub async fn on_error(
        &self,
        context: &mut crate::rpc::RpcContext,
        error: &crate::RpcError,
    ) -> crate::RpcResult<()> {
        for interceptor in self.interceptors.iter().rev() {
            interceptor.on_error(context, error).await?;
        }
        Ok(())
    }

    /// Execute on_timeout interceptors
    pub async fn on_timeout(&self, context: &mut crate::rpc::RpcContext) -> crate::RpcResult<()> {
        for interceptor in self.interceptors.iter().rev() {
            interceptor.on_timeout(context).await?;
        }
        Ok(())
    }

    /// Execute on_retry interceptors
    pub async fn on_retry(
        &self,
        context: &mut crate::rpc::RpcContext,
        attempt: u32,
        error: &crate::RpcError,
    ) -> crate::RpcResult<()> {
        for interceptor in self.interceptors.iter().rev() {
            interceptor.on_retry(context, attempt, error).await?;
        }
        Ok(())
    }
}

impl Default for InterceptorChain {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Built-in Interceptors
// ============================================================================

/// Logging interceptor for request/response logging
#[derive(Debug, Default)]
pub struct LoggingInterceptor {
    log_requests: bool,
    log_responses: bool,
    log_errors: bool,
}

impl LoggingInterceptor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_request_logging(mut self, enabled: bool) -> Self {
        self.log_requests = enabled;
        self
    }

    pub fn with_response_logging(mut self, enabled: bool) -> Self {
        self.log_responses = enabled;
        self
    }

    pub fn with_error_logging(mut self, enabled: bool) -> Self {
        self.log_errors = enabled;
        self
    }
}

#[async_trait::async_trait]
impl Interceptor for LoggingInterceptor {
    async fn before_request(
        &self,
        context: &mut crate::rpc::RpcContext,
        params: &mut bytes::Bytes,
    ) -> crate::RpcResult<()> {
        if self.log_requests {
            log::info!(
                "RPC Request: service={} method={} request_id={} params_len={}",
                context.service,
                context.method,
                context.request_id,
                params.len()
            );
        }
        Ok(())
    }

    async fn after_response(
        &self,
        context: &mut crate::rpc::RpcContext,
        response: &mut bytes::Bytes,
    ) -> crate::RpcResult<()> {
        if self.log_responses {
            log::info!(
                "RPC Response: service={} method={} request_id={} response_len={}",
                context.service,
                context.method,
                context.request_id,
                response.len()
            );
        }
        Ok(())
    }

    async fn on_error(
        &self,
        context: &mut crate::rpc::RpcContext,
        error: &crate::RpcError,
    ) -> crate::RpcResult<()> {
        if self.log_errors {
            log::error!(
                "RPC Error: service={} method={} request_id={} error={}",
                context.service,
                context.method,
                context.request_id,
                error
            );
        }
        Ok(())
    }
}

/// Metrics interceptor for collecting RPC metrics
#[derive(Debug, Default)]
pub struct MetricsInterceptor {
    pub requests_total: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub requests_succeeded: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub requests_failed: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub total_latency_ms: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl MetricsInterceptor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn requests_total(&self) -> u64 {
        self.requests_total
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn requests_succeeded(&self) -> u64 {
        self.requests_succeeded
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn requests_failed(&self) -> u64 {
        self.requests_failed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn avg_latency_ms(&self) -> f64 {
        let total = self
            .requests_total
            .load(std::sync::atomic::Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            self.total_latency_ms
                .load(std::sync::atomic::Ordering::Relaxed) as f64
                / total as f64
        }
    }
}

#[async_trait::async_trait]
impl Interceptor for MetricsInterceptor {
    async fn before_request(
        &self,
        context: &mut crate::rpc::RpcContext,
        _params: &mut bytes::Bytes,
    ) -> crate::RpcResult<()> {
        context.start_time = std::time::SystemTime::now();
        self.requests_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn after_response(
        &self,
        context: &mut crate::rpc::RpcContext,
        _response: &mut bytes::Bytes,
    ) -> crate::RpcResult<()> {
        self.requests_succeeded
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let latency = context.start_time.elapsed().unwrap_or_default().as_millis() as u64;
        self.total_latency_ms
            .fetch_add(latency, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn on_error(
        &self,
        _context: &mut crate::rpc::RpcContext,
        _error: &crate::RpcError,
    ) -> crate::RpcResult<()> {
        self.requests_failed
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}

/// Timeout interceptor for enforcing request timeouts
#[derive(Debug, Clone)]
pub struct TimeoutInterceptor {
    default_timeout: std::time::Duration,
}

impl TimeoutInterceptor {
    pub fn new(default_timeout: std::time::Duration) -> Self {
        Self { default_timeout }
    }
}

#[async_trait]
impl Interceptor for TimeoutInterceptor {
    async fn before_request(
        &self,
        context: &mut crate::rpc::RpcContext,
        _params: &mut bytes::Bytes,
    ) -> crate::RpcResult<()> {
        if context.timeout.is_none() {
            context.timeout = Some(self.default_timeout);
        }
        Ok(())
    }
}

/// Circuit breaker interceptor for integration with circuit breaker
#[derive(Debug, Clone)]
pub struct CircuitBreakerInterceptor {
    circuit_breaker: std::sync::Arc<CircuitBreaker>,
}

impl CircuitBreakerInterceptor {
    pub fn new(circuit_breaker: std::sync::Arc<CircuitBreaker>) -> Self {
        Self { circuit_breaker }
    }
}

#[async_trait]
impl Interceptor for CircuitBreakerInterceptor {
    async fn before_request(
        &self,
        _context: &mut crate::rpc::RpcContext,
        _params: &mut bytes::Bytes,
    ) -> crate::RpcResult<()> {
        if !self.circuit_breaker.can_execute() {
            return Err(crate::RpcError::ServiceUnavailable(
                "Circuit breaker is open".to_string(),
            ));
        }
        Ok(())
    }

    async fn after_response(
        &self,
        _context: &mut crate::rpc::RpcContext,
        _response: &mut bytes::Bytes,
    ) -> crate::RpcResult<()> {
        self.circuit_breaker.record_success();
        Ok(())
    }

    async fn on_error(
        &self,
        _context: &mut crate::rpc::RpcContext,
        error: &crate::RpcError,
    ) -> crate::RpcResult<()> {
        let retryable: RetryableError = error.clone().into();
        // Only record failure for retryable errors
        if matches!(
            retryable,
            RetryableError::Timeout
                | RetryableError::ConnectionError
                | RetryableError::ServiceUnavailable
                | RetryableError::InternalError
        ) {
            self.circuit_breaker.record_failure();
        }
        Ok(())
    }
}

// Authentication and Authorization module
#[cfg(feature = "auth")]
pub mod auth;
#[cfg(feature = "auth")]
pub use self::auth::{
    create_auth_interceptor, create_rbac_auth_interceptor, AuthConfig, AuthInterceptor,
    AuthInterceptorBuilder, Claims, Permission, Role,
};
