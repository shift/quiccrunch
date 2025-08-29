# QuicCrunch Security Framework

A comprehensive security framework for the QuicCrunch QUIC-based RPC framework, addressing critical security gaps identified in the SDLC gap analysis.

## Overview

The QuicCrunch Security Framework provides enterprise-grade security capabilities including:

- **Authentication & Authorization**: JWT/OAuth2 authentication with RBAC
- **Security Monitoring**: Real-time threat detection and alerting
- **Audit Logging**: Comprehensive security event logging and compliance tracking
- **Security Testing**: Automated penetration testing and vulnerability scanning
- **Compliance Management**: SOC2, GDPR, and other regulatory framework support
- **Incident Response**: Structured incident response procedures and tools

## Features

### 🔐 Authentication Framework
- JWT-based stateless authentication
- OAuth2 integration support
- Multi-factor authentication ready
- Session management with configurable expiration
- Password complexity enforcement
- Account lockout protection

### 🛡️ Role-Based Access Control (RBAC)
- Fine-grained permission system
- Role inheritance and delegation
- Method-level access control for RPC
- Dynamic policy evaluation
- Permission caching for performance
- Casbin integration for policy management

### 📊 Security Monitoring
- Real-time threat detection
- Anomaly detection algorithms
- Security alert management
- Threat indicator tracking
- SIEM integration support
- Automated incident escalation

### 📋 Audit & Compliance
- Comprehensive security event logging
- SOC2 Type II preparation
- GDPR compliance support
- Automated evidence collection
- Compliance assessment tools
- Regulatory reporting capabilities

### 🔍 Security Testing
- Automated penetration testing
- Vulnerability scanning
- Security regression testing
- Protocol-specific security tests
- Continuous security validation
- Detailed security reporting

## Quick Start

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
quiccrunch-security = { path = "./quiccrunch-security" }
tokio = { version = "1.0", features = ["full"] }
```

### Basic Usage

```rust
use quiccrunch_security::{SecurityFramework, SecurityConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = SecurityConfig::from_env()?;
    
    // Initialize security framework
    let security = SecurityFramework::new(config)?;
    
    // Create a user
    let user = security.auth()
        .create_user(
            "user123".to_string(),
            "john_doe".to_string(),
            Some("john@example.com".to_string()),
            vec!["user".to_string()],
        )
        .await?;
    
    // Authenticate user
    let token = security.auth()
        .authenticate_user("john_doe", "secure_password")
        .await?;
    
    // Validate token and get security context
    let context = security.auth()
        .validate_token(&token.access_token)
        .await?;
    
    // Check permissions
    let decision = security.rbac()
        .check_permission_with_context(&context, "user_data", "read")
        .await?;
    
    if decision.granted {
        println!("Access granted!");
    } else {
        println!("Access denied: {}", decision.reason);
    }
    
    Ok(())
}
```

### CLI Usage

The security framework includes a command-line interface for management:

```bash
# Create a user
./security-cli auth create-user \
    --user-id user123 \
    --username john_doe \
    --email john@example.com \
    --roles admin,user

# Authenticate user
./security-cli auth login \
    --username john_doe \
    --password secure_password

# Create a role
./security-cli rbac create-role \
    --name admin \
    --description "Administrator role" \
    --permissions "user_data:read,user_data:write,system:admin"

# Check permissions
./security-cli rbac check-permission \
    --user-id user123 \
    --resource user_data \
    --action read

# View security alerts
./security-cli monitor alerts --limit 10

# Run security tests
./security-cli test run \
    --host localhost \
    --port 8080 \
    --categories auth,authz,input

# Perform compliance assessment
./security-cli compliance assess \
    --framework soc2 \
    --assessor "Security Team" \
    --scope "Production Environment"
```

## Configuration

### Environment Variables

```bash
# Authentication configuration
export SECURITY_AUTH_JWT_SECRET="your-256-bit-secret-key-here"
export SECURITY_AUTH_JWT_EXPIRATION="3600"  # 1 hour
export SECURITY_AUTH_REQUIRE_CLIENT_CERTIFICATES="false"

# OAuth2 configuration (optional)
export SECURITY_AUTH_OAUTH2_CLIENT_ID="your-oauth2-client-id"
export SECURITY_AUTH_OAUTH2_CLIENT_SECRET="your-oauth2-client-secret"
export SECURITY_AUTH_OAUTH2_AUTH_URL="https://oauth.provider.com/auth"
export SECURITY_AUTH_OAUTH2_TOKEN_URL="https://oauth.provider.com/token"

# RBAC configuration
export SECURITY_RBAC_DEFAULT_ROLE="user"
export SECURITY_RBAC_PERMISSION_CACHE_TTL="300"  # 5 minutes

# Monitoring configuration
export SECURITY_MONITORING_ENABLED="true"
export SECURITY_MONITORING_SIEM_ENDPOINT="https://siem.company.com/events"
export SECURITY_MONITORING_ALERT_WEBHOOK="https://alerts.company.com/webhook"
export SECURITY_MONITORING_THREAT_DETECTION_ENABLED="true"

# Logging configuration
export SECURITY_LOGGING_LOG_FILE="/var/log/quiccrunch/security.log"
export SECURITY_LOGGING_AUDIT_LOG_FILE="/var/log/quiccrunch/audit.log"
export SECURITY_LOGGING_RETENTION_DAYS="90"

# Rate limiting configuration
export SECURITY_RATE_LIMITING_ENABLED="true"
export SECURITY_RATE_LIMITING_DEFAULT_REQUESTS_PER_SECOND="100"
```

### Configuration File

Create `security-config.toml`:

```toml
[auth]
jwt_secret = "your-256-bit-secret-key-here"
jwt_expiration = "1h"
jwt_refresh_expiration = "30d"
require_client_certificates = false

[rbac]
default_role = "user"
admin_roles = ["admin", "super_admin"]
permission_cache_ttl = "5m"

[monitoring]
enabled = true
log_level = "info"
threat_detection_enabled = true
anomaly_detection_threshold = 0.95

[logging]
enabled = true
log_format = "Json"
retention_days = 90

[rate_limiting]
enabled = true
default_requests_per_second = 100
default_burst_size = 200
```

## Integration with QuicCrunch

### Server Integration

```rust
use quiccrunch_security::{SecurityFramework, SecurityContext};
use quiccrunch_core::rpc::{RpcServer, RpcContext};

// Security middleware for RPC methods
async fn security_middleware(
    security: &SecurityFramework,
    ctx: &RpcContext,
    method: &str,
    required_permissions: &[String],
) -> Result<SecurityContext, SecurityError> {
    // Extract JWT token from headers
    let token = ctx.headers().get("authorization")
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(SecurityError::AuthenticationFailed("No token provided".to_string()))?;
    
    // Validate token
    let security_ctx = security.auth().validate_token(token).await?;
    
    // Check permissions
    let decision = security.rbac()
        .check_permission_with_context(&security_ctx, method, "execute")
        .await?;
    
    if !decision.granted {
        return Err(SecurityError::AuthorizationDenied(decision.reason));
    }
    
    // Log RPC invocation
    security.logger().log_rpc_event(
        security_ctx.user_id.clone(),
        security_ctx.session_id.clone(),
        security_ctx.request_id.clone(),
        method.to_string(),
        true,
        0, // execution time will be updated later
    ).await?;
    
    Ok(security_ctx)
}

// Example RPC method with security
async fn secure_get_user_data(
    ctx: &RpcContext,
    security: &SecurityFramework,
    user_id: String,
) -> Result<UserData, RpcError> {
    // Apply security middleware
    let security_ctx = security_middleware(
        security,
        ctx,
        "get_user_data",
        &["user_data:read".to_string()],
    ).await?;
    
    // Method implementation
    let user_data = get_user_data_impl(user_id).await?;
    
    Ok(user_data)
}
```

### Client Integration

```rust
use quiccrunch_security::auth::AuthenticationManager;
use quiccrunch_core::rpc::RpcClient;

// Authenticated RPC client
pub struct SecureRpcClient {
    client: RpcClient,
    auth: AuthenticationManager,
    token: Option<String>,
}

impl SecureRpcClient {
    pub async fn login(&mut self, username: &str, password: &str) -> Result<(), SecurityError> {
        let auth_token = self.auth.authenticate_user(username, password).await?;
        self.token = Some(auth_token.access_token);
        Ok(())
    }
    
    pub async fn call_method<T, R>(&self, method: &str, params: T) -> Result<R, RpcError>
    where
        T: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        let token = self.token.as_ref()
            .ok_or(SecurityError::AuthenticationFailed("Not authenticated".to_string()))?;
        
        // Add authentication header
        let mut headers = std::collections::HashMap::new();
        headers.insert("authorization".to_string(), format!("Bearer {}", token));
        
        self.client.call_with_headers(method, params, headers).await
    }
}
```

## Security Testing

### Running Security Tests

```bash
# Run all security tests
cargo run --bin security-cli test run --host localhost --port 8080

# Run specific test categories
cargo run --bin security-cli test run \
    --categories auth,authz,input \
    --host production.example.com \
    --port 443

# Generate security test report
cargo run --bin security-cli test report --format json > security-test-report.json
```

### Custom Security Tests

```rust
use quiccrunch_security::testing::{SecurityTestFramework, SecurityTestConfig};

async fn run_custom_security_tests() -> Result<(), Box<dyn std::error::Error>> {
    let config = SecurityTestConfig {
        target_host: "localhost".to_string(),
        target_port: 8080,
        test_categories: vec![
            SecurityTestCategory::Authentication,
            SecurityTestCategory::Authorization,
        ],
        max_concurrent_tests: 5,
        timeout_seconds: 30,
        credentials: Some(TestCredentials {
            username: "test_user".to_string(),
            password: "test_password".to_string(),
            jwt_token: None,
            api_key: None,
        }),
        custom_payloads: std::collections::HashMap::new(),
    };
    
    let mut framework = SecurityTestFramework::new(config);
    let results = framework.run_security_tests().await?;
    
    for result in &results {
        println!("{}: {} ({})", 
            result.test_name,
            if result.passed { "PASS" } else { "FAIL" },
            result.severity
        );
    }
    
    let report = framework.generate_report();
    println!("Overall security score: {:.1}%", report.overall_score);
    
    Ok(())
}
```

## Compliance Management

### SOC2 Type II Preparation

The framework includes built-in SOC2 Type II controls and automated evidence collection:

```rust
use quiccrunch_security::compliance::{ComplianceManager, ComplianceFramework};

async fn soc2_assessment() -> Result<(), Box<dyn std::error::Error>> {
    let mut compliance = ComplianceManager::new();
    
    // Perform SOC2 assessment
    let assessment = compliance.perform_assessment(
        ComplianceFramework::SOC2TypeII,
        "External Auditor".to_string(),
        "Production Environment".to_string(),
    ).await?;
    
    println!("SOC2 Assessment Results:");
    println!("Overall Rating: {:?}", assessment.overall_rating);
    println!("Controls Tested: {}", assessment.control_results.len());
    println!("Findings: {}", assessment.findings.len());
    
    // Generate compliance report
    let report = compliance.generate_compliance_report(ComplianceFramework::SOC2TypeII);
    println!("Implementation Status: {:.1}%", report.metrics.implementation_percentage);
    
    Ok(())
}
```

### GDPR Compliance

```rust
// Example GDPR data protection logging
security.logger().log_data_access(
    user_id.clone(),
    "personal_data".to_string(),
    "read".to_string(),
    true,
).await?;

// Log administrative actions for compliance
security.audit_logger().log_admin_action(
    admin_user_id.clone(),
    "delete_user_data".to_string(),
    Some(target_user_id.clone()),
    true,
).await?;
```

## Monitoring and Alerting

### Security Dashboard Metrics

The framework provides comprehensive security metrics:

- Authentication success/failure rates
- Authorization decision times
- Security event volumes
- Threat indicator confidence levels
- Compliance control status
- Incident response metrics

### Setting Up Alerts

Configure webhook alerts for security events:

```bash
export SECURITY_MONITORING_ALERT_WEBHOOK="https://slack.com/api/hooks/..."

# Or configure SIEM integration
export SECURITY_MONITORING_SIEM_ENDPOINT="https://splunk.company.com/api/events"
export SECURITY_MONITORING_SIEM_API_KEY="your-siem-api-key"
```

### Custom Threat Detection

```rust
use quiccrunch_security::monitoring::{SecurityMonitor, SecurityAlert, AlertLevel};

// Custom threat detection
async fn detect_custom_threats(monitor: &SecurityMonitor) -> Result<(), SecurityError> {
    // Get recent security events
    let violations = monitor.get_security_violations(
        chrono::Utc::now() - chrono::Duration::hours(1)
    ).await?;
    
    if violations.len() > 10 {
        // Create custom alert
        let alert = SecurityAlert::new(
            AlertLevel::High,
            "high_violation_rate".to_string(),
            "High Security Violation Rate".to_string(),
            format!("Detected {} security violations in the last hour", violations.len()),
            "custom_detector".to_string(),
            0.8,
        );
        
        // Alert would be automatically sent to configured webhooks
    }
    
    Ok(())
}
```

## Performance Considerations

### Caching

- Permission checks are cached for configurable TTL (default 5 minutes)
- JWT token validation uses in-memory caching
- RBAC policies are cached and updated on change

### Rate Limiting

- Per-user rate limiting with configurable limits
- Method-specific rate limiting for RPC endpoints
- Distributed rate limiting support for cluster deployments

### Scalability

- Stateless JWT authentication for horizontal scaling
- Async/await throughout for non-blocking operations
- Efficient event processing with batching
- Optional database persistence for audit logs

## Development and Testing

### Running Tests

```bash
# Run unit tests
cargo test

# Run integration tests
cargo test --test integration

# Run security-specific tests
cargo test --package quiccrunch-security

# Run with logging
RUST_LOG=debug cargo test
```

### Development Setup

```bash
# Clone the repository
git clone https://github.com/quiccrunch/quiccrunch
cd quiccrunch/quiccrunch-security

# Install dependencies
cargo build

# Run security CLI
cargo run --bin security-cli --help

# Run with development config
export SECURITY_CONFIG_FILE=./config/dev-config.toml
cargo run --bin security-cli
```

## Documentation

- [Threat Model](docs/threat-model.md) - Comprehensive STRIDE threat analysis
- [Security Architecture](docs/architecture/security-architecture.md) - Detailed architecture documentation
- [Incident Response Plan](docs/incident-response.md) - Security incident response procedures
- [Compliance Documentation](docs/compliance/) - SOC2, GDPR, and other compliance frameworks
- [API Documentation](https://docs.rs/quiccrunch-security) - Rust API documentation

## Contributing

1. Review the security architecture and threat model
2. Follow secure coding practices
3. Add security tests for new features
4. Update compliance documentation as needed
5. Follow the incident response plan for security issues

### Security Issues

Report security vulnerabilities to: security@quiccrunch.org

Do not file public issues for security vulnerabilities.

## License

Licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](../LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](../LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Changelog

### Version 0.1.0 (August 2025)

- Initial release
- JWT/OAuth2 authentication framework
- RBAC authorization system
- Security monitoring and alerting
- Automated security testing
- SOC2 Type II compliance framework
- Comprehensive audit logging
- Incident response procedures
- Security CLI tools

---

**Security Classification**: Internal Use  
**Document Version**: 1.0  
**Last Updated**: August 2025