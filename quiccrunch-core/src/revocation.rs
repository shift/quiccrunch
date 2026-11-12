//! Certificate revocation (CRL) enforcement for the QUIC transport.
//!
//! BoringSSL verifies the chain and validity period during the handshake but
//! this vendored tQUIC build exposes no hook for a CRL store or OCSP. The
//! adapter therefore performs the revocation check itself, immediately after
//! the handshake completes and before the connection is reported as
//! established, using the same `x509-parser` path that extracts the SPIFFE SAN.
//! See [`docs/SECURITY.md`](../../docs/SECURITY.md) for the full policy and for
//! the OCSP decision.
//!
//! # Fail-closed policy
//!
//! When at least one CRL file is configured:
//!
//! * every CRL is parsed and its signature verified against an issuer
//!   certificate found in the configured trust anchors. A malformed CRL, an
//!   unknown issuer, a bad signature, or a missing `nextUpdate` makes the whole
//!   bundle *unusable*. At startup this is a configuration error; on reload the
//!   previously active bundle stays active;
//! * a peer certificate is accepted only if a currently valid CRL for its
//!   issuer was loaded and the peer serial is not listed;
//! * with [`CrlPolicy::Required`] (the default) a missing or expired CRL for
//!   the peer's issuer rejects the handshake;
//! * [`CrlPolicy::IfPresent`] is an explicit, documented opt-out that keeps
//!   checking CRLs when a valid one is available but allows a handshake when
//!   none is (soft-fail). It is never selected implicitly.
//!
//! CRLs are evaluated at handshake time, so an already-established connection
//! keeps the identity it verified at handshake time; a revoked peer is rejected
//! on its next connection after the new CRL is loaded.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use x509_parser::num_bigint::BigUint;
use x509_parser::prelude::FromDer;
use x509_parser::revocation_list::CertificateRevocationList;

/// How a missing or unusable CRL for the peer's issuer is treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrlPolicy {
    /// Fail closed: a CRL must be configured, parseable, signature-valid and
    /// currently within its validity window for the peer's issuer. This is the
    /// default whenever CRL files are configured.
    Required,
    /// Soft-fail when no *valid* CRL is available for the peer's issuer. CRLs
    /// are still enforced whenever a valid one exists. Callers must opt in
    /// explicitly; this weakens the revocation guarantee and is logged.
    IfPresent,
}

impl Default for CrlPolicy {
    fn default() -> Self {
        Self::Required
    }
}

impl fmt::Display for CrlPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Required => f.write_str("required"),
            Self::IfPresent => f.write_str("if-present"),
        }
    }
}

/// Why a bundle of CRL files could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrlLoadError {
    Io { path: PathBuf, error: String },
    Malformed { path: PathBuf, error: String },
    NoTrustAnchors,
    NoTrustAnchorParsed { path: PathBuf },
    UnknownIssuer { path: PathBuf, issuer: String },
    BadSignature { path: PathBuf, issuer: String },
    MissingNextUpdate { path: PathBuf, issuer: String },
}

impl fmt::Display for CrlLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "cannot read CRL {}: {error}", path.display()),
            Self::Malformed { path, error } => {
                write!(f, "malformed CRL {}: {error}", path.display())
            }
            Self::NoTrustAnchors => f.write_str(
                "CRLs are configured but no explicit trust-anchor file is available to verify \
                 their issuer signatures",
            ),
            Self::NoTrustAnchorParsed { path } => {
                write!(f, "no X.509 certificates parsed from {}", path.display())
            }
            Self::UnknownIssuer { path, issuer } => write!(
                f,
                "CRL {} is issued by {issuer}, which is not one of the configured trust anchors",
                path.display()
            ),
            Self::BadSignature { path, issuer } => write!(
                f,
                "CRL {} signature does not verify against issuer {issuer}",
                path.display()
            ),
            Self::MissingNextUpdate { path, issuer } => write!(
                f,
                "CRL {} from issuer {issuer} has no nextUpdate; it can never be shown to be current",
                path.display()
            ),
        }
    }
}

impl std::error::Error for CrlLoadError {}

/// Why a verified peer certificate was rejected by revocation policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevocationError {
    /// The leaf certificate could not be parsed as DER X.509.
    CertificateParse(String),
    /// No CRL was loaded for the issuer of the peer certificate.
    MissingCrl { issuer: String },
    /// A CRL for the issuer exists but none is currently within its validity
    /// window (expired, not yet valid, or clock skew beyond the tolerance).
    UnusableCrl { issuer: String, reason: String },
    /// The peer certificate serial appears in a valid CRL.
    Revoked { issuer: String, serial: String },
}

impl fmt::Display for RevocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CertificateParse(e) => write!(f, "peer certificate parse failed: {e}"),
            Self::MissingCrl { issuer } => {
                write!(f, "no CRL loaded for certificate issuer {issuer}")
            }
            Self::UnusableCrl { issuer, reason } => {
                write!(f, "no usable CRL for certificate issuer {issuer}: {reason}")
            }
            Self::Revoked { issuer, serial } => write!(
                f,
                "certificate serial {serial} is revoked by CRL from issuer {issuer}"
            ),
        }
    }
}

impl std::error::Error for RevocationError {}

#[derive(Debug)]
struct StoredCrl {
    issuer_raw: Vec<u8>,
    revoked: HashSet<Vec<u8>>,
    this_update: i64,
    next_update: i64,
}

impl StoredCrl {
    fn status_at(&self, now: i64, skew: i64) -> Result<(), String> {
        if self.this_update > now + skew {
            return Err(format!(
                "CRL is not valid before {} (thisUpdate)",
                self.this_update
            ));
        }
        if self.next_update < now - skew {
            return Err(format!("CRL expired at {} (nextUpdate)", self.next_update));
        }
        Ok(())
    }

    fn is_revoked(&self, serial: &BigUint) -> bool {
        self.revoked.contains(&serial.to_bytes_be())
    }
}

/// An immutable, fully validated set of CRLs plus the issuer certificates used
/// to verify them.
#[derive(Debug)]
pub struct RevocationStore {
    policy: CrlPolicy,
    crls: Vec<StoredCrl>,
    configured: bool,
}

impl RevocationStore {
    /// An empty store: revocation checking is not configured and every chain is
    /// allowed by this layer (the TLS stack still verifies the chain).
    pub fn empty() -> Self {
        Self {
            policy: CrlPolicy::Required,
            crls: Vec::new(),
            configured: false,
        }
    }

    /// Whether any CRL file was configured.
    pub fn is_configured(&self) -> bool {
        self.configured
    }

    /// The configured failure policy.
    pub fn policy(&self) -> CrlPolicy {
        self.policy
    }

    /// Number of loaded CRL entries (one per issuer/CRL pair).
    pub fn len(&self) -> usize {
        self.crls.len()
    }

    pub fn is_empty(&self) -> bool {
        self.crls.is_empty()
    }

    /// Load and validate every configured CRL.
    ///
    /// `trust_anchor_paths` are the same files/directories handed to BoringSSL
    /// for chain verification; the issuer certificate of each CRL must be
    /// present there. Nothing is mutated on failure, which is what lets the
    /// caller keep the previous bundle active on a bad reload.
    pub fn load(
        crl_paths: &[PathBuf],
        trust_anchor_paths: &[PathBuf],
        policy: CrlPolicy,
    ) -> Result<Self, CrlLoadError> {
        if crl_paths.is_empty() {
            return Ok(Self::empty());
        }

        let mut issuers = Vec::new();
        for path in trust_anchor_paths {
            let parsed = parse_certificates(path)?;
            if parsed.is_empty() {
                return Err(CrlLoadError::NoTrustAnchorParsed { path: path.clone() });
            }
            issuers.extend(parsed);
        }
        if issuers.is_empty() {
            return Err(CrlLoadError::NoTrustAnchors);
        }

        let mut crls = Vec::new();
        for path in crl_paths {
            for der in parse_der_objects(path, "CRL")? {
                let (_, crl) = CertificateRevocationList::from_der(&der).map_err(|e| {
                    CrlLoadError::Malformed {
                        path: path.clone(),
                        error: e.to_string(),
                    }
                })?;

                let issuer_raw = crl.issuer().as_raw().to_vec();
                let issuer_display = crl.issuer().to_string();
                let issuer = issuers
                    .iter()
                    .find(|cert| cert.subject_raw == issuer_raw)
                    .ok_or_else(|| CrlLoadError::UnknownIssuer {
                        path: path.clone(),
                        issuer: issuer_display.clone(),
                    })?;
                let (_, issuer_cert) = x509_parser::certificate::X509Certificate::from_der(
                    &issuer.der,
                )
                .map_err(|e| CrlLoadError::Malformed {
                    path: path.clone(),
                    error: format!("issuer certificate: {e}"),
                })?;

                crl.verify_signature(issuer_cert.public_key())
                    .map_err(|e| CrlLoadError::BadSignature {
                        path: path.clone(),
                        issuer: format!("{issuer_display}: {e}"),
                    })?;

                let next_update =
                    crl.next_update()
                        .ok_or_else(|| CrlLoadError::MissingNextUpdate {
                            path: path.clone(),
                            issuer: issuer_display.clone(),
                        })?;

                let revoked = crl
                    .iter_revoked_certificates()
                    .map(|revoked| revoked.serial().to_bytes_be())
                    .collect();

                crls.push(StoredCrl {
                    issuer_raw,
                    revoked,
                    this_update: crl.last_update().timestamp(),
                    next_update: next_update.timestamp(),
                });
            }
        }

        Ok(Self {
            policy,
            crls,
            configured: true,
        })
    }

    /// Check the leaf of a verified peer chain against the loaded CRLs.
    ///
    /// `chain` is DER, leaf first, exactly as
    /// `tquic::Connection::peer_cert_chain()` returns it.
    pub fn check_chain(&self, chain: &[Vec<u8>]) -> Result<(), RevocationError> {
        if !self.configured {
            return Ok(());
        }
        let leaf = chain
            .first()
            .ok_or_else(|| RevocationError::CertificateParse("empty certificate chain".into()))?;
        let (_, cert) = x509_parser::certificate::X509Certificate::from_der(leaf)
            .map_err(|e| RevocationError::CertificateParse(e.to_string()))?;

        let issuer_raw = cert.issuer().as_raw();
        let issuer_display = cert.issuer().to_string();
        let matching: Vec<&StoredCrl> = self
            .crls
            .iter()
            .filter(|crl| crl.issuer_raw == issuer_raw)
            .collect();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // Tolerate modest clock skew between the CA and the verifier.
        const CLOCK_SKEW_SECS: i64 = 300;

        let mut valid: Vec<&StoredCrl> = Vec::new();
        let mut unusable_reason: Option<String> = None;
        for crl in &matching {
            match crl.status_at(now, CLOCK_SKEW_SECS) {
                Ok(()) => valid.push(crl),
                Err(reason) => unusable_reason = Some(reason),
            }
        }

        if valid.is_empty() {
            if self.policy == CrlPolicy::IfPresent {
                return Ok(());
            }
            return match unusable_reason {
                Some(reason) => Err(RevocationError::UnusableCrl {
                    issuer: issuer_display,
                    reason,
                }),
                None => Err(RevocationError::MissingCrl {
                    issuer: issuer_display,
                }),
            };
        }

        let serial = BigUint::from_bytes_be(cert.raw_serial());
        if valid.iter().any(|crl| crl.is_revoked(&serial)) {
            return Err(RevocationError::Revoked {
                issuer: issuer_display,
                serial: serial.to_str_radix(16),
            });
        }

        Ok(())
    }
}

struct IssuerCert {
    subject_raw: Vec<u8>,
    der: Vec<u8>,
}

/// Read every certificate from a PEM file, or from every file in a directory
/// (the hashed-CA directory layout BoringSSL also accepts). A single
/// DER-encoded certificate file is accepted too.
fn parse_certificates(path: &Path) -> Result<Vec<IssuerCert>, CrlLoadError> {
    let mut out = Vec::new();
    let files = collect_files(path)?;
    for file in files {
        let bytes = std::fs::read(&file).map_err(|e| CrlLoadError::Io {
            path: file.clone(),
            error: e.to_string(),
        })?;
        let mut found_pem = false;
        for pem in x509_parser::pem::Pem::iter_from_buffer(&bytes) {
            let pem = pem.map_err(|e| CrlLoadError::Malformed {
                path: file.clone(),
                error: e.to_string(),
            })?;
            if !pem.label.contains("CERTIFICATE") {
                continue;
            }
            found_pem = true;
            let der = pem.contents;
            push_issuer(&mut out, &der, &file)?;
        }
        if !found_pem {
            // DER certificate (no PEM armour).
            push_issuer(&mut out, &bytes, &file)?;
        }
    }
    Ok(out)
}

fn push_issuer(out: &mut Vec<IssuerCert>, der: &[u8], path: &Path) -> Result<(), CrlLoadError> {
    let (_, cert) = x509_parser::certificate::X509Certificate::from_der(der).map_err(|e| {
        CrlLoadError::Malformed {
            path: path.to_path_buf(),
            error: format!("certificate: {e}"),
        }
    })?;
    out.push(IssuerCert {
        subject_raw: cert.subject().as_raw().to_vec(),
        der: der.to_vec(),
    });
    Ok(())
}

/// Read every PEM object whose label contains `label` (for example `CRL`), or
/// treat a file without PEM armour as one DER object.
fn parse_der_objects(path: &Path, label: &str) -> Result<Vec<Vec<u8>>, CrlLoadError> {
    let bytes = std::fs::read(path).map_err(|e| CrlLoadError::Io {
        path: path.to_path_buf(),
        error: e.to_string(),
    })?;
    let mut out = Vec::new();
    let mut found_pem = false;
    for pem in x509_parser::pem::Pem::iter_from_buffer(&bytes) {
        let pem = pem.map_err(|e| CrlLoadError::Malformed {
            path: path.to_path_buf(),
            error: e.to_string(),
        })?;
        if !pem.label.to_ascii_uppercase().contains(label) {
            continue;
        }
        found_pem = true;
        out.push(pem.contents);
    }
    if !found_pem {
        out.push(bytes);
    }
    Ok(out)
}

fn collect_files(path: &Path) -> Result<Vec<PathBuf>, CrlLoadError> {
    let meta = std::fs::metadata(path).map_err(|e| CrlLoadError::Io {
        path: path.to_path_buf(),
        error: e.to_string(),
    })?;
    if meta.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut files = Vec::new();
    let entries = std::fs::read_dir(path).map_err(|e| CrlLoadError::Io {
        path: path.to_path_buf(),
        error: e.to_string(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| CrlLoadError::Io {
            path: path.to_path_buf(),
            error: e.to_string(),
        })?;
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

/// Atomically replace `path` with `contents`: write and fsync a temporary file
/// in the same directory, then `rename` it over the target. Consumers must use
/// this (or an equivalent temp-file + rename) so a reload can never observe a
/// half-written certificate, key or CRL.
pub fn write_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "material".to_string());
    let tmp = dir.join(format!(".{file_name}.tmp.{}", std::process::id()));
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    if let Ok(dir_file) = std::fs::File::open(dir) {
        let _ = dir_file.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_store_allows_everything() {
        let store = RevocationStore::empty();
        assert!(!store.is_configured());
        assert!(store.check_chain(&[]).is_ok());
    }

    #[test]
    fn write_atomic_replaces_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crl.pem");
        std::fs::write(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }
}
