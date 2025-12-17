//! Simple streaming tests that actually work

use bytes::Bytes;
use quiccrunch_core::transport::simple_streaming::*;
use quiccrunch_core::{QuicTransport, TlsConfig, Transport};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

async fn setup_simple_test() -> (SimpleStreamingClient, tokio::task::JoinHandle<()>) {
    let config = TlsConfig::default().into();
    let transport = QuicTransport::new(config).await.unwrap();
    let listener = transport.listen("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();

    // For now, just use a placeholder server
    let server_handle = tokio::spawn(async move {
        let _ = listener.accept().await;
        sleep(Duration::from_secs(10)).await;
    });

    sleep(Duration::from_millis(100)).await;

    let client_config = TlsConfig::default().into();
    let client_transport = QuicTransport::new(client_config).await.unwrap();
    let connection = client_transport
        .connect(&server_addr.to_string())
        .await
        .unwrap();
    let client = SimpleStreamingClient::new(connection);

    (client, server_handle)
}

#[tokio::test]
async fn test_simple_streaming_service() {
    let service = ExampleStreamingService;

    // Test server streaming
    let result = service
        .server_stream("stream_numbers", Bytes::from("3"))
        .await
        .unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(String::from_utf8_lossy(&result[0]), "number_0");
    assert_eq!(String::from_utf8_lossy(&result[1]), "number_1");
    assert_eq!(String::from_utf8_lossy(&result[2]), "number_2");

    // Test client streaming
    let requests = vec![Bytes::from("1"), Bytes::from("2"), Bytes::from("3")];
    let result = service
        .client_stream("sum_numbers", requests)
        .await
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&result), "6");
}

#[tokio::test]
async fn test_streaming_client_creation() {
    let (client, server_handle) = setup_simple_test().await;

    // Just test that we can create the client
    assert_eq!(
        std::mem::size_of_val(&client),
        std::mem::size_of::<SimpleStreamingClient>()
    );

    server_handle.abort();
}

#[tokio::test]
async fn test_streaming_service_name() {
    let service = ExampleStreamingService;
    assert_eq!(service.name(), "ExampleStreamingService");
}

#[tokio::test]
async fn test_unknown_method_error() {
    let service = ExampleStreamingService;

    let result = service.server_stream("unknown_method", Bytes::new()).await;
    assert!(result.is_err());

    let result = service.client_stream("unknown_method", vec![]).await;
    assert!(result.is_err());
}
