//! Example: Client→Server Unary RPC using tQUIC and Cap'n Proto

use quiccrunch_core::transport::tquic::{AsyncTquicClient, AsyncTquicServer};
use capnp::message::{Builder, ReaderOptions};
use capnp::serialize::{write_message, read_message};
use quiccrunch_core::schema::rpc_capnp::{rpc_message, rpc_request, streaming_type};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Client→Server Unary RPC Example");
    println!("================================");

    // Start server in background
    let server_addr = "127.0.0.1:4444";
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
        println!("[Server] Received request: service={} method={}", req.get_service().unwrap(), req.get_method().unwrap());
        // Build response
        let mut builder = Builder::new_default();
        {
            let mut msg = builder.init_root::<rpc_message::Builder>();
            msg.set_message_type(rpc_message::MessageType::Response);
            msg.set_request_id(rpc_msg.get_request_id());
            let mut resp = msg.init_response();
            resp.set_result(b"Hello from server!".to_vec().as_slice());
        }
        let mut resp_buf = Vec::new();
        write_message(&mut resp_buf, &builder).unwrap();
        stream.write_all(&resp_buf).await.unwrap();
        println!("[Server] Sent response");
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
        msg.set_request_id(42);
        let mut req = msg.init_request();
        req.set_service("EchoService");
        req.set_method("Echo");
        req.set_parameters(b"Hello from client!".to_vec().as_slice());
        req.set_streaming_type(streaming_type::StreamingType::Unary);
    }
    let mut req_buf = Vec::new();
    write_message(&mut req_buf, &builder).unwrap();
    stream.write_all(&req_buf).await?;
    println!("[Client] Sent request");
    // Read response
    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await?;
    let mut cursor = std::io::Cursor::new(&buf[..n]);
    let message_reader = read_message(&mut cursor, ReaderOptions::new()).unwrap();
    let rpc_msg = message_reader.get_root::<rpc_message::Reader>().unwrap();
    let resp = rpc_msg.get_response().unwrap();
    println!("[Client] Received response: {}", String::from_utf8_lossy(resp.get_result().unwrap()));

    Ok(())
}
