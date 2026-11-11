//! Library-level API examples used in the top-level README.
//!
//! CI builds all targets, so the README snippets cannot drift from the code
//! without breaking `cargo build --workspace --all-targets`.
//!
//! The JWT/RBAC interceptor is intentionally not shown here: it lives behind
//! the non-default `auth` feature, which does not currently compile. See
//! `docs/known-issues/auth-feature-does-not-build.md`.

use std::time::Duration;

use quiccrunch_core::rpc::metrics::RpcMetrics;
use quiccrunch_core::rpc::rate_limit::{DdosProtectionConfig, RateLimitInterceptor};
use quiccrunch_core::rpc::service::{MethodDefinition, ServiceDefinition, StreamingType};

/// Define a unary method on a service.
fn define_service() -> ServiceDefinition {
    let mut service = ServiceDefinition::new(1, "Calculator");
    service.add_method(MethodDefinition {
        id: 1,
        name: "Add".to_string(),
        streaming_type: StreamingType::Unary,
        request_schema: None,
        response_schema: None,
    });
    service
}

/// Apply the token-bucket rate limiter to a request.
async fn check_rate_limit() -> Result<(), Box<dyn std::error::Error>> {
    let rate_limiter = RateLimitInterceptor::new(
        1000, // requests per second
        5000, // burst capacity
        DdosProtectionConfig::default(),
    );
    rate_limiter
        .check_rate_limit("client1", "my_service", 1024)
        .await?;
    Ok(())
}

/// Record one request and render the Prometheus text exposition.
async fn collect_metrics() -> String {
    let metrics = RpcMetrics::new();
    metrics
        .record_request("my_service", "my_method", Duration::from_millis(2))
        .await;
    metrics.to_prometheus().await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = define_service();
    println!(
        "service {} has {} method(s)",
        service.name,
        service.methods.len()
    );

    check_rate_limit().await?;
    let exposition = collect_metrics().await;
    println!("{exposition}");

    Ok(())
}
