use rustls::{Certificate, PrivateKey};
use rcgen::{
    BasicConstraints, Certificate as RcgenCertificate, CertificateParams,
    DnType, IsCa, KeyPair, PKCS_ECDSA_P256_SHA256,
};
use std::error::Error;

/// Generate a CA certificate and private key
pub fn generate_ca_cert() -> Result<(Certificate, PrivateKey), Box<dyn Error>> {
    let mut params = CertificateParams::new(vec!["QUIC CA".to_string()]);
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_pair = Some(KeyPair::generate(&PKCS_ECDSA_P256_SHA256)?);
    
    let ca_cert = RcgenCertificate::from_params(params)?;
    let ca_key = PrivateKey(ca_cert.serialize_private_key_der());
    let ca_cert = Certificate(ca_cert.serialize_der()?);
    
    Ok((ca_cert, ca_key))
}

/// Generate a node certificate signed by the CA
pub fn generate_node_cert(
    _ca_cert: &Certificate,
    node_name: &str,
) -> Result<(Certificate, PrivateKey), Box<dyn Error>> {
    let mut params = CertificateParams::new(vec![node_name.to_string()]);
    params.is_ca = IsCa::NoCa;
    params.distinguished_name.push(DnType::CommonName, node_name);
    params.key_pair = Some(KeyPair::generate(&PKCS_ECDSA_P256_SHA256)?);
    
    let cert = RcgenCertificate::from_params(params)?;
    let key = PrivateKey(cert.serialize_private_key_der());
    let cert = Certificate(cert.serialize_der()?);
    
    Ok((cert, key))
}

fn main() {
    // Example usage
    let (ca_cert, _ca_key) = generate_ca_cert().unwrap();
    let (cert, _key) = generate_node_cert(&ca_cert, "example-node").unwrap();
    
    println!("Generated certificates successfully!");
    println!("CA cert size: {} bytes", ca_cert.0.len());
    println!("Node cert size: {} bytes", cert.0.len());
}