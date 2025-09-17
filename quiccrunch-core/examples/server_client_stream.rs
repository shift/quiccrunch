//! Example: Server→Client Streaming RPC using tQUIC and Cap'n Proto

use capnp::message::{Builder, ReaderOptions};
use capnp::serialize::{read_message, write_message};
use quiccrunch_core::schema::rpc_capnp::{
    rpc_message, rpc_request, rpc_stream_chunk, streaming_type,
};
use quiccrunch_core::transport::tquic::{AsyncTquicClient, AsyncTquicServer};
use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Server→Client Streaming RPC Example");
    println!("====================================");

    // Start server in background
    let server_addr = "127.0.0.1:4445";
    tokio::spawn(async move {
        let server = AsyncTquicServer::new().await.unwrap();
        let mut listener = server.listen(server_addr).await.unwrap();
        println!("[Server] Listening on {}", server_addr);
        let mut stream = listener.accept().await.unwrap();
        println!("[Server] Accepted connection");
        // Read request
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let mut cursor = std::io::Cursor::new(&buf[..n]);
        let message_reader = read_message(&mut cursor, ReaderOptions::new()).unwrap();
        let rpc_msg = message_reader.get_root::<rpc_message::Reader>().unwrap();
        let req = rpc_msg.get_request().unwrap();
        println!(
            "[Server] Received request: service={} method={}",
            req.get_service().unwrap(),
            req.get_method().unwrap()
        );
        // Send multiple stream chunks
        for i in 0..3 {
            let mut builder = Builder::new_default();
            {
                let mut msg = builder.init_root::<rpc_message::Builder>();
                msg.set_message_type(rpc_message::MessageType::StreamChunk);
                msg.set_request_id(rpc_msg.get_request_id());
                let mut chunk = msg.init_stream_chunk();
                chunk.set_stream_id(1);
                chunk.set_sequence_id(i);
                chunk.set_data(format!("Chunk {} from server", i).as_bytes());
                chunk.set_is_last(i == 2);
            }
            let mut chunk_buf = Vec::new();
            write_message(&mut chunk_buf, &builder).unwrap();
            stream.write_all(&chunk_buf).await.unwrap();
            println!("[Server] Sent chunk {}", i);
        }
    });

    // Give server time to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Client connects and sends request
    let client = AsyncTquicClient::new().await?;
    let mut stream = client.connect(server_addr).await?;
    println!("[Client] Connected to server");
    // Build request
    let mut builder = Builder::new_default();
    {
        let mut msg = builder.init_root::<rpc_message::Builder>();
        msg.set_message_type(rpc_message::MessageType::Request);
        msg.set_request_id(43);
        let mut req = msg.init_request();
        req.set_service("StreamService");
        req.set_method("GetStream");
        req.set_parameters(b"Start stream".to_vec().as_slice());
        req.set_streaming_type(streaming_type::StreamingType::ServerStreaming);
    }
    let mut req_buf = Vec::new();
    write_message(&mut req_buf, &builder).unwrap();
    stream.write_all(&req_buf).await?;
    println!("[Client] Sent request");
    // Read stream chunks
    for _ in 0..3 {
        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await?;
        let mut cursor = std::io::Cursor::new(&buf[..n]);
        let message_reader = read_message(&mut cursor, ReaderOptions::new()).unwrap();
        let rpc_msg = message_reader.get_root::<rpc_message::Reader>().unwrap();
        let chunk = rpc_msg.get_stream_chunk().unwrap();
        println!(
            "[Client] Received chunk {}: {}",
            chunk.get_sequence_id(),
            String::from_utf8_lossy(chunk.get_data().unwrap())
        );
        if chunk.get_is_last() {
            break;
        }
    }

    Ok(())
}
