/// Real Network Worker Pool Tests
///
/// These tests validate the tQUIC worker pool architecture with REAL network operations:
/// - UDP socket binding with SO_REUSEPORT
/// - Multi-worker coordination and load balancing
/// - Command processing through worker channels
/// - Network-level validation of socket sharing
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;

use quiccrunch_core::transport::{
    messages::{TquicCommand, WorkerConfig, WorkerHealth},
    worker_pool::TquicWorkerPool,
};
use quiccrunch_core::TransportError;

/// Helper function to create a worker config for network testing
fn create_worker_config(worker_id: usize, port: u16) -> WorkerConfig {
    WorkerConfig {
        worker_id,
        bind_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)),
        cert_file: None,
        key_file: None,
        is_server: false,
        verify_cert: true,
        ca_cert_file: None,
        max_idle_timeout: Duration::from_secs(30),
        max_udp_payload_size: 1350,
        enable_early_data: false,
        application_protocols: vec![b"h3".to_vec()],
    }
}

#[tokio::test]
async fn test_worker_pool_creation_and_health() {
    let port = 17000;
    let worker_config = create_worker_config(0, port);

    // Test creating worker pool with real network configuration
    let pool = TquicWorkerPool::new(4, worker_config);
    assert!(
        pool.is_ok(),
        "Failed to create worker pool: {:?}",
        pool.err()
    );

    let pool = pool.unwrap();

    // Allow workers to initialize
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Check worker health status
    let health_status = pool.health_check();
    assert_eq!(health_status.len(), 4, "Expected 4 workers");

    for (i, health) in health_status.iter().enumerate() {
        assert!(
            matches!(health, WorkerHealth::Healthy),
            "Worker {} is not healthy: {:?}",
            i,
            health
        );
    }

    // Verify worker status details
    let worker_status = pool.get_worker_status();
    assert_eq!(worker_status.len(), 4);

    for status in &worker_status {
        assert!(matches!(status.health, WorkerHealth::Healthy));
        assert_eq!(status.stats.errors, 0);
    }

    // Clean shutdown
    let shutdown_result = pool.shutdown(Duration::from_secs(5)).await;
    assert!(
        shutdown_result.is_ok(),
        "Failed to shutdown cleanly: {:?}",
        shutdown_result.err()
    );
}

#[tokio::test]
async fn test_so_reuseport_multi_worker_binding() {
    let port = 17001;
    let worker_config = create_worker_config(0, port);

    // Test that SO_REUSEPORT allows multiple workers to bind to same port
    let pool = TquicWorkerPool::new(6, worker_config);
    assert!(
        pool.is_ok(),
        "SO_REUSEPORT multi-worker binding failed: {:?}",
        pool.err()
    );

    let pool = pool.unwrap();

    // Allow all workers to bind
    tokio::time::sleep(Duration::from_millis(300)).await;

    // All workers should be healthy if SO_REUSEPORT worked
    let health_status = pool.health_check();
    assert_eq!(health_status.len(), 6);

    let healthy_count = health_status
        .iter()
        .filter(|h| matches!(h, WorkerHealth::Healthy))
        .count();

    assert_eq!(
        healthy_count, 6,
        "Not all workers healthy - SO_REUSEPORT may have failed. Health: {:?}",
        health_status
    );

    pool.shutdown(Duration::from_secs(5)).await.unwrap();
}

#[tokio::test]
#[ignore = "requires a TLS-configured worker endpoint; the pool harness has no TLS selector"]
async fn test_worker_command_processing() {
    let port = 17002;
    let worker_config = create_worker_config(0, port);
    let pool = TquicWorkerPool::new(3, worker_config).unwrap();

    tokio::time::sleep(Duration::from_millis(150)).await;

    // Test submitting connect commands to workers
    let mut command_futures = Vec::new();

    for i in 0..10 {
        let remote_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8000 + i));
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        let connect_cmd = TquicCommand::Connect {
            connection_id: 0,
            remote_addr,
            response: response_tx,
        };

        // Submit command and prepare to wait for response
        let submit_future = pool.submit_command(connect_cmd);
        let response_future = async move {
            submit_future.await?;
            timeout(Duration::from_millis(1000), response_rx)
                .await
                .map_err(|_| TransportError::ConnectionFailed("Response timeout".into()))?
                .map_err(|_| TransportError::ConnectionFailed("Response channel closed".into()))
        };

        command_futures.push(response_future);
    }

    // Wait for all commands to complete
    let results = futures::future::join_all(command_futures).await;

    let mut successful = 0;
    for result in results {
        match result {
            Ok(Ok(_connection_id)) => successful += 1,
            Ok(Err(e)) => eprintln!("Command failed: {:?}", e),
            Err(e) => eprintln!("Submit failed: {:?}", e),
        }
    }

    assert!(
        successful >= 8,
        "Too many command failures: {}/10",
        successful
    );

    pool.shutdown(Duration::from_secs(3)).await.unwrap();
}

#[tokio::test]
#[ignore = "requires a TLS-configured worker endpoint; the pool harness has no TLS selector"]
async fn test_load_balancing_and_metrics() {
    let port = 17003;
    let worker_config = create_worker_config(0, port);
    let pool = TquicWorkerPool::new(4, worker_config).unwrap();

    tokio::time::sleep(Duration::from_millis(150)).await;

    // Submit many commands to test load distribution
    let command_count = 40;
    let mut handles = Vec::new();

    for i in 0..command_count {
        let remote_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 9000 + i as u16));
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        let connect_cmd = TquicCommand::Connect {
            connection_id: 0,
            remote_addr,
            response: response_tx,
        };

        pool.submit_command(connect_cmd).await.unwrap();
        let handle = tokio::spawn(async move {
            timeout(Duration::from_millis(500), response_rx)
                .await
                .unwrap()
                .unwrap()
        });

        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        let result = handle.await;
        assert!(
            result.is_ok(),
            "Command processing failed: {:?}",
            result.err()
        );
    }

    // Check performance metrics
    let metrics = pool.get_performance_metrics();
    assert!(metrics.total_commands >= command_count);
    assert!(metrics.commands_per_second > 0.0);

    println!("Load balancing test metrics:");
    println!("  Total commands: {}", metrics.total_commands);
    println!("  Commands/sec: {:.2}", metrics.commands_per_second);
    println!("  Uptime: {:?}", metrics.uptime);

    // Check load distribution across workers
    let load_distribution = pool.get_load_distribution();
    assert_eq!(load_distribution.len(), 4);

    println!("Load distribution: {:?}", load_distribution);

    // Verify load is reasonably distributed (no worker should have 0 or all commands)
    let max_load = *load_distribution.iter().max().unwrap();
    let total_load: usize = load_distribution.iter().sum();

    assert!(total_load > 0, "No load recorded");
    assert!(max_load < total_load, "One worker got all the load");

    // Check socket distribution analysis
    let socket_analysis = pool.analyze_socket_distribution();
    assert_eq!(socket_analysis.worker_count, 4);

    println!("Socket distribution analysis:");
    println!("  Total packets: {}", socket_analysis.total_packets);
    println!("  Fairness score: {:.2}", socket_analysis.fairness_score);
    println!("  Is balanced: {}", socket_analysis.is_balanced);
    println!(
        "  Per-worker packets: {:?}",
        socket_analysis.per_worker_packets
    );

    pool.shutdown(Duration::from_secs(3)).await.unwrap();
}

#[tokio::test]
#[ignore = "requires a TLS-configured worker endpoint; the pool harness has no TLS selector"]
async fn test_connection_affinity() {
    let port = 17004;
    let worker_config = create_worker_config(0, port);
    let pool = TquicWorkerPool::new(3, worker_config).unwrap();

    tokio::time::sleep(Duration::from_millis(150)).await;

    // Create a connection
    let remote_addr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8080));
    let (connect_tx, connect_rx) = tokio::sync::oneshot::channel();

    let connect_cmd = TquicCommand::Connect {
        connection_id: 0,
        remote_addr,
        response: connect_tx,
    };

    pool.submit_command(connect_cmd).await.unwrap();
    let connection_id = timeout(Duration::from_millis(500), connect_rx)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    // Map connection to worker 1
    pool.map_connection_to_worker(connection_id, 1);

    // Send multiple operations on the same connection
    for i in 0..5 {
        let (send_tx, send_rx) = tokio::sync::oneshot::channel();
        let send_cmd = TquicCommand::Send {
            connection_id,
            stream_id: i,
            data: format!("test message {}", i).into_bytes(),
            fin: false,
            response: send_tx,
        };

        pool.submit_command(send_cmd).await.unwrap();
        let send_result = timeout(Duration::from_millis(500), send_rx).await.unwrap();
        assert!(
            send_result.is_ok(),
            "Send command failed: {:?}",
            send_result.err()
        );
    }

    // Cleanup
    pool.unmap_connection(connection_id);

    pool.shutdown(Duration::from_secs(3)).await.unwrap();
}

#[tokio::test]
async fn test_concurrent_worker_operations() {
    let port = 17005;
    let worker_config = create_worker_config(0, port);
    let pool = Arc::new(TquicWorkerPool::new(6, worker_config).unwrap());

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Spawn multiple concurrent tasks
    let task_count = 8;
    let operations_per_task = 10;
    let mut task_handles = Vec::new();

    for task_id in 0..task_count {
        let pool_clone = pool.clone();

        let handle = tokio::spawn(async move {
            let mut results = Vec::new();

            for op_id in 0..operations_per_task {
                let remote_addr = SocketAddr::V4(SocketAddrV4::new(
                    Ipv4Addr::LOCALHOST,
                    10000 + (task_id * 100) + op_id,
                ));

                let (connect_tx, connect_rx) = tokio::sync::oneshot::channel();
                let connect_cmd = TquicCommand::Connect {
                    connection_id: 0,
                    remote_addr,
                    response: connect_tx,
                };

                let submit_result = pool_clone.submit_command(connect_cmd).await;
                if submit_result.is_ok() {
                    if let Ok(Ok(_connection_id)) =
                        timeout(Duration::from_millis(200), connect_rx).await
                    {
                        results.push(true);
                    } else {
                        results.push(false);
                    }
                } else {
                    results.push(false);
                }
            }

            results
        });

        task_handles.push(handle);
    }

    // Collect results
    let mut total_operations = 0;
    let mut successful_operations = 0;

    for handle in task_handles {
        let task_results = handle.await.unwrap();
        total_operations += task_results.len();
        successful_operations += task_results.iter().filter(|&&success| success).count();
    }

    println!("Concurrent operations test:");
    println!("  Total operations: {}", total_operations);
    println!("  Successful operations: {}", successful_operations);
    println!(
        "  Success rate: {:.1}%",
        successful_operations as f64 / total_operations as f64 * 100.0
    );

    // Should have high success rate
    assert!(
        successful_operations >= total_operations * 80 / 100,
        "Too many failures in concurrent test: {}/{}",
        successful_operations,
        total_operations
    );

    // Final metrics
    let metrics = pool.get_performance_metrics();
    assert!(metrics.total_commands >= successful_operations);

    println!("Final metrics:");
    println!("  Commands processed: {}", metrics.total_commands);
    println!("  Commands/sec: {:.2}", metrics.commands_per_second);

    pool.shutdown(Duration::from_secs(5)).await.unwrap();
}

#[tokio::test]
#[ignore = "requires a TLS-configured worker endpoint; the pool harness has no TLS selector"]
async fn test_high_frequency_commands() {
    let port = 17006;
    let worker_config = create_worker_config(0, port);
    let pool = TquicWorkerPool::new(8, worker_config).unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Submit commands rapidly
    let command_count = 200;
    let start_time = Instant::now();
    let mut futures = Vec::new();

    for i in 0..command_count {
        let remote_addr = SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::LOCALHOST,
            11000 + (i % 1000) as u16,
        ));
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        let connect_cmd = TquicCommand::Connect {
            connection_id: 0,
            remote_addr,
            response: response_tx,
        };

        // Submit immediately without waiting
        let submit_future = pool.submit_command(connect_cmd);
        let response_future = async move {
            submit_future.await?;
            timeout(Duration::from_millis(1000), response_rx)
                .await
                .map_err(|_| TransportError::ConnectionFailed("Timeout".into()))?
                .map_err(|_| TransportError::ConnectionFailed("Channel closed".into()))
        };

        futures.push(response_future);
    }

    // Wait for all commands
    let results = futures::future::join_all(futures).await;
    let duration = start_time.elapsed();

    let successful = results.iter().filter(|r| r.is_ok()).count();
    let commands_per_second = successful as f64 / duration.as_secs_f64();

    println!("High frequency test results:");
    println!("  Commands submitted: {}", command_count);
    println!("  Commands successful: {}", successful);
    println!("  Duration: {:?}", duration);
    println!("  Rate: {:.1} commands/sec", commands_per_second);

    // Should handle at least 80% successfully
    assert!(
        successful >= command_count * 80 / 100,
        "High frequency test failed: {}/{}",
        successful,
        command_count
    );

    // Should achieve reasonable throughput
    assert!(
        commands_per_second >= 50.0,
        "Command rate too low: {:.1}/sec",
        commands_per_second
    );

    // Check final load distribution
    let distribution = pool.analyze_socket_distribution();
    println!("Load distribution: {:?}", distribution.per_worker_packets);

    // Verify load is spread across workers
    let non_zero_workers = distribution
        .per_worker_packets
        .iter()
        .filter(|(_, count)| *count > 0)
        .count();

    assert!(non_zero_workers >= 4, "Load not distributed across workers");

    pool.shutdown(Duration::from_secs(5)).await.unwrap();
}

#[tokio::test]
async fn test_worker_error_handling() {
    let port = 17007;
    let worker_config = create_worker_config(0, port);
    let pool = TquicWorkerPool::new(2, worker_config).unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // All workers should start healthy
    let initial_health = pool.health_check();
    assert_eq!(initial_health.len(), 2);

    for health in &initial_health {
        assert!(matches!(health, WorkerHealth::Healthy));
    }

    // Test worker status reporting
    let worker_status = pool.get_worker_status();
    assert_eq!(worker_status.len(), 2);

    for (i, status) in worker_status.iter().enumerate() {
        assert!(matches!(status.health, WorkerHealth::Healthy));
        assert_eq!(status.stats.errors, 0);
        println!(
            "Worker {}: health={:?}, connections={}, errors={}",
            i, status.health, status.stats.active_connections, status.stats.errors
        );
    }

    // Submit some valid commands
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
    let valid_cmd = TquicCommand::Connect {
        connection_id: 0,
        remote_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8080)),
        response: response_tx,
    };

    let submit_result = pool.submit_command(valid_cmd).await;
    assert!(submit_result.is_ok(), "Valid command should succeed");

    // Response should be received
    let response = timeout(Duration::from_millis(500), response_rx).await;
    assert!(response.is_ok(), "Should receive response");

    pool.shutdown(Duration::from_secs(3)).await.unwrap();
}
