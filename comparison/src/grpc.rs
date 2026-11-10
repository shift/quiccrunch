//! tonic (HTTP/2 over TLS on TCP) backend for the comparison harness.

use anyhow::{Context, Result};
use bytes::Bytes;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Certificate, Channel, ClientTlsConfig};

use crate::bench::{Backend, ClientConnection, ClientStream, RecvBatch};
use crate::pb;

/// Per-stream application -> tonic queue. This mirrors QuicCrunch's bounded
/// per-runtime command queue: messages are admitted here and the transport
/// runtime drains them asynchronously.
const OUTBOUND_QUEUE: usize = 2048;

pub struct GrpcBackend {
    ca_pem: Vec<u8>,
    domain: String,
}

impl GrpcBackend {
    pub fn new(ca_pem: Vec<u8>, domain: String) -> Self {
        Self { ca_pem, domain }
    }
}

#[async_trait::async_trait]
impl Backend for GrpcBackend {
    fn name(&self) -> &'static str {
        "tonic-h2"
    }

    fn notes(&self) -> String {
        format!(
            "h2 over TCP, rustls TLS1.3/1.2, ALPN h2, initial_stream_window=16MiB, initial_conn_window=128MiB, outbound_queue={OUTBOUND_QUEUE}"
        )
    }

    async fn connect(&self, _conn_index: usize, target: &str) -> Result<Box<dyn ClientConnection>> {
        let uri = if target.starts_with("https://") {
            target.to_string()
        } else {
            format!("https://{}", target)
        };
        let tls = ClientTlsConfig::new()
            .ca_certificate(Certificate::from_pem(self.ca_pem.clone()))
            .domain_name(self.domain.clone());
        let channel = Channel::from_shared(uri)
            .context("invalid gRPC endpoint")?
            .tls_config(tls)?
            // Match QuicCrunch's benchmark flow-control windows so the pipeline
            // depth is transport-buffered, not application-throttled.
            .initial_stream_window_size(16 * 1024 * 1024)
            .initial_connection_window_size(128 * 1024 * 1024)
            .connect()
            .await
            .context("gRPC channel connect failed")?;
        Ok(Box::new(GrpcConnection { channel }))
    }
}

pub struct GrpcConnection {
    channel: Channel,
}

#[async_trait::async_trait]
impl ClientConnection for GrpcConnection {
    async fn open_stream(&self) -> Result<Box<dyn ClientStream>> {
        let mut client = pb::echo_client::EchoClient::new(self.channel.clone());
        let (tx, rx) = mpsc::channel::<pb::EchoRequest>(OUTBOUND_QUEUE);
        let response = client
            .echo(ReceiverStream::new(rx))
            .await
            .context("open bidi stream")?;
        Ok(Box::new(GrpcStream {
            tx,
            responses: response.into_inner(),
        }))
    }
}

pub struct GrpcStream {
    tx: mpsc::Sender<pb::EchoRequest>,
    responses: tonic::codec::Streaming<pb::EchoResponse>,
}

#[async_trait::async_trait]
impl ClientStream for GrpcStream {
    async fn send_batch(&mut self, payload: &Bytes, count: usize) -> Result<()> {
        for _ in 0..count {
            self.tx
                .send(pb::EchoRequest {
                    payload: payload.clone(),
                })
                .await
                .map_err(|_| anyhow::anyhow!("outbound stream closed"))?;
        }
        Ok(())
    }

    async fn recv(&mut self) -> Result<RecvBatch> {
        match self.responses.message().await {
            Ok(Some(resp)) => Ok(RecvBatch {
                messages: 1,
                bytes: resp.payload.len() as u64,
            }),
            Ok(None) => Ok(RecvBatch::default()),
            Err(e) => Err(anyhow::anyhow!("grpc recv: {}", e)),
        }
    }
}

/// Tonic server-side service used by `grpc-server`.
pub mod server {
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use futures::Stream;
    use tonic::{Request, Response, Status};

    use crate::pb;

    #[derive(Default)]
    pub struct EchoService;

    /// Echo responses are produced by mapping the inbound message stream
    /// directly. No per-message task or channel hop: the server cost is the
    /// protobuf decode/encode plus the transport, nothing else.
    pub struct EchoResponseStream {
        inbound: tonic::Streaming<pb::EchoRequest>,
    }

    impl Stream for EchoResponseStream {
        type Item = Result<pb::EchoResponse, Status>;

        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            match Pin::new(&mut self.inbound).poll_next(cx) {
                Poll::Ready(Some(Ok(req))) => Poll::Ready(Some(Ok(pb::EchoResponse {
                    payload: req.payload,
                }))),
                Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    #[tonic::async_trait]
    impl pb::echo_server::Echo for EchoService {
        type EchoStream = EchoResponseStream;

        async fn echo(
            &self,
            request: Request<tonic::Streaming<pb::EchoRequest>>,
        ) -> Result<Response<Self::EchoStream>, Status> {
            Ok(Response::new(EchoResponseStream {
                inbound: request.into_inner(),
            }))
        }
    }
}

pub fn make_server() -> pb::echo_server::EchoServer<server::EchoService> {
    pb::echo_server::EchoServer::new(server::EchoService)
}

pub fn tls_identity(
    cert_pem: Vec<u8>,
    key_pem: Vec<u8>,
) -> Result<tonic::transport::ServerTlsConfig> {
    Ok(tonic::transport::ServerTlsConfig::new()
        .identity(tonic::transport::Identity::from_pem(cert_pem, key_pem)))
}

pub fn server_builder() -> tonic::transport::Server {
    use tonic::transport::Server;
    Server::builder()
        .initial_stream_window_size(16 * 1024 * 1024)
        .initial_connection_window_size(128 * 1024 * 1024)
}
