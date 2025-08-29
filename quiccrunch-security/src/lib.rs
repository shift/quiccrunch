//! QuicCrunch Security Framework
//! 
//! Comprehensive security framework providing authentication, authorization,
//! monitoring, and compliance capabilities for QuicCrunch RPC services.

pub mod auth;
pub mod rbac;
pub mod monitoring;
pub mod logging;
pub mod testing;
pub mod compliance;

pub mod error;
pub mod config;

pub use error::{SecurityError, SecurityResult};
pub use config::SecurityConfig;

/// Security framework version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Security event types for monitoring and audit
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum SecurityEventType {
    Authentication,
    Authorization,
    RpcInvocation,
    SecurityViolation,
    SystemAnomaly,
    ComplianceEvent,
}

/// Security context for RPC operations
#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub user_id: Option<String>,
    pub session_id: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub authenticated: bool,
    pub request_id: String,
}

impl SecurityContext {
    pub fn new(session_id: String, request_id: String) -> Self {
        Self {
            user_id: None,
            session_id,
            roles: Vec::new(),
            permissions: Vec::new(),
            authenticated: false,
            request_id,
        }
    }

    pub fn with_user(mut self, user_id: String, roles: Vec<String>) -> Self {
        self.user_id = Some(user_id);
        self.roles = roles;
        self.authenticated = true;
        self
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission)
    }
}

/// Main security framework entry point
pub struct SecurityFramework {
    auth: auth::AuthenticationManager,
    rbac: rbac::RbacManager,
    monitor: monitoring::SecurityMonitor,
    logger: logging::SecurityLogger,
}

impl SecurityFramework {
    pub fn new(config: SecurityConfig) -> SecurityResult<Self> {
        Ok(Self {
            auth: auth::AuthenticationManager::new(config.auth.clone())?,
            rbac: rbac::RbacManager::new(config.rbac.clone())?,
            monitor: monitoring::SecurityMonitor::new(config.monitoring.clone())?,
            logger: logging::SecurityLogger::new(config.logging.clone())?,
        })
    }

    pub fn auth(&self) -> &auth::AuthenticationManager {
        &self.auth
    }

    pub fn rbac(&self) -> &rbac::RbacManager {
        &self.rbac
    }

    pub fn monitor(&self) -> &monitoring::SecurityMonitor {
        &self.monitor
    }

    pub fn logger(&self) -> &logging::SecurityLogger {
        &self.logger
    }
}