//! Security monitoring and threat detection

use crate::{SecurityError, SecurityResult, SecurityEventType};
use crate::config::MonitoringConfig;
use crate::logging::{SecurityEvent, SecurityEventSeverity};
use chrono::{DateTime, Utc, Duration, Timelike, Datelike};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Security alert levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertLevel {
    Info,
    Warning,
    High,
    Critical,
}

/// Security alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAlert {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub level: AlertLevel,
    pub alert_type: String,
    pub title: String,
    pub description: String,
    pub source: String,
    pub affected_user: Option<String>,
    pub affected_resource: Option<String>,
    pub risk_score: f64,
    pub indicators: Vec<String>,
    pub recommended_actions: Vec<String>,
    pub acknowledged: bool,
    pub resolved: bool,
}

impl SecurityAlert {
    pub fn new(
        level: AlertLevel,
        alert_type: String,
        title: String,
        description: String,
        source: String,
        risk_score: f64,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            level,
            alert_type,
            title,
            description,
            source,
            affected_user: None,
            affected_resource: None,
            risk_score,
            indicators: Vec::new(),
            recommended_actions: Vec::new(),
            acknowledged: false,
            resolved: false,
        }
    }

    pub fn with_user(mut self, user_id: String) -> Self {
        self.affected_user = Some(user_id);
        self
    }

    pub fn with_resource(mut self, resource: String) -> Self {
        self.affected_resource = Some(resource);
        self
    }

    pub fn with_indicators(mut self, indicators: Vec<String>) -> Self {
        self.indicators = indicators;
        self
    }

    pub fn with_actions(mut self, actions: Vec<String>) -> Self {
        self.recommended_actions = actions;
        self
    }
}

/// Threat indicators
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatIndicator {
    pub indicator_type: String,
    pub value: String,
    pub confidence: f64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub count: u64,
    pub severity: AlertLevel,
}

/// Anomaly detection metrics
#[derive(Debug, Clone)]
pub struct AnomalyMetrics {
    pub baseline_avg: f64,
    pub baseline_stddev: f64,
    pub current_value: f64,
    pub anomaly_score: f64,
    pub threshold: f64,
}

/// Rate limiting violations
#[derive(Debug, Clone)]
pub struct RateViolation {
    pub user_id: Option<String>,
    pub ip_address: Option<String>,
    pub method: String,
    pub requests_per_minute: u64,
    pub limit: u64,
    pub first_violation: DateTime<Utc>,
    pub violation_count: u64,
}

/// Security monitoring system
pub struct SecurityMonitor {
    config: MonitoringConfig,
    alerts: Arc<RwLock<Vec<SecurityAlert>>>,
    threat_indicators: Arc<RwLock<HashMap<String, ThreatIndicator>>>,
    event_history: Arc<RwLock<VecDeque<SecurityEvent>>>,
    rate_violations: Arc<RwLock<HashMap<String, RateViolation>>>,
    baseline_metrics: Arc<RwLock<HashMap<String, AnomalyMetrics>>>,
}

impl SecurityMonitor {
    pub fn new(config: MonitoringConfig) -> SecurityResult<Self> {
        Ok(Self {
            config,
            alerts: Arc::new(RwLock::new(Vec::new())),
            threat_indicators: Arc::new(RwLock::new(HashMap::new())),
            event_history: Arc::new(RwLock::new(VecDeque::new())),
            rate_violations: Arc::new(RwLock::new(HashMap::new())),
            baseline_metrics: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Process security event for monitoring
    pub async fn process_security_event(&self, event: SecurityEvent) -> SecurityResult<()> {
        // Store event in history
        {
            let mut history = self.event_history.write().await;
            history.push_back(event.clone());
            
            // Keep only last 10000 events in memory
            if history.len() > 10000 {
                history.pop_front();
            }
        }

        // Analyze event for threats
        self.analyze_event_for_threats(&event).await?;

        // Check for anomalies
        self.detect_anomalies(&event).await?;

        // Update threat indicators
        self.update_threat_indicators(&event).await?;

        Ok(())
    }

    /// Analyze security event for threat patterns
    async fn analyze_event_for_threats(&self, event: &SecurityEvent) -> SecurityResult<()> {
        match event.event_type {
            SecurityEventType::Authentication => {
                self.analyze_auth_threats(event).await?;
            }
            SecurityEventType::Authorization => {
                self.analyze_authz_threats(event).await?;
            }
            SecurityEventType::RpcInvocation => {
                self.analyze_rpc_threats(event).await?;
            }
            SecurityEventType::SecurityViolation => {
                self.analyze_security_violations(event).await?;
            }
            _ => {}
        }

        Ok(())
    }

    /// Analyze authentication threats
    async fn analyze_auth_threats(&self, event: &SecurityEvent) -> SecurityResult<()> {
        if !event.success {
            // Failed authentication - potential brute force
            let user_key = event.user_id.clone().unwrap_or_else(|| "unknown".to_string());
            let ip_key = event.ip_address.clone().unwrap_or_else(|| "unknown".to_string());

            // Check for brute force by user
            let failed_attempts = self.count_failed_auth_attempts(&user_key, Duration::minutes(5)).await?;
            if failed_attempts >= 5 {
                self.create_alert(
                    AlertLevel::Warning,
                    "brute_force_user".to_string(),
                    "Potential brute force attack".to_string(),
                    format!("User {} has {} failed login attempts in 5 minutes", user_key, failed_attempts),
                    "auth_analyzer".to_string(),
                    0.7,
                ).await?;
            }

            // Check for brute force by IP
            let ip_failed_attempts = self.count_failed_auth_attempts_by_ip(&ip_key, Duration::minutes(5)).await?;
            if ip_failed_attempts >= 10 {
                self.create_alert(
                    AlertLevel::High,
                    "brute_force_ip".to_string(),
                    "IP-based brute force attack".to_string(),
                    format!("IP {} has {} failed login attempts in 5 minutes", ip_key, ip_failed_attempts),
                    "auth_analyzer".to_string(),
                    0.8,
                ).await?;
            }
        }

        Ok(())
    }

    /// Analyze authorization threats
    async fn analyze_authz_threats(&self, event: &SecurityEvent) -> SecurityResult<()> {
        if !event.success {
            // Failed authorization - potential privilege escalation attempt
            if let Some(user_id) = &event.user_id {
                let authz_failures = self.count_authz_failures(user_id, Duration::minutes(10)).await?;
                if authz_failures >= 3 {
                    self.create_alert(
                        AlertLevel::Warning,
                        "privilege_escalation".to_string(),
                        "Potential privilege escalation attempt".to_string(),
                        format!("User {} has {} authorization failures in 10 minutes", user_id, authz_failures),
                        "authz_analyzer".to_string(),
                        0.6,
                    ).await?;
                }
            }
        }

        Ok(())
    }

    /// Analyze RPC invocation threats
    async fn analyze_rpc_threats(&self, event: &SecurityEvent) -> SecurityResult<()> {
        // Check for unusual RPC method access patterns
        if let Some(method) = &event.method {
            let invocation_rate = self.calculate_rpc_invocation_rate(method, Duration::minutes(1)).await?;
            
            // Get baseline for this method
            let baseline_rate = self.get_baseline_rate(method).await;
            
            if invocation_rate > baseline_rate * 3.0 {
                self.create_alert(
                    AlertLevel::Warning,
                    "rpc_rate_anomaly".to_string(),
                    "Unusual RPC invocation rate".to_string(),
                    format!("Method {} invoked {} times/min, baseline: {}", method, invocation_rate, baseline_rate),
                    "rpc_analyzer".to_string(),
                    0.5,
                ).await?;
            }
        }

        Ok(())
    }

    /// Analyze security violations
    async fn analyze_security_violations(&self, event: &SecurityEvent) -> SecurityResult<()> {
        // All security violations are critical and should generate alerts
        let risk_score = event.risk_score.unwrap_or(0.9);
        
        let level = if risk_score >= 0.9 {
            AlertLevel::Critical
        } else if risk_score >= 0.7 {
            AlertLevel::High
        } else {
            AlertLevel::Warning
        };

        self.create_alert(
            level,
            "security_violation".to_string(),
            "Security violation detected".to_string(),
            event.message.clone(),
            event.source.clone(),
            risk_score,
        ).await?;

        Ok(())
    }

    /// Detect anomalies in system behavior
    async fn detect_anomalies(&self, event: &SecurityEvent) -> SecurityResult<()> {
        if !self.config.threat_detection_enabled {
            return Ok(());
        }

        // Detect anomalies based on event timing
        let hour_of_day = event.timestamp.hour();
        let day_of_week = event.timestamp.weekday().num_days_from_monday();

        // Check for unusual access times
        if hour_of_day < 6 || hour_of_day > 22 { // Outside normal business hours
            if event.event_type == SecurityEventType::RpcInvocation {
                self.create_alert(
                    AlertLevel::Info,
                    "unusual_access_time".to_string(),
                    "Access outside normal hours".to_string(),
                    format!("RPC access at {}:00 on day {}", hour_of_day, day_of_week),
                    "anomaly_detector".to_string(),
                    0.3,
                ).await?;
            }
        }

        Ok(())
    }

    /// Update threat indicators
    async fn update_threat_indicators(&self, event: &SecurityEvent) -> SecurityResult<()> {
        if let Some(ip_address) = &event.ip_address {
            let indicator_key = format!("ip:{}", ip_address);
            
            let mut indicators = self.threat_indicators.write().await;
            let indicator = indicators.entry(indicator_key.clone()).or_insert_with(|| ThreatIndicator {
                indicator_type: "ip_address".to_string(),
                value: ip_address.clone(),
                confidence: 0.1,
                first_seen: event.timestamp,
                last_seen: event.timestamp,
                count: 0,
                severity: AlertLevel::Info,
            });

            indicator.last_seen = event.timestamp;
            indicator.count += 1;

            // Increase confidence based on event severity
            match event.severity {
                SecurityEventSeverity::Critical => indicator.confidence += 0.3,
                SecurityEventSeverity::Error => indicator.confidence += 0.2,
                SecurityEventSeverity::Warning => indicator.confidence += 0.1,
                SecurityEventSeverity::Info => indicator.confidence += 0.05,
            }

            // Cap confidence at 1.0
            indicator.confidence = indicator.confidence.min(1.0);

            // Update severity based on confidence
            indicator.severity = if indicator.confidence >= 0.8 {
                AlertLevel::Critical
            } else if indicator.confidence >= 0.6 {
                AlertLevel::High
            } else if indicator.confidence >= 0.4 {
                AlertLevel::Warning
            } else {
                AlertLevel::Info
            };
        }

        Ok(())
    }

    /// Create and store security alert
    async fn create_alert(
        &self,
        level: AlertLevel,
        alert_type: String,
        title: String,
        description: String,
        source: String,
        risk_score: f64,
    ) -> SecurityResult<()> {
        let alert = SecurityAlert::new(level, alert_type, title, description, source, risk_score);

        // Store alert
        {
            let mut alerts = self.alerts.write().await;
            alerts.push(alert.clone());
        }

        // Send alert notification if configured
        if let Some(webhook_url) = &self.config.alert_webhook {
            self.send_alert_notification(webhook_url, &alert).await?;
        }

        Ok(())
    }

    /// Send alert notification to external system
    async fn send_alert_notification(&self, webhook_url: &str, alert: &SecurityAlert) -> SecurityResult<()> {
        let client = reqwest::Client::new();
        
        let response = client
            .post(webhook_url)
            .json(alert)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(SecurityError::NetworkError(
                format!("Failed to send alert notification: {}", response.status())
            ));
        }

        Ok(())
    }

    /// Count failed authentication attempts
    async fn count_failed_auth_attempts(&self, user_id: &str, duration: Duration) -> SecurityResult<u64> {
        let since = Utc::now() - duration;
        let history = self.event_history.read().await;
        
        let count = history
            .iter()
            .filter(|e| {
                e.timestamp >= since
                    && e.event_type == SecurityEventType::Authentication
                    && !e.success
                    && e.user_id.as_ref() == Some(&user_id.to_string())
            })
            .count() as u64;

        Ok(count)
    }

    /// Count failed authentication attempts by IP
    async fn count_failed_auth_attempts_by_ip(&self, ip_address: &str, duration: Duration) -> SecurityResult<u64> {
        let since = Utc::now() - duration;
        let history = self.event_history.read().await;
        
        let count = history
            .iter()
            .filter(|e| {
                e.timestamp >= since
                    && e.event_type == SecurityEventType::Authentication
                    && !e.success
                    && e.ip_address.as_ref() == Some(&ip_address.to_string())
            })
            .count() as u64;

        Ok(count)
    }

    /// Count authorization failures
    async fn count_authz_failures(&self, user_id: &str, duration: Duration) -> SecurityResult<u64> {
        let since = Utc::now() - duration;
        let history = self.event_history.read().await;
        
        let count = history
            .iter()
            .filter(|e| {
                e.timestamp >= since
                    && e.event_type == SecurityEventType::Authorization
                    && !e.success
                    && e.user_id.as_ref() == Some(&user_id.to_string())
            })
            .count() as u64;

        Ok(count)
    }

    /// Calculate RPC invocation rate
    async fn calculate_rpc_invocation_rate(&self, method: &str, duration: Duration) -> SecurityResult<f64> {
        let since = Utc::now() - duration;
        let history = self.event_history.read().await;
        
        let count = history
            .iter()
            .filter(|e| {
                e.timestamp >= since
                    && e.event_type == SecurityEventType::RpcInvocation
                    && e.method.as_ref() == Some(&method.to_string())
            })
            .count() as f64;

        Ok(count / duration.num_minutes() as f64)
    }

    /// Get baseline rate for RPC method
    async fn get_baseline_rate(&self, method: &str) -> f64 {
        let metrics = self.baseline_metrics.read().await;
        metrics.get(method)
            .map(|m| m.baseline_avg)
            .unwrap_or(10.0) // Default baseline
    }

    /// Get recent security alerts
    pub async fn get_recent_alerts(&self, limit: usize) -> SecurityResult<Vec<SecurityAlert>> {
        let alerts = self.alerts.read().await;
        let recent_alerts: Vec<SecurityAlert> = alerts
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect();
        Ok(recent_alerts)
    }

    /// Get unresolved alerts
    pub async fn get_unresolved_alerts(&self) -> SecurityResult<Vec<SecurityAlert>> {
        let alerts = self.alerts.read().await;
        let unresolved: Vec<SecurityAlert> = alerts
            .iter()
            .filter(|a| !a.resolved)
            .cloned()
            .collect();
        Ok(unresolved)
    }

    /// Acknowledge alert
    pub async fn acknowledge_alert(&self, alert_id: &str) -> SecurityResult<()> {
        let mut alerts = self.alerts.write().await;
        if let Some(alert) = alerts.iter_mut().find(|a| a.id == alert_id) {
            alert.acknowledged = true;
            Ok(())
        } else {
            Err(SecurityError::MonitoringError("Alert not found".to_string()))
        }
    }

    /// Resolve alert
    pub async fn resolve_alert(&self, alert_id: &str) -> SecurityResult<()> {
        let mut alerts = self.alerts.write().await;
        if let Some(alert) = alerts.iter_mut().find(|a| a.id == alert_id) {
            alert.resolved = true;
            alert.acknowledged = true;
            Ok(())
        } else {
            Err(SecurityError::MonitoringError("Alert not found".to_string()))
        }
    }

    /// Get threat indicators
    pub async fn get_threat_indicators(&self) -> SecurityResult<Vec<ThreatIndicator>> {
        let indicators = self.threat_indicators.read().await;
        Ok(indicators.values().cloned().collect())
    }

    /// Get high-confidence threat indicators
    pub async fn get_high_confidence_indicators(&self) -> SecurityResult<Vec<ThreatIndicator>> {
        let indicators = self.threat_indicators.read().await;
        let high_confidence: Vec<ThreatIndicator> = indicators
            .values()
            .filter(|i| i.confidence >= 0.7)
            .cloned()
            .collect();
        Ok(high_confidence)
    }

    /// Generate security metrics report
    pub async fn generate_metrics_report(&self, since: DateTime<Utc>) -> SecurityResult<SecurityMetricsReport> {
        let history = self.event_history.read().await;
        let alerts = self.alerts.read().await;

        let events_since = history
            .iter()
            .filter(|e| e.timestamp >= since)
            .collect::<Vec<_>>();

        let auth_events = events_since
            .iter()
            .filter(|e| e.event_type == SecurityEventType::Authentication)
            .count();

        let auth_failures = events_since
            .iter()
            .filter(|e| e.event_type == SecurityEventType::Authentication && !e.success)
            .count();

        let authz_events = events_since
            .iter()
            .filter(|e| e.event_type == SecurityEventType::Authorization)
            .count();

        let security_violations = events_since
            .iter()
            .filter(|e| e.event_type == SecurityEventType::SecurityViolation)
            .count();

        let critical_alerts = alerts
            .iter()
            .filter(|a| a.timestamp >= since && a.level == AlertLevel::Critical)
            .count();

        Ok(SecurityMetricsReport {
            period_start: since,
            period_end: Utc::now(),
            total_events: events_since.len(),
            authentication_events: auth_events,
            authentication_failures: auth_failures,
            authorization_events: authz_events,
            security_violations,
            critical_alerts,
            auth_success_rate: if auth_events > 0 {
                ((auth_events - auth_failures) as f64 / auth_events as f64) * 100.0
            } else {
                100.0
            },
        })
    }
}

/// Security metrics report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityMetricsReport {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_events: usize,
    pub authentication_events: usize,
    pub authentication_failures: usize,
    pub authorization_events: usize,
    pub security_violations: usize,
    pub critical_alerts: usize,
    pub auth_success_rate: f64,
}