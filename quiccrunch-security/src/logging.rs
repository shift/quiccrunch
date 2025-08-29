//! Security event logging and audit trail

use crate::{SecurityError, SecurityResult, SecurityEventType};
use crate::config::LoggingConfig;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{event, Level};
use uuid::Uuid;

/// Security event for logging and audit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: SecurityEventType,
    pub severity: SecurityEventSeverity,
    pub source: String,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub request_id: Option<String>,
    pub method: Option<String>,
    pub resource: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub success: bool,
    pub message: String,
    pub details: HashMap<String, serde_json::Value>,
    pub risk_score: Option<f64>,
}

impl SecurityEvent {
    pub fn new(
        event_type: SecurityEventType,
        severity: SecurityEventSeverity,
        source: String,
        message: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event_type,
            severity,
            source,
            user_id: None,
            session_id: None,
            request_id: None,
            method: None,
            resource: None,
            ip_address: None,
            user_agent: None,
            success: true,
            message,
            details: HashMap::new(),
            risk_score: None,
        }
    }

    pub fn with_user(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn with_session(mut self, session_id: String) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub fn with_request(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }

    pub fn with_method(mut self, method: String) -> Self {
        self.method = Some(method);
        self
    }

    pub fn with_success(mut self, success: bool) -> Self {
        self.success = success;
        self
    }

    pub fn with_detail<V: Into<serde_json::Value>>(mut self, key: String, value: V) -> Self {
        self.details.insert(key, value.into());
        self
    }

    pub fn with_risk_score(mut self, score: f64) -> Self {
        self.risk_score = Some(score);
        self
    }
}

/// Security event severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecurityEventSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

impl SecurityEventSeverity {
    pub fn to_tracing_level(&self) -> Level {
        match self {
            SecurityEventSeverity::Info => Level::INFO,
            SecurityEventSeverity::Warning => Level::WARN,
            SecurityEventSeverity::Error => Level::ERROR,
            SecurityEventSeverity::Critical => Level::ERROR,
        }
    }
}

/// Security logger for audit trails and monitoring
pub struct SecurityLogger {
    config: LoggingConfig,
    event_buffer: Arc<RwLock<Vec<SecurityEvent>>>,
    #[cfg(feature = "compliance")]
    db_pool: Option<sqlx::PgPool>,
}

impl SecurityLogger {
    pub fn new(config: LoggingConfig) -> SecurityResult<Self> {
        Ok(Self {
            config,
            event_buffer: Arc::new(RwLock::new(Vec::new())),
            #[cfg(feature = "compliance")]
            db_pool: None,
        })
    }

    #[cfg(feature = "compliance")]
    pub async fn with_database(mut self, database_url: &str) -> SecurityResult<Self> {
        let pool = sqlx::PgPool::connect(database_url).await?;
        
        // Create audit table if it doesn't exist
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS security_audit_log (
                id UUID PRIMARY KEY,
                timestamp TIMESTAMPTZ NOT NULL,
                event_type VARCHAR(50) NOT NULL,
                severity VARCHAR(20) NOT NULL,
                source VARCHAR(100) NOT NULL,
                user_id VARCHAR(100),
                session_id VARCHAR(100),
                request_id VARCHAR(100),
                method VARCHAR(100),
                resource VARCHAR(200),
                ip_address INET,
                user_agent TEXT,
                success BOOLEAN NOT NULL,
                message TEXT NOT NULL,
                details JSONB,
                risk_score REAL
            )
            "#,
        )
        .execute(&pool)
        .await?;

        self.db_pool = Some(pool);
        Ok(self)
    }

    /// Log a security event
    pub async fn log_event(&self, event: SecurityEvent) -> SecurityResult<()> {
        // Log to tracing
        event!(
            target: "security",
            event.severity.to_tracing_level(),
            event_id = %event.id,
            event_type = ?event.event_type,
            user_id = ?event.user_id,
            session_id = ?event.session_id,
            success = event.success,
            "{}",
            event.message
        );

        // Store in buffer for batch processing
        {
            let mut buffer = self.event_buffer.write().await;
            buffer.push(event.clone());
        }

        // Store in database if compliance feature is enabled
        #[cfg(feature = "compliance")]
        if let Some(pool) = &self.db_pool {
            self.store_event_in_db(pool, &event).await?;
        }

        // Flush buffer if needed
        self.flush_buffer_if_needed().await?;

        Ok(())
    }

    /// Log authentication event
    pub async fn log_auth_event(
        &self,
        user_id: Option<String>,
        session_id: String,
        success: bool,
        message: String,
    ) -> SecurityResult<()> {
        let severity = if success {
            SecurityEventSeverity::Info
        } else {
            SecurityEventSeverity::Warning
        };

        let mut event = SecurityEvent::new(
            SecurityEventType::Authentication,
            severity,
            "auth_manager".to_string(),
            message,
        )
        .with_session(session_id)
        .with_success(success);

        if let Some(user_id) = user_id {
            event = event.with_user(user_id);
        }

        self.log_event(event).await
    }

    /// Log authorization event
    pub async fn log_authz_event(
        &self,
        user_id: String,
        resource: String,
        permission: String,
        granted: bool,
        message: String,
    ) -> SecurityResult<()> {
        let severity = if granted {
            SecurityEventSeverity::Info
        } else {
            SecurityEventSeverity::Warning
        };

        let event = SecurityEvent::new(
            SecurityEventType::Authorization,
            severity,
            "rbac_manager".to_string(),
            message,
        )
        .with_user(user_id)
        .with_detail("resource".to_string(), resource)
        .with_detail("permission".to_string(), permission)
        .with_success(granted);

        self.log_event(event).await
    }

    /// Log RPC invocation event
    pub async fn log_rpc_event(
        &self,
        user_id: Option<String>,
        session_id: String,
        request_id: String,
        method: String,
        success: bool,
        execution_time_ms: u64,
    ) -> SecurityResult<()> {
        let severity = if success {
            SecurityEventSeverity::Info
        } else {
            SecurityEventSeverity::Error
        };

        let mut event = SecurityEvent::new(
            SecurityEventType::RpcInvocation,
            severity,
            "rpc_server".to_string(),
            format!("RPC method {} {}", method, if success { "succeeded" } else { "failed" }),
        )
        .with_session(session_id)
        .with_request(request_id)
        .with_method(method)
        .with_success(success)
        .with_detail("execution_time_ms".to_string(), execution_time_ms);

        if let Some(user_id) = user_id {
            event = event.with_user(user_id);
        }

        self.log_event(event).await
    }

    /// Log security violation
    pub async fn log_security_violation(
        &self,
        violation_type: String,
        source: String,
        user_id: Option<String>,
        details: HashMap<String, serde_json::Value>,
        risk_score: f64,
    ) -> SecurityResult<()> {
        let mut event = SecurityEvent::new(
            SecurityEventType::SecurityViolation,
            SecurityEventSeverity::Critical,
            source,
            format!("Security violation: {}", violation_type),
        )
        .with_risk_score(risk_score)
        .with_success(false);

        if let Some(user_id) = user_id {
            event = event.with_user(user_id);
        }

        for (key, value) in details {
            event = event.with_detail(key, value);
        }

        self.log_event(event).await
    }

    /// Get recent security events
    pub async fn get_recent_events(&self, limit: usize) -> SecurityResult<Vec<SecurityEvent>> {
        let buffer = self.event_buffer.read().await;
        let events: Vec<SecurityEvent> = buffer
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect();
        Ok(events)
    }

    /// Get events by user
    pub async fn get_events_by_user(&self, user_id: &str, limit: usize) -> SecurityResult<Vec<SecurityEvent>> {
        let buffer = self.event_buffer.read().await;
        let events: Vec<SecurityEvent> = buffer
            .iter()
            .filter(|e| e.user_id.as_ref() == Some(&user_id.to_string()))
            .rev()
            .take(limit)
            .cloned()
            .collect();
        Ok(events)
    }

    /// Get security violations
    pub async fn get_security_violations(&self, since: DateTime<Utc>) -> SecurityResult<Vec<SecurityEvent>> {
        let buffer = self.event_buffer.read().await;
        let events: Vec<SecurityEvent> = buffer
            .iter()
            .filter(|e| {
                e.event_type == SecurityEventType::SecurityViolation
                    && e.timestamp >= since
            })
            .cloned()
            .collect();
        Ok(events)
    }

    /// Flush event buffer to persistent storage
    async fn flush_buffer_if_needed(&self) -> SecurityResult<()> {
        let should_flush = {
            let buffer = self.event_buffer.read().await;
            buffer.len() >= 1000 // Flush when buffer reaches 1000 events
        };

        if should_flush {
            self.flush_buffer().await?;
        }

        Ok(())
    }

    /// Force flush event buffer
    pub async fn flush_buffer(&self) -> SecurityResult<()> {
        let events = {
            let mut buffer = self.event_buffer.write().await;
            let events = buffer.clone();
            buffer.clear();
            events
        };

        if !events.is_empty() {
            tracing::info!("Flushing {} security events to persistent storage", events.len());
            
            // Here you would implement additional persistent storage
            // For now, we'll just log the flush operation
        }

        Ok(())
    }

    #[cfg(feature = "compliance")]
    async fn store_event_in_db(&self, pool: &sqlx::PgPool, event: &SecurityEvent) -> SecurityResult<()> {
        let event_type_str = format!("{:?}", event.event_type);
        let severity_str = format!("{:?}", event.severity);
        let details_json = serde_json::to_value(&event.details)?;

        sqlx::query(
            r#"
            INSERT INTO security_audit_log (
                id, timestamp, event_type, severity, source, user_id, session_id,
                request_id, method, resource, ip_address, user_agent, success,
                message, details, risk_score
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            "#,
        )
        .bind(Uuid::parse_str(&event.id).map_err(|e| SecurityError::InternalError(e.to_string()))?)
        .bind(event.timestamp)
        .bind(event_type_str)
        .bind(severity_str)
        .bind(&event.source)
        .bind(&event.user_id)
        .bind(&event.session_id)
        .bind(&event.request_id)
        .bind(&event.method)
        .bind(&event.resource)
        .bind(event.ip_address.as_ref().and_then(|ip| ip.parse::<std::net::IpAddr>().ok()))
        .bind(&event.user_agent)
        .bind(event.success)
        .bind(&event.message)
        .bind(details_json)
        .bind(event.risk_score)
        .execute(pool)
        .await?;

        Ok(())
    }
}

/// Audit logger for compliance requirements
pub struct AuditLogger {
    security_logger: SecurityLogger,
}

impl AuditLogger {
    pub fn new(config: LoggingConfig) -> SecurityResult<Self> {
        Ok(Self {
            security_logger: SecurityLogger::new(config)?,
        })
    }

    /// Log data access event for compliance
    pub async fn log_data_access(
        &self,
        user_id: String,
        resource: String,
        operation: String,
        success: bool,
    ) -> SecurityResult<()> {
        let event = SecurityEvent::new(
            SecurityEventType::ComplianceEvent,
            SecurityEventSeverity::Info,
            "audit_logger".to_string(),
            format!("Data access: {} {} on {}", user_id, operation, resource),
        )
        .with_user(user_id)
        .with_detail("operation".to_string(), operation)
        .with_detail("resource".to_string(), resource)
        .with_success(success);

        self.security_logger.log_event(event).await
    }

    /// Log administrative action for compliance
    pub async fn log_admin_action(
        &self,
        admin_user_id: String,
        action: String,
        target: Option<String>,
        success: bool,
    ) -> SecurityResult<()> {
        let mut event = SecurityEvent::new(
            SecurityEventType::ComplianceEvent,
            SecurityEventSeverity::Warning,
            "audit_logger".to_string(),
            format!("Administrative action: {} performed {}", admin_user_id, action),
        )
        .with_user(admin_user_id)
        .with_detail("action".to_string(), action)
        .with_success(success);

        if let Some(target) = target {
            event = event.with_detail("target".to_string(), target);
        }

        self.security_logger.log_event(event).await
    }

    /// Log configuration change for compliance
    pub async fn log_config_change(
        &self,
        user_id: String,
        component: String,
        old_value: Option<String>,
        new_value: String,
    ) -> SecurityResult<()> {
        let mut event = SecurityEvent::new(
            SecurityEventType::ComplianceEvent,
            SecurityEventSeverity::Warning,
            "audit_logger".to_string(),
            format!("Configuration change in {}", component),
        )
        .with_user(user_id)
        .with_detail("component".to_string(), component)
        .with_detail("new_value".to_string(), new_value);

        if let Some(old_value) = old_value {
            event = event.with_detail("old_value".to_string(), old_value);
        }

        self.security_logger.log_event(event).await
    }
}