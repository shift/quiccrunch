use quiccrunch_core::transport::adapter::{TquicClient, TquicServer};
use std::time::Duration;
use tokio::time::timeout;

const TEST_SERVER_ADDR: &str = "127.0.0.1:5551";
const TEST_SERVER_ADDR_2: &str = "127.0.0.1:5552";
const TEST_SERVER_ADDR_3: &str = "127.0.0.1:5553";

#[tokio::test]
async fn test_tquic_basic_send_receive() -> Result<(), Box<dyn std::error::Error>> {
    let server = TquicServer::new(Some(2)).await?;
    let mut listener = server.listen(TEST_SERVER_ADDR).await?;

    let server_handle = tokio::spawn(async move {
        let connection = listener.accept().await.unwrap();
        let mut stream = connection.accept_stream().await.unwrap();
        
        let data = loop {
            let data = stream.receive(1024).await.unwrap();
            if !data.is_empty() {
                break data;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        assert_eq!(&data, b"Hello from client");
        
        stream.send(b"Hello from server", false).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let client = TquicClient::new(Some(1)).await?;
    let connection = client.connect(TEST_SERVER_ADDR).await?;

    let mut stream = connection.open_stream().await?;
    stream.send(b"Hello from client", false).await?;

    let data = timeout(Duration::from_secs(5), async {
        loop {
            let data = stream.receive(1024).await.unwrap();
            if !data.is_empty() {
                return data;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await?;
    assert_eq!(&data, b"Hello from server");

    server_handle.await?;

    Ok(())
}

#[tokio::test]
async fn test_tquic_multiple_messages() -> Result<(), Box<dyn std::error::Error>> {
    let server = TquicServer::new(Some(2)).await?;
    let mut listener = server.listen(TEST_SERVER_ADDR_2).await?;

    let server_handle = tokio::spawn(async move {
        let connection = listener.accept().await.unwrap();
        let mut stream = connection.accept_stream().await.unwrap();
        
        for i in 0..10 {
            let data = loop {
                let data = stream.receive(1024).await.unwrap();
                if !data.is_empty() {
                    break data;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            };
            let expected = format!("Message {}", i);
            assert_eq!(&data, expected.as_bytes());
            
            let response = format!("Response {}", i);
            stream.send(response.as_bytes(), false).await.unwrap();
        }
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let client = TquicClient::new(Some(1)).await?;
    let connection = client.connect(TEST_SERVER_ADDR_2).await?;

    let mut stream = connection.open_stream().await?;

    for i in 0..10 {
        let message = format!("Message {}", i);
        stream.send(message.as_bytes(), false).await?;

        let data = timeout(Duration::from_secs(5), async {
            loop {
                let data = stream.receive(1024).await.unwrap();
                if !data.is_empty() {
                    return data;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }).await?;
        let expected = format!("Response {}", i);
        assert_eq!(&data, expected.as_bytes());
    }

    server_handle.await?;

    Ok(())
}

#[tokio::test]
async fn test_tquic_large_payload() -> Result<(), Box<dyn std::error::Error>> {
    let server = TquicServer::new(Some(2)).await?;
    let mut listener = server.listen(TEST_SERVER_ADDR_3).await?;

    let large_data = vec![0xABu8; 1024 * 100];
    let large_data_clone = large_data.clone();

    let server_handle = tokio::spawn(async move {
        let connection = listener.accept().await.unwrap();
        let mut stream = connection.accept_stream().await.unwrap();
        
        let mut received = Vec::new();
        let mut empty_count = 0;
        loop {
            let data = stream.receive(8192).await.unwrap();
            if data.is_empty() {
                empty_count += 1;
                if empty_count > 100 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
                continue;
            }
            empty_count = 0;
            received.extend_from_slice(&data);
            if received.len() >= large_data_clone.len() {
                break;
            }
        }
        
        assert_eq!(received.len(), large_data_clone.len());
        assert_eq!(received, large_data_clone);
        
        stream.send(b"ACK", false).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let client = TquicClient::new(Some(1)).await?;
    let connection = client.connect(TEST_SERVER_ADDR_3).await?;

    let mut stream = connection.open_stream().await?;
    stream.send(&large_data, false).await?;

    let data = timeout(Duration::from_secs(10), async {
        loop {
            let data = stream.receive(1024).await.unwrap();
            if !data.is_empty() {
                return data;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await?;
    assert_eq!(&data, b"ACK");

    server_handle.await?;

    Ok(())
}
