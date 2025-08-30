use rcgen::{Certificate, CertificateParams, date_time_ymd, DistinguishedName, DnType, PKCS_ECDSA_P256_SHA256};
use quiccrunch_core::TlsConfig;


pub fn generate_ca_cert() -> rustls::Certificate {
    let mut params = CertificateParams::new(vec!["CA".to_string()]);
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, "Test CA");
    params.distinguished_name.push(DnType::OrganizationName, "Test Org");
    params.alg = &PKCS_ECDSA_P256_SHA256;
    params.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign, rcgen::KeyUsagePurpose::DigitalSignature];

    let cert = Certificate::from_params(params).expect("Failed to generate CA certificate");
    rustls::Certificate(cert.serialize_der().expect("Failed to serialize CA certificate"))
}

pub fn generate_node_cert(node_name: &str) -> TlsConfig {
    // Generate CA first
    let mut ca_params = CertificateParams::new(vec!["Test CA".to_string()]);
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params.distinguished_name = DistinguishedName::new();
    ca_params.distinguished_name.push(DnType::CommonName, "Test CA");
    ca_params.distinguished_name.push(DnType::OrganizationName, "Test Org");
    ca_params.alg = &PKCS_ECDSA_P256_SHA256;
    ca_params.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign, rcgen::KeyUsagePurpose::DigitalSignature];
    ca_params.not_before = date_time_ymd(2020, 1, 1);
    ca_params.not_after = date_time_ymd(2050, 1, 1);

    let ca_cert = Certificate::from_params(ca_params).expect("Failed to generate CA certificate");

    // Generate node cert signed by CA
    let mut params = CertificateParams::new(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
    ]);
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, node_name);
    params.distinguished_name.push(DnType::OrganizationName, "Test Org");
    params.alg = &PKCS_ECDSA_P256_SHA256;
    params.not_before = date_time_ymd(2020, 1, 1);
    params.not_after = date_time_ymd(2050, 1, 1);
    params.is_ca = rcgen::IsCa::NoCa;

    // Sign node cert with CA
    let node_cert = Certificate::from_params(params).expect("Failed to generate node certificate");
    let node_cert_signed = node_cert.serialize_der_with_signer(&ca_cert).expect("Failed to sign node certificate");
    let node_key = node_cert.serialize_private_key_der();

    // Convert to rustls types
    let node_cert = rustls::Certificate(node_cert_signed);
    let node_key = rustls::PrivateKey(node_key);
    let ca_cert_der = ca_cert.serialize_der().expect("Failed to serialize CA certificate");
    let ca_cert = rustls::Certificate(ca_cert_der);

    // Generate PEM data
    let cert_pem = node_cert.serialize_pem().expect("Failed to serialize cert to PEM");
    let key_pem = node_cert.serialize_private_key_pem();
    let ca_cert_pem = ca_cert.serialize_pem().expect("Failed to serialize CA cert to PEM");

    TlsConfig {
        cert: node_cert,
        key: node_key,
        ca_cert,
        cert_pem: cert_pem.into_bytes(),
        key_pem: key_pem.into_bytes(),
        ca_cert_pem: ca_cert_pem.into_bytes(),
    }
}