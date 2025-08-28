use quiccrunch_core::{QuicTransport, Transport, TlsConfig};
use std::error::Error;
use opentelemetry_sdk::{trace::config, Resource};
use opentelemetry::KeyValue;
use tracing::{info, info_span};
use tracing_subscriber::prelude::*;
use tracing_opentelemetry::OpenTelemetryLayer;

#[path = "cert_utils.rs"]
mod cert_utils;

fn init_telemetry() -> Result<opentelemetry_sdk::trace::Tracer, Box<dyn Error>> {
    let tracer = opentelemetry_jaeger::new_agent_pipeline()
        .with_service_name("quiccrunch-example")
        .with_trace_config(
            config().with_resource(Resource::new(vec![KeyValue::new(
                "service.name",
                "quiccrunch-example",
            )]))
        )
        .install_simple()?;

    Ok(tracer)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize telemetry
    let tracer = init_telemetry()?;
    
    // Set up tracing subscriber
    let telemetry = OpenTelemetryLayer::new(tracer);
    tracing_subscriber::registry()
        .with(telemetry)
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Main span for our example
    let main_span = info_span!("example_run");
    let _guard = main_span.enter();

    info!("Starting QuicCrunch example with tracing");

    // Generate TLS certificates (in production you'd load real certs)
    let (ca_cert, _) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert, "localhost")?;
    
    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };

    // Create transport
    let transport = QuicTransport::new(tls_config)?;
    
    // Server
    let server_span = info_span!("server_setup");
    let _guard = server_span.enter();
    
    let mut incoming = transport.listen("127.0.0.1:50051").await?;
    info!("Server listening on port 50051");
    
    drop(_guard);
    
    // Client
    let client_span = info_span!("client_connect");
    let _guard = client_span.enter();
    
    let client = transport.connect("127.0.0.1:50051").await?;
    let mut stream = client.open_bi().await?;
    info!("Client connected successfully");
    
    drop(_guard);
    
    // Send message
    let message_span = info_span!("message_exchange");
    let _guard = message_span.enter();
    
    let message = b"Hello, World!";
    stream.send_message(bytes::Bytes::from(&message[..])).await?;
    info!("Client sent message");
    
    // Receive on server
    let conn = incoming.accept().await?;
    let mut server_stream = conn.open_bi().await?;
    let received = server_stream.receive_message().await?;
    
    assert_eq!(&received[..], message);
    info!(message = ?std::str::from_utf8(&received)?, "Server received message");
    
    // Ensure all spans are exported
    opentelemetry::global::shutdown_tracer_provider();
    
    Ok(())
}