#![allow(dead_code)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use quiccrunch_core::messaging::client::{MessagingPool, PoolConfig};
use quiccrunch_core::messaging::retention::PolicyStore;
use quiccrunch_core::messaging::server::{AuthorizationPolicy, MessagingServer};
use quiccrunch_core::messaging::transport::{serve_listener, TquicConnector};
use quiccrunch_core::messaging::{
    BackpressureConfig, MessagingMetrics, PeerIdentity, RetentionConfig,
};
use quiccrunch_core::transport::adapter::{RuntimeOptions, TquicClient, TquicServer};
use rcgen::{
    date_time_ymd, BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    IsCa, KeyUsagePurpose, SanType, PKCS_ECDSA_P256_SHA256,
};
use tokio::task::JoinHandle;

pub struct TestTls {
    _dir: tempfile::TempDir,
    pub ca_file: String,
    pub cert_file: String,
    pub key_file: String,
}

impl TestTls {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut ca_params = CertificateParams::new(vec!["Messaging Test CA".to_string()]);
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.alg = &PKCS_ECDSA_P256_SHA256;
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::CrlSign,
        ];
        ca_params.not_before = date_time_ymd(2020, 1, 1);
        ca_params.not_after = date_time_ymd(2050, 1, 1);
        ca_params.distinguished_name = DistinguishedName::new();
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "Messaging Test CA");
        let ca = Certificate::from_params(ca_params).expect("CA cert");

        let mut leaf_params =
            CertificateParams::new(vec!["localhost".to_string(), "127.0.0.1".to_string()]);
        leaf_params.distinguished_name = DistinguishedName::new();
        leaf_params
            .distinguished_name
            .push(DnType::CommonName, "messaging-test-node");
        leaf_params.is_ca = IsCa::NoCa;
        leaf_params.alg = &PKCS_ECDSA_P256_SHA256;
        leaf_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        leaf_params.not_before = date_time_ymd(2020, 1, 1);
        leaf_params.not_after = date_time_ymd(2050, 1, 1);
        leaf_params
            .subject_alt_names
            .push(SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        leaf_params
            .subject_alt_names
            .push(SanType::DnsName("127.0.0.1".to_string()));
        let leaf = Certificate::from_params(leaf_params).expect("leaf cert");

        let ca_path = dir.path().join("ca.pem");
        let cert_path = dir.path().join("node.pem");
        let key_path = dir.path().join("node.key");
        std::fs::write(&ca_path, ca.serialize_pem().unwrap()).unwrap();
        std::fs::write(&cert_path, leaf.serialize_pem_with_signer(&ca).unwrap()).unwrap();
        std::fs::write(&key_path, leaf.serialize_private_key_pem()).unwrap();

        Self {
            _dir: dir,
            ca_file: ca_path.to_string_lossy().into_owned(),
            cert_file: cert_path.to_string_lossy().into_owned(),
            key_file: key_path.to_string_lossy().into_owned(),
        }
    }

    pub fn client_options(&self) -> RuntimeOptions {
        RuntimeOptions {
            verify_server_cert: true,
            ca_cert_file: Some(self.ca_file.clone()),
            ..RuntimeOptions::default()
        }
    }
}

pub fn free_port() -> u16 {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind udp");
    socket.local_addr().unwrap().port()
}

pub fn identity() -> PeerIdentity {
    PeerIdentity {
        spiffe_id: "spiffe://distriwaf/node-b".into(),
        node_id: "node-b".into(),
        roles: vec!["waf".into()],
    }
}

pub struct RunningServer {
    pub addr: String,
    pub server: Arc<MessagingServer>,
    tls_cert: String,
    tls_key: String,
    handle: JoinHandle<()>,
}

impl RunningServer {
    pub fn url(&self) -> String {
        self.addr.clone()
    }

    pub async fn stop(mut self) {
        self.handle.abort();
        let _ = (&mut self.handle).await;
        // Dropping `server` releases the runtime; give the worker thread a
        // moment to release the UDP socket before a restart rebinds it.
        drop(self.server);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    /// Restart the listener on the same address with the same store.
    pub async fn restart(self) -> RunningServer {
        let addr = self.addr.clone();
        let tls_cert = self.tls_cert.clone();
        let tls_key = self.tls_key.clone();
        let store = self.server.policy_store().clone();
        let metrics = self.server.metrics();
        let backpressure = *self.server.backpressure();
        let handle = self.handle;
        let server = self.server;
        handle.abort();
        let _ = handle.await;
        drop(server);
        tokio::time::sleep(Duration::from_millis(100)).await;

        let socket: SocketAddr = addr.parse().expect("socket addr");
        let mut attempts = 0;
        let server_handle = loop {
            match TquicServer::new(socket, tls_cert.clone(), tls_key.clone(), None).await {
                Ok(server) => break server,
                Err(err) => {
                    attempts += 1;
                    if attempts >= 20 {
                        panic!("restart bind failed after {attempts} attempts: {err}");
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        };
        let listener = server_handle.listen(&addr).await.expect("restart listener");
        let server = Arc::new(
            MessagingServer::with_policy_store(
                Arc::new(
                    quiccrunch_core::messaging::server::InMemoryService::with_store(store.clone()),
                ),
                store,
                metrics,
            )
            .with_backpressure(backpressure)
            .with_authorization(AuthorizationPolicy {
                require_identity: true,
                ..Default::default()
            }),
        );
        let serving = server.clone();
        let handle = tokio::spawn(async move {
            let _ = serve_listener(serving, listener, Some(identity())).await;
        });
        RunningServer {
            addr,
            server,
            tls_cert,
            tls_key,
            handle,
        }
    }
}

pub async fn spawn_server(
    tls: &TestTls,
    store: Arc<PolicyStore>,
    metrics: Arc<MessagingMetrics>,
    backpressure: BackpressureConfig,
    peer: PeerIdentity,
) -> RunningServer {
    let addr = format!("127.0.0.1:{}", free_port());
    let socket: SocketAddr = addr.parse().expect("socket addr");
    let server = TquicServer::new(socket, tls.cert_file.clone(), tls.key_file.clone(), None)
        .await
        .expect("server");
    let listener = server.listen(&addr).await.expect("listener");
    let service =
        Arc::new(quiccrunch_core::messaging::server::InMemoryService::with_store(store.clone()));
    let messaging = Arc::new(
        MessagingServer::with_policy_store(service, store, metrics)
            .with_backpressure(backpressure)
            .with_authorization(AuthorizationPolicy {
                require_identity: true,
                ..Default::default()
            }),
    );
    let serving = messaging.clone();
    let handle = tokio::spawn(async move {
        let _ = serve_listener(serving, listener, Some(peer)).await;
    });
    RunningServer {
        addr,
        server: messaging,
        tls_cert: tls.cert_file.clone(),
        tls_key: tls.key_file.clone(),
        handle,
    }
}

pub async fn pool(tls: &TestTls, config: PoolConfig) -> Arc<MessagingPool> {
    let client = TquicClient::with_options(tls.client_options())
        .await
        .expect("tquic client");
    let connector = Arc::new(TquicConnector::new(client).await);
    Arc::new(MessagingPool::new(connector, config))
}

pub fn retention(max_entries: usize, max_bytes: usize) -> RetentionConfig {
    RetentionConfig {
        max_entries,
        max_bytes,
    }
}
