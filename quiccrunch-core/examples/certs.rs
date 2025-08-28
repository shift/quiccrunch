use quiccrunch_core::transport::TlsConfig;
use rcgen::{CertificateParams, BasicConstraints, Certificate, IsCa, KeyPair, PKCS_ECDSA_P256_SHA256, SanType};
use std::net::{Ipv4Addr, IpAddr};

pub fn generate_node_cert(node_name: &str) -> TlsConfig {
    // Generate a CA certificate
    let mut ca_params = CertificateParams::new(vec![node_name.to_string()]);
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.alg = &PKCS_ECDSA_P256_SHA256;
    let ca_cert = Certificate::from_params(ca_params).unwrap();

    // Generate a node certificate signed by the CA
    let mut node_params = CertificateParams::new(vec![node_name.to_string()]);
    node_params.alg = &PKCS_ECDSA_P256_SHA256;

    // Add SANs for localhost and 127.0.0.1 to satisfy SNI validation
    node_params.subject_alt_names.push(SanType::DnsName("localhost".to_string()));
    node_params.subject_alt_names.push(SanType::IpAddress(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));

    let node_key = KeyPair::generate(&PKCS_ECDSA_P256_SHA256).unwrap();
    node_params.key_pair = Some(node_key);
    let node_cert = Certificate::from_params(node_params).unwrap();

    // Serialize certificates and keys
    let ca_der = ca_cert.serialize_der().unwrap();
    let node_der = node_cert.serialize_der_with_signer(&ca_cert).unwrap();
    let node_key_der = node_cert.get_key_pair().serialize_der();

    TlsConfig {
        cert: rustls::Certificate(node_der),
        key: rustls::PrivateKey(node_key_der),
        ca_cert: rustls::Certificate(ca_der),
    }
}

fn main() {}
