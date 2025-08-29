//! Authentication framework with JWT and OAuth2 support

use crate::{SecurityError, SecurityResult, SecurityContext};
use crate::config::AuthConfig;
use chrono::{DateTime, Duration, Utc};
#[cfg(feature = "jwt")]
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
#[cfg(feature = "oauth2")]
use oauth2::{
    AuthorizationCode, AuthUrl, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// JWT claims structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    pub sub: String,        // Subject (user ID)
    pub iat: i64,          // Issued at
    pub exp: i64,          // Expiration
    pub aud: String,       // Audience
    pub iss: String,       // Issuer
    pub roles: Vec<String>, // User roles
    pub permissions: Vec<String>, // User permissions
    pub session_id: String, // Session identifier
}

/// Authentication token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub token_type: String,
    pub expires_in: i64,
    pub scope: Option<String>,
}

/// User authentication info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAuth {
    pub user_id: String,
    pub username: String,
    pub email: Option<String>,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_login: Option<DateTime<Utc>>,
    pub active: bool,
}

/// Authentication session
#[derive(Debug, Clone)]
pub struct AuthSession {
    pub session_id: String,
    pub user_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

/// Authentication manager
pub struct AuthenticationManager {
    config: AuthConfig,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    sessions: Arc<RwLock<HashMap<String, AuthSession>>>,
    users: Arc<RwLock<HashMap<String, UserAuth>>>,
    oauth2_client: Option<oauth2::basic::BasicClient>,
}

impl AuthenticationManager {
    pub fn new(config: AuthConfig) -> SecurityResult<Self> {
        let encoding_key = EncodingKey::from_secret(config.jwt_secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(config.jwt_secret.as_bytes());

        let oauth2_client = if let (Some(client_id), Some(client_secret)) = 
            (&config.oauth2_client_id, &config.oauth2_client_secret) {
            
            let auth_url = config.oauth2_auth_url.as_ref()
                .ok_or_else(|| SecurityError::ConfigurationError("OAuth2 auth URL required".to_string()))?;
            let token_url = config.oauth2_token_url.as_ref()
                .ok_or_else(|| SecurityError::ConfigurationError("OAuth2 token URL required".to_string()))?;

            let client = oauth2::basic::BasicClient::new(
                ClientId::new(client_id.clone()),
                Some(ClientSecret::new(client_secret.clone())),
                AuthUrl::new(auth_url.clone())
                    .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?,
                Some(TokenUrl::new(token_url.clone())
                    .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?),
            );
            Some(client)
        } else {
            None
        };

        Ok(Self {
            config,
            encoding_key,
            decoding_key,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            users: Arc::new(RwLock::new(HashMap::new())),
            oauth2_client,
        })
    }

    /// Create a new user
    pub async fn create_user(
        &self,
        user_id: String,
        username: String,
        email: Option<String>,
        roles: Vec<String>,
    ) -> SecurityResult<UserAuth> {
        let user = UserAuth {
            user_id: user_id.clone(),
            username,
            email,
            roles,
            permissions: Vec::new(),
            created_at: Utc::now(),
            last_login: None,
            active: true,
        };

        let mut users = self.users.write().await;
        users.insert(user_id, user.clone());

        Ok(user)
    }

    /// Authenticate user with username/password (placeholder for actual auth)
    pub async fn authenticate_user(
        &self,
        username: &str,
        password: &str,
    ) -> SecurityResult<AuthToken> {
        // In a real implementation, you would verify the password against a secure hash
        // This is a placeholder implementation
        let users = self.users.read().await;
        let user = users.values()
            .find(|u| u.username == username && u.active)
            .ok_or_else(|| SecurityError::AuthenticationFailed("Invalid credentials".to_string()))?;

        // Create session
        let session_id = Uuid::new_v4().to_string();
        let session = AuthSession {
            session_id: session_id.clone(),
            user_id: user.user_id.clone(),
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::from_std(self.config.jwt_expiration)
                .map_err(|e| SecurityError::InternalError(e.to_string()))?,
            last_activity: Utc::now(),
            ip_address: None,
            user_agent: None,
        };

        // Store session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), session);
        }

        // Update last login
        {
            let mut users_mut = self.users.write().await;
            if let Some(user_mut) = users_mut.get_mut(&user.user_id) {
                user_mut.last_login = Some(Utc::now());
            }
        }

        // Generate JWT token
        self.generate_token(&user.user_id, &user.roles, &session_id).await
    }

    /// Generate JWT token
    pub async fn generate_token(
        &self,
        user_id: &str,
        roles: &[String],
        session_id: &str,
    ) -> SecurityResult<AuthToken> {
        let now = Utc::now();
        let exp = now + Duration::from_std(self.config.jwt_expiration)
            .map_err(|e| SecurityError::InternalError(e.to_string()))?;

        let claims = JwtClaims {
            sub: user_id.to_string(),
            iat: now.timestamp(),
            exp: exp.timestamp(),
            aud: "quiccrunch".to_string(),
            iss: "quiccrunch-auth".to_string(),
            roles: roles.to_vec(),
            permissions: Vec::new(), // TODO: Load from RBAC system
            session_id: session_id.to_string(),
        };

        let header = Header::new(Algorithm::HS256);
        let access_token = encode(&header, &claims, &self.encoding_key)?;

        // Generate refresh token
        let refresh_token = if self.config.jwt_refresh_expiration.as_secs() > 0 {
            Some(Uuid::new_v4().to_string())
        } else {
            None
        };

        Ok(AuthToken {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: self.config.jwt_expiration.as_secs() as i64,
            scope: None,
        })
    }

    /// Validate JWT token
    pub async fn validate_token(&self, token: &str) -> SecurityResult<SecurityContext> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_audience(&["quiccrunch"]);
        validation.set_issuer(&["quiccrunch-auth"]);

        let token_data = decode::<JwtClaims>(token, &self.decoding_key, &validation)?;
        let claims = token_data.claims;

        // Check if session is still valid
        let sessions = self.sessions.read().await;
        let session = sessions.get(&claims.session_id)
            .ok_or_else(|| SecurityError::InvalidToken("Session not found".to_string()))?;

        if session.expires_at < Utc::now() {
            return Err(SecurityError::InvalidToken("Session expired".to_string()));
        }

        // Update last activity
        drop(sessions);
        {
            let mut sessions_mut = self.sessions.write().await;
            if let Some(session_mut) = sessions_mut.get_mut(&claims.session_id) {
                session_mut.last_activity = Utc::now();
            }
        }

        Ok(SecurityContext {
            user_id: Some(claims.sub),
            session_id: claims.session_id,
            roles: claims.roles,
            permissions: claims.permissions,
            authenticated: true,
            request_id: Uuid::new_v4().to_string(),
        })
    }

    /// Refresh token
    pub async fn refresh_token(&self, refresh_token: &str) -> SecurityResult<AuthToken> {
        // In a real implementation, you would store and validate refresh tokens
        // This is a placeholder implementation
        
        // For now, just validate that it's a valid UUID format
        Uuid::parse_str(refresh_token)
            .map_err(|_| SecurityError::InvalidToken("Invalid refresh token".to_string()))?;

        // Generate new access token
        // In a real implementation, you would look up the user associated with the refresh token
        Err(SecurityError::InternalError("Refresh token implementation needed".to_string()))
    }

    /// Logout user
    pub async fn logout(&self, session_id: &str) -> SecurityResult<()> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
        Ok(())
    }

    /// Get OAuth2 authorization URL
    pub fn get_oauth2_auth_url(&self, redirect_uri: &str) -> SecurityResult<(String, String)> {
        let client = self.oauth2_client.as_ref()
            .ok_or_else(|| SecurityError::ConfigurationError("OAuth2 not configured".to_string()))?;

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let (auth_url, csrf_token) = client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new("read".to_string()))
            .set_redirect_uri(
                std::borrow::Cow::Owned(RedirectUrl::new(redirect_uri.to_string())
                    .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?),
            )
            .set_pkce_challenge(pkce_challenge)
            .url();

        Ok((auth_url.to_string(), csrf_token.secret().clone()))
    }

    /// Exchange OAuth2 authorization code for token
    pub async fn exchange_oauth2_code(
        &self,
        code: &str,
        _csrf_token: &str, // Should validate CSRF token
    ) -> SecurityResult<AuthToken> {
        let client = self.oauth2_client.as_ref()
            .ok_or_else(|| SecurityError::ConfigurationError("OAuth2 not configured".to_string()))?;

        let _token_response = client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .request_async(oauth2::reqwest::async_http_client)
            .await
            .map_err(|e| SecurityError::AuthenticationFailed(e.to_string()))?;

        // Extract user information from OAuth2 provider
        // This would typically involve calling the user info endpoint
        // For now, return an error indicating implementation needed
        Err(SecurityError::InternalError("OAuth2 user info extraction needed".to_string()))
    }

    /// Get user by ID
    pub async fn get_user(&self, user_id: &str) -> SecurityResult<Option<UserAuth>> {
        let users = self.users.read().await;
        Ok(users.get(user_id).cloned())
    }

    /// Update user roles
    pub async fn update_user_roles(&self, user_id: &str, roles: Vec<String>) -> SecurityResult<()> {
        let mut users = self.users.write().await;
        if let Some(user) = users.get_mut(user_id) {
            user.roles = roles;
            Ok(())
        } else {
            Err(SecurityError::AuthenticationFailed("User not found".to_string()))
        }
    }

    /// Deactivate user
    pub async fn deactivate_user(&self, user_id: &str) -> SecurityResult<()> {
        let mut users = self.users.write().await;
        if let Some(user) = users.get_mut(user_id) {
            user.active = false;
            
            // Invalidate all sessions for the user
            let mut sessions = self.sessions.write().await;
            sessions.retain(|_, session| session.user_id != user_id);
            
            Ok(())
        } else {
            Err(SecurityError::AuthenticationFailed("User not found".to_string()))
        }
    }

    /// Get active sessions
    pub async fn get_active_sessions(&self) -> SecurityResult<Vec<AuthSession>> {
        let sessions = self.sessions.read().await;
        let now = Utc::now();
        
        let active_sessions: Vec<AuthSession> = sessions
            .values()
            .filter(|session| session.expires_at > now)
            .cloned()
            .collect();
            
        Ok(active_sessions)
    }

    /// Clean up expired sessions
    pub async fn cleanup_expired_sessions(&self) -> SecurityResult<usize> {
        let mut sessions = self.sessions.write().await;
        let now = Utc::now();
        let initial_count = sessions.len();
        
        sessions.retain(|_, session| session.expires_at > now);
        
        let removed_count = initial_count - sessions.len();
        Ok(removed_count)
    }
}

/// Password utilities
pub mod password {
    use ring::{digest, pbkdf2};
    use crate::SecurityResult;
    use std::num::NonZeroU32;

    const CREDENTIAL_LEN: usize = digest::SHA256_OUTPUT_LEN;
    const PBKDF2_ITERATIONS: u32 = 100_000;

    /// Hash a password using PBKDF2
    pub fn hash_password(password: &str, salt: &[u8]) -> SecurityResult<Vec<u8>> {
        let mut hash = [0u8; CREDENTIAL_LEN];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            NonZeroU32::new(PBKDF2_ITERATIONS).unwrap(),
            salt,
            password.as_bytes(),
            &mut hash,
        );
        Ok(hash.to_vec())
    }

    /// Verify a password against a hash
    pub fn verify_password(password: &str, salt: &[u8], hash: &[u8]) -> SecurityResult<bool> {
        let computed_hash = hash_password(password, salt)?;
        Ok(computed_hash == hash)
    }

    /// Generate a random salt
    pub fn generate_salt() -> [u8; 16] {
        use ring::rand::{generate, SystemRandom};
        let rng = SystemRandom::new();
        let mut salt = [0u8; 16];
        generate(&rng, &mut salt).unwrap();
        salt
    }
}