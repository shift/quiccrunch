//! Production Infrastructure for QUIC Server/Client
//!
//! Provides health checks, graceful shutdown, configuration management,
//! and other production-grade features.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::signal;
use tokio::sync::{broadcast, RwLock};

/// Health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
        }
    }
}

/// Health check result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealthCheckResult {
    pub status: HealthStatus,
    pub message: Option<String>,
    pub timestamp: u64,
    pub details: HashMap<String, serde_json::Value>,
}

/// Health check trait
#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync {
    /// Check liveness (is the server running?)
    async fn check_liveness(&self) -> HealthCheckResult;

    /// Check readiness (can the server handle requests?)
    async fn check_readiness(&self) -> HealthCheckResult;

    /// Check specific component health
    async fn check_component(&self, component: &str) -> Option<HealthCheckResult>;
}

/// Server health check implementation
pub struct ServerHealthCheck {
    start_time: Instant,
    connections: Arc<RwLock<usize>>,
    max_connections: usize,
    is_shutting_down: Arc<std::sync::atomic::AtomicBool>,
}

impl ServerHealthCheck {
    pub fn new(max_connections: usize) -> Self {
        Self {
            start_time: Instant::now(),
            connections: Arc::new(RwLock::new(0)),
            max_connections,
            is_shutting_down: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn set_shutting_down(&self, shutting_down: bool) {
        self.is_shutting_down
            .store(shutting_down, std::sync::atomic::Ordering::Relaxed);
    }

    pub async fn increment_connections(&self) {
        let mut connections = self.connections.write().await;
        *connections += 1;
    }

    pub async fn decrement_connections(&self) {
        let mut connections = self.connections.write().await;
        *connections = connections.saturating_sub(1);
    }
}

#[async_trait::async_trait]
impl HealthCheck for ServerHealthCheck {
    async fn check_liveness(&self) -> HealthCheckResult {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        HealthCheckResult {
            status: HealthStatus::Healthy,
            message: Some("Server is running".to_string()),
            timestamp,
            details: HashMap::new(),
        }
    }

    async fn check_readiness(&self) -> HealthCheckResult {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let connections = *self.connections.read().await;
        let is_shutting_down = self
            .is_shutting_down
            .load(std::sync::atomic::Ordering::Relaxed);

        let (status, message) = if is_shutting_down {
            (
                HealthStatus::Unhealthy,
                Some("Server is shutting down".to_string()),
            )
        } else if connections >= self.max_connections {
            (
                HealthStatus::Degraded,
                Some(format!(
                    "Max connections reached: {}/{}",
                    connections, self.max_connections
                )),
            )
        } else {
            (
                HealthStatus::Healthy,
                Some(format!(
                    "Connections: {}/{}",
                    connections, self.max_connections
                )),
            )
        };

        let mut details = HashMap::new();
        details.insert("connections".to_string(), serde_json::json!(connections));
        details.insert(
            "max_connections".to_string(),
            serde_json::json!(self.max_connections),
        );
        details.insert(
            "uptime_seconds".to_string(),
            serde_json::json!(self.start_time.elapsed().as_secs()),
        );

        HealthCheckResult {
            status,
            message,
            timestamp,
            details,
        }
    }

    async fn check_component(&self, component: &str) -> Option<HealthCheckResult> {
        match component {
            "connections" => {
                let result = self.check_readiness().await;
                Some(result)
            }
            _ => None,
        }
    }
}

/// Graceful shutdown manager
pub struct ShutdownManager {
    shutdown_tx: broadcast::Sender<()>,
    _shutdown_rx: broadcast::Receiver<()>,
    is_shutting_down: Arc<std::sync::atomic::AtomicBool>,
    drain_timeout: Duration,
    active_connections: Arc<RwLock<usize>>,
}

impl ShutdownManager {
    pub fn new(drain_timeout: Duration) -> Self {
        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

        Self {
            shutdown_tx,
            _shutdown_rx: shutdown_rx,
            is_shutting_down: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            drain_timeout,
            active_connections: Arc::new(RwLock::new(0)),
        }
    }

    /// Start listening for shutdown signals
    pub async fn start_signal_handler(&self) {
        let shutdown_tx = self.shutdown_tx.clone();
        let is_shutting_down = self.is_shutting_down.clone();

        tokio::spawn(async move {
            // Wait for Ctrl+C
            let ctrl_c = signal::ctrl_c();

            #[cfg(unix)]
            {
                let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
                    .expect("Failed to install SIGTERM handler");

                tokio::select! {
                    _ = ctrl_c => {
                        log::info!("Received Ctrl+C, initiating graceful shutdown");
                    }
                    _ = sigterm.recv() => {
                        log::info!("Received SIGTERM, initiating graceful shutdown");
                    }
                }
            }

            #[cfg(not(unix))]
            {
                ctrl_c.await.ok();
                log::info!("Received Ctrl+C, initiating graceful shutdown");
            }

            is_shutting_down.store(true, std::sync::atomic::Ordering::Relaxed);
            let _ = shutdown_tx.send(());
        });
    }

    /// Wait for shutdown signal
    pub async fn wait_for_shutdown(&self) {
        let mut rx = self.shutdown_tx.subscribe();
        let _ = rx.recv().await;
    }

    /// Check if shutting down
    pub fn is_shutting_down(&self) -> bool {
        self.is_shutting_down
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Start draining connections
    pub async fn start_drain(&self) {
        log::info!(
            "Starting connection drain with timeout: {:?}",
            self.drain_timeout
        );

        let start = Instant::now();
        let active_connections = self.active_connections.clone();

        loop {
            let connections = *active_connections.read().await;
            if connections == 0 {
                log::info!("All connections drained");
                break;
            }

            if start.elapsed() >= self.drain_timeout {
                log::warn!(
                    "Drain timeout reached with {} active connections",
                    connections
                );
                break;
            }

            log::debug!("Waiting for {} connections to drain...", connections);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Register a new connection
    pub async fn register_connection(&self) {
        let mut connections = self.active_connections.write().await;
        *connections += 1;
    }

    /// Unregister a connection
    pub async fn unregister_connection(&self) {
        let mut connections = self.active_connections.write().await;
        *connections = connections.saturating_sub(1);
    }

    /// Get shutdown receiver
    pub fn shutdown_receiver(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }
}

/// Configuration management
pub struct ConfigManager {
    config: Arc<RwLock<serde_json::Value>>,
    /// Source path retained for configuration reload support.
    #[allow(dead_code)]
    config_path: Option<String>,
}

impl ConfigManager {
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(serde_json::Value::Object(
                serde_json::Map::new(),
            ))),
            config_path: None,
        }
    }

    pub fn with_config_path(path: String) -> Self {
        Self {
            config: Arc::new(RwLock::new(serde_json::Value::Object(
                serde_json::Map::new(),
            ))),
            config_path: Some(path),
        }
    }

    /// Load configuration from JSON file
    pub async fn load_from_json(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let content = tokio::fs::read_to_string(path).await?;
        let config: serde_json::Value = serde_json::from_str(&content)?;

        let mut current_config = self.config.write().await;
        *current_config = config;

        Ok(())
    }

    /// Load configuration from TOML file
    pub async fn load_from_toml(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let content = tokio::fs::read_to_string(path).await?;
        let config: serde_json::Value = toml::from_str(&content)?;

        let mut current_config = self.config.write().await;
        *current_config = config;

        Ok(())
    }

    /// Load configuration from YAML file
    pub async fn load_from_yaml(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let content = tokio::fs::read_to_string(path).await?;
        let config: serde_json::Value = serde_yaml::from_str(&content)?;

        let mut current_config = self.config.write().await;
        *current_config = config;

        Ok(())
    }

    /// Auto-detect file format and load configuration
    pub async fn load(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        if path.ends_with(".json") {
            self.load_from_json(path).await
        } else if path.ends_with(".toml") {
            self.load_from_toml(path).await
        } else if path.ends_with(".yaml") || path.ends_with(".yml") {
            self.load_from_yaml(path).await
        } else {
            // Try JSON as default
            self.load_from_json(path).await
        }
    }

    /// Load configuration from environment variables
    pub async fn load_from_env(&self, prefix: &str) {
        let mut config = self.config.write().await;
        let obj = config.as_object_mut().unwrap();

        for (key, value) in std::env::vars() {
            if key.starts_with(prefix) {
                let config_key = key
                    .strip_prefix(&format!("{}_", prefix))
                    .unwrap_or(&key)
                    .to_lowercase()
                    .replace('_', ".");

                // Try to parse as JSON value
                if let Ok(json_value) = serde_json::from_str(&format!("\"{}\"", value)) {
                    obj.insert(config_key, json_value);
                }
            }
        }
    }

    /// Get a configuration value
    pub async fn get(&self, key: &str) -> Option<serde_json::Value> {
        let config = self.config.read().await;
        let parts: Vec<&str> = key.split('.').collect();
        let mut current = &*config;

        for part in parts {
            match current.get(part) {
                Some(value) => current = value,
                None => return None,
            }
        }

        Some(current.clone())
    }

    /// Set a configuration value
    pub async fn set(&self, key: &str, value: serde_json::Value) {
        let mut config = self.config.write().await;
        let parts: Vec<&str> = key.split('.').collect();
        let mut current = &mut *config;

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                if let Some(obj) = current.as_object_mut() {
                    obj.insert(part.to_string(), value);
                }
                break;
            }

            // Create intermediate objects instead of panicking on a missing
            // path segment.
            let obj = match current.as_object_mut() {
                Some(obj) => obj,
                None => return,
            };
            current = obj
                .entry(part.to_string())
                .or_insert_with(|| serde_json::json!({}));
        }
    }

    /// Save configuration to JSON file
    pub async fn save_to_json(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let config = self.config.read().await;
        let content = serde_json::to_string_pretty(&*config)?;
        tokio::fs::write(path, content).await?;
        Ok(())
    }

    /// Save configuration to TOML file
    pub async fn save_to_toml(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let config = self.config.read().await;
        let content: String = toml::to_string_pretty(&*config)?;
        tokio::fs::write(path, content).await?;
        Ok(())
    }

    /// Save configuration to YAML file
    pub async fn save_to_yaml(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let config = self.config.read().await;
        let content: String = serde_yaml::to_string(&*config)?;
        tokio::fs::write(path, content).await?;
        Ok(())
    }
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Structured logging with correlation IDs
pub struct StructuredLogger {
    service_name: String,
    correlation_id: Arc<RwLock<Option<String>>>,
}

impl StructuredLogger {
    pub fn new(service_name: String) -> Self {
        Self {
            service_name,
            correlation_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Set correlation ID for current request
    pub async fn set_correlation_id(&self, id: String) {
        let mut correlation_id = self.correlation_id.write().await;
        *correlation_id = Some(id);
    }

    /// Get current correlation ID
    pub async fn get_correlation_id(&self) -> Option<String> {
        let correlation_id = self.correlation_id.read().await;
        correlation_id.clone()
    }

    /// Log a message with correlation ID
    pub async fn log(
        &self,
        level: log::Level,
        message: &str,
        context: HashMap<String, serde_json::Value>,
    ) {
        let correlation_id = self.get_correlation_id().await;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();

        let mut log_entry = serde_json::json!({
            "timestamp": timestamp,
            "service": self.service_name,
            "level": level.to_string(),
            "message": message,
        });

        if let Some(id) = correlation_id {
            log_entry["correlation_id"] = serde_json::json!(id);
        }

        for (key, value) in context {
            log_entry[key] = value;
        }

        match level {
            log::Level::Error => log::error!("{}", log_entry),
            log::Level::Warn => log::warn!("{}", log_entry),
            log::Level::Info => log::info!("{}", log_entry),
            log::Level::Debug => log::debug!("{}", log_entry),
            log::Level::Trace => log::trace!("{}", log_entry),
        }
    }

    /// Log a request
    pub async fn log_request(
        &self,
        method: &str,
        path: &str,
        headers: &HashMap<String, String>,
        body_size: usize,
    ) {
        let mut context = HashMap::new();
        context.insert("type".to_string(), serde_json::json!("request"));
        context.insert("method".to_string(), serde_json::json!(method));
        context.insert("path".to_string(), serde_json::json!(path));
        context.insert("body_size".to_string(), serde_json::json!(body_size));
        context.insert("headers".to_string(), serde_json::json!(headers));

        self.log(log::Level::Info, "Incoming request", context)
            .await;
    }

    /// Log a response
    pub async fn log_response(&self, status: u16, path: &str, body_size: usize, duration_ms: u64) {
        let mut context = HashMap::new();
        context.insert("type".to_string(), serde_json::json!("response"));
        context.insert("status".to_string(), serde_json::json!(status));
        context.insert("path".to_string(), serde_json::json!(path));
        context.insert("body_size".to_string(), serde_json::json!(body_size));
        context.insert("duration_ms".to_string(), serde_json::json!(duration_ms));

        self.log(log::Level::Info, "Outgoing response", context)
            .await;
    }

    /// Log an error
    pub async fn log_error(&self, error: &str, path: &str, status: u16) {
        let mut context = HashMap::new();
        context.insert("type".to_string(), serde_json::json!("error"));
        context.insert("error".to_string(), serde_json::json!(error));
        context.insert("path".to_string(), serde_json::json!(path));
        context.insert("status".to_string(), serde_json::json!(status));

        self.log(log::Level::Error, "Request error", context).await;
    }
}

/// TLS certificate rotation support
pub struct CertificateRotator {
    cert_path: String,
    key_path: String,
    check_interval: Duration,
    last_check: Arc<RwLock<Instant>>,
    on_rotation: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl CertificateRotator {
    pub fn new(cert_path: String, key_path: String, check_interval: Duration) -> Self {
        Self {
            cert_path,
            key_path,
            check_interval,
            last_check: Arc::new(RwLock::new(Instant::now())),
            on_rotation: None,
        }
    }

    pub fn with_on_rotation<F: Fn() + Send + Sync + 'static>(mut self, callback: F) -> Self {
        self.on_rotation = Some(Arc::new(callback));
        self
    }

    /// Start certificate rotation checker
    pub fn start(self: &Arc<Self>) {
        let cert_path = self.cert_path.clone();
        let key_path = self.key_path.clone();
        let check_interval = self.check_interval;
        let last_check = self.last_check.clone();
        let on_rotation = self.on_rotation.clone();

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(check_interval).await;

                let mut last = last_check.write().await;
                if last.elapsed() >= check_interval {
                    // Check if certificates have been modified
                    if let (Ok(_cert_meta), Ok(_key_meta)) = (
                        tokio::fs::metadata(&cert_path).await,
                        tokio::fs::metadata(&key_path).await,
                    ) {
                        log::debug!(
                            "Checking certificate rotation: cert={}, key={}",
                            cert_path,
                            key_path
                        );

                        // In a real implementation, we would check modification times
                        // and reload certificates if needed

                        if let Some(callback) = &on_rotation {
                            callback();
                        }
                    }

                    *last = Instant::now();
                }
            }
        });
    }
}

/// Production server wrapper
pub struct ProductionServer {
    health_check: Arc<ServerHealthCheck>,
    shutdown_manager: Arc<ShutdownManager>,
    /// Retained for runtime configuration access.
    #[allow(dead_code)]
    config_manager: Arc<ConfigManager>,
    /// Retained for structured production logging.
    #[allow(dead_code)]
    logger: Arc<StructuredLogger>,
}

impl ProductionServer {
    pub fn new(service_name: String, max_connections: usize, drain_timeout: Duration) -> Self {
        Self {
            health_check: Arc::new(ServerHealthCheck::new(max_connections)),
            shutdown_manager: Arc::new(ShutdownManager::new(drain_timeout)),
            config_manager: Arc::new(ConfigManager::new()),
            logger: Arc::new(StructuredLogger::new(service_name)),
        }
    }

    /// Start the production server
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Start shutdown signal handler
        self.shutdown_manager.start_signal_handler().await;

        log::info!("Production server started");

        Ok(())
    }

    /// Wait for shutdown signal and drain connections
    pub async fn shutdown(&self) {
        log::info!("Initiating graceful shutdown");

        // Mark as shutting down
        self.health_check.set_shutting_down(true);

        // Wait for shutdown signal
        self.shutdown_manager.wait_for_shutdown().await;

        // Start draining connections
        self.shutdown_manager.start_drain().await;

        log::info!("Graceful shutdown complete");
    }

    /// Get health check endpoint data
    pub async fn health_check(&self) -> HealthCheckResult {
        self.health_check.check_readiness().await
    }

    /// Get liveness probe data
    pub async fn liveness(&self) -> HealthCheckResult {
        self.health_check.check_liveness().await
    }

    /// Get readiness probe data
    pub async fn readiness(&self) -> HealthCheckResult {
        self.health_check.check_readiness().await
    }
}

/// Health check HTTP endpoint handler
pub async fn handle_health_endpoint(
    health_check: Arc<ServerHealthCheck>,
    path: &str,
) -> (u16, serde_json::Value) {
    match path {
        "/health/live" => {
            let result = health_check.check_liveness().await;
            let status = match result.status {
                HealthStatus::Healthy => 200,
                HealthStatus::Degraded => 200,
                HealthStatus::Unhealthy => 503,
            };
            (status, serde_json::to_value(result).unwrap_or_default())
        }
        "/health/ready" => {
            let result = health_check.check_readiness().await;
            let status = match result.status {
                HealthStatus::Healthy => 200,
                HealthStatus::Degraded => 200,
                HealthStatus::Unhealthy => 503,
            };
            (status, serde_json::to_value(result).unwrap_or_default())
        }
        "/health" => {
            let result = health_check.check_readiness().await;
            let status = match result.status {
                HealthStatus::Healthy => 200,
                HealthStatus::Degraded => 200,
                HealthStatus::Unhealthy => 503,
            };
            (status, serde_json::to_value(result).unwrap_or_default())
        }
        _ => (404, serde_json::json!({"error": "Not found"})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check() {
        let health_check = ServerHealthCheck::new(100);

        let liveness = health_check.check_liveness().await;
        assert_eq!(liveness.status, HealthStatus::Healthy);

        let readiness = health_check.check_readiness().await;
        assert_eq!(readiness.status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_config_manager() {
        let config_manager = ConfigManager::new();

        config_manager
            .set("server.host", serde_json::json!("127.0.0.1"))
            .await;
        config_manager
            .set("server.port", serde_json::json!(8080))
            .await;

        let host = config_manager.get("server.host").await;
        assert_eq!(host, Some(serde_json::json!("127.0.0.1")));

        let port = config_manager.get("server.port").await;
        assert_eq!(port, Some(serde_json::json!(8080)));
    }

    #[tokio::test]
    async fn test_shutdown_manager() {
        let shutdown_manager = ShutdownManager::new(Duration::from_secs(30));

        assert!(!shutdown_manager.is_shutting_down());
    }
}
