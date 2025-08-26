#[cfg(test)]
mod tests {
    use quiccrunch::transport::{QuicTransport, Transport, TransportSettings};
    use quiccrunch::test_utils::certs::generate_node_cert;
    use tokio::test;
    use bytes::Bytes;
    use std::time::Duration;
    use tokio::time::timeout;
    // use quinn::VarInt;
    // use portpicker;

    // Helper function to create test data of specified size
    fn create_test_data(size: usize) -> Bytes {
        Bytes::from(vec![0u8; size])
    }

    #[test]
    async fn test_stream_flow_control() {
        let tls_config = generate_node_cert("test-node");
        let server = QuicTransport::new(tls_config.clone()).unwrap();
        let client = QuicTransport::new(tls_config).unwrap();
        
        let mut listener = server.listen("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        
        let client_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let conn = client.connect(&addr).await.unwrap();
            let mut stream = conn.open_bi().await.unwrap();
            
            // Send data larger than initial window size
            let large_data = vec![0u8; 100_000];
            stream.send_message(Bytes::from(large_data)).await.unwrap();
            // Explicitly close to signal end of message sequence
            let _ = stream.close().await;
        });
        
        let server_task = tokio::spawn(async move {
            let conn = listener.accept().await.unwrap();
            let mut stream = conn.accept_bi().await.unwrap();
            
            let msg = stream.receive_message().await.unwrap();
            assert_eq!(msg.len(), 100_000);
        });
        
        let (client_result, server_result) = tokio::join!(
            timeout(Duration::from_secs(5), client_task),
            timeout(Duration::from_secs(5), server_task)
        );
        client_result.unwrap().unwrap();
        server_result.unwrap().unwrap();
    }

    #[test]
    async fn test_connection_flow_control() {
        let tls_config = generate_node_cert("test-node");
        let server = QuicTransport::new(tls_config.clone()).unwrap();
        let client = QuicTransport::new(tls_config).unwrap();
        
        let mut listener = server.listen("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        
        let client_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let conn = client.connect(&addr).await.unwrap();
            
            let mut streams = Vec::new();
            for _ in 0..3 {
                let stream = conn.open_bi().await.unwrap();
                streams.push(stream);
            }
            
            for stream in &mut streams {
                let data = vec![0u8; 50_000];
                stream.send_message(Bytes::from(data)).await.unwrap();
            }
            // Close all streams to signal completion
            for mut stream in streams {
                let _ = stream.close().await;
            }
        });
        
        let server_task = tokio::spawn(async move {
            let conn = listener.accept().await.unwrap();
            
            for _ in 0..3 {
                let mut stream = conn.accept_bi().await.unwrap();
                let msg = stream.receive_message().await.unwrap();
                assert_eq!(msg.len(), 50_000);
            }
        });
        
        let (client_result, server_result) = tokio::join!(
            timeout(Duration::from_secs(5), client_task),
            timeout(Duration::from_secs(5), server_task)
        );
        client_result.unwrap().unwrap();
        server_result.unwrap().unwrap();
    }

    #[test]
    async fn test_backpressure_handling() {
        let tls_config = generate_node_cert("test-node");
        let server = QuicTransport::new(tls_config.clone()).unwrap();
        let client = QuicTransport::new(tls_config).unwrap();
        
        let mut listener = server.listen("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        
        let client_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let conn = client.connect(&addr).await.unwrap();
            let mut stream = conn.open_bi().await.unwrap();
            
            for _ in 0..5 {
                let data = create_test_data(100_000);
                stream.send_message(data).await.unwrap();
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });
        
        let server_task = tokio::spawn(async move {
            let conn = listener.accept().await.unwrap();
            let mut stream = conn.accept_bi().await.unwrap();
            
            for i in 0..5 {
                let msg = stream.receive_message().await.unwrap();
                assert_eq!(msg.len(), 100_000);
                if i < 4 {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        });
        
        let (client_result, server_result) = tokio::join!(
            timeout(Duration::from_secs(15), client_task),
            timeout(Duration::from_secs(15), server_task)
        );
        client_result.unwrap().unwrap();
        server_result.unwrap().unwrap();
    }

    #[test]
    async fn test_stream_multiplexing_limits() {
        let mut settings = TransportSettings::default();
        settings.max_concurrent_uni_streams = 50;
        settings.max_concurrent_bidi_streams = 50;

        let tls_config = generate_node_cert("test-node");
        let server = QuicTransport::with_settings(tls_config.clone(), settings.clone()).unwrap();
        let client = QuicTransport::with_settings(tls_config, settings).unwrap();
        
        let mut listener = server.listen("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        
        let client_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let conn = client.connect(&addr).await.unwrap();
            
            let mut streams = Vec::new();
            for i in 0..100 {
                match conn.open_bi().await {
                    Ok(stream) => streams.push(stream),
                    Err(_e) => {
                        assert!(i >= 50);
                        return;
                    }
                }
            }

            panic!("Opened more streams than the limit");
        });
        
        let server_task = tokio::spawn(async move {
            let conn = listener.accept().await.unwrap();
            let mut accepted = 0;
            
            loop {
                match timeout(Duration::from_millis(200), conn.accept_bi()).await {
                    Ok(Ok(_stream)) => accepted += 1,
                    Ok(Err(_)) => break,
                    Err(_) => break,
                }
            }
            
            assert!(accepted >= 0 && accepted <= 50);
        });
        
        let (client_result, server_result) = tokio::join!(
            timeout(Duration::from_secs(10), client_task),
            timeout(Duration::from_secs(10), server_task)
        );
        client_result.unwrap().unwrap();
        server_result.unwrap().unwrap();
    }

    #[test]
    async fn test_flow_control_error_conditions() {
        let tls_config = generate_node_cert("test-node");
        let server = QuicTransport::new(tls_config.clone()).unwrap();
        let client = QuicTransport::new(tls_config).unwrap();
        
        let mut listener = server.listen("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        
        let client_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let conn = client.connect(&addr).await.unwrap();
            let mut stream = conn.open_bi().await.unwrap();
            
            stream.close().await.unwrap();
            let result = stream.send_message(create_test_data(1000)).await;
            assert!(result.is_err());
        });
        
        let server_task = tokio::spawn(async move {
            let conn = listener.accept().await.unwrap();
            let mut stream = conn.accept_bi().await.unwrap();
            
            let result = stream.receive_message().await;
            assert!(result.is_err());
        });
        
        let (client_result, server_result) = tokio::join!(
            timeout(Duration::from_secs(5), client_task),
            timeout(Duration::from_secs(5), server_task)
        );
        client_result.unwrap().unwrap();
        server_result.unwrap().unwrap();
    }

    #[test]
    async fn test_message_size_limit() {
        let mut settings = TransportSettings::default();
        settings.stream_receive_window = 1024; // 1KB window
        
        let tls_config = generate_node_cert("test-node");
        let server = QuicTransport::with_settings(tls_config.clone(), settings.clone()).unwrap();
        let client = QuicTransport::with_settings(tls_config, settings).unwrap();
        
        let mut listener = server.listen("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        
        let server_handle = tokio::spawn(async move {
            let conn = listener.accept().await.unwrap();
            let mut stream = conn.accept_bi().await.unwrap();
            
            let result = stream.receive_message().await;
            assert!(result.is_err());
        });
        
        tokio::time::sleep(Duration::from_millis(100)).await;

        let client_conn = client.connect(&addr).await.unwrap();
        let mut stream = client_conn.open_bi().await.unwrap();
        
        let large_data = vec![0u8; 2048];
        let large_msg = Bytes::from(large_data);
        let result = stream.send_message(large_msg).await;
        
        assert!(result.is_err());
        // Close the stream to signal EOF to the server so it doesn't hang on receive
        let _ = stream.close().await;
        
        // Wait for server with timeout to prevent hangs
        tokio::time::timeout(Duration::from_secs(5), server_handle)
            .await
            .expect("server task timed out")
            .unwrap();
    }
}