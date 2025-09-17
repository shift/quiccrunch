//! Example: Bidirectional Streaming RPC using QuicCrunch high-level abstractions

use quiccrunch_core::rpc::{RpcClient, RpcResult, RpcServer, RpcStreamSink, RpcStreamSource};
use quiccrunch_core::{QuicTransport, TlsConfig};
use quiccrunch_macros::rpc_service;
use std::sync::Arc;
use tokio::time::Duration;

#[rpc_service(name = "BidiService")]
trait BidiService {
    // Bidirectional streaming: stream of requests, stream of responses
    async fn bidi_stream(
        &self,
        input: RpcStreamSource<String>,
    ) -> RpcResult<RpcStreamSource<String>>;
}

struct BidiServiceImpl;

#[async_trait::async_trait]
impl BidiService for BidiServiceImpl {
    async fn bidi_stream(
        &self,
        mut input: RpcStreamSource<String>,
    ) -> RpcResult<RpcStreamSource<String>> {
        let (mut sink, source) = RpcStreamSink::create();
        tokio::spawn(async move {
            let mut i = 0;
            while let Some(chunk) = input.next().await {
                if let Ok(text) = chunk {
                    println!("[Server] Received chunk: {}", text);
                    let resp = format!("Server response chunk {}", i);
                    let _ = sink.send(resp).await;
                    i += 1;
                }
            }
            let _ = sink.finish().await;
        });
        Ok(source)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Bidirectional Streaming RPC Example");
    println!("====================================");

    // Start server in background
    let server_addr = "127.0.0.1:4446";
    tokio::spawn(async move {
        let tls_config = TlsConfig::generate_self_signed("localhost").unwrap();
        let transport = QuicTransport::new(tls_config).unwrap();
        let server = RpcServer::new();
        let service = Arc::new(BidiServiceImpl);
        server
            .register_streaming_service("BidiService".to_string(), service)
            .await;
        server.serve(&transport, server_addr).await.unwrap();
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Client connects and sends request
    let tls_config = TlsConfig::load_ca_cert("ca.crt")
        .unwrap_or_else(|_| TlsConfig::generate_self_signed("localhost").unwrap());
    let transport = QuicTransport::new(tls_config)?;
    let connection = transport.connect(server_addr).await?;
    let client = RpcClient::new(connection);
    let bidi_client = BidiServiceClient::new(client);

    // Create a stream of client chunks
    let (mut sink, source) = RpcStreamSink::create();
    tokio::spawn(async move {
        for i in 0..2 {
            let chunk = format!("Client chunk {}", i);
            println!("[Client] Sent chunk {}", i);
            let _ = sink.send(chunk).await;
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let _ = sink.finish().await;
    });

    // Call the bidirectional streaming method
    let mut resp_stream = bidi_client.bidi_stream(source).await?;
    let mut i = 0;
    while let Some(resp) = resp_stream.next().await {
        let text = resp?;
        println!("[Client] Received response chunk {}: {}", i, text);
        i += 1;
    }

    Ok(())
}
