use rcgen::{Certificate, CertificateParams, DistinguishedName, DnType, PKCS_ECDSA_P256_SHA256};
use crate::transport::tquic::TlsConfig;
use rustls;

pub fn generate_ca_cert() -> rustls::Certificate {
    let mut params = CertificateParams::new(vec!["CA".to_string()]);
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, "Test CA");
    params.alg = &PKCS_ECDSA_P256_SHA256;

    let cert = Certificate::from_params(params).expect("Failed to generate CA certificate");
    rustls::Certificate(cert.serialize_der().expect("Failed to serialize CA certificate"))
}

pub fn generate_node_cert(_node_name: &str) -> TlsConfig {
    // For tQUIC v1.6.0, TlsConfig API has changed.
    // Return a default config for now - proper TLS setup needs to be implemented
    // based on the new TlsConfig API
    TlsConfig::default()
}

/// Generate a self-signed certificate for testing (alias for generate_node_cert)
pub fn generate_self_signed_cert(name: &str) -> Result<TlsConfig, Box<dyn std::error::Error>> {
    Ok(generate_node_cert(name))
}