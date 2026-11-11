//! SPIFFE-bound peer identity for the QUIC transport.
//!
//! The transport already proves possession of the private key for the peer
//! certificate through the TLS handshake. This module turns that certificate
//! into a workload identity by extracting the `spiffe://` URI subject
//! alternative name (SAN) of the *verified* leaf certificate and validating it
//! against the configured trust domain.
//!
//! The identity is deliberately derived only from the certificate chain that
//! BoringSSL accepted. It is never read from application metadata, so a client
//! cannot claim an identity it cannot prove.
//!
//! # Policy model
//!
//! [`SpiffePolicy`] maps an RPC method (`Service.Method`) to the set of exact
//! SPIFFE IDs that may call it. Methods without a rule are not restricted by
//! this policy (they may still be restricted by the JWT `AuthInterceptor`,
//! which is only compiled with the `auth` feature).
//! A rule with an empty set denies every identity, which is useful to fail a
//! method closed while its allowed callers are being rolled out.

use std::collections::HashMap;
use std::fmt;

use x509_parser::extensions::{GeneralName, ParsedExtension};
use x509_parser::prelude::FromDer;

/// RPC metadata key used to carry the transport-verified SPIFFE ID from the
/// server connection into the interceptor chain.
///
/// The server must copy [`PeerIdentity::spiffe_id`] into this key when it
/// builds the `RpcContext`; clients must never be allowed to supply it.
pub const PEER_SPIFFE_ID_METADATA: &str = "x-quiccrunch-peer-spiffe-id";

/// Error returned while extracting or validating a peer identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    /// The peer did not present a certificate.
    MissingCertificate,
    /// The leaf certificate could not be parsed as DER X.509.
    CertificateParse(String),
    /// The leaf certificate has no `spiffe://` URI SAN.
    MissingSpiffeSan,
    /// The `spiffe://` URI SAN is syntactically invalid.
    MalformedSpiffeId(String),
    /// The SAN trust domain is not the configured trust domain.
    TrustDomainMismatch { expected: String, actual: String },
    /// The configured policy does not allow this identity for the method.
    PolicyDenied {
        method: String,
        identity: Option<String>,
    },
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCertificate => write!(f, "peer did not present a certificate"),
            Self::CertificateParse(e) => write!(f, "peer certificate parse failed: {e}"),
            Self::MissingSpiffeSan => write!(
                f,
                "peer certificate has no spiffe:// URI subject alternative name"
            ),
            Self::MalformedSpiffeId(e) => write!(f, "malformed SPIFFE ID: {e}"),
            Self::TrustDomainMismatch { expected, actual } => write!(
                f,
                "SPIFFE trust domain {actual:?} is not the configured {expected:?}"
            ),
            Self::PolicyDenied { method, identity } => match identity {
                Some(id) => write!(f, "identity {id} is not allowed to call {method}"),
                None => write!(f, "no peer identity for protected method {method}"),
            },
        }
    }
}

impl std::error::Error for IdentityError {}

/// A parsed and normalized `spiffe://` workload identifier.
///
/// The full SPIFFE path is preserved, and the Kubernetes-style
/// `/ns/<namespace>/sa/<service-account>` segments are exposed separately for
/// policy authors. Other path layouts remain valid SPIFFE IDs but have no
/// namespace/service-account mapping.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpiffeId {
    raw: String,
    trust_domain: String,
    path_segments: Vec<String>,
}

impl SpiffeId {
    /// Parse and validate a SPIFFE ID.
    ///
    /// Rules enforced here:
    /// * scheme is exactly `spiffe://`;
    /// * the trust domain is a non-empty DNS-like name without userinfo, port,
    ///   path, whitespace or uppercase characters;
    /// * the path is non-empty and has no empty or `.`/`..` segments;
    /// * the canonical string is preserved as `spiffe://<domain>/<path>`.
    pub fn parse(raw: &str) -> Result<Self, IdentityError> {
        let rest = raw.strip_prefix("spiffe://").ok_or_else(|| {
            IdentityError::MalformedSpiffeId(format!("{raw:?} does not start with spiffe://"))
        })?;

        let (trust_domain, path) = rest.split_once('/').ok_or_else(|| {
            IdentityError::MalformedSpiffeId(format!("{raw:?} has no workload path"))
        })?;

        if trust_domain.is_empty() {
            return Err(IdentityError::MalformedSpiffeId(
                "empty trust domain".to_string(),
            ));
        }
        if trust_domain.contains(['@', ':', '?', '#', ' ', '\t']) {
            return Err(IdentityError::MalformedSpiffeId(format!(
                "invalid trust domain {trust_domain:?}"
            )));
        }
        if trust_domain.chars().any(|c| c.is_ascii_uppercase()) {
            return Err(IdentityError::MalformedSpiffeId(format!(
                "trust domain {trust_domain:?} must be lowercase"
            )));
        }

        if path.is_empty() {
            return Err(IdentityError::MalformedSpiffeId(
                "empty workload path".to_string(),
            ));
        }
        let path_segments: Vec<String> = path.split('/').map(str::to_string).collect();
        if path_segments
            .iter()
            .any(|s| s.is_empty() || s == "." || s == "..")
        {
            return Err(IdentityError::MalformedSpiffeId(format!(
                "invalid path segment in {raw:?}"
            )));
        }

        Ok(Self {
            raw: format!("spiffe://{trust_domain}/{path}"),
            trust_domain: trust_domain.to_string(),
            path_segments,
        })
    }

    /// The trust domain (the `spiffe://` authority component).
    pub fn trust_domain(&self) -> &str {
        &self.trust_domain
    }

    /// The full path, without the leading slash.
    pub fn path(&self) -> String {
        self.path_segments.join("/")
    }

    /// The workload namespace when the path uses `/ns/<ns>/sa/<sa>`.
    pub fn namespace(&self) -> Option<&str> {
        self.segment_after("ns")
    }

    /// The workload service account when the path uses `/ns/<ns>/sa/<sa>`.
    pub fn service_account(&self) -> Option<&str> {
        self.segment_after("sa")
    }

    fn segment_after(&self, key: &str) -> Option<&str> {
        self.path_segments
            .iter()
            .position(|s| s == key)
            .and_then(|i| self.path_segments.get(i + 1))
            .map(String::as_str)
    }
}

impl fmt::Display for SpiffeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

impl std::str::FromStr for SpiffeId {
    type Err = IdentityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Identity derived from the verified peer certificate chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerIdentity {
    spiffe_id: SpiffeId,
    leaf_der: Vec<u8>,
    chain_len: usize,
}

impl PeerIdentity {
    /// The validated SPIFFE ID of the peer.
    pub fn spiffe_id(&self) -> &SpiffeId {
        &self.spiffe_id
    }

    /// DER bytes of the verified peer leaf certificate.
    pub fn leaf_certificate_der(&self) -> &[u8] {
        &self.leaf_der
    }

    /// Number of certificates in the verified peer chain, including the leaf.
    pub fn chain_len(&self) -> usize {
        self.chain_len
    }

    /// Record this identity under [`PEER_SPIFFE_ID_METADATA`] on server-side
    /// request metadata. Never call this with client-supplied metadata.
    pub fn insert_metadata(&self, metadata: &mut HashMap<String, String>) {
        metadata.insert(
            PEER_SPIFFE_ID_METADATA.to_string(),
            self.spiffe_id.to_string(),
        );
    }
}

impl fmt::Display for PeerIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.spiffe_id.to_string())
    }
}

/// Extract a SPIFFE identity from a verified peer certificate chain.
///
/// `chain` must contain DER-encoded certificates with the leaf first, exactly
/// as returned by `tquic::Connection::peer_cert_chain()`. The TLS stack has
/// already validated the chain; this function only parses the leaf SAN.
///
/// When `trust_domain` is `Some`, an identity in any other trust domain is
/// rejected. When it is `None`, the SAN is still required and must be a valid
/// SPIFFE ID, which keeps the identity model uniform.
pub fn peer_identity_from_chain(
    chain: &[Vec<u8>],
    trust_domain: Option<&str>,
) -> Result<PeerIdentity, IdentityError> {
    let leaf_der = chain
        .first()
        .ok_or(IdentityError::MissingCertificate)?
        .clone();

    let (_, cert) = x509_parser::certificate::X509Certificate::from_der(&leaf_der)
        .map_err(|e| IdentityError::CertificateParse(e.to_string()))?;

    let mut spiffe_uri: Option<&str> = None;
    for extension in cert.extensions() {
        if let ParsedExtension::SubjectAlternativeName(san) = extension.parsed_extension() {
            for name in &san.general_names {
                if let GeneralName::URI(uri) = name {
                    if uri.starts_with("spiffe://") {
                        // Take the first SPIFFE URI; RFC 5280 allows several
                        // SANs, but a workload must have exactly one identity
                        // in the path model used here.
                        if spiffe_uri.is_some() {
                            return Err(IdentityError::MalformedSpiffeId(
                                "certificate has more than one spiffe:// URI SAN".to_string(),
                            ));
                        }
                        spiffe_uri = Some(uri);
                    }
                }
            }
        }
    }

    let uri = spiffe_uri.ok_or(IdentityError::MissingSpiffeSan)?;
    let spiffe_id = SpiffeId::parse(uri)?;

    if let Some(expected) = trust_domain {
        if spiffe_id.trust_domain() != expected {
            return Err(IdentityError::TrustDomainMismatch {
                expected: expected.to_string(),
                actual: spiffe_id.trust_domain().to_string(),
            });
        }
    }

    Ok(PeerIdentity {
        spiffe_id,
        leaf_der,
        chain_len: chain.len(),
    })
}

/// Per-method SPIFFE authorization policy.
///
/// This complements, rather than replaces, the JWT `AuthInterceptor`: the
/// transport identity is a stronger statement about the workload process than
/// a bearer token, while the token can still carry end-user claims.
#[derive(Debug, Clone, Default)]
pub struct SpiffePolicy {
    rules: HashMap<String, Vec<SpiffeId>>,
}

impl SpiffePolicy {
    /// Create an empty policy. Unconfigured methods are not restricted.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allow exactly `allowed` identities to call `method`.
    ///
    /// Passing an empty list fails the method closed for every identity.
    pub fn allow(&mut self, method: impl Into<String>, allowed: Vec<SpiffeId>) {
        self.rules.insert(method.into(), allowed);
    }

    /// Deny every identity for `method`.
    pub fn deny_all(&mut self, method: impl Into<String>) {
        self.rules.insert(method.into(), Vec::new());
    }

    /// Check whether `identity` may call `method`.
    pub fn check(
        &self,
        method: &str,
        identity: Option<&PeerIdentity>,
    ) -> Result<(), IdentityError> {
        self.check_id(method, identity.map(PeerIdentity::spiffe_id))
    }

    /// Check a bare SPIFFE ID, for callers (such as the RPC interceptor) that
    /// only have the identity string from request metadata.
    pub fn check_id(&self, method: &str, identity: Option<&SpiffeId>) -> Result<(), IdentityError> {
        let Some(allowed) = self.rules.get(method) else {
            return Ok(());
        };

        let Some(identity) = identity else {
            return Err(IdentityError::PolicyDenied {
                method: method.to_string(),
                identity: None,
            });
        };

        if allowed.iter().any(|id| id == identity) {
            Ok(())
        } else {
            Err(IdentityError::PolicyDenied {
                method: method.to_string(),
                identity: Some(identity.to_string()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kubernetes_spiffe_id() {
        let id = SpiffeId::parse("spiffe://cluster.local/ns/payments/sa/ledger").unwrap();
        assert_eq!(id.trust_domain(), "cluster.local");
        assert_eq!(id.namespace(), Some("payments"));
        assert_eq!(id.service_account(), Some("ledger"));
        assert_eq!(
            id.to_string(),
            "spiffe://cluster.local/ns/payments/sa/ledger"
        );
    }

    #[test]
    fn rejects_malformed_spiffe_ids() {
        for raw in [
            "https://cluster.local/ns/a/sa/b",
            "spiffe:///ns/a/sa/b",
            "spiffe://cluster.local",
            "spiffe://cluster.local/",
            "spiffe://cluster.local//ns/a",
            "spiffe://user@cluster.local/ns/a/sa/b",
            "spiffe://Cluster.local/ns/a/sa/b",
            "spiffe://cluster.local/ns/../sa/b",
        ] {
            assert!(SpiffeId::parse(raw).is_err(), "{raw} should be rejected");
        }
    }
}
