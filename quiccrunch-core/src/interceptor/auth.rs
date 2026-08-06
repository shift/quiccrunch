//! Authentication and Authorization Interceptor for RPC
//!
//! Provides JWT token authentication, role-based access control (RBAC),
//! and permission-based method access for RPC services.

use async_trait::async_trait;
use bytes::Bytes;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::rpc::error::{RpcError, RpcResult};
use crate::rpc::service::RpcContext;
use super::{Interceptor, InterceptorChain};

/// Claims in a JWT token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// Expiration time (Unix timestamp)
    pub exp: u64,
    /// Not before time (Unix timestamp)
    pub nbf: u64,
    /// Issued at time (Unix timestamp)
    pub iat: u64,
    /// Issuer
    pub iss: Option<String>,
    /// Roles assigned to the user
    #[serde(default)]
    pub roles: Vec<String>,
    /// Permissions granted to the user
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Custom claims
    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,
}

/// Role definition with permissions
#[derive(Debug, Clone)]
pub struct Role {
    pub name: String,
    pub permissions: HashSet<String>,
}

/// Permission definition
#[derive(Debug, Clone)]
pub struct Permission {
    pub name: String,
    pub description: String,
}

/// Configuration for JWT authentication
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Secret key for signing JWT tokens (HS256/HS384/HS512)
    pub secret: Option<String>,
    /// Public key for verifying JWT tokens (RS256/RS384/RS512)
    pub public_key: Option<String>,
    /// Private key for signing JWT tokens (RS256/RS384/RS512)
    pub private_key: Option<String>,
    /// Algorithm to use for JWT
    pub algorithm: Algorithm,
    /// Token expiration time in seconds
    pub token_expiry: Duration,
    /// Issuer name
    pub issuer: Option<String>,
    /// Allowed audiences
    pub allowed_audiences: Option<Vec<String>>,
    /// Whether to require authentication for all methods
    pub require_auth: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            secret: None,
            public_key: None,
            private_key: None,
            algorithm: Algorithm::HS256,
            token_expiry: Duration::from_secs(3600), // 1 hour
            issuer: None,
            allowed_audiences: None,
            require_auth: true,
        }
    }
}

/// Authentication and authorization interceptor
pub struct AuthInterceptor {
    config: AuthConfig,
    roles: Arc<tokio::sync::RwLock<HashMap<String, Role>>>,
    permissions: Arc<tokio::sync::RwLock<HashMap<String, Permission>>>,
    /// Method-specific authentication requirements
    method_auth: Arc<tokio::sync::RwLock<HashMap<String, bool>>>,
    /// Method-specific role requirements
    method_roles: Arc<tokio::sync::RwLock<HashMap<String, HashSet<String>>>>,
    /// Method-specific permission requirements
    method_permissions: Arc<tokio::sync::RwLock<HashMap<String, HashSet<String>>>>,
}

impl AuthInterceptor {
    /// Create a new auth interceptor with the given configuration
    pub fn new(config: AuthConfig) -> Self {
        Self {
            config,
            roles: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            permissions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            method_auth: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            method_roles: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            method_permissions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Create a new auth interceptor with HMAC secret
    pub fn with_secret(secret: String) -> Self {
        let config = AuthConfig {
            secret: Some(secret),
            algorithm: Algorithm::HS256,
            ..Default::default()
        };
        Self::new(config)
    }

    /// Create a new auth interceptor with RSA keys
    pub fn with_rsa_keys(private_key: String, public_key: String) -> Self {
        let config = AuthConfig {
            private_key: Some(private_key),
            public_key: Some(public_key),
            algorithm: Algorithm::RS256,
            ..Default::default()
        };
        Self::new(config)
    }

    /// Set token expiry duration
    pub fn with_token_expiry(mut self, expiry: Duration) -> Self {
        self.config.token_expiry = expiry;
        self
    }

    /// Set issuer name
    pub fn with_issuer(mut self, issuer: String) -> Self {
        self.config.issuer = Some(issuer);
        self
    }

    /// Set allowed audiences
    pub fn with_allowed_audiences(mut self, audiences: Vec<String>) -> Self {
        self.config.allowed_audiences = Some(audiences);
        self
    }

    /// Set whether to require authentication for all methods
    pub fn with_require_auth(mut self, require: bool) -> Self {
        self.config.require_auth = require;
        self
    }

    /// Add a role with permissions
    pub async fn add_role(&self, role: Role) {
        let mut roles = self.roles.write().await;
        roles.insert(role.name.clone(), role);
    }

    /// Add a permission
    pub async fn add_permission(&self, permission: Permission) {
        let mut permissions = self.permissions.write().await;
        permissions.insert(permission.name.clone(), permission);
    }

    /// Set authentication requirement for a specific method
    pub async fn set_method_auth(&self, method: &str, require_auth: bool) {
        let mut method_auth = self.method_auth.write().await;
        method_auth.insert(method.to_string(), require_auth);
    }

    /// Set role requirements for a specific method
    pub async fn set_method_roles(&self, method: &str, roles: HashSet<String>) {
        let mut method_roles = self.method_roles.write().await;
        method_roles.insert(method.to_string(), roles);
    }

    /// Set permission requirements for a specific method
    pub async fn set_method_permissions(&self, method: &str, permissions: HashSet<String>) {
        let mut method_permissions = self.method_permissions.write().await;
        method_permissions.insert(method.to_string(), permissions);
    }

    /// Generate a JWT token for a user
    pub fn generate_token(
        &self,
        subject: &str,
        roles: Vec<String>,
        permissions: Vec<String>,
        custom_claims: HashMap<String, serde_json::Value>,
    ) -> RpcResult<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| RpcError::Internal("System time error".to_string()))?
            .as_secs();

        let claims = Claims {
            sub: subject.to_string(),
            exp: now + self.config.token_expiry.as_secs(),
            nbf: now,
            iat: now,
            iss: self.config.issuer.clone(),
            roles,
            permissions,
            custom: custom_claims,
        };

        let header = Header::new(self.config.algorithm);

        let token = match self.config.algorithm {
            Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => {
                let secret = self.config.secret.as_ref()
                    .ok_or_else(|| RpcError::Internal("Secret not configured".to_string()))?;
                encode(&header, &claims, &EncodingKey::from_secret(secret.as_bytes()))
            }
            Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => {
                let private_key = self.config.private_key.as_ref()
                    .ok_or_else(|| RpcError::Internal("Private key not configured".to_string()))?;
                encode(&header, &claims, &EncodingKey::from_rsa_pem(private_key.as_bytes())
                    .map_err(|e| RpcError::Internal(format!("Failed to encode RSA key: {}", e)))?)
            }
            _ => return Err(RpcError::Internal("Unsupported algorithm".to_string())),
        }
        .map_err(|e| RpcError::Internal(format!("Failed to generate token: {}", e)))?;

        Ok(token)
    }

    /// Verify and decode a JWT token
    pub fn verify_token(&self, token: &str) -> RpcResult<Claims> {
        let mut validation = Validation::new(self.config.algorithm);

        if let Some(ref issuer) = self.config.issuer {
            validation.set_issuer(&[issuer.as_str()]);
        }

        if let Some(ref audiences) = self.config.allowed_audiences {
            validation.set_audience(audiences);
        }

        let token_data = match self.config.algorithm {
            Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => {
                let secret = self.config.secret.as_ref()
                    .ok_or_else(|| RpcError::Internal("Secret not configured".to_string()))?;
                decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &validation)
            }
            Algorithm::RS256 | Algorithm::RS384 | Algorithm::RS512 => {
                let public_key = self.config.public_key.as_ref()
                    .ok_or_else(|| RpcError::Internal("Public key not configured".to_string()))?;
                decode::<Claims>(token, &DecodingKey::from_rsa_pem(public_key.as_bytes())
                    .map_err(|e| RpcError::Internal(format!("Failed to decode RSA key: {}", e)))?, &validation)
            }
            _ => return Err(RpcError::Internal("Unsupported algorithm".to_string())),
        }
        .map_err(|e| RpcError::Unauthenticated)?;

        Ok(token_data.claims)
    }

    /// Check if a user has a specific role
    pub fn has_role(claims: &Claims, role: &str) -> bool {
        claims.roles.iter().any(|r| r == role)
    }

    /// Check if a user has a specific permission
    pub fn has_permission(claims: &Claims, permission: &str) -> bool {
        claims.permissions.iter().any(|p| p == permission)
    }

    /// Check if a user has any of the required roles
    pub fn has_any_role(claims: &Claims, roles: &HashSet<String>) -> bool {
        claims.roles.iter().any(|r| roles.contains(r))
    }

    /// Check if a user has all required permissions
    pub fn has_all_permissions(claims: &Claims, permissions: &HashSet<String>) -> bool {
        permissions.iter().all(|p| claims.permissions.contains(p))
    }

    /// Extract token from request context
    fn extract_token(context: &RpcContext) -> Option<&str> {
        // Check for token in metadata
        context.metadata.get("authorization")
            .or_else(|| context.metadata.get("x-auth-token"))
            .map(|s| s.as_str())
    }
}

#[async_trait]
impl Interceptor for AuthInterceptor {
    async fn before_request(&self, context: &mut RpcContext, _params: &mut Bytes) -> RpcResult<()> {
        // Check if authentication is required for this method
        let method_key = format!("{}.{}", context.service, context.method);
        let method_auth = self.method_auth.read().await;
        let require_auth = method_auth.get(&method_key).copied().unwrap_or(self.config.require_auth);

        if !require_auth {
            return Ok(());
        }

        // Extract token from context
        let token = Self::extract_token(context)
            .ok_or(RpcError::Unauthenticated)?;

        // Verify token
        let claims = self.verify_token(token)?;

        // Store claims in context for later use
        context.extensions.insert("claims".to_string(), serde_json::to_value(&claims).unwrap_or_default());

        // Check role requirements
        let method_roles = self.method_roles.read().await;
        if let Some(required_roles) = method_roles.get(&method_key) {
            if !required_roles.is_empty() && !Self::has_any_role(&claims, required_roles) {
                return Err(RpcError::PermissionDenied {
                    reason: format!("Required roles: {:?}", required_roles),
                });
            }
        }

        // Check permission requirements
        let method_permissions = self.method_permissions.read().await;
        if let Some(required_permissions) = method_permissions.get(&method_key) {
            if !required_permissions.is_empty() && !Self::has_all_permissions(&claims, required_permissions) {
                return Err(RpcError::PermissionDenied {
                    reason: format!("Required permissions: {:?}", required_permissions),
                });
            }
        }

        Ok(())
    }
}

/// Builder for creating configured auth interceptors
pub struct AuthInterceptorBuilder {
    config: AuthConfig,
    roles: Vec<Role>,
    permissions: Vec<Permission>,
    method_auth: Vec<(String, bool)>,
    method_roles: Vec<(String, HashSet<String>)>,
    method_permissions: Vec<(String, HashSet<String>)>,
}

impl AuthInterceptorBuilder {
    pub fn new() -> Self {
        Self {
            config: AuthConfig::default(),
            roles: Vec::new(),
            permissions: Vec::new(),
            method_auth: Vec::new(),
            method_roles: Vec::new(),
            method_permissions: Vec::new(),
        }
    }

    /// Set HMAC secret for HS256/HS384/HS512
    pub fn with_secret(mut self, secret: String) -> Self {
        self.config.secret = Some(secret);
        self.config.algorithm = Algorithm::HS256;
        self
    }

    /// Set RSA keys for RS256/RS384/RS512
    pub fn with_rsa_keys(mut self, private_key: String, public_key: String) -> Self {
        self.config.private_key = Some(private_key);
        self.config.public_key = Some(public_key);
        self.config.algorithm = Algorithm::RS256;
        self
    }

    /// Set token expiry
    pub fn with_token_expiry(mut self, expiry: Duration) -> Self {
        self.config.token_expiry = expiry;
        self
    }

    /// Set issuer
    pub fn with_issuer(mut self, issuer: String) -> Self {
        self.config.issuer = Some(issuer);
        self
    }

    /// Set allowed audiences
    pub fn with_allowed_audiences(mut self, audiences: Vec<String>) -> Self {
        self.config.allowed_audiences = Some(audiences);
        self
    }

    /// Set require auth for all methods
    pub fn with_require_auth(mut self, require: bool) -> Self {
        self.config.require_auth = require;
        self
    }

    /// Add a role
    pub fn add_role(mut self, role: Role) -> Self {
        self.roles.push(role);
        self
    }

    /// Add a permission
    pub fn add_permission(mut self, permission: Permission) -> Self {
        self.permissions.push(permission);
        self
    }

    /// Set method authentication requirement
    pub fn with_method_auth(mut self, method: String, require_auth: bool) -> Self {
        self.method_auth.push((method, require_auth));
        self
    }

    /// Set method role requirements
    pub fn with_method_roles(mut self, method: String, roles: HashSet<String>) -> Self {
        self.method_roles.push((method, roles));
        self
    }

    /// Set method permission requirements
    pub fn with_method_permissions(mut self, method: String, permissions: HashSet<String>) -> Self {
        self.method_permissions.push((method, permissions));
        self
    }

    /// Build the auth interceptor
    pub async fn build(self) -> AuthInterceptor {
        let interceptor = AuthInterceptor::new(self.config);

        for role in self.roles {
            interceptor.add_role(role).await;
        }

        for permission in self.permissions {
            interceptor.add_permission(permission).await;
        }

        for (method, require_auth) in self.method_auth {
            interceptor.set_method_auth(&method, require_auth).await;
        }

        for (method, roles) in self.method_roles {
            interceptor.set_method_roles(&method, roles).await;
        }

        for (method, permissions) in self.method_permissions {
            interceptor.set_method_permissions(&method, permissions).await;
        }

        interceptor
    }
}

impl Default for AuthInterceptorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to create an auth interceptor with common settings
pub async fn create_auth_interceptor(secret: String) -> AuthInterceptor {
    AuthInterceptorBuilder::new()
        .with_secret(secret)
        .with_require_auth(true)
        .build()
        .await
}

/// Convenience function to create an auth interceptor with RBAC
pub async fn create_rbac_auth_interceptor(
    secret: String,
    roles: Vec<Role>,
) -> AuthInterceptor {
    let mut builder = AuthInterceptorBuilder::new()
        .with_secret(secret)
        .with_require_auth(true);

    for role in roles {
        builder = builder.add_role(role);
    }

    builder.build().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_generate_and_verify_token() {
        let secret = "test-secret-key".to_string();
        let interceptor = AuthInterceptor::with_secret(secret);

        let mut custom_claims = HashMap::new();
        custom_claims.insert("org".to_string(), serde_json::json!("acme"));

        let token = interceptor.generate_token(
            "user123",
            vec!["admin".to_string()],
            vec!["read".to_string(), "write".to_string()],
            custom_claims,
        ).unwrap();

        let claims = interceptor.verify_token(&token).unwrap();
        assert_eq!(claims.sub, "user123");
        assert!(claims.roles.contains(&"admin".to_string()));
        assert!(claims.permissions.contains(&"read".to_string()));
        assert!(claims.permissions.contains(&"write".to_string()));
    }

    #[test]
    fn test_has_role() {
        let claims = Claims {
            sub: "user123".to_string(),
            exp: 0,
            nbf: 0,
            iat: 0,
            iss: None,
            roles: vec!["admin".to_string(), "user".to_string()],
            permissions: vec![],
            custom: HashMap::new(),
        };

        assert!(AuthInterceptor::has_role(&claims, "admin"));
        assert!(AuthInterceptor::has_role(&claims, "user"));
        assert!(!AuthInterceptor::has_role(&claims, "superadmin"));
    }

    #[test]
    fn test_has_permission() {
        let claims = Claims {
            sub: "user123".to_string(),
            exp: 0,
            nbf: 0,
            iat: 0,
            iss: None,
            roles: vec![],
            permissions: vec!["read".to_string(), "write".to_string()],
            custom: HashMap::new(),
        };

        assert!(AuthInterceptor::has_permission(&claims, "read"));
        assert!(AuthInterceptor::has_permission(&claims, "write"));
        assert!(!AuthInterceptor::has_permission(&claims, "delete"));
    }
}
