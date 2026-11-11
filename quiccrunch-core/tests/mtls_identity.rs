//! End-to-end mTLS and SPIFFE identity tests over real UDP sockets.
//!
//! Every certificate is generated in-test with `rcgen`; nothing is read from
//! the repository or the platform trust store. These tests exercise the real
//! tQUIC adapter and BoringSSL handshake, not a mock.

#![cfg(feature = "tls")]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use quiccrunch_core::identity::{peer_identity_from_chain, SpiffeId, SpiffePolicy};
use quiccrunch_core::transport::adapter::{
    RuntimeOptions, TquicClient, TquicListener, TquicServer,
};
use rcgen::{
    date_time_ymd, BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, KeyUsagePurpose, SanType, PKCS_ECDSA_P256_SHA256,
};
use tempfile::TempDir;

const TRUST_DOMAIN: &str = "cluster.local";
const CLIENT_A: &str = "spiffe://cluster.local/ns/payments/sa/ledger";
const CLIENT_B: &str = "spiffe://cluster.local/ns/payments/sa/ledger-v2";

struct Pki {
    dir: TempDir,
    ca_cert: Certificate,
}

impl Pki {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let ca_cert = make_ca(dir.path(), "test-ca");
        Self { dir, ca_cert }
    }

    fn ca_pem(&self) -> PathBuf {
        self.dir.path().join("test-ca.pem")
    }

    fn server_cert(&self) -> (String, String) {
        make_server_cert(self.dir.path(), &self.ca_cert, "server")
    }

    fn client_cert(&self, name: &str, spiffe: &str) -> (String, String) {
        make_client_cert(self.dir.path(), &self.ca_cert, name, spiffe)
    }
}

fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("write cert material");
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
    write(
        &dir.join(format!("{name}.pem")),
        &cert.serialize_pem().unwrap(),
    );
    cert
}

fn make_server_cert(dir: &Path, ca: &Certificate, name: &str) -> (String, String) {
    let mut params = CertificateParams::new(Vec::<String>::new());
    // tQUIC configures BoringSSL's X509_VERIFY_PARAM_set1_host, which matches
    // DNS SANs. The adapter connects by IP string, so include a DNS SAN with
    // that exact value as well as the proper IP SAN.
    params.subject_alt_names = vec![
        SanType::DnsName("127.0.0.1".to_string()),
        SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
        SanType::URI("spiffe://cluster.local/ns/kv/sa/server".to_string()),
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
    write(
        &cert_path,
        &cert
            .serialize_pem_with_signer(ca)
            .expect("sign server cert"),
    );
    write(&key_path, &cert.serialize_private_key_pem());
    (
        cert_path.to_string_lossy().into_owned(),
        key_path.to_string_lossy().into_owned(),
    )
}

fn make_client_cert(dir: &Path, ca: &Certificate, name: &str, spiffe: &str) -> (String, String) {
    make_client_cert_with_validity(dir, ca, name, spiffe, false)
}

fn make_client_cert_with_validity(
    dir: &Path,
    ca: &Certificate,
    name: &str,
    spiffe: &str,
    expired: bool,
) -> (String, String) {
    let mut params = CertificateParams::new(Vec::<String>::new());
    params.subject_alt_names = vec![SanType::URI(spiffe.to_string())];
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, name);
    params.alg = &PKCS_ECDSA_P256_SHA256;
    params.is_ca = IsCa::NoCa;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    if expired {
        params.not_before = date_time_ymd(2020, 1, 1);
        params.not_after = date_time_ymd(2021, 1, 1);
    }
    let cert = Certificate::from_params(params).expect("generate client cert");
    let cert_path = dir.join(format!("{name}-cert.pem"));
    let key_path = dir.join(format!("{name}-key.pem"));
    write(
        &cert_path,
        &cert
            .serialize_pem_with_signer(ca)
            .expect("sign client cert"),
    );
    write(&key_path, &cert.serialize_private_key_pem());
    (
        cert_path.to_string_lossy().into_owned(),
        key_path.to_string_lossy().into_owned(),
    )
}

fn server_options(pki: &Pki) -> RuntimeOptions {
    RuntimeOptions {
        client_ca_file: Some(pki.ca_pem().to_string_lossy().into_owned()),
        trust_domain: Some(TRUST_DOMAIN.to_string()),
        ..RuntimeOptions::default()
    }
}

fn client_options(pki: &Pki, client: Option<&(String, String)>) -> RuntimeOptions {
    RuntimeOptions {
        verify_server_cert: true,
        ca_cert_file: Some(pki.ca_pem().to_string_lossy().into_owned()),
        client_cert_file: client.map(|(cert, _)| cert.clone()),
        client_key_file: client.map(|(_, key)| key.clone()),
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
        .expect("start mTLS server");
    let listener = server.listen("").await.expect("listener");
    (server, listener)
}

#[tokio::test]
async fn mutual_tls_handshake_succeeds_and_exposes_spiffe_identity() {
    let pki = Pki::new();
    let (server, mut listener) = start_server(pki.server_cert(), server_options(&pki)).await;
    assert!(
        server.metrics().snapshot().mtls_enabled,
        "server must report mTLS as enabled"
    );

    let client_cert = pki.client_cert("client-a", CLIENT_A);
    let client = TquicClient::with_options(client_options(&pki, Some(&client_cert)))
        .await
        .expect("client");

    let addr = listener.local_addr().unwrap().to_string();
    let connection = tokio::time::timeout(Duration::from_secs(20), client.connect(&addr))
        .await
        .expect("client handshake must not hang")
        .expect("mTLS handshake must succeed");

    let (accepted, identity) =
        tokio::time::timeout(Duration::from_secs(10), listener.accept_with_identity())
            .await
            .expect("server accept must not hang")
            .expect("server must accept mTLS connection");

    let identity = identity.expect("server must expose a peer identity");
    assert_eq!(identity.spiffe_id().to_string(), CLIENT_A);
    assert_eq!(identity.chain_len(), 1);
    assert_eq!(accepted.peer_identity().unwrap(), &identity);
    assert_eq!(
        connection.peer_identity().unwrap().spiffe_id().to_string(),
        "spiffe://cluster.local/ns/kv/sa/server",
        "client must expose the verified server identity"
    );

    // The identity survives a round trip through DER and the same parser the
    // policy layer uses.
    let reparsed = peer_identity_from_chain(
        &[identity.leaf_certificate_der().to_vec()],
        Some(TRUST_DOMAIN),
    )
    .expect("reparse identity");
    assert_eq!(reparsed.spiffe_id().to_string(), CLIENT_A);
}

#[tokio::test]
async fn server_rejects_client_without_certificate() {
    let pki = Pki::new();
    let (server, mut listener) = start_server(pki.server_cert(), server_options(&pki)).await;

    let client = TquicClient::with_options(client_options(&pki, None))
        .await
        .expect("client");
    let addr = listener.local_addr().unwrap().to_string();

    // The client may see its own side of the handshake complete before the
    // server-side rejection is processed. The security property under test is
    // that the server never reports an authenticated connection.
    let _ = tokio::time::timeout(Duration::from_secs(15), client.connect(&addr)).await;

    let accepted =
        tokio::time::timeout(Duration::from_secs(3), listener.accept_with_identity()).await;
    assert!(
        accepted.is_err(),
        "server must never report an mTLS connection"
    );
    assert!(
        server.metrics().snapshot().mtls_rejections >= 1,
        "handshake failure must increment the mTLS rejection counter"
    );
}

#[tokio::test]
async fn server_rejects_certificate_from_foreign_ca() {
    let pki = Pki::new();
    let foreign = Pki::new();
    let (server, mut listener) = start_server(pki.server_cert(), server_options(&pki)).await;

    let foreign_client = foreign.client_cert("foreign-client", CLIENT_A);
    // The client still trusts the real server CA; only its own certificate is
    // issued by a different CA.
    let client = TquicClient::with_options(client_options(&pki, Some(&foreign_client)))
        .await
        .expect("client");
    let addr = listener.local_addr().unwrap().to_string();

    let _ = tokio::time::timeout(Duration::from_secs(15), client.connect(&addr)).await;
    let accepted =
        tokio::time::timeout(Duration::from_secs(3), listener.accept_with_identity()).await;
    assert!(accepted.is_err(), "foreign-CA client must not be accepted");
    assert!(server.metrics().snapshot().mtls_rejections >= 1);
}

#[tokio::test]
async fn server_rejects_expired_client_certificate() {
    let pki = Pki::new();
    let (server, mut listener) = start_server(pki.server_cert(), server_options(&pki)).await;

    let expired = make_client_cert_with_validity(
        pki.dir.path(),
        &pki.ca_cert,
        "expired-client",
        CLIENT_A,
        true,
    );
    let client = TquicClient::with_options(client_options(&pki, Some(&expired)))
        .await
        .expect("client");
    let addr = listener.local_addr().unwrap().to_string();

    let _ = tokio::time::timeout(Duration::from_secs(15), client.connect(&addr)).await;
    let accepted =
        tokio::time::timeout(Duration::from_secs(3), listener.accept_with_identity()).await;
    assert!(accepted.is_err(), "expired client must not be accepted");
    assert!(server.metrics().snapshot().mtls_rejections >= 1);
}

#[tokio::test]
async fn server_rejects_spiffe_san_outside_trust_domain() {
    let pki = Pki::new();
    let (server, mut listener) = start_server(pki.server_cert(), server_options(&pki)).await;

    // Valid chain and valid clientAuth EKU, but the workload is in a different
    // trust domain. TLS succeeds; the application identity policy must reject
    // it and close the connection without reporting it as established.
    let rogue = pki.client_cert(
        "rogue-client",
        "spiffe://evil.example/ns/payments/sa/ledger",
    );
    let client = TquicClient::with_options(client_options(&pki, Some(&rogue)))
        .await
        .expect("client");
    let addr = listener.local_addr().unwrap().to_string();

    // The client may see the handshake complete before the server-side
    // rejection arrives; either outcome is acceptable as long as the server
    // never accepts the connection.
    let _ = tokio::time::timeout(Duration::from_secs(10), client.connect(&addr)).await;

    let accepted =
        tokio::time::timeout(Duration::from_secs(3), listener.accept_with_identity()).await;
    assert!(
        accepted.is_err(),
        "out-of-domain SPIFFE identity must not be accepted"
    );
    assert!(
        server.metrics().snapshot().mtls_rejections >= 1,
        "out-of-domain SAN must increment the mTLS rejection counter"
    );
}

#[tokio::test]
async fn client_rejects_server_certificate_from_foreign_ca_by_default() {
    let pki = Pki::new();
    let foreign = Pki::new();
    let foreign_server = foreign.server_cert();

    // Server is signed by a CA the client does not trust.
    let (server, mut listener) = start_server(foreign_server, server_options(&pki)).await;

    let client_cert = pki.client_cert("client-a", CLIENT_A);
    let client = TquicClient::with_options(client_options(&pki, Some(&client_cert)))
        .await
        .expect("client");
    let addr = listener.local_addr().unwrap().to_string();

    let result = tokio::time::timeout(Duration::from_secs(15), client.connect(&addr)).await;
    assert!(
        matches!(result, Ok(Err(_))),
        "default client must reject an untrusted server certificate"
    );
    let accepted =
        tokio::time::timeout(Duration::from_secs(3), listener.accept_with_identity()).await;
    assert!(accepted.is_err());
    // The server-side counter is about client authentication; this test is
    // about the client failing closed, which is asserted above.
    let _ = server.metrics().snapshot();
}

#[tokio::test]
async fn spiffe_identity_reaches_method_policy_and_denies_others() {
    let pki = Pki::new();
    let (_server, mut listener) = start_server(pki.server_cert(), server_options(&pki)).await;

    let client_cert = pki.client_cert("client-a", CLIENT_A);
    let client = TquicClient::with_options(client_options(&pki, Some(&client_cert)))
        .await
        .expect("client");
    let addr = listener.local_addr().unwrap().to_string();

    let _connection = tokio::time::timeout(Duration::from_secs(20), client.connect(&addr))
        .await
        .expect("handshake timeout")
        .expect("handshake");
    let (_accepted, identity) =
        tokio::time::timeout(Duration::from_secs(10), listener.accept_with_identity())
            .await
            .expect("accept timeout")
            .expect("accept");
    let identity = identity.expect("peer identity");

    let mut policy = SpiffePolicy::new();
    policy.allow(
        "Raft.AppendEntries",
        vec![SpiffeId::parse(CLIENT_A).unwrap()],
    );

    policy
        .check("Raft.AppendEntries", Some(&identity))
        .expect("allowlisted identity must be allowed");

    let rogue = SpiffeId::parse("spiffe://cluster.local/ns/other/sa/rogue").unwrap();
    assert!(
        policy.check_id("Raft.AppendEntries", Some(&rogue)).is_err(),
        "policy must deny an identity that is not allowlisted"
    );
    assert!(
        policy.check("Raft.AppendEntries", None).is_err(),
        "protected method must deny a missing identity"
    );
}

#[tokio::test]
async fn client_certificate_rotation_without_server_restart() {
    let pki = Pki::new();
    let (_server, mut listener) = start_server(pki.server_cert(), server_options(&pki)).await;
    let addr = listener.local_addr().unwrap().to_string();

    let cert_a = pki.client_cert("client-a", CLIENT_A);
    let client_a = TquicClient::with_options(client_options(&pki, Some(&cert_a)))
        .await
        .expect("client A");
    let _conn_a = tokio::time::timeout(Duration::from_secs(20), client_a.connect(&addr))
        .await
        .expect("handshake A timeout")
        .expect("handshake A");
    let (_accepted_a, identity_a) =
        tokio::time::timeout(Duration::from_secs(10), listener.accept_with_identity())
            .await
            .expect("accept A timeout")
            .expect("accept A");
    assert_eq!(
        identity_a.unwrap().spiffe_id().to_string(),
        CLIENT_A,
        "first rotation identity"
    );

    // A second certificate from the same CA, issued after the server started.
    let cert_b = pki.client_cert("client-b", CLIENT_B);
    let client_b = TquicClient::with_options(client_options(&pki, Some(&cert_b)))
        .await
        .expect("client B");
    let _conn_b = tokio::time::timeout(Duration::from_secs(20), client_b.connect(&addr))
        .await
        .expect("handshake B timeout")
        .expect("handshake B");
    let (_accepted_b, identity_b) =
        tokio::time::timeout(Duration::from_secs(10), listener.accept_with_identity())
            .await
            .expect("accept B timeout")
            .expect("accept B");
    assert_eq!(
        identity_b.unwrap().spiffe_id().to_string(),
        CLIENT_B,
        "second identity must work without restarting the server"
    );
}
