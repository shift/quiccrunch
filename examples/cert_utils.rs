use rustls::{Certificate, PrivateKey};
use rcgen::{
    BasicConstraints, Certificate as RcgenCertificate, CertificateParams,
    DnType, IsCa, KeyPair, PKCS_ECDSA_P256_SHA256,
};
use std::error::Error;

/// Generate a CA certificate and private key
pub fn generate_ca_cert() -> Result<(Certificate, PrivateKey, RcgenCertificate), Box<dyn Error>> {
    let mut params = CertificateParams::new(vec!["QUIC CA".to_string()]);
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_pair = Some(KeyPair::generate(&PKCS_ECDSA_P256_SHA256)?);
    
    let ca_cert = RcgenCertificate::from_params(params)?;
    let ca_key = PrivateKey(ca_cert.serialize_private_key_der());
    let ca_cert_der = Certificate(ca_cert.serialize_der()?);
    
    Ok((ca_cert_der, ca_key, ca_cert))
}

/// Generate a node certificate signed by the CA
pub fn generate_node_cert(
    ca_cert: &RcgenCertificate,
    node_name: &str,
) -> Result<(Certificate, PrivateKey), Box<dyn Error>> {
    let mut params = CertificateParams::new(vec![node_name.to_string()]);
    params.is_ca = IsCa::NoCa;
    params.distinguished_name.push(DnType::CommonName, node_name);
    
    // Add SAN (Subject Alternative Name) for IP addresses
    if node_name.parse::<std::net::IpAddr>().is_ok() {
        params.subject_alt_names = vec![
            rcgen::SanType::IpAddress(node_name.parse().unwrap()),
        ];
    } else {
        params.subject_alt_names = vec![
            rcgen::SanType::DnsName(node_name.to_string()),
        ];
    }
    
    params.key_pair = Some(KeyPair::generate(&PKCS_ECDSA_P256_SHA256)?);
    
    let cert = RcgenCertificate::from_params(params)?;
    
    // Sign the certificate with the CA
    let signed_cert_der = cert.serialize_der_with_signer(ca_cert)?;
    
    let key = PrivateKey(cert.serialize_private_key_der());
    let cert = Certificate(signed_cert_der);
    
    Ok((cert, key))
}

#[cfg(not(test))]
fn main() {
    // Example usage
    let (ca_cert, _ca_key, ca_cert_obj) = generate_ca_cert().unwrap();
    let (cert, _key) = generate_node_cert(&ca_cert_obj, "example-node").unwrap();
    
    println!("Generated certificates successfully!");
    println!("CA cert length: {} bytes", ca_cert.0.len());
    println!("Node cert length: {} bytes", cert.0.len());
}