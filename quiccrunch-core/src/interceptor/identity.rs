//! SPIFFE peer-identity authorization interceptor.
//!
//! The QUIC transport verifies the client certificate and extracts its
//! `spiffe://` URI SAN (`crate::identity`). The server copies the resulting
//! identity into [`crate::identity::PEER_SPIFFE_ID_METADATA`] when it builds
//! the request context. This interceptor enforces the per-method
//! [`SpiffePolicy`] against that metadata before a request reaches the
//! service.
//!
//! It is intentionally independent of the JWT `AuthInterceptor`: use it for
//! workload identity, and keep JWT (or another mechanism) for end-user claims.

use async_trait::async_trait;
use bytes::Bytes;

use crate::identity::{IdentityError, SpiffeId, SpiffePolicy, PEER_SPIFFE_ID_METADATA};
use crate::rpc::RpcContext;
use crate::{RpcError, RpcResult};

use super::Interceptor;

/// Interceptor enforcing a [`SpiffePolicy`] on transport-verified identities.
#[derive(Debug, Clone, Default)]
pub struct SpiffeIdentityInterceptor {
    policy: SpiffePolicy,
    /// When true, methods without an explicit rule still require a verified
    /// identity. This is the fail-closed mode for a zero-trust service mesh.
    require_identity: bool,
}

impl SpiffeIdentityInterceptor {
    pub fn new(policy: SpiffePolicy) -> Self {
        Self {
            policy,
            require_identity: false,
        }
    }

    /// Require a transport identity even for methods without an explicit rule.
    pub fn with_require_identity(mut self, require: bool) -> Self {
        self.require_identity = require;
        self
    }

    fn identity_for(context: &RpcContext) -> Result<Option<SpiffeId>, IdentityError> {
        // The value must have been inserted by the server from the connection,
        // not by the peer. There is no way for this interceptor to distinguish
        // the two, which is why the transport-to-context copy is the trust
        // boundary and must never copy request metadata.
        match context.metadata.get(PEER_SPIFFE_ID_METADATA) {
            Some(raw) => SpiffeId::parse(raw).map(Some),
            None => Ok(None),
        }
    }
}

#[async_trait]
impl Interceptor for SpiffeIdentityInterceptor {
    async fn before_request(&self, context: &mut RpcContext, _params: &mut Bytes) -> RpcResult<()> {
        let identity = Self::identity_for(context).map_err(|e| {
            RpcError::PermissionDenied(format!("invalid peer SPIFFE identity: {e}"))
        })?;

        let method = format!("{}.{}", context.service, context.method);

        if self.require_identity && identity.is_none() {
            return Err(RpcError::PermissionDenied(format!(
                "method {method} requires a verified SPIFFE identity"
            )));
        }

        self.policy
            .check_id(&method, identity.as_ref())
            .map_err(|e| RpcError::PermissionDenied(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::SpiffePolicy;

    fn context(service: &str, method: &str, identity: Option<&str>) -> RpcContext {
        let mut ctx = RpcContext::new(service.to_string(), method.to_string());
        if let Some(identity) = identity {
            ctx.metadata
                .insert(PEER_SPIFFE_ID_METADATA.to_string(), identity.to_string());
        }
        ctx
    }

    #[tokio::test]
    async fn allows_configured_identity() {
        let mut policy = SpiffePolicy::new();
        policy.allow(
            "Raft.AppendEntries",
            vec![SpiffeId::parse("spiffe://cluster.local/ns/kv/sa/raft").unwrap()],
        );
        let interceptor = SpiffeIdentityInterceptor::new(policy);

        let mut ctx = context(
            "Raft",
            "AppendEntries",
            Some("spiffe://cluster.local/ns/kv/sa/raft"),
        );
        interceptor
            .before_request(&mut ctx, &mut Bytes::new())
            .await
            .expect("allowed identity");
    }

    #[tokio::test]
    async fn denies_unlisted_identity() {
        let mut policy = SpiffePolicy::new();
        policy.allow(
            "Raft.AppendEntries",
            vec![SpiffeId::parse("spiffe://cluster.local/ns/kv/sa/raft").unwrap()],
        );
        let interceptor = SpiffeIdentityInterceptor::new(policy);

        let mut ctx = context(
            "Raft",
            "AppendEntries",
            Some("spiffe://cluster.local/ns/other/sa/rogue"),
        );
        let err = interceptor
            .before_request(&mut ctx, &mut Bytes::new())
            .await
            .expect_err("unlisted identity must be denied");
        assert!(matches!(err, RpcError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn denies_missing_identity_for_protected_method() {
        let mut policy = SpiffePolicy::new();
        policy.allow(
            "Raft.AppendEntries",
            vec![SpiffeId::parse("spiffe://cluster.local/ns/kv/sa/raft").unwrap()],
        );
        let interceptor = SpiffeIdentityInterceptor::new(policy);

        let mut ctx = context("Raft", "AppendEntries", None);
        assert!(interceptor
            .before_request(&mut ctx, &mut Bytes::new())
            .await
            .is_err());
    }
}
