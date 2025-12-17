// High-performance binary serialization using Cap'n Proto
// This replaces the slow JSON serialization with efficient binary format

use crate::common::{
    SimpleRequest, SimpleResponse, BenchmarkRequest, BenchmarkResponse, ServerMetrics
};
use quiccrunch_core::rpc::{RpcError, RpcResult};
use bytes::Bytes;
use capnp::serialize;
use capnp::message::{self, TypedReader, Builder, ReaderOptions};

// Cap'n Proto schema structures for benchmark messages
pub mod benchmark_capnp {
    include!(concat!(env!("OUT_DIR"), "/benchmark_capnp.rs"));
}

pub struct BenchmarkProto;

impl BenchmarkProto {
    // SimpleRequest serialization/deserialization with Cap'n Proto
    pub fn serialize_simple_request(req: &SimpleRequest) -> RpcResult<Bytes> {
        let mut message = Builder::new_default();
        let mut request = message.init_root::<benchmark_capnp::simple_request::Builder>();
        
        request.set_id(&req.id);
        request.set_timestamp(req.timestamp);
        request.set_test_type(&req.test_type);
        request.set_sequence_number(req.sequence_number);
        request.set_payload(&req.payload);

        let mut buffer = Vec::new();
        serialize::write_message(&mut buffer, &message)
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        
        Ok(Bytes::from(buffer))
    }

    pub fn deserialize_simple_request(data: &[u8]) -> RpcResult<SimpleRequest> {
        let reader = serialize::read_message(data, ReaderOptions::new())
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        
        let request = reader.get_root::<benchmark_capnp::simple_request::Reader>()
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;

        Ok(SimpleRequest {
            id: request.get_id().map_err(|e| RpcError::SerializationError(e.to_string()))?.to_string(),
            timestamp: request.get_timestamp(),
            payload: request.get_payload().map_err(|e| RpcError::SerializationError(e.to_string()))?.to_vec(),
            test_type: request.get_test_type().map_err(|e| RpcError::SerializationError(e.to_string()))?.to_string(),
            sequence_number: request.get_sequence_number(),
        })
    }

    // SimpleResponse serialization/deserialization with Cap'n Proto
    pub fn serialize_simple_response(resp: &SimpleResponse) -> RpcResult<Bytes> {
        let mut message = Builder::new_default();
        let mut response = message.init_root::<benchmark_capnp::simple_response::Builder>();
        
        response.set_id(&resp.id);
        response.set_timestamp(resp.timestamp);
        response.set_status(&resp.status);
        response.set_payload(&resp.payload);

        if let Some(metrics) = &resp.metrics {
            let mut server_metrics = response.init_metrics();
            server_metrics.set_concurrent_connections(metrics.concurrent_connections);
            server_metrics.set_total_messages(metrics.total_messages);
            server_metrics.set_cpu_usage(metrics.cpu_usage);
            server_metrics.set_memory_usage(metrics.memory_usage);
        }

        let mut buffer = Vec::new();
        serialize::write_message(&mut buffer, &message)
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        
        Ok(Bytes::from(buffer))
    }

    pub fn deserialize_simple_response(data: &[u8]) -> RpcResult<SimpleResponse> {
        let reader = serialize::read_message(data, ReaderOptions::new())
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        
        let response = reader.get_root::<benchmark_capnp::simple_response::Reader>()
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;

        let mut result = SimpleResponse {
            id: response.get_id().map_err(|e| RpcError::SerializationError(e.to_string()))?.to_string(),
            timestamp: response.get_timestamp(),
            payload: response.get_payload().map_err(|e| RpcError::SerializationError(e.to_string()))?.to_vec(),
            status: response.get_status().map_err(|e| RpcError::SerializationError(e.to_string()))?.to_string(),
            metrics: None,
        };

        if response.has_metrics() {
            let metrics = response.get_metrics().map_err(|e| RpcError::SerializationError(e.to_string()))?;
            result.metrics = Some(ServerMetrics {
                concurrent_connections: metrics.get_concurrent_connections(),
                total_messages: metrics.get_total_messages(),
                cpu_usage: metrics.get_cpu_usage(),
                memory_usage: metrics.get_memory_usage(),
            });
        }

        Ok(result)
    }

    // BenchmarkRequest serialization/deserialization with Cap'n Proto
    pub fn serialize_benchmark_request(req: &BenchmarkRequest) -> RpcResult<Bytes> {
        let mut message = Builder::new_default();
        let mut request = message.init_root::<benchmark_capnp::benchmark_request::Builder>();
        
        request.set_id(&req.id);
        request.set_timestamp(req.timestamp);
        request.set_test_type(&req.test_type);
        request.set_sequence_number(req.sequence_number);
        request.set_payload(&req.payload);

        let mut buffer = Vec::new();
        serialize::write_message(&mut buffer, &message)
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        
        Ok(Bytes::from(buffer))
    }

    pub fn deserialize_benchmark_request(data: &[u8]) -> RpcResult<BenchmarkRequest> {
        let reader = serialize::read_message(data, ReaderOptions::new())
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        
        let request = reader.get_root::<benchmark_capnp::benchmark_request::Reader>()
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;

        Ok(BenchmarkRequest {
            id: request.get_id().map_err(|e| RpcError::SerializationError(e.to_string()))?.to_string(),
            timestamp: request.get_timestamp(),
            payload: request.get_payload().map_err(|e| RpcError::SerializationError(e.to_string()))?.to_vec(),
            test_type: request.get_test_type().map_err(|e| RpcError::SerializationError(e.to_string()))?.to_string(),
            sequence_number: request.get_sequence_number(),
        })
    }

    // BenchmarkResponse serialization/deserialization with Cap'n Proto
    pub fn serialize_benchmark_response(resp: &BenchmarkResponse) -> RpcResult<Bytes> {
        let mut message = Builder::new_default();
        let mut response = message.init_root::<benchmark_capnp::benchmark_response::Builder>();
        
        response.set_id(&resp.id);
        response.set_timestamp(resp.timestamp);
        response.set_status(&resp.status);
        response.set_payload(&resp.payload);

        if let Some(metrics) = &resp.metrics {
            let mut server_metrics = response.init_metrics();
            server_metrics.set_concurrent_connections(metrics.concurrent_connections);
            server_metrics.set_total_messages(metrics.total_messages);
            server_metrics.set_cpu_usage(metrics.cpu_usage);
            server_metrics.set_memory_usage(metrics.memory_usage);
        }

        let mut buffer = Vec::new();
        serialize::write_message(&mut buffer, &message)
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        
        Ok(Bytes::from(buffer))
    }

    pub fn deserialize_benchmark_response(data: &[u8]) -> RpcResult<BenchmarkResponse> {
        let reader = serialize::read_message(data, ReaderOptions::new())
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        
        let response = reader.get_root::<benchmark_capnp::benchmark_response::Reader>()
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;

        let mut result = BenchmarkResponse {
            id: response.get_id().map_err(|e| RpcError::SerializationError(e.to_string()))?.to_string(),
            timestamp: response.get_timestamp(),
            payload: response.get_payload().map_err(|e| RpcError::SerializationError(e.to_string()))?.to_vec(),
            status: response.get_status().map_err(|e| RpcError::SerializationError(e.to_string()))?.to_string(),
            metrics: None,
        };

        if response.has_metrics() {
            let metrics = response.get_metrics().map_err(|e| RpcError::SerializationError(e.to_string()))?;
            result.metrics = Some(ServerMetrics {
                concurrent_connections: metrics.get_concurrent_connections(),
                total_messages: metrics.get_total_messages(),
                cpu_usage: metrics.get_cpu_usage(),
                memory_usage: metrics.get_memory_usage(),
            });
        }

        Ok(result)
    }
}

// Service method names matching the gRPC benchmark service
pub struct BenchmarkMethods;
impl BenchmarkMethods {
    pub const UNARY_CALL: &'static str = "UnaryCall";
    pub const CLIENT_STREAMING: &'static str = "ClientStreaming";
    pub const SERVER_STREAMING: &'static str = "ServerStreaming";
    pub const BIDIRECTIONAL_STREAMING: &'static str = "BidirectionalStreaming";
    pub const LATENCY_TEST: &'static str = "LatencyTest";
    pub const FLOOD_TEST: &'static str = "FloodTest";
    pub const STREAM_TEST: &'static str = "StreamTest";
}