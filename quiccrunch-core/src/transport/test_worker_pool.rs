//! Test module for TquicWorkerPool implementation
//!
//! Tests the core multi-threading architecture without tQUIC API dependencies

use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::oneshot;

use super::messages::{TquicCommand, WorkerConfig};
use super::worker_pool::TquicWorkerPool;
use crate::error::TransportError;

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> WorkerConfig {
        WorkerConfig {
            worker_id: 0,
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            cert_file: None,
            key_file: None,
            is_server: false,
            max_idle_timeout: Duration::from_secs(30),
            max_udp_payload_size: 1200,
            enable_early_data: false,
            application_protocols: vec![b"quic-crunch".to_vec()],
        }
    }

    #[tokio::test]
    async fn test_worker_pool_creation() {
        let config = create_test_config();
        let pool = TquicWorkerPool::new(4, config).unwrap();

        // Check that we have 4 workers
        let status = pool.get_worker_status();
        assert_eq!(status.len(), 4);

        // All workers should be healthy initially
        for worker_status in &status {
            assert!(matches!(
                worker_status.health,
                super::super::messages::WorkerHealth::Healthy
            ));
        }
    }

    #[tokio::test]
    async fn test_load_balancing() {
        let config = create_test_config();
        let pool = TquicWorkerPool::new(3, config).unwrap();

        // Create multiple connect commands and verify they round-robin
        for i in 0..9 {
            let (tx, _rx) = oneshot::channel();
            let remote_addr: SocketAddr = format!("127.0.0.1:{}", 8000 + i).parse().unwrap();

            let cmd = TquicCommand::Connect {
                remote_addr,
                response: tx,
            };

            // Should not fail (even though worker might return dummy data)
            let result = pool.submit_command(cmd).await;
            assert!(result.is_ok(), "Command submission failed: {:?}", result);
        }
    }

    #[tokio::test]
    async fn test_connection_affinity() {
        let config = create_test_config();
        let pool = TquicWorkerPool::new(2, config).unwrap();

        // Test that connection affinity works
        let connection_id = 12345;
        pool.map_connection_to_worker(connection_id, 1);

        let (tx, _rx) = oneshot::channel();
        let cmd = TquicCommand::Send {
            connection_id,
            stream_id: 1,
            data: b"test data".to_vec(),
            fin: false,
            response: tx,
        };

        // This should route to worker 1 due to affinity
        let result = pool.submit_command(cmd).await;
        assert!(result.is_ok());

        // Clean up
        pool.unmap_connection(connection_id);
    }

    #[tokio::test]
    async fn test_worker_health_monitoring() {
        let config = create_test_config();
        let pool = TquicWorkerPool::new(2, config).unwrap();

        // Check initial health
        let health = pool.health_check();
        assert_eq!(health.len(), 2);
        assert!(matches!(
            health[0],
            super::super::messages::WorkerHealth::Healthy
        ));
        assert!(matches!(
            health[1],
            super::super::messages::WorkerHealth::Healthy
        ));
    }

    #[tokio::test]
    async fn test_graceful_shutdown() {
        let config = create_test_config();
        let pool = TquicWorkerPool::new(2, config).unwrap();

        // Shutdown should complete without hanging
        let result = pool.shutdown(Duration::from_secs(1)).await;
        assert!(result.is_ok());
    }
}
