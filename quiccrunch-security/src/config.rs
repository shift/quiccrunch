//! Security configuration management

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Main security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub auth: AuthConfig,
    pub rbac: RbacConfig,
    pub monitoring: MonitoringConfig,
    pub logging: LoggingConfig,
    pub rate_limiting: RateLimitingConfig,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            auth: AuthConfig::default(),
            rbac: RbacConfig::default(),
            monitoring: MonitoringConfig::default(),
            logging: LoggingConfig::default(),
            rate_limiting: RateLimitingConfig::default(),
        }
    }
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub jwt_expiration: Duration,
    pub jwt_refresh_expiration: Duration,
    pub oauth2_client_id: Option<String>,
    pub oauth2_client_secret: Option<String>,
    pub oauth2_auth_url: Option<String>,
    pub oauth2_token_url: Option<String>,
    pub require_client_certificates: bool,
    pub certificate_validation_strict: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: "default-secret-change-in-production".to_string(),
            jwt_expiration: Duration::from_secs(3600), // 1 hour
            jwt_refresh_expiration: Duration::from_secs(2592000), // 30 days
            oauth2_client_id: None,
            oauth2_client_secret: None,
            oauth2_auth_url: None,
            oauth2_token_url: None,
            require_client_certificates: false,
            certificate_validation_strict: true,
        }
    }
}

/// RBAC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RbacConfig {
    pub default_role: String,
    pub admin_roles: Vec<String>,
    pub permission_cache_ttl: Duration,
    pub policy_refresh_interval: Duration,
    pub policy_file: Option<String>,
    pub policy_adapter: Option<String>,
}

impl Default for RbacConfig {
    fn default() -> Self {
        Self {
            default_role: "user".to_string(),
            admin_roles: vec!["admin".to_string(), "super_admin".to_string()],
            permission_cache_ttl: Duration::from_secs(300), // 5 minutes
            policy_refresh_interval: Duration::from_secs(3600), // 1 hour
            policy_file: None,
            policy_adapter: None,
        }
    }
}

/// Security monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub enabled: bool,
    pub log_level: String,
    pub siem_endpoint: Option<String>,
    pub siem_api_key: Option<String>,
    pub threat_detection_enabled: bool,
    pub alert_webhook: Option<String>,
    pub metrics_endpoint: Option<String>,
    pub anomaly_detection_threshold: f64,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_level: "info".to_string(),
            siem_endpoint: None,
            siem_api_key: None,
            threat_detection_enabled: true,
            alert_webhook: None,
            metrics_endpoint: None,
            anomaly_detection_threshold: 0.95,
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub enabled: bool,
    pub log_file: Option<String>,
    pub audit_log_file: Option<String>,
    pub log_format: LogFormat,
    pub retention_days: u32,
    pub max_file_size: u64,
    pub syslog_enabled: bool,
    pub syslog_facility: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_file: Some("/var/log/quiccrunch/security.log".to_string()),
            audit_log_file: Some("/var/log/quiccrunch/audit.log".to_string()),
            log_format: LogFormat::Json,
            retention_days: 90,
            max_file_size: 100 * 1024 * 1024, // 100MB
            syslog_enabled: false,
            syslog_facility: "local0".to_string(),
        }
    }
}

/// Log format options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    Json,
    Text,
    Syslog,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitingConfig {
    pub enabled: bool,
    pub default_requests_per_second: u32,
    pub default_burst_size: u32,
    pub per_method_limits: std::collections::HashMap<String, MethodRateLimit>,
    pub per_user_limits: std::collections::HashMap<String, UserRateLimit>,
}

impl Default for RateLimitingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_requests_per_second: 100,
            default_burst_size: 200,
            per_method_limits: std::collections::HashMap::new(),
            per_user_limits: std::collections::HashMap::new(),
        }
    }
}

/// Method-specific rate limiting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodRateLimit {
    pub requests_per_second: u32,
    pub burst_size: u32,
}

/// User-specific rate limiting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRateLimit {
    pub requests_per_second: u32,
    pub burst_size: u32,
    pub daily_limit: Option<u32>,
}

impl SecurityConfig {
    /// Load configuration from environment variables and config files
    pub fn from_env() -> crate::SecurityResult<Self> {
        let mut config = config::Config::builder();

        // Load from default configuration
        config = config.add_source(config::Config::try_from(&SecurityConfig::default())?);

        // Load from environment variables with SECURITY_ prefix
        config = config.add_source(
            config::Environment::with_prefix("SECURITY")
                .separator("_")
                .try_parsing(true),
        );

        // Load from config file if exists
        if let Ok(config_file) = std::env::var("SECURITY_CONFIG_FILE") {
            config = config.add_source(config::File::with_name(&config_file));
        }

        let config = config.build()
            .map_err(|e| crate::SecurityError::ConfigurationError(e.to_string()))?;

        config.try_deserialize()
            .map_err(|e| crate::SecurityError::ConfigurationError(e.to_string()))
    }

    /// Validate configuration
    pub fn validate(&self) -> crate::SecurityResult<()> {
        // Validate JWT secret strength
        if self.auth.jwt_secret.len() < 32 {
            return Err(crate::SecurityError::ConfigurationError(
                "JWT secret must be at least 32 characters long".to_string(),
            ));
        }

        // Validate OAuth2 configuration if enabled
        if self.auth.oauth2_client_id.is_some() {
            if self.auth.oauth2_client_secret.is_none() {
                return Err(crate::SecurityError::ConfigurationError(
                    "OAuth2 client secret is required when client ID is set".to_string(),
                ));
            }
        }

        // Validate log file paths
        if let Some(log_file) = &self.logging.log_file {
            if let Some(parent) = std::path::Path::new(log_file).parent() {
                if !parent.exists() {
                    return Err(crate::SecurityError::ConfigurationError(
                        format!("Log directory does not exist: {}", parent.display()),
                    ));
                }
            }
        }

        Ok(())
    }
}