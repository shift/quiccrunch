use rcgen::{Certificate, CertificateParams, date_time_ymd, DistinguishedName, DnType, PKCS_ECDSA_P256_SHA256};
use quiccrunch_core::TlsConfig;


pub fn generate_ca_cert() -> (rustls::Certificate, rustls::PrivateKey, Certificate) {
    let mut params = CertificateParams::new(vec!["CA".to_string()]);
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, "Test CA");
    params.distinguished_name.push(DnType::OrganizationName, "Test Org");
    params.alg = &PKCS_ECDSA_P256_SHA256;
    params.key_usages = vec![rcgen::KeyUsagePurpose::KeyCertSign, rcgen::KeyUsagePurpose::DigitalSignature];

    let cert = Certificate::from_params(params).expect("Failed to generate CA certificate");
    let cert_der = cert.serialize_der().expect("Failed to serialize CA certificate");
    let key_der = cert.serialize_private_key_der();
    (rustls::Certificate(cert_der), rustls::PrivateKey(key_der), cert)
}

pub fn generate_node_cert(ca_cert_obj: &Certificate, node_name: &str) -> (rustls::Certificate, rustls::PrivateKey) {
    // Generate node cert signed by CA
    let mut params = CertificateParams::new(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        node_name.to_string(),
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
    let node_cert_signed = node_cert.serialize_der_with_signer(ca_cert_obj).expect("Failed to sign node certificate");
    let node_key = node_cert.serialize_private_key_der();

    // Convert to rustls types
    (rustls::Certificate(node_cert_signed), rustls::PrivateKey(node_key))
}

pub fn generate_tls_config(node_name: &str) -> TlsConfig {
    let (ca_cert, ca_key, ca_cert_obj) = generate_ca_cert();
    let (cert, key) = generate_node_cert(&ca_cert_obj, node_name);
    
    TlsConfig {
        cert,
        key,
        ca_cert,
        cert_pem: Vec::new(), // Not used in this test setup
        key_pem: Vec::new(),
        ca_cert_pem: Vec::new(),
    }
}