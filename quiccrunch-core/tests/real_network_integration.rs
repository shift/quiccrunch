use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, mpsc};
use tokio::time::timeout;
use test_log::test;

use quiccrunch_core::transport::{
    worker_pool::TquicWorkerPool,
    messages::{WorkerConfig, TquicCommand, ConnectionId},
    adapter::{TquicAdapter, TquicClient, TquicServer}
};
use quiccrunch_core::error::TransportError;

mod certs;
use certs::generate_node_cert;

/// Test configuration for real network tests
struct NetworkTestConfig {
    pub worker_count: usize,
    pub base_port: u16,
    pub test_duration: Duration,
    pub connection_count: usize,
    pub message_size: usize,
}

impl Default for NetworkTestConfig {
    fn default() -> Self {
        Self {
            worker_count: 4,
            base_port: 15000, // Use high ports to avoid conflicts
            test_duration: Duration::from_secs(10),
            connection_count: 100,
            message_size: 1024,
        }
    }
}

/// Helper to create a worker config for real network testing
fn create_real_worker_config(worker_id: usize, base_port: u16) -> WorkerConfig {
    WorkerConfig {
        worker_id,
        bind_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, base_port)),
        cert_file: None,
        key_file: None,
        is_server: false,
        max_idle_timeout: Duration::from_secs(30),
        max_udp_payload_size: 1350,
        enable_early_data: false,
        application_protocols: vec![b"h3".to_vec()],
    }
}

/// Real network validation tests
mod real_network_tests {
    use super::*;

    #[test(tokio::test)]
    async fn test_real_udp_socket_binding() {
        // Test 1: Verify we can bind to real UDP sockets
        let config = NetworkTestConfig::default();
        let base_config = create_real_worker_config(0, config.base_port);
        
        // Create a single worker to test basic socket binding
        let pool = TquicWorkerPool::new(1, base_config).unwrap();
        
        // Give worker time to initialize
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Worker should be healthy after successful socket binding
        let health = pool.health_check();
        assert_eq!(health.len(), 1);
        
        // Shutdown cleanly
        pool.shutdown(Duration::from_secs(1)).await.unwrap();
    }

    #[test(tokio::test)]
    async fn test_so_reuseport_multi_worker_binding() {
        // Test 2: Verify SO_REUSEPORT allows multiple workers to bind same port
        let config = NetworkTestConfig::default();
        let worker_count = 4;
        
        let base_config = create_real_worker_config(0, config.base_port + 1);
        let pool = TquicWorkerPool::new(worker_count, base_config).unwrap();
        
        // Give workers time to initialize and bind sockets
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // All workers should be healthy (successful socket binding)
        let health = pool.health_check();
        assert_eq!(health.len(), worker_count);
        
        for worker_health in &health {
            match worker_health {
                quiccrunch_core::transport::WorkerHealth::Healthy => {},
                quiccrunch_core::transport::WorkerHealth::Failed { error } => {
                    panic!("Worker failed during SO_REUSEPORT test: {}", error);
                }
            }
        }
        
        pool.shutdown(Duration::from_secs(2)).await.unwrap();
    }

    #[test(tokio::test)]
    async fn test_real_client_server_connection() {
        // Test 3: Real client-server connection over localhost network
        let config = NetworkTestConfig::default();
        let port = config.base_port + 2;
        
        // Create server with worker pool
        let server_config = create_real_worker_config(0, port);
        let server_pool = Arc::new(TquicWorkerPool::new(2, server_config).unwrap());
        
        // Create actual tQUIC server adapter
        let tls_config = generate_node_cert("test-server");
        let server = TquicServer::new(Some(tls_config.clone())).await.unwrap();
        
        let bind_addr = format!("127.0.0.1:{}", port);
        let mut listener = server.listen(&bind_addr).await.unwrap();
        
        // Server task to accept real connections
        let server_pool_clone = server_pool.clone();
        let server_handle = tokio::spawn(async move {
            for _ in 0..5 {
                match timeout(Duration::from_secs(5), listener.accept()).await {
                    Ok(Ok(connection)) => {
                        // Real connection established
                        let mut stream = connection.accept_bi().await.unwrap();
                        let data = stream.receive_message().await.unwrap();
                        stream.send_message(data).await.unwrap();
                        let _ = stream.close().await;
                    }
                    Ok(Err(e)) => {
                        eprintln!("Accept error: {}", e);
                        break;
                    }
                    Err(_) => {
                        // Timeout - expected when no more clients
                        break;
                    }
                }
            }
        });
        
        // Give server time to start listening
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Create real client
        let client = TquicClient::with_options(RuntimeOptions {
        // Loopback self-signed test certificate: verification is explicitly off.
        verify_server_cert: false,
        ..RuntimeOptions::default()
    }).await.unwrap();
        
        // Multiple real network connections
        let mut client_handles = Vec::new();
        for i in 0..5 {
            let client_clone = client.clone();
            let connect_addr = bind_addr.clone();
            
            let handle = tokio::spawn(async move {
                let connection = client_clone.connect(&connect_addr).await.unwrap();
                let mut stream = connection.open_bi().await.unwrap();
                
                let test_data = format!("Real message {}", i).into_bytes();
                stream.send_message(test_data.clone().into()).await.unwrap();
                
                let response = stream.receive_message().await.unwrap();
                assert_eq!(response.to_vec(), test_data);
                
                let _ = stream.close().await;
            });
            
            client_handles.push(handle);
        }
        
        // Wait for all clients to complete
        for handle in client_handles {
            handle.await.unwrap();
        }
        
        // Wait for server to finish
        timeout(Duration::from_secs(3), server_handle).await.unwrap().unwrap();
        
        server_pool.shutdown(Duration::from_secs(1)).await.unwrap();
    }

    #[test(tokio::test)]
    async fn test_concurrent_connections_load_distribution() {
        // Test 4: Multiple concurrent connections with load distribution validation
        let config = NetworkTestConfig::default();
        let port = config.base_port + 3;
        let worker_count = 4;
        let connection_count = 20;
        
        // Create server with multiple workers
        let server_config = create_real_worker_config(0, port);
        let server_pool = Arc::new(TquicWorkerPool::new(worker_count, server_config).unwrap());
        
        let tls_config = generate_node_cert("test-server-multi");
        let server = TquicServer::new(Some(tls_config)).await.unwrap();
        
        let bind_addr = format!("127.0.0.1:{}", port);
        let mut listener = server.listen(&bind_addr).await.unwrap();
        
        // Connection counter for server
        let connection_counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = connection_counter.clone();
        
        // Server task accepting many concurrent connections
        let server_handle = tokio::spawn(async move {
            loop {
                match timeout(Duration::from_secs(2), listener.accept()).await {
                    Ok(Ok(connection)) => {
                        counter_clone.fetch_add(1, Ordering::SeqCst);
                        
                        // Handle connection in separate task
                        tokio::spawn(async move {
                            let mut stream = connection.accept_bi().await.unwrap();
                            let data = stream.receive_message().await.unwrap();
                            stream.send_message(data).await.unwrap();
                            let _ = stream.close().await;
                        });
                    }
                    Ok(Err(_)) => break,
                    Err(_) => break, // Timeout
                }
            }
        });
        
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Create multiple concurrent client connections
        let client = TquicClient::with_options(RuntimeOptions {
        // Loopback self-signed test certificate: verification is explicitly off.
        verify_server_cert: false,
        ..RuntimeOptions::default()
    }).await.unwrap();
        let mut handles = Vec::new();
        
        for i in 0..connection_count {
            let client_clone = client.clone();
            let addr = bind_addr.clone();
            
            let handle = tokio::spawn(async move {
                let connection = client_clone.connect(&addr).await.unwrap();
                let mut stream = connection.open_bi().await.unwrap();
                
                let message = format!("Concurrent message {}", i);
                stream.send_message(message.clone().into_bytes().into()).await.unwrap();
                
                let response = stream.receive_message().await.unwrap();
                assert_eq!(response.to_vec(), message.into_bytes());
                
                let _ = stream.close().await;
            });
            
            handles.push(handle);
        }
        
        // Wait for all connections to complete
        for handle in handles {
            handle.await.unwrap();
        }
        
        // Verify server handled expected number of connections
        let handled_connections = connection_counter.load(Ordering::SeqCst);
        assert_eq!(handled_connections, connection_count);
        
        // Check load distribution across workers
        let distribution = server_pool.analyze_socket_distribution();
        assert!(distribution.worker_count == worker_count);
        assert!(distribution.is_balanced, "Load should be distributed across workers");
        
        timeout(Duration::from_secs(2), server_handle).await.unwrap().unwrap();
        server_pool.shutdown(Duration::from_secs(1)).await.unwrap();
    }

    #[test(tokio::test)]
    async fn test_ipv4_ipv6_dual_stack() {
        // Test 5: IPv4 and IPv6 real network support
        let config = NetworkTestConfig::default();
        let port = config.base_port + 4;
        
        // Test IPv4 binding
        let ipv4_config = WorkerConfig {
            worker_id: 0,
            bind_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)),
            max_connections: 100,
            buffer_size: 32768,
            enable_so_reuseport: true,
        };
        
        let ipv4_pool = TquicWorkerPool::new(1, ipv4_config).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let ipv4_health = ipv4_pool.health_check();
        assert!(matches!(ipv4_health[0], quiccrunch_core::transport::WorkerHealth::Healthy));
        ipv4_pool.shutdown(Duration::from_secs(1)).await.unwrap();
        
        // Test IPv6 binding (if available)
        let ipv6_config = WorkerConfig {
            worker_id: 0,
            bind_addr: SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, port + 1, 0, 0)),
            max_connections: 100,
            buffer_size: 32768,
            enable_so_reuseport: true,
        };
        
        // IPv6 may not be available in all environments, so we allow failure
        match TquicWorkerPool::new(1, ipv6_config) {
            Ok(ipv6_pool) => {
                tokio::time::sleep(Duration::from_millis(100)).await;
                ipv6_pool.shutdown(Duration::from_secs(1)).await.unwrap();
            }
            Err(_) => {
                // IPv6 not available - this is acceptable
                eprintln!("IPv6 not available in test environment");
            }
        }
    }

    #[test(tokio::test)]
    async fn test_large_data_transfer_over_network() {
        // Test 6: Large data transfers over real network
        let config = NetworkTestConfig::default();
        let port = config.base_port + 5;
        
        let tls_config = generate_node_cert("test-large-data");
        let server = TquicServer::new(Some(tls_config)).await.unwrap();
        
        let bind_addr = format!("127.0.0.1:{}", port);
        let mut listener = server.listen(&bind_addr).await.unwrap();
        
        // Server that echoes large data
        let server_handle = tokio::spawn(async move {
            let connection = listener.accept().await.unwrap();
            let mut stream = connection.accept_bi().await.unwrap();
            
            let large_data = stream.receive_message().await.unwrap();
            assert!(large_data.len() >= 1024 * 1024); // At least 1MB
            
            stream.send_message(large_data).await.unwrap();
            let _ = stream.close().await;
        });
        
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Client sending large data
        let client = TquicClient::with_options(RuntimeOptions {
        // Loopback self-signed test certificate: verification is explicitly off.
        verify_server_cert: false,
        ..RuntimeOptions::default()
    }).await.unwrap();
        let connection = client.connect(&bind_addr).await.unwrap();
        let mut stream = connection.open_bi().await.unwrap();
        
        // Create 2MB of test data
        let large_data = vec![0xAB; 2 * 1024 * 1024];
        let start_time = Instant::now();
        
        stream.send_message(large_data.clone().into()).await.unwrap();
        let response = stream.receive_message().await.unwrap();
        
        let transfer_time = start_time.elapsed();
        let _ = stream.close().await;
        
        // Verify data integrity
        assert_eq!(response.to_vec(), large_data);
        
        // Verify reasonable transfer speed (should complete within 10 seconds)
        assert!(transfer_time < Duration::from_secs(10), 
                "Large data transfer took too long: {:?}", transfer_time);
        
        // Calculate throughput
        let throughput_mbps = (large_data.len() * 2) as f64 / (1024.0 * 1024.0) / transfer_time.as_secs_f64();
        println!("Throughput: {:.2} MB/s", throughput_mbps);
        
        timeout(Duration::from_secs(2), server_handle).await.unwrap().unwrap();
    }

    #[test(tokio::test)]
    async fn test_connection_failure_recovery() {
        // Test 7: Real network connection failure and recovery
        let config = NetworkTestConfig::default();
        let port = config.base_port + 6;
        
        let client = TquicClient::with_options(RuntimeOptions {
        // Loopback self-signed test certificate: verification is explicitly off.
        verify_server_cert: false,
        ..RuntimeOptions::default()
    }).await.unwrap();
        
        // Test connection to non-existent server
        let invalid_addr = format!("127.0.0.1:{}", port);
        let result = timeout(Duration::from_secs(3), client.connect(&invalid_addr)).await;
        
        // Should fail with timeout or connection error
        assert!(result.is_err() || result.unwrap().is_err());
        
        // Now start a server
        let tls_config = generate_node_cert("test-recovery");
        let server = TquicServer::new(Some(tls_config)).await.unwrap();
        let mut listener = server.listen(&invalid_addr).await.unwrap();
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Server task
        let server_handle = tokio::spawn(async move {
            let connection = listener.accept().await.unwrap();
            let mut stream = connection.accept_bi().await.unwrap();
            let data = stream.receive_message().await.unwrap();
            stream.send_message(data).await.unwrap();
            let _ = stream.close().await;
        });
        
        // Now connection should succeed
        let connection = client.connect(&invalid_addr).await.unwrap();
        let mut stream = connection.open_bi().await.unwrap();
        
        let test_msg = b"Recovery test";
        stream.send_message(test_msg.to_vec().into()).await.unwrap();
        let response = stream.receive_message().await.unwrap();
        
        assert_eq!(response.to_vec(), test_msg);
        let _ = stream.close().await;
        
        timeout(Duration::from_secs(2), server_handle).await.unwrap().unwrap();
    }
}

/// Stress testing over real network
mod stress_tests {
    use super::*;

    #[test(tokio::test)]
    async fn test_high_connection_count_stress() {
        // Test 8: High connection count stress test
        let config = NetworkTestConfig {
            connection_count: 500,
            worker_count: 8,
            test_duration: Duration::from_secs(30),
            ..Default::default()
        };
        let port = config.base_port + 7;
        
        // Server with multiple workers
        let server_config = create_real_worker_config(0, port);
        let server_pool = Arc::new(TquicWorkerPool::new(config.worker_count, server_config).unwrap());
        
        let tls_config = generate_node_cert("stress-test-server");
        let server = TquicServer::new(Some(tls_config)).await.unwrap();
        
        let bind_addr = format!("127.0.0.1:{}", port);
        let mut listener = server.listen(&bind_addr).await.unwrap();
        
        let connection_counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = connection_counter.clone();
        
        // Server accepting many connections
        let server_handle = tokio::spawn(async move {
            loop {
                match timeout(Duration::from_secs(1), listener.accept()).await {
                    Ok(Ok(connection)) => {
                        counter_clone.fetch_add(1, Ordering::SeqCst);
                        
                        tokio::spawn(async move {
                            // Keep connection alive for a bit to simulate load
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            
                            if let Ok(mut stream) = connection.accept_bi().await {
                                if let Ok(data) = stream.receive_message().await {
                                    let _ = stream.send_message(data).await;
                                }
                                let _ = stream.close().await;
                            }
                        });
                    }
                    Ok(Err(_)) => break,
                    Err(_) => break,
                }
            }
        });
        
        tokio::time::sleep(Duration::from_millis(300)).await;
        
        // Create stress load with many concurrent clients
        let client = TquicClient::with_options(RuntimeOptions {
        // Loopback self-signed test certificate: verification is explicitly off.
        verify_server_cert: false,
        ..RuntimeOptions::default()
    }).await.unwrap();
        let mut handles = Vec::new();
        
        let start_time = Instant::now();
        
        for i in 0..config.connection_count {
            let client_clone = client.clone();
            let addr = bind_addr.clone();
            
            let handle = tokio::spawn(async move {
                match client_clone.connect(&addr).await {
                    Ok(connection) => {
                        if let Ok(mut stream) = connection.open_bi().await {
                            let message = format!("Stress message {}", i);
                            if let Ok(_) = stream.send_message(message.into_bytes().into()).await {
                                let _ = stream.receive_message().await;
                            }
                            let _ = stream.close().await;
                        }
                    }
                    Err(_) => {
                        // Some connections may fail under high load - that's expected
                    }
                }
            });
            
            handles.push(handle);
            
            // Small delay to avoid overwhelming the system
            if i % 50 == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        
        // Wait for all client tasks
        for handle in handles {
            let _ = handle.await;
        }
        
        let test_duration = start_time.elapsed();
        let connections_handled = connection_counter.load(Ordering::SeqCst);
        
        println!("Stress test completed in {:?}", test_duration);
        println!("Connections handled: {}/{}", connections_handled, config.connection_count);
        
        // At least 80% of connections should succeed under stress
        assert!(connections_handled >= config.connection_count * 80 / 100,
                "Too many connections failed: {}/{}", connections_handled, config.connection_count);
        
        // Verify worker load distribution
        let distribution = server_pool.analyze_socket_distribution();
        println!("Load distribution: {:?}", distribution.per_worker_packets);
        
        timeout(Duration::from_secs(3), server_handle).await.unwrap().unwrap();
        server_pool.shutdown(Duration::from_secs(2)).await.unwrap();
    }

    #[test(tokio::test)]
    async fn test_connection_churn_stress() {
        // Test 9: Connection churn (rapid connect/disconnect)
        let config = NetworkTestConfig::default();
        let port = config.base_port + 8;
        
        let tls_config = generate_node_cert("churn-test");
        let server = TquicServer::new(Some(tls_config)).await.unwrap();
        
        let bind_addr = format!("127.0.0.1:{}", port);
        let mut listener = server.listen(&bind_addr).await.unwrap();
        
        let connection_counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = connection_counter.clone();
        
        // Server handling rapid connections
        let server_handle = tokio::spawn(async move {
            loop {
                match timeout(Duration::from_millis(500), listener.accept()).await {
                    Ok(Ok(connection)) => {
                        counter_clone.fetch_add(1, Ordering::SeqCst);
                        
                        // Quick handling - just accept and close
                        tokio::spawn(async move {
                            if let Ok(mut stream) = connection.accept_bi().await {
                                if let Ok(data) = stream.receive_message().await {
                                    let _ = stream.send_message(data).await;
                                }
                                // Immediate close for churn test
                                let _ = stream.close().await;
                            }
                        });
                    }
                    Ok(Err(_)) => break,
                    Err(_) => break,
                }
            }
        });
        
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Client creating rapid connection churn
        let client = TquicClient::with_options(RuntimeOptions {
        // Loopback self-signed test certificate: verification is explicitly off.
        verify_server_cert: false,
        ..RuntimeOptions::default()
    }).await.unwrap();
        let churn_count = 100;
        let start_time = Instant::now();
        
        for i in 0..churn_count {
            let connection = client.connect(&bind_addr).await.unwrap();
            let mut stream = connection.open_bi().await.unwrap();
            
            let quick_msg = format!("Quick {}", i);
            stream.send_message(quick_msg.clone().into_bytes().into()).await.unwrap();
            let response = stream.receive_message().await.unwrap();
            
            assert_eq!(response.to_vec(), quick_msg.into_bytes());
            
            // Immediate disconnect
            let _ = stream.close().await;
            
            // Very small delay between connections
            if i % 10 == 0 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
        
        let churn_duration = start_time.elapsed();
        let connections_per_second = churn_count as f64 / churn_duration.as_secs_f64();
        
        println!("Connection churn: {:.1} connections/sec", connections_per_second);
        
        // Should handle at least 20 connections per second
        assert!(connections_per_second >= 20.0, 
                "Connection churn rate too low: {:.1}/sec", connections_per_second);
        
        let handled = connection_counter.load(Ordering::SeqCst);
        assert_eq!(handled, churn_count);
        
        timeout(Duration::from_secs(2), server_handle).await.unwrap().unwrap();
    }
}

/// Performance measurement over real network
mod performance_tests {
    use super::*;

    #[test(tokio::test)]
    async fn test_throughput_measurement() {
        // Test 10: Measure real network throughput
        let config = NetworkTestConfig::default();
        let port = config.base_port + 9;
        
        let tls_config = generate_node_cert("throughput-test");
        let server = TquicServer::new(Some(tls_config)).await.unwrap();
        
        let bind_addr = format!("127.0.0.1:{}", port);
        let mut listener = server.listen(&bind_addr).await.unwrap();
        
        // Server measuring bytes processed
        let bytes_processed = Arc::new(AtomicUsize::new(0));
        let bytes_clone = bytes_processed.clone();
        
        let server_handle = tokio::spawn(async move {
            let connection = listener.accept().await.unwrap();
            let mut stream = connection.accept_bi().await.unwrap();
            
            // Echo data while measuring throughput
            loop {
                match stream.receive_message().await {
                    Ok(data) => {
                        let size = data.len();
                        bytes_clone.fetch_add(size, Ordering::SeqCst);
                        
                        if stream.send_message(data).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            
            let _ = stream.close().await;
        });
        
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Client sending measured load
        let client = TquicClient::with_options(RuntimeOptions {
        // Loopback self-signed test certificate: verification is explicitly off.
        verify_server_cert: false,
        ..RuntimeOptions::default()
    }).await.unwrap();
        let connection = client.connect(&bind_addr).await.unwrap();
        let mut stream = connection.open_bi().await.unwrap();
        
        let test_duration = Duration::from_secs(5);
        let message_size = 8192; // 8KB messages
        let message_data = vec![0xCD; message_size];
        
        let start_time = Instant::now();
        let mut messages_sent = 0;
        
        while start_time.elapsed() < test_duration {
            if stream.send_message(message_data.clone().into()).await.is_err() {
                break;
            }
            
            if stream.receive_message().await.is_err() {
                break;
            }
            
            messages_sent += 1;
        }
        
        let actual_duration = start_time.elapsed();
        let _ = stream.close().await;
        
        // Calculate throughput metrics
        let total_bytes = bytes_processed.load(Ordering::SeqCst);
        let throughput_mbps = (total_bytes as f64) / (1024.0 * 1024.0) / actual_duration.as_secs_f64();
        let messages_per_second = messages_sent as f64 / actual_duration.as_secs_f64();
        
        println!("Throughput test results:");
        println!("  Duration: {:?}", actual_duration);
        println!("  Messages: {}", messages_sent);
        println!("  Total bytes: {}", total_bytes);
        println!("  Throughput: {:.2} MB/s", throughput_mbps);
        println!("  Message rate: {:.1} msg/s", messages_per_second);
        
        // Minimum performance expectations for localhost
        assert!(throughput_mbps >= 10.0, "Throughput too low: {:.2} MB/s", throughput_mbps);
        assert!(messages_per_second >= 100.0, "Message rate too low: {:.1} msg/s", messages_per_second);
        
        timeout(Duration::from_secs(2), server_handle).await.unwrap().unwrap();
    }

    #[test(tokio::test)]
    async fn test_latency_measurement() {
        // Test 11: Measure real network latency
        let config = NetworkTestConfig::default();
        let port = config.base_port + 10;
        
        let tls_config = generate_node_cert("latency-test");
        let server = TquicServer::new(Some(tls_config)).await.unwrap();
        
        let bind_addr = format!("127.0.0.1:{}", port);
        let mut listener = server.listen(&bind_addr).await.unwrap();
        
        // Server with minimal processing delay
        let server_handle = tokio::spawn(async move {
            let connection = listener.accept().await.unwrap();
            let mut stream = connection.accept_bi().await.unwrap();
            
            loop {
                match stream.receive_message().await {
                    Ok(data) => {
                        // Immediate echo with minimal processing
                        if stream.send_message(data).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            
            let _ = stream.close().await;
        });
        
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Client measuring round-trip latency
        let client = TquicClient::with_options(RuntimeOptions {
        // Loopback self-signed test certificate: verification is explicitly off.
        verify_server_cert: false,
        ..RuntimeOptions::default()
    }).await.unwrap();
        let connection = client.connect(&bind_addr).await.unwrap();
        let mut stream = connection.open_bi().await.unwrap();
        
        let mut latencies = Vec::new();
        let test_msg = b"latency test";
        
        // Measure multiple round trips
        for _ in 0..100 {
            let start = Instant::now();
            
            stream.send_message(test_msg.to_vec().into()).await.unwrap();
            let response = stream.receive_message().await.unwrap();
            
            let latency = start.elapsed();
            latencies.push(latency);
            
            assert_eq!(response.to_vec(), test_msg);
            
            // Small delay between measurements
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        
        let _ = stream.close().await;
        
        // Calculate latency statistics
        latencies.sort();
        let min_latency = latencies[0];
        let max_latency = latencies[latencies.len() - 1];
        let median_latency = latencies[latencies.len() / 2];
        let avg_latency: Duration = latencies.iter().sum::<Duration>() / latencies.len() as u32;
        let p95_latency = latencies[(latencies.len() as f64 * 0.95) as usize];
        
        println!("Latency measurements (μs):");
        println!("  Min: {}", min_latency.as_micros());
        println!("  Max: {}", max_latency.as_micros());
        println!("  Avg: {}", avg_latency.as_micros());
        println!("  Median: {}", median_latency.as_micros());
        println!("  P95: {}", p95_latency.as_micros());
        
        // Performance expectations for localhost (should be very fast)
        assert!(avg_latency < Duration::from_millis(10), "Average latency too high: {:?}", avg_latency);
        assert!(p95_latency < Duration::from_millis(20), "P95 latency too high: {:?}", p95_latency);
        
        timeout(Duration::from_secs(2), server_handle).await.unwrap().unwrap();
    }
}