// High-performance protocol compatibility module for QUICCRUNCH benchmark
// This module provides serialization/deserialization for the benchmark protocol
// Currently using JSON (will be upgraded to Cap'n Proto in future optimization)

use crate::common::{
    SimpleRequest, SimpleResponse, BenchmarkRequest, BenchmarkResponse, ServerMetrics
};
use quiccrunch_core::rpc::{RpcError, RpcResult};
use bytes::Bytes;

pub struct BenchmarkProto;

impl BenchmarkProto {
    // SimpleRequest serialization/deserialization (JSON for now, Cap'n Proto later)
    pub fn serialize_simple_request(req: &SimpleRequest) -> RpcResult<Bytes> {
        let json = serde_json::to_string(req)
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        Ok(Bytes::from(json))
    }

    pub fn deserialize_simple_request(data: &[u8]) -> RpcResult<SimpleRequest> {
        serde_json::from_slice(data)
            .map_err(|e| RpcError::SerializationError(e.to_string()))
    }

    // SimpleResponse serialization/deserialization
    pub fn serialize_simple_response(resp: &SimpleResponse) -> RpcResult<Bytes> {
        let json = serde_json::to_string(resp)
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        Ok(Bytes::from(json))
    }

    pub fn deserialize_simple_response(data: &[u8]) -> RpcResult<SimpleResponse> {
        serde_json::from_slice(data)
            .map_err(|e| RpcError::SerializationError(e.to_string()))
    }

    // BenchmarkRequest serialization/deserialization
    pub fn serialize_benchmark_request(req: &BenchmarkRequest) -> RpcResult<Bytes> {
        let json = serde_json::to_string(req)
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        Ok(Bytes::from(json))
    }

    pub fn deserialize_benchmark_request(data: &[u8]) -> RpcResult<BenchmarkRequest> {
        serde_json::from_slice(data)
            .map_err(|e| RpcError::SerializationError(e.to_string()))
    }

    // BenchmarkResponse serialization/deserialization
    pub fn serialize_benchmark_response(resp: &BenchmarkResponse) -> RpcResult<Bytes> {
        let json = serde_json::to_string(resp)
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        Ok(Bytes::from(json))
    }

    pub fn deserialize_benchmark_response(data: &[u8]) -> RpcResult<BenchmarkResponse> {
        serde_json::from_slice(data)
            .map_err(|e| RpcError::SerializationError(e.to_string()))
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