use rcgen::{Certificate, CertificateParams, DistinguishedName, DnType, PKCS_ECDSA_P256_SHA256};
use crate::transport::TlsConfig;
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

pub fn generate_node_cert(node_name: &str) -> TlsConfig {
    // First generate the CA
    let ca_params = CertificateParams::new(vec!["CA".to_string()]);
    let mut ca_params = ca_params;
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.distinguished_name = DistinguishedName::new();
    ca_params.distinguished_name.push(DnType::CommonName, "Test CA");
    ca_params.alg = &PKCS_ECDSA_P256_SHA256;

    let ca_cert = Certificate::from_params(ca_params).expect("Failed to generate CA certificate");
    let ca_cert_der = ca_cert.serialize_der().expect("Failed to serialize CA certificate");
    
    // Now create and sign the node certificate
    let mut params = CertificateParams::new(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "*.localhost".to_string(), // Allow subdomains
    ]);
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, node_name);
    params.alg = &PKCS_ECDSA_P256_SHA256;
    
    // Add required certificate usage
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::DigitalSignature,
        rcgen::KeyUsagePurpose::KeyEncipherment,
    ];
    // Allow server auth
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    
    // Sign the certificate with our CA
    let cert = Certificate::from_params(params).expect("Failed to generate node certificate");
    let signed_cert = cert.serialize_der_with_signer(&ca_cert).expect("Failed to sign certificate");
    
    TlsConfig {
        cert: rustls::Certificate(signed_cert),
        key: rustls::PrivateKey(cert.serialize_private_key_der()),
        ca_cert: rustls::Certificate(ca_cert_der),
    }
}

/// Generate a self-signed certificate for testing (alias for generate_node_cert)
pub fn generate_self_signed_cert(name: &str) -> Result<TlsConfig, Box<dyn std::error::Error>> {
    Ok(generate_node_cert(name))
}