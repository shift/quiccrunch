#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ca_cert() {
        let result = generate_ca_cert();
        assert!(result.is_ok());
        
        let (cert, key) = result.unwrap();
        assert!(!cert.0.is_empty());
        assert!(!key.0.is_empty());
    }

    #[test]
    fn test_generate_node_cert() {
        let (ca_cert, _) = generate_ca_cert().unwrap();
        let result = generate_node_cert(&ca_cert, "test-node");
        
        assert!(result.is_ok());
        
        let (cert, key) = result.unwrap();
        assert!(!cert.0.is_empty());
        assert!(!key.0.is_empty());
    }

    #[test]
    fn test_generate_multiple_node_certs() {
        let (ca_cert, _) = generate_ca_cert().unwrap();
        
        for i in 0..5 {
            let node_id = format!("node-{}", i);
            let result = generate_node_cert(&ca_cert, &node_id);
            assert!(result.is_ok());
        }
    }
}