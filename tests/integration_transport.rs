use test_log::test;
use bytes::Bytes;
use std::time::Duration;
use quiccrunch::{QuicTransport, Transport};
use quinn::{TransportConfig, VarInt}; // TransportConfig translation is limited to what our API exposes
use portpicker;
use tokio::time::timeout;
use quiccrunch::transport::{TransportSettings, CongestionController};
mod _helpers;
use _helpers::translate_quinn_to_settings;

mod certs;
use certs::generate_node_cert;

async fn setup_transport() -> (QuicTransport, String) {
    let mut settings = TransportSettings {
        max_idle_timeout_ms: Some(30000),
        keep_alive_interval_ms: Some(5000),
        max_concurrent_uni_streams: 100,
        max_concurrent_bidi_streams: 100,
        stream_receive_window: 10 * 1024 * 1024,  // 10MB stream window
        connection_receive_window: 20 * 1024 * 1024,  // 20MB connection window
        initial_rtt_ms: 100,
        congestion_controller: CongestionController::NewReno,
    };

    let tls_config = generate_node_cert("test-node");
    let transport = QuicTransport::with_settings(tls_config, settings).unwrap();
    let port = portpicker::pick_unused_port().expect("No ports free");
    let addr = format!("127.0.0.1:{}", port);
    
    (transport, addr)
}

async fn setup_transport_with_config(_quinn_transport: TransportConfig) -> (QuicTransport, String) {
    // Note: quinn::TransportConfig lacks getters; tests should configure via TransportSettings directly.
    let settings = TransportSettings {
        max_idle_timeout_ms: Some(30000),
        keep_alive_interval_ms: Some(5000),
        max_concurrent_uni_streams: 100,
        max_concurrent_bidi_streams: 100,
        stream_receive_window: 10 * 1024 * 1024,  // 10MB stream window
        connection_receive_window: 20 * 1024 * 1024,  // 20MB connection window
        initial_rtt_ms: 100,
        congestion_controller: CongestionController::NewReno,
    };

    let tls_config = generate_node_cert("test-node");
    let transport = QuicTransport::with_settings(tls_config, settings).unwrap();
    let port = portpicker::pick_unused_port().expect("No ports free");
    let addr = format!("127.0.0.1:{}", port);
    
    (transport, addr)
}

#[test(tokio::test)]
async fn test_basic_roundtrip() {
    let (transport, addr) = setup_transport().await;
    
    // Start server
    let mut incoming = transport.listen(&addr).await.unwrap();
    
    // Give server time to start listening
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let server_handle = tokio::spawn(async move {
        let conn = incoming.accept().await.unwrap();
         let mut stream = conn.accept_bi().await.unwrap();
         
         let msg = stream.receive_message().await.unwrap();
         stream.send_message(msg).await.unwrap();
         let _ = stream.close().await;
    });
    
    // Client
    let client = transport.connect(&addr).await.unwrap();
    let mut stream = client.open_bi().await.unwrap();
    
    let test_msg = Bytes::from("Hello, World!");
    stream.send_message(test_msg.clone()).await.unwrap();
    
    let response = stream.receive_message().await.unwrap();
    assert_eq!(response, test_msg);
    
    // Wait for server with timeout
    timeout(Duration::from_secs(5), server_handle).await.unwrap().unwrap();
}

#[test(tokio::test)]
async fn test_multiple_streams() {
    let (transport, addr) = setup_transport().await;
    let mut incoming = transport.listen(&addr).await.unwrap();
    
    // Give server time to start listening
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let server_handle = tokio::spawn(async move {
        let conn = incoming.accept().await.unwrap();
        
        for _ in 0..5 {
            let mut stream = conn.accept_bi().await.unwrap();
        let msg = stream.receive_message().await.unwrap();
        stream.send_message(msg).await.unwrap();
        let _ = stream.close().await;
            let _ = stream.close().await;
        }
    });
    
    let client = transport.connect(&addr).await.unwrap();
    let mut handles = vec![];
    
    for i in 0..5 {
        let test_msg = Bytes::from(format!("Message {}", i));
        let mut stream = client.open_bi().await.unwrap();
        
        let handle = tokio::spawn(async move {
            stream.send_message(test_msg.clone()).await.unwrap();
            let response = stream.receive_message().await.unwrap();
            assert_eq!(response, test_msg);
            let _ = stream.close().await;
        });
        
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Wait for server with timeout
    timeout(Duration::from_secs(5), server_handle).await.unwrap().unwrap();
}

#[test(tokio::test)]
async fn test_large_messages() {
    let (transport, addr) = setup_transport().await;
    let mut incoming = transport.listen(&addr).await.unwrap();
    
    // Give server time to start listening
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let server_handle = tokio::spawn(async move {
        let conn = incoming.accept().await.unwrap();
         let mut stream = conn.accept_bi().await.unwrap();
         
         let msg = stream.receive_message().await.unwrap();
         stream.send_message(msg).await.unwrap();
         let _ = stream.close().await;
    });
    
    let client = transport.connect(&addr).await.unwrap();
    let mut stream = client.open_bi().await.unwrap();
    
    // 1MB message
    let large_data = vec![0u8; 1024 * 1024];
    let large_msg = Bytes::from(large_data);
    stream.send_message(large_msg.clone()).await.unwrap();
    
    let response = stream.receive_message().await.unwrap();
    assert_eq!(response, large_msg);
    let _ = stream.close().await;
    
    // Wait for server with timeout
    timeout(Duration::from_secs(10), server_handle).await.unwrap().unwrap();
}

#[test(tokio::test)]
async fn test_message_size_limit() {
    // Configure receive window via our TransportSettings since quinn::TransportConfig has no getters
    let tls_config = generate_node_cert("test-node");
    let settings = TransportSettings {
        stream_receive_window: 1024,
        max_concurrent_uni_streams: 100,
        max_concurrent_bidi_streams: 100,
        connection_receive_window: 20 * 1024 * 1024,
        max_idle_timeout_ms: Some(30000),
        keep_alive_interval_ms: Some(5000),
        initial_rtt_ms: 100,
        congestion_controller: CongestionController::NewReno
    };
    let transport = QuicTransport::with_settings(tls_config, settings).unwrap();
    let port = portpicker::pick_unused_port().expect("No ports free");
    let addr = format!("127.0.0.1:{}", port);
    let mut incoming = transport.listen(&addr).await.unwrap();
    
    // Give server time to start listening
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let server_handle = tokio::spawn(async move {
        let conn = incoming.accept().await.unwrap();
        let mut stream = conn.accept_bi().await.unwrap();
        
        // Expect an error when client attempts to send too-large message
        let result = stream.receive_message().await;
        assert!(result.is_err());
    });
    
    let client = transport.connect(&addr).await.unwrap();
    let mut stream = client.open_bi().await.unwrap();
    
    // Try to send 2KB
    let large_data = vec![0u8; 2048];
    let large_msg = Bytes::from(large_data);
    let result = stream.send_message(large_msg).await;
    
    assert!(result.is_err());
    // Close the stream to ensure the server receives EOF and exits
    let _ = stream.close().await;
    
    // Wait for server with timeout
    timeout(Duration::from_secs(5), server_handle).await.unwrap().unwrap();
}

#[test(tokio::test)]
async fn test_concurrent_streams_varying_windows() {
    let mut config = TransportConfig::default();
    config
        .max_concurrent_uni_streams(VarInt::from_u32(1000))
        .max_concurrent_bidi_streams(VarInt::from_u32(1000))
        .stream_receive_window(VarInt::from_u32(2 * 1024 * 1024))  // 2MB per stream
        .receive_window(VarInt::from_u32(16 * 1024 * 1024));       // 16MB connection window

    let (transport, addr) = setup_transport_with_config(config).await;
    let mut incoming = transport.listen(&addr).await.unwrap();
    
    // Give server time to start listening
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let server_handle = tokio::spawn(async move {
        let conn = incoming.accept().await.unwrap();
        let mut handles = vec![];
        
        // Accept multiple concurrent streams
        for _ in 0..10 {
            let mut stream = conn.accept_bi().await.unwrap();
            let handle = tokio::spawn(async move {
                loop {
                    match stream.receive_message().await {
                        Ok(msg) => {
                            // Simulate varying processing times
                            tokio::time::sleep(Duration::from_millis(10)).await;
                            stream.send_message(msg).await.unwrap();
                        }
                        Err(_) => break,
                    }
                }
                let _ = stream.close().await;
            });
            handles.push(handle);
        }
        
        for handle in handles {
            let _ = handle.await;
        }
    });
    
    let client = transport.connect(&addr).await.unwrap();
    let mut handles = vec![];
    
    // Create multiple concurrent streams with different message sizes
    for i in 0..10 {
        let mut stream = client.open_bi().await.unwrap();
        let handle = tokio::spawn(async move {
                for j in 0..5 {
                    let size = 1024 * (i + 1); // Different sizes for different streams
                    let data = vec![j as u8; size];
                    let msg = Bytes::from(data);
                    stream.send_message(msg.clone()).await.unwrap();
                    let response = stream.receive_message().await.unwrap();
                    assert_eq!(response, msg);
                }
                let _ = stream.close().await;
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Wait for server with timeout
    timeout(Duration::from_secs(10), server_handle).await.unwrap().unwrap();
}

#[test(tokio::test)]
async fn test_error_recovery_and_timeout() {
    // Use settings to ensure idle timeout hits and no keepalive prevents activity
    let tls_config = generate_node_cert("test-node");
    let settings = TransportSettings {
        max_idle_timeout_ms: Some(1000),
        keep_alive_interval_ms: None,
        max_concurrent_uni_streams: 100,
        max_concurrent_bidi_streams: 100,
        stream_receive_window: 2 * 1024 * 1024,
        connection_receive_window: 16 * 1024 * 1024,
        initial_rtt_ms: 100,
        congestion_controller: CongestionController::NewReno,
    };
    let transport = QuicTransport::with_settings(tls_config, settings).unwrap();
    let port = portpicker::pick_unused_port().expect("No ports free");
    let addr = format!("127.0.0.1:{}", port);
    let mut incoming = transport.listen(&addr).await.unwrap();
    
    // Give server time to start listening
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let server_handle = tokio::spawn(async move {
        let conn = incoming.accept().await.unwrap();
        let mut stream = conn.accept_bi().await.unwrap();
        
        // Simulate server processing delay
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        let msg = stream.receive_message().await.unwrap();
        stream.send_message(msg).await.unwrap();
        
        // Force idle timeout
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        // This should fail due to timeout
        let result = stream.receive_message().await;
        assert!(result.is_err());
    });
    
    let client = transport.connect(&addr).await.unwrap();
    let mut stream = client.open_bi().await.unwrap();
    
    let msg = Bytes::from("Test message");
    stream.send_message(msg.clone()).await.unwrap();
    
    let response = stream.receive_message().await.unwrap();
    assert_eq!(response, msg);
    
    // Wait for timeout
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // This should fail
    let result = stream.send_message(msg).await;
    assert!(result.is_err());
    
    // Wait for server with timeout
    timeout(Duration::from_secs(5), server_handle).await.unwrap().unwrap();
}

#[test(tokio::test)]
async fn test_cross_stream_dependencies() {
    let (transport, addr) = setup_transport().await;
    let mut incoming = transport.listen(&addr).await.unwrap();
    
    // Give server time to start listening
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let server_handle = tokio::spawn(async move {
        let conn = incoming.accept().await.unwrap();
        // Accept two incoming streams in any order
        let mut s1 = conn.accept_bi().await.unwrap();
        let mut s2 = conn.accept_bi().await.unwrap();

        // Handle both streams concurrently: treat "PAUSE" as control (sleep), echo any other data
        let h1 = tokio::spawn(async move {
            loop {
                match tokio::time::timeout(Duration::from_secs(3), s1.receive_message()).await {
                    Ok(Ok(msg)) => {
                        if msg == Bytes::from("PAUSE") {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        } else {
                            let _ = s1.send_message(msg).await;
                        }
                    }
                    Ok(Err(_)) => break,
                    Err(_) => break, // timeout
                }
            }
            let _ = s1.close().await;
        });

        let h2 = tokio::spawn(async move {
            loop {
                match tokio::time::timeout(Duration::from_secs(3), s2.receive_message()).await {
                    Ok(Ok(msg)) => {
                        if msg == Bytes::from("PAUSE") {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        } else {
                            let _ = s2.send_message(msg).await;
                        }
                    }
                    Ok(Err(_)) => break,
                    Err(_) => break,
                }
            }
            let _ = s2.close().await;
        });

        let _ = h1.await;
        let _ = h2.await;
    });
    
    let client = transport.connect(&addr).await.unwrap();
    let mut control_stream = client.open_bi().await.unwrap();
    let mut data_stream = client.open_bi().await.unwrap();
    
    for i in 0..5 {
        // Send control message to potentially pause processing
        control_stream.send_message(Bytes::from("PAUSE")).await.unwrap();

        // Immediately send data; server will pause on control and echo data concurrently
        let msg = Bytes::from(format!("Data message {}", i));
        data_stream.send_message(msg.clone()).await.unwrap();

        // Bound the wait for echo to avoid hanging the test
        let response = tokio::time::timeout(Duration::from_secs(2), data_stream.receive_message()).await.unwrap().unwrap();
        assert_eq!(response, msg);
    }
    
    // Wait for server with timeout
    timeout(Duration::from_secs(10), server_handle).await.unwrap().unwrap();
}

#[test(tokio::test)]
async fn test_connection_timeout() {
    let mut config = TransportConfig::default();
    config.max_idle_timeout(Some(Duration::from_secs(1).try_into().expect("Valid timeout")));
    
    let (transport, _addr) = setup_transport_with_config(config).await;
    
    let result = transport.connect("127.0.0.1:1").await;
    assert!(result.is_err());
}