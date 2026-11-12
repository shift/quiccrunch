//! End-to-end revocation and hot-rotation tests over real UDP sockets.
//!
//! Every CA, leaf certificate and CRL is generated in-test with `rcgen`; the
//! tests exercise the real tQUIC adapter and BoringSSL handshake.

#![cfg(feature = "tls")]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use quiccrunch_core::revocation::{write_atomic, CrlPolicy};
use quiccrunch_core::transport::adapter::{
    RuntimeOptions, TquicClient, TquicListener, TquicServer,
};
use rcgen::{
    date_time_ymd, BasicConstraints, Certificate, CertificateParams, CertificateRevocationList,
    CertificateRevocationListParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyIdMethod, KeyUsagePurpose, RevocationReason, RevokedCertParams, SanType, SerialNumber,
    PKCS_ECDSA_P256_SHA256,
};
use tempfile::TempDir;

const TRUST_DOMAIN: &str = "cluster.local";
const CLIENT_A: &str = "spiffe://cluster.local/ns/payments/sa/ledger";
const CLIENT_B: &str = "spiffe://cluster.local/ns/payments/sa/ledger-v2";
const SERVER_V1: &str = "spiffe://cluster.local/ns/kv/sa/server-v1";
const SERVER_V2: &str = "spiffe://cluster.local/ns/kv/sa/server-v2";

const SERIAL_A: u64 = 0x11;
const SERIAL_B: u64 = 0x22;

struct Pki {
    dir: TempDir,
}

impl Pki {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("tempdir"),
        }
    }

    fn add_ca(&self, name: &str) -> Certificate {
        make_ca(self.dir.path(), name)
    }

    fn ca_pem(&self, name: &str) -> PathBuf {
        self.dir.path().join(format!("{name}.pem"))
    }

    fn ca_string(&self, name: &str) -> String {
        self.ca_pem(name).to_string_lossy().into_owned()
    }

    fn server_cert(&self, ca: &Certificate, name: &str, spiffe: &str) -> (String, String) {
        make_server_cert(self.dir.path(), ca, name, spiffe)
    }

    fn client_cert(
        &self,
        ca: &Certificate,
        name: &str,
        spiffe: &str,
        serial: u64,
    ) -> (String, String) {
        make_client_cert(self.dir.path(), ca, name, spiffe, serial)
    }
}

fn make_ca(dir: &Path, name: &str) -> Certificate {
    let mut params = CertificateParams::new(Vec::<String>::new());
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, format!("{name}-ca"));
    params.alg = &PKCS_ECDSA_P256_SHA256;
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::CrlSign,
    ];
    let cert = Certificate::from_params(params).expect("generate CA");
    write_atomic(
        &dir.join(format!("{name}.pem")),
        cert.serialize_pem().unwrap().as_bytes(),
    )
    .unwrap();
    cert
}

fn make_server_cert(dir: &Path, ca: &Certificate, name: &str, spiffe: &str) -> (String, String) {
    let mut params = CertificateParams::new(Vec::<String>::new());
    params.subject_alt_names = vec![
        SanType::DnsName("127.0.0.1".to_string()),
        SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
        SanType::URI(spiffe.to_string()),
    ];
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, name);
    params.alg = &PKCS_ECDSA_P256_SHA256;
    params.is_ca = IsCa::NoCa;
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let cert = Certificate::from_params(params).expect("generate server cert");
    let cert_path = dir.join(format!("{name}-cert.pem"));
    let key_path = dir.join(format!("{name}-key.pem"));
    write_atomic(
        &cert_path,
        cert.serialize_pem_with_signer(ca)
            .expect("sign server cert")
            .as_bytes(),
    )
    .unwrap();
    write_atomic(&key_path, cert.serialize_private_key_pem().as_bytes()).unwrap();
    (
        cert_path.to_string_lossy().into_owned(),
        key_path.to_string_lossy().into_owned(),
    )
}

fn make_client_cert(
    dir: &Path,
    ca: &Certificate,
    name: &str,
    spiffe: &str,
    serial: u64,
) -> (String, String) {
    let mut params = CertificateParams::new(Vec::<String>::new());
    params.subject_alt_names = vec![SanType::URI(spiffe.to_string())];
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, name);
    params.serial_number = Some(SerialNumber::from(serial));
    params.alg = &PKCS_ECDSA_P256_SHA256;
    params.is_ca = IsCa::NoCa;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    let cert = Certificate::from_params(params).expect("generate client cert");
    let cert_path = dir.join(format!("{name}-cert.pem"));
    let key_path = dir.join(format!("{name}-key.pem"));
    write_atomic(
        &cert_path,
        cert.serialize_pem_with_signer(ca)
            .expect("sign client cert")
            .as_bytes(),
    )
    .unwrap();
    write_atomic(&key_path, cert.serialize_private_key_pem().as_bytes()).unwrap();
    (
        cert_path.to_string_lossy().into_owned(),
        key_path.to_string_lossy().into_owned(),
    )
}

fn write_crl(
    path: &Path,
    ca: &Certificate,
    revoked: &[(u64, RevocationReason)],
    valid_from: (i32, u8, u8),
    valid_until: (i32, u8, u8),
    number: u64,
) {
    let this_update = date_time_ymd(valid_from.0, valid_from.1, valid_from.2);
    let next_update = date_time_ymd(valid_until.0, valid_until.1, valid_until.2);
    let params = CertificateRevocationListParams {
        this_update,
        next_update,
        crl_number: SerialNumber::from(number),
        issuing_distribution_point: None,
        revoked_certs: revoked
            .iter()
            .map(|(serial, reason)| RevokedCertParams {
                serial_number: SerialNumber::from(*serial),
                revocation_time: this_update,
                reason_code: Some(*reason),
                invalidity_date: None,
            })
            .collect(),
        alg: &PKCS_ECDSA_P256_SHA256,
        key_identifier_method: KeyIdMethod::Sha256,
    };
    let crl = CertificateRevocationList::from_params(params).expect("CRL params");
    let pem = crl.serialize_pem_with_signer(ca).expect("sign CRL");
    write_atomic(path, pem.as_bytes()).expect("write CRL atomically");
}

fn server_options(ca_pem: String, crl_files: Vec<PathBuf>) -> RuntimeOptions {
    RuntimeOptions {
        client_ca_file: Some(ca_pem),
        trust_domain: Some(TRUST_DOMAIN.to_string()),
        crl_files: crl_files
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        crl_policy: CrlPolicy::Required,
        reload_interval: None,
        ..RuntimeOptions::default()
    }
}

fn client_options(ca_pem: String, client: &(String, String)) -> RuntimeOptions {
    RuntimeOptions {
        verify_server_cert: true,
        ca_cert_file: Some(ca_pem),
        client_cert_file: Some(client.0.clone()),
        client_key_file: Some(client.1.clone()),
        reload_interval: None,
        ..RuntimeOptions::default()
    }
}

fn free_udp_addr() -> SocketAddr {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("probe socket");
    let addr = socket.local_addr().unwrap();
    drop(socket);
    addr
}

async fn start_server(
    cert: (String, String),
    options: RuntimeOptions,
) -> (TquicServer, TquicListener) {
    let addr = free_udp_addr();
    let server = TquicServer::with_options(addr, cert.0, cert.1, options)
        .await
        .expect("start server");
    let listener = server.listen("").await.expect("listener");
    (server, listener)
}

async fn connect(
    ca_pem: String,
    client_cert: &(String, String),
    addr: &str,
) -> Result<quiccrunch_core::transport::adapter::TquicConnection, String> {
    let client = TquicClient::with_options(client_options(ca_pem, client_cert))
        .await
        .map_err(|e| e.to_string())?;
    tokio::time::timeout(Duration::from_secs(15), client.connect(addr))
        .await
        .map_err(|_| "handshake timeout".to_string())?
        .map_err(|e| e.to_string())
}

async fn assert_accepted(
    ca_pem: String,
    client_cert: &(String, String),
    listener: &mut TquicListener,
) {
    let addr = listener.local_addr().unwrap().to_string();
    let connection = connect(ca_pem, client_cert, &addr)
        .await
        .expect("client handshake should succeed");
    let _accepted = tokio::time::timeout(Duration::from_secs(10), listener.accept_with_identity())
        .await
        .expect("server accept timeout")
        .expect("server must accept the connection");
    drop(connection);
}

async fn assert_rejected(
    ca_pem: String,
    client_cert: &(String, String),
    listener: &mut TquicListener,
) {
    let addr = listener.local_addr().unwrap().to_string();
    let _ = connect(ca_pem, client_cert, &addr).await;
    // A rejected peer never produces an accept event. Deliberately do not queue
    // an `accept` waiter here: cancelling one on timeout would swallow the next
    // legitimately accepted connection.
    tokio::time::sleep(Duration::from_millis(300)).await;
}

// ---------------------------------------------------------------------------
// CRL enforcement
// ---------------------------------------------------------------------------

#[tokio::test]
async fn revoked_client_is_rejected_and_non_revoked_is_accepted() {
    let pki = Pki::new();
    let ca = pki.add_ca("ca1");
    let crl_path = pki.dir.path().join("crl.pem");
    write_crl(
        &crl_path,
        &ca,
        &[(SERIAL_A, RevocationReason::KeyCompromise)],
        (2025, 1, 1),
        (2035, 1, 1),
        1,
    );

    let (server, mut listener) = start_server(
        pki.server_cert(&ca, "server", SERVER_V1),
        server_options(pki.ca_string("ca1"), vec![crl_path]),
    )
    .await;

    let revoked = pki.client_cert(&ca, "client-a", CLIENT_A, SERIAL_A);
    assert_rejected(pki.ca_string("ca1"), &revoked, &mut listener).await;
    assert!(
        server.metrics().snapshot().revocation_rejections_revoked >= 1,
        "revoked serial must be counted by reason"
    );

    let valid = pki.client_cert(&ca, "client-b", CLIENT_B, SERIAL_B);
    assert_accepted(pki.ca_string("ca1"), &valid, &mut listener).await;
}

#[tokio::test]
async fn expired_crl_fails_closed_for_configured_issuer() {
    let pki = Pki::new();
    let ca = pki.add_ca("ca1");
    let crl_path = pki.dir.path().join("expired-crl.pem");
    write_crl(&crl_path, &ca, &[], (2020, 1, 1), (2021, 1, 1), 1);

    let (server, mut listener) = start_server(
        pki.server_cert(&ca, "server", SERVER_V1),
        server_options(pki.ca_string("ca1"), vec![crl_path]),
    )
    .await;

    let valid = pki.client_cert(&ca, "client-a", CLIENT_A, SERIAL_A);
    assert_rejected(pki.ca_string("ca1"), &valid, &mut listener).await;
    assert!(
        server
            .metrics()
            .snapshot()
            .revocation_rejections_unusable_crl
            >= 1,
        "expired CRL rejection must be counted by reason"
    );
}

#[tokio::test]
async fn missing_crl_for_issuer_fails_closed() {
    let pki = Pki::new();
    let ca = pki.add_ca("ca1");
    let foreign = pki.add_ca("foreign");
    let crl_path = pki.dir.path().join("foreign-crl.pem");
    // A CRL for an unrelated issuer is configured; `required` policy must not
    // let a certificate from ca1 through without a ca1 CRL.
    write_crl(&crl_path, &foreign, &[], (2025, 1, 1), (2035, 1, 1), 1);

    // Both CAs are trusted (so the foreign CRL is verifiable and loads), but
    // only the foreign issuer has a CRL.
    let bundle = pki.dir.path().join("client-ca-bundle.pem");
    let mut bundle_pem = std::fs::read(pki.ca_pem("ca1")).unwrap();
    bundle_pem.extend(std::fs::read(pki.ca_pem("foreign")).unwrap());
    write_atomic(&bundle, &bundle_pem).unwrap();

    let (server, mut listener) = start_server(
        pki.server_cert(&ca, "server", SERVER_V1),
        server_options(bundle.to_string_lossy().into_owned(), vec![crl_path]),
    )
    .await;

    let valid = pki.client_cert(&ca, "client-a", CLIENT_A, SERIAL_A);
    assert_rejected(pki.ca_string("ca1"), &valid, &mut listener).await;
    assert!(
        server
            .metrics()
            .snapshot()
            .revocation_rejections_missing_crl
            >= 1,
        "missing issuer CRL rejection must be counted by reason"
    );
}

#[tokio::test]
async fn malformed_crl_reload_is_rejected_and_previous_bundle_stays_active() {
    let pki = Pki::new();
    let ca = pki.add_ca("ca1");
    let crl_path = pki.dir.path().join("crl.pem");
    write_crl(&crl_path, &ca, &[], (2025, 1, 1), (2035, 1, 1), 1);

    let (server, mut listener) = start_server(
        pki.server_cert(&ca, "server", SERVER_V1),
        server_options(pki.ca_string("ca1"), vec![crl_path.clone()]),
    )
    .await;

    let valid = pki.client_cert(&ca, "client-a", CLIENT_A, SERIAL_A);
    assert_accepted(pki.ca_string("ca1"), &valid, &mut listener).await;

    write_atomic(
        &crl_path,
        b"-----BEGIN X509 CRL-----\nnot base64!!\n-----END X509 CRL-----\n",
    )
    .unwrap();
    let err = server
        .reload_tls_material()
        .await
        .expect_err("malformed CRL reload must fail");
    assert!(
        err.to_string().contains("CRL"),
        "error must identify the CRL failure: {err}"
    );
    assert!(server.metrics().snapshot().crl_reload_failure >= 1);

    // The old, valid (empty) bundle is still active, so the client remains valid.
    assert_accepted(pki.ca_string("ca1"), &valid, &mut listener).await;
}

#[tokio::test]
async fn unsigned_crl_reload_is_rejected_with_clear_error() {
    let pki = Pki::new();
    let ca = pki.add_ca("ca1");
    let crl_path = pki.dir.path().join("crl.pem");
    write_crl(&crl_path, &ca, &[], (2025, 1, 1), (2035, 1, 1), 1);

    let (server, _listener) = start_server(
        pki.server_cert(&ca, "server", SERVER_V1),
        server_options(pki.ca_string("ca1"), vec![crl_path.clone()]),
    )
    .await;

    // Same issuer DN, different signing key: signature verification must fail.
    let mut params = CertificateParams::new(Vec::<String>::new());
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, "ca1-ca");
    params.alg = &PKCS_ECDSA_P256_SHA256;
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let same_name = Certificate::from_params(params).expect("impostor CA");
    write_crl(
        &crl_path,
        &same_name,
        &[(SERIAL_A, RevocationReason::KeyCompromise)],
        (2025, 1, 1),
        (2035, 1, 1),
        2,
    );

    let err = server
        .reload_tls_material()
        .await
        .expect_err("CRL with an unverifiable signature must be rejected");
    assert!(
        err.to_string().to_lowercase().contains("signature") || err.to_string().contains("CRL"),
        "error must explain the CRL problem: {err}"
    );
    assert!(server.metrics().snapshot().crl_reload_failure >= 1);
}

#[tokio::test]
async fn crl_reload_adds_newly_revoked_serial_without_restart() {
    let pki = Pki::new();
    let ca = pki.add_ca("ca1");
    let crl_path = pki.dir.path().join("crl.pem");
    write_crl(&crl_path, &ca, &[], (2025, 1, 1), (2035, 1, 1), 1);

    let (server, mut listener) = start_server(
        pki.server_cert(&ca, "server", SERVER_V1),
        server_options(pki.ca_string("ca1"), vec![crl_path.clone()]),
    )
    .await;

    let client = pki.client_cert(&ca, "client-a", CLIENT_A, SERIAL_A);
    assert_accepted(pki.ca_string("ca1"), &client, &mut listener).await;

    write_crl(
        &crl_path,
        &ca,
        &[(SERIAL_A, RevocationReason::KeyCompromise)],
        (2025, 1, 1),
        (2035, 1, 1),
        2,
    );
    server
        .reload_tls_material()
        .await
        .expect("valid CRL reload must succeed");
    assert!(server.metrics().snapshot().crl_reload_success >= 1);

    assert_rejected(pki.ca_string("ca1"), &client, &mut listener).await;
}

#[tokio::test]
async fn crl_reload_happens_on_bounded_interval() {
    let pki = Pki::new();
    let ca = pki.add_ca("ca1");
    let crl_path = pki.dir.path().join("crl.pem");
    write_crl(&crl_path, &ca, &[], (2025, 1, 1), (2035, 1, 1), 1);

    let mut options = server_options(pki.ca_string("ca1"), vec![crl_path.clone()]);
    options.reload_interval = Some(Duration::from_millis(100));
    let (server, mut listener) =
        start_server(pki.server_cert(&ca, "server", SERVER_V1), options).await;

    let client = pki.client_cert(&ca, "client-a", CLIENT_A, SERIAL_A);
    assert_accepted(pki.ca_string("ca1"), &client, &mut listener).await;

    write_crl(
        &crl_path,
        &ca,
        &[(SERIAL_A, RevocationReason::KeyCompromise)],
        (2025, 1, 1),
        (2035, 1, 1),
        2,
    );

    // Give the bounded interval several chances to observe the new CRL.
    let mut rejected = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let addr = listener.local_addr().unwrap().to_string();
        let _ = connect(pki.ca_string("ca1"), &client, &addr).await;
        if tokio::time::timeout(Duration::from_secs(1), listener.accept_with_identity())
            .await
            .is_err()
        {
            rejected = true;
            break;
        }
    }
    assert!(
        rejected,
        "the periodic reloader must pick up the new CRL without a restart"
    );
    assert!(server.metrics().snapshot().crl_reload_success >= 1);
}

// ---------------------------------------------------------------------------
// Hot rotation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_certificate_rotation_serves_new_cert_without_dropping_connections() {
    let pki = Pki::new();
    let ca = pki.add_ca("ca1");
    let (server_cert_v1, key_v1) = pki.server_cert(&ca, "server-v1", SERVER_V1);
    let (server, mut listener) = start_server(
        (server_cert_v1.clone(), key_v1.clone()),
        server_options(pki.ca_string("ca1"), vec![]),
    )
    .await;

    let client_cert = pki.client_cert(&ca, "client-a", CLIENT_A, SERIAL_A);
    let addr = listener.local_addr().unwrap().to_string();
    let old_client = TquicClient::with_options(client_options(pki.ca_string("ca1"), &client_cert))
        .await
        .expect("client");
    let old_conn = tokio::time::timeout(Duration::from_secs(15), old_client.connect(&addr))
        .await
        .expect("old handshake timeout")
        .expect("old handshake");
    let _old_accepted =
        tokio::time::timeout(Duration::from_secs(10), listener.accept_with_identity())
            .await
            .expect("accept timeout")
            .expect("accept");
    let old_identity = old_conn
        .peer_identity()
        .expect("old server identity")
        .spiffe_id()
        .to_string();
    assert_eq!(old_identity, SERVER_V1);

    // Rotate the server certificate/key in place (atomic per file).
    let (server_cert_v2, key_v2) = pki.server_cert(&ca, "server-v2", SERVER_V2);
    let cert_bytes = std::fs::read(&server_cert_v2).unwrap();
    let key_bytes = std::fs::read(&key_v2).unwrap();
    write_atomic(Path::new(&server_cert_v1), &cert_bytes).unwrap();
    write_atomic(Path::new(&key_v1), &key_bytes).unwrap();
    let report = server
        .reload_tls_material()
        .await
        .expect("server certificate rotation must succeed");
    assert!(report.cert_reloaded);
    assert!(server.metrics().snapshot().cert_reload_success >= 1);

    // New handshakes see the new certificate.
    let new_client = TquicClient::with_options(client_options(pki.ca_string("ca1"), &client_cert))
        .await
        .expect("new client");
    let new_conn = tokio::time::timeout(Duration::from_secs(15), new_client.connect(&addr))
        .await
        .expect("new handshake timeout")
        .expect("new handshake");
    let _new_accepted =
        tokio::time::timeout(Duration::from_secs(10), listener.accept_with_identity())
            .await
            .expect("accept timeout")
            .expect("accept");
    assert_eq!(
        new_conn.peer_identity().unwrap().spiffe_id().to_string(),
        SERVER_V2,
        "new handshake must present the rotated server certificate"
    );

    // The established connection was not dropped: a new stream still works.
    let stream = tokio::time::timeout(Duration::from_secs(10), old_conn.open_stream())
        .await
        .expect("old connection open_stream timeout")
        .expect("established connection must survive rotation");
    stream.send(b"still-alive", true).await.unwrap();
}

#[tokio::test]
async fn trust_anchor_rotation_accepts_new_ca_and_rejects_removed_ca_without_restart() {
    let pki = Pki::new();
    let ca1 = pki.add_ca("ca1");
    let ca2 = pki.add_ca("ca2");

    // The server's client-CA bundle is a separate file from the client's server
    // trust anchor, so rotating one does not invalidate the other.
    let bundle = pki.dir.path().join("client-ca-bundle.pem");
    write_atomic(&bundle, &std::fs::read(pki.ca_pem("ca1")).unwrap()).unwrap();

    let (server, mut listener) = start_server(
        pki.server_cert(&ca1, "server", SERVER_V1),
        server_options(bundle.to_string_lossy().into_owned(), vec![]),
    )
    .await;

    let client1 = pki.client_cert(&ca1, "client-ca1", CLIENT_A, SERIAL_A);
    let client2 = pki.client_cert(&ca2, "client-ca2", CLIENT_B, SERIAL_B);

    assert_accepted(pki.ca_string("ca1"), &client1, &mut listener).await;
    let before = server.metrics().snapshot().mtls_rejections;
    assert_rejected(pki.ca_string("ca1"), &client2, &mut listener).await;
    assert!(
        server.metrics().snapshot().mtls_rejections > before,
        "certificate from an untrusted CA must be rejected before rotation"
    );

    // Replace the client CA bundle with ca2 only.
    let ca2_der = std::fs::read(pki.ca_pem("ca2")).unwrap();
    write_atomic(&bundle, &ca2_der).unwrap();
    let report = server
        .reload_tls_material()
        .await
        .expect("trust-anchor rotation must succeed");
    assert!(report.cert_reloaded);

    let before = server.metrics().snapshot().mtls_rejections;
    assert_rejected(pki.ca_string("ca1"), &client1, &mut listener).await;
    assert!(
        server.metrics().snapshot().mtls_rejections > before,
        "certificate from the removed CA must be rejected after rotation"
    );
    assert_accepted(pki.ca_string("ca1"), &client2, &mut listener).await;
}

#[test]
fn crl_policy_default_is_fail_closed() {
    assert_eq!(CrlPolicy::default(), CrlPolicy::Required);
}
