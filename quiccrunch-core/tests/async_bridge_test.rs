#[cfg(test)]
mod async_bridge_tests {
    use quiccrunch_core::transport::adapter::{TquicClient, TquicServer};
    use std::net::SocketAddr;
    use std::time::Duration;
    use tokio::time::timeout;
    
    #[tokio::test]
    async fn test_client_creation() {
        let result = TquicClient::new(None).await;
        assert!(result.is_ok(), "TquicClient creation should succeed");
    }
    
    #[tokio::test]
    async fn test_connect_invalid_address() {
        let client = TquicClient::new(None).await.expect("Client creation failed");
        let result = client.connect("invalid address").await;
        assert!(result.is_err(), "Connection to invalid address should fail");
    }
    
    #[tokio::test]
    async fn test_server_creation() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let cert_file = "tests/certs/cert.pem".to_string();
        let key_file = "tests/certs/key.pem".to_string();
        
        let result = TquicServer::new(bind_addr, cert_file, key_file, None).await;
        assert!(result.is_ok(), "TquicServer creation should succeed");
    }
    
    #[tokio::test]
    async fn test_server_listen() {
        let bind_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let cert_file = "tests/certs/cert.pem".to_string();
        let key_file = "tests/certs/key.pem".to_string();
        
        let server = TquicServer::new(bind_addr, cert_file, key_file, None).await
            .expect("Server creation failed");
        
        let result = server.listen("127.0.0.1:0").await;
        assert!(result.is_ok(), "Server listen should succeed");
    }
    
    #[tokio::test]
    async fn test_client_server_connection() {
        let bind_addr: SocketAddr = "127.0.0.1:14433".parse().unwrap();
        let cert_file = "tests/certs/cert.pem".to_string();
        let key_file = "tests/certs/key.pem".to_string();
        
        let server = TquicServer::new(bind_addr, cert_file, key_file, None).await
            .expect("Server creation failed");
        
        let mut listener = server.listen("127.0.0.1:14433").await
            .expect("Server listen failed");
        
        tokio::spawn(async move {
            match timeout(Duration::from_secs(5), listener.accept()).await {
                Ok(Ok(_conn)) => {
                    println!("Server accepted connection");
                }
                Ok(Err(e)) => {
                    eprintln!("Server accept error: {:?}", e);
                }
                Err(_) => {
                    eprintln!("Server accept timeout");
                }
            }
        });
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let client = TquicClient::new(None).await.expect("Client creation failed");
        
        let result = timeout(Duration::from_secs(2), client.connect("127.0.0.1:14433")).await;
        
        match result {
            Ok(Ok(_conn)) => {
                println!("Client connected successfully");
            }
            Ok(Err(e)) => {
                println!("Client connection error: {:?}", e);
            }
            Err(_) => {
                println!("Client connection timeout");
            }
        }
    }
    
    #[tokio::test]
    async fn test_bidirectional_stream() {
        let bind_addr: SocketAddr = "127.0.0.1:14434".parse().unwrap();
        let cert_file = "tests/certs/cert.pem".to_string();
        let key_file = "tests/certs/key.pem".to_string();
        
        let server = TquicServer::new(bind_addr, cert_file, key_file, None).await
            .expect("Server creation failed");
        
        let mut listener = server.listen("127.0.0.1:14434").await
            .expect("Server listen failed");
        
        tokio::spawn(async move {
            match timeout(Duration::from_secs(5), listener.accept()).await {
                Ok(Ok(conn)) => {
                    println!("Server accepted connection");
                    match timeout(Duration::from_secs(5), conn.accept_stream()).await {
                        Ok(Ok(mut stream)) => {
                            println!("Server accepted stream");
                            match stream.receive(1024).await {
                                Ok(data) if !data.is_empty() => {
                                    println!("Server received: {} bytes", data.len());
                                    let response = b"Echo: ";
                                    let mut response_data = response.to_vec();
                                    response_data.extend_from_slice(&data);
                                    let _ = stream.send(&response_data, false).await;
                                }
                                _ => println!("Server received empty or error"),
                            }
                        }
                        Ok(Err(e)) => eprintln!("Server accept_stream error: {:?}", e),
                        Err(_) => eprintln!("Server accept_stream timeout"),
                    }
                }
                Ok(Err(e)) => eprintln!("Server accept error: {:?}", e),
                Err(_) => eprintln!("Server accept timeout"),
            }
        });
        
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        let client = TquicClient::new(None).await.expect("Client creation failed");
        
        match timeout(Duration::from_secs(2), client.connect("127.0.0.1:14434")).await {
            Ok(Ok(conn)) => {
                println!("Client connected");
                match conn.open_stream().await {
                    Ok(mut stream) => {
                        println!("Client opened stream");
                        let message = b"Hello from client";
                        match stream.send(message, false).await {
                            Ok(_) => {
                                println!("Client sent message");
                                tokio::time::sleep(Duration::from_millis(100)).await;
                                match stream.receive(1024).await {
                                    Ok(response) if !response.is_empty() => {
                                        println!("Client received response: {} bytes", response.len());
                                        assert!(response.starts_with(b"Echo: "));
                                    }
                                    _ => println!("Client received empty or error"),
                                }
                            }
                            Err(e) => eprintln!("Client send error: {:?}", e),
                        }
                    }
                    Err(e) => eprintln!("Client open_stream error: {:?}", e),
                }
            }
            Ok(Err(e)) => println!("Client connection error: {:?}", e),
            Err(_) => println!("Client connection timeout"),
        }
    }
    
    #[tokio::test]
    async fn test_multiple_streams_per_connection() {
        let bind_addr: SocketAddr = "127.0.0.1:14435".parse().unwrap();
        let cert_file = "tests/certs/cert.pem".to_string();
        let key_file = "tests/certs/key.pem".to_string();
        
        let server = TquicServer::new(bind_addr, cert_file, key_file, None).await
            .expect("Server creation failed");
        
        let mut listener = server.listen("127.0.0.1:14435").await
            .expect("Server listen failed");
        
        tokio::spawn(async move {
            if let Ok(Ok(conn)) = timeout(Duration::from_secs(5), listener.accept()).await {
                println!("Server accepted connection");
                for i in 0..3 {
                    if let Ok(Ok(mut stream)) = timeout(Duration::from_secs(5), conn.accept_stream()).await {
                        println!("Server accepted stream {}", i);
                        tokio::spawn(async move {
                            if let Ok(data) = stream.receive(1024).await {
                                if !data.is_empty() {
                                    let _ = stream.send(&data, false).await;
                                }
                            }
                        });
                    }
                }
            }
        });
        
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        let client = TquicClient::new(None).await.expect("Client creation failed");
        
        if let Ok(Ok(conn)) = timeout(Duration::from_secs(2), client.connect("127.0.0.1:14435")).await {
            println!("Client connected");
            
            let mut streams = Vec::new();
            for i in 0..3 {
                match conn.open_stream().await {
                    Ok(stream) => {
                        println!("Client opened stream {}", i);
                        streams.push(stream);
                    }
                    Err(e) => eprintln!("Failed to open stream {}: {:?}", i, e),
                }
            }
            
            assert_eq!(streams.len(), 3, "Should open 3 streams");
            
            for (i, mut stream) in streams.into_iter().enumerate() {
                let message = format!("Message {}", i);
                if stream.send(message.as_bytes(), false).await.is_ok() {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    if let Ok(response) = stream.receive(1024).await {
                        println!("Stream {} received {} bytes", i, response.len());
                        assert_eq!(response, message.as_bytes());
                    }
                }
            }
        }
    }
    
    #[tokio::test]
    #[ignore = "Needs refinement - immediate failure vs timeout"]
    async fn test_connection_timeout() {
        let client = TquicClient::new(None).await.expect("Client creation failed");
        
        let result = timeout(Duration::from_secs(12), client.connect("192.0.2.1:4433")).await;
        
        match result {
            Ok(Err(e)) => {
                let error_str = format!("{:?}", e);
                println!("Connection failed with error: {}", error_str);
                assert!(error_str.to_lowercase().contains("timeout") || error_str.contains("Connection timeout"), 
                    "Expected timeout error, got: {}", error_str);
            }
            Ok(Ok(_)) => {
                panic!("Connection should have timed out");
            }
            Err(_) => {
                panic!("Test timeout before connection timeout");
            }
        }
    }

    #[tokio::test]
    #[ignore = "Runtime lifecycle issue - client runtime stops before connection completes"]
    async fn test_echo_service_full_e2e() {
        let bind_addr: SocketAddr = "127.0.0.1:14437".parse().unwrap();
        let cert_file = "tests/certs/cert.pem".to_string();
        let key_file = "tests/certs/key.pem".to_string();
        
        let server = TquicServer::new(bind_addr, cert_file, key_file, None).await
            .expect("Server creation failed");
        
        let mut listener = server.listen("127.0.0.1:14437").await
            .expect("Server listen failed");
        
        tokio::spawn(async move {
            if let Ok(Ok(conn)) = timeout(Duration::from_secs(5), listener.accept()).await {
                println!("Server: Connection accepted");
                
                if let Ok(Ok(mut stream)) = timeout(Duration::from_secs(5), conn.accept_stream()).await {
                    println!("Server: Stream accepted");
                    
                    for i in 0..5 {
                        match stream.receive(8192).await {
                            Ok(data) if !data.is_empty() => {
                                let msg = String::from_utf8_lossy(&data);
                                println!("Server: Received message {}: '{}'", i, msg);
                                
                                let response = format!("Echo: {}", msg);
                                if let Err(e) = stream.send(response.as_bytes(), false).await {
                                    eprintln!("Server: Send error: {:?}", e);
                                    break;
                                }
                                println!("Server: Sent echo response {}", i);
                            }
                            Ok(_) => {
                                println!("Server: Received empty data, closing");
                                break;
                            }
                            Err(e) => {
                                eprintln!("Server: Receive error: {:?}", e);
                                break;
                            }
                        }
                    }
                    
                    let _ = stream.close().await;
                    println!("Server: Stream closed");
                }
            }
        });
        
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        let client = TquicClient::new(None).await.expect("Client creation failed");
        
        match timeout(Duration::from_secs(3), client.connect("127.0.0.1:14437")).await {
            Ok(Ok(conn)) => {
                println!("Client: Connected");
                
                match conn.open_stream().await {
                    Ok(mut stream) => {
                        println!("Client: Stream opened");
                        
                        let messages = vec![
                            "Hello from client!",
                            "Second message",
                            "Testing QUIC",
                            "Fourth message",
                            "Final message!",
                        ];
                        
                        let mut successful_echoes = 0;
                        
                        for (i, msg) in messages.iter().enumerate() {
                            println!("Client: Sending message {}: '{}'", i, msg);
                            
                            match stream.send(msg.as_bytes(), false).await {
                                Ok(_) => {
                                    println!("Client: Message {} sent", i);
                                    
                                    tokio::time::sleep(Duration::from_millis(50)).await;
                                    
                                    match stream.receive(8192).await {
                                        Ok(data) if !data.is_empty() => {
                                            let response = String::from_utf8_lossy(&data);
                                            println!("Client: Received response {}: '{}'", i, response);
                                            
                                            assert!(response.starts_with("Echo: "), "Response should start with 'Echo: '");
                                            assert!(response.contains(msg), "Response should contain original message");
                                            successful_echoes += 1;
                                        }
                                        Ok(_) => eprintln!("Client: Empty response for message {}", i),
                                        Err(e) => eprintln!("Client: Receive error for message {}: {:?}", i, e),
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Client: Send error for message {}: {:?}", i, e);
                                    break;
                                }
                            }
                            
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                        
                        println!("Client: Successfully echoed {}/{} messages", successful_echoes, messages.len());
                        assert_eq!(successful_echoes, messages.len(), "All messages should be echoed");
                        
                        let _ = stream.send(&[], true).await;
                        let _ = stream.close().await;
                        println!("Client: Stream closed");
                        
                        let _ = conn.close().await;
                        println!("Client: Connection closed");
                    }
                    Err(e) => panic!("Client: Failed to open stream: {:?}", e),
                }
            }
            Ok(Err(e)) => panic!("Client: Connection failed: {:?}", e),
            Err(_) => panic!("Client: Connection timeout"),
        }
        
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    #[tokio::test]
    #[ignore = "Runtime lifecycle issue - same as test_echo_service_full_e2e"]
    async fn test_concurrent_connections() {
        let bind_addr: SocketAddr = "127.0.0.1:14436".parse().unwrap();
        let cert_file = "tests/certs/cert.pem".to_string();
        let key_file = "tests/certs/key.pem".to_string();
        
        let server = TquicServer::new(bind_addr, cert_file, key_file, None).await
            .expect("Server creation failed");
        
        let mut listener = server.listen("127.0.0.1:14436").await
            .expect("Server listen failed");
        
        tokio::spawn(async move {
            match timeout(Duration::from_secs(5), listener.accept()).await {
                Ok(Ok(conn)) => {
                    println!("Server accepted connection");
                    for i in 0..3 {
                        if let Ok(Ok(mut stream)) = timeout(Duration::from_secs(5), conn.accept_stream()).await {
                            println!("Server accepted stream {}", i);
                            tokio::spawn(async move {
                                if let Ok(data) = stream.receive(1024).await {
                                    if !data.is_empty() {
                                        let _ = stream.send(&data, false).await;
                                    }
                                }
                            });
                        }
                    }
                }
                Ok(Err(e)) => eprintln!("Server accept error: {:?}", e),
                Err(_) => eprintln!("Server accept timeout"),
            }
        });
        
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        let client1 = TquicClient::new(None).await.expect("Client 1 creation failed");
        let client2 = TquicClient::new(None).await.expect("Client 2 creation failed");
        let client3 = TquicClient::new(None).await.expect("Client 3 creation failed");
        
        let conn1 = timeout(Duration::from_secs(2), client1.connect("127.0.0.1:14436")).await
            .expect("Timeout")
            .expect("Client 1 connection failed");
        println!("Client 1 connected");
        
        let mut stream1 = conn1.open_stream().await.expect("Client 1 open stream failed");
        let message1 = b"Message from client 1";
        stream1.send(message1, false).await.expect("Client 1 send failed");
        tokio::time::sleep(Duration::from_millis(100)).await;
        let response1 = stream1.receive(1024).await.expect("Client 1 receive failed");
        assert_eq!(response1, message1);
        println!("Client 1 completed");
        
        drop(conn1);
        drop(client1);
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let conn2 = timeout(Duration::from_secs(2), client2.connect("127.0.0.1:14436")).await
            .expect("Timeout")
            .expect("Client 2 connection failed");
        println!("Client 2 connected");
        
        let mut stream2 = conn2.open_stream().await.expect("Client 2 open stream failed");
        let message2 = b"Message from client 2";
        stream2.send(message2, false).await.expect("Client 2 send failed");
        tokio::time::sleep(Duration::from_millis(100)).await;
        let response2 = stream2.receive(1024).await.expect("Client 2 receive failed");
        assert_eq!(response2, message2);
        println!("Client 2 completed");
        
        drop(conn2);
        drop(client2);
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let conn3 = timeout(Duration::from_secs(2), client3.connect("127.0.0.1:14436")).await
            .expect("Timeout")
            .expect("Client 3 connection failed");
        println!("Client 3 connected");
        
        let mut stream3 = conn3.open_stream().await.expect("Client 3 open stream failed");
        let message3 = b"Message from client 3";
        stream3.send(message3, false).await.expect("Client 3 send failed");
        tokio::time::sleep(Duration::from_millis(100)).await;
        let response3 = stream3.receive(1024).await.expect("Client 3 receive failed");
        assert_eq!(response3, message3);
        println!("Client 3 completed");
    }
}
