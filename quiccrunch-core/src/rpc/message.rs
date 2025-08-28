//! RPC Message Serialization and Deserialization
//!
//! Handles Cap'n Proto serialization for RPC messages.

use bytes::{Bytes, BytesMut, BufMut};
use capnp::message::{Builder, HeapAllocator, Reader, ReaderOptions};
use capnp::serialize;
use std::io::Cursor;

use crate::rpc::error::{RpcResult, RpcError};
use crate::rpc::service::{RequestId, ServiceId, MethodId, TraceContext};

/// RPC request message
#[derive(Debug, Clone)]
pub struct RpcRequest {
    pub request_id: RequestId,
    pub service_id: ServiceId,
    pub method_id: MethodId,
    pub payload: Bytes,
    pub trace_context: Option<TraceContext>,
    pub timeout_ms: u32,
}

/// RPC response message
#[derive(Debug, Clone)]
pub enum RpcResponse {
    Success {
        request_id: RequestId,
        payload: Bytes,
        trace_context: Option<TraceContext>,
    },
    Error {
        request_id: RequestId,
        error_code: u16,
        error_message: String,
        trace_context: Option<TraceContext>,
    },
}

impl RpcRequest {
    pub fn new(
        request_id: RequestId,
        service_id: ServiceId,
        method_id: MethodId,
        payload: Bytes,
    ) -> Self {
        Self {
            request_id,
            service_id,
            method_id,
            payload,
            trace_context: None,
            timeout_ms: 30000,
        }
    }

    pub fn with_trace_context(mut self, trace_context: TraceContext) -> Self {
        self.trace_context = Some(trace_context);
        self
    }

    pub fn with_timeout(mut self, timeout_ms: u32) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Serialize to Cap'n Proto bytes
    pub fn serialize(&self) -> RpcResult<Bytes> {
        let mut message = Builder::new_default();
        
        // Build the request using the generated schema
        // This would use the generated code from rpc.capnp
        // For now, we'll use a simple binary format as a placeholder
        
        let mut buffer = BytesMut::new();
        
        // Header: message type (1 byte) + request_id (8 bytes) + service_id (4 bytes) + method_id (4 bytes) + timeout (4 bytes)
        buffer.put_u8(0x01); // Request message type
        buffer.put_u64(self.request_id);
        buffer.put_u32(self.service_id);
        buffer.put_u32(self.method_id);
        buffer.put_u32(self.timeout_ms);
        
        // Trace context (if present)
        if let Some(ref trace_ctx) = self.trace_context {
            buffer.put_u8(0x01); // Has trace context
            buffer.put_u16(trace_ctx.trace_id.len() as u16);
            buffer.extend_from_slice(&trace_ctx.trace_id);
            buffer.put_u64(trace_ctx.span_id);
            buffer.put_u8(trace_ctx.flags);
            buffer.put_u16(trace_ctx.state.len() as u16);
            buffer.extend_from_slice(trace_ctx.state.as_bytes());
        } else {
            buffer.put_u8(0x00); // No trace context
        }
        
        // Payload length and payload
        buffer.put_u32(self.payload.len() as u32);
        buffer.extend_from_slice(&self.payload);
        
        Ok(buffer.freeze())
    }

    /// Deserialize from Cap'n Proto bytes
    pub fn deserialize(data: Bytes) -> RpcResult<Self> {
        let mut cursor = Cursor::new(data);
        
        // Read header
        let msg_type = read_u8(&mut cursor)?;
        if msg_type != 0x01 {
            return Err(RpcError::Deserialization("Invalid message type".to_string()));
        }
        
        let request_id = read_u64(&mut cursor)?;
        let service_id = read_u32(&mut cursor)?;
        let method_id = read_u32(&mut cursor)?;
        let timeout_ms = read_u32(&mut cursor)?;
        
        // Read trace context
        let has_trace = read_u8(&mut cursor)? != 0;
        let trace_context = if has_trace {
            let trace_id_len = read_u16(&mut cursor)? as usize;
            let mut trace_id = vec![0u8; trace_id_len];
            read_exact(&mut cursor, &mut trace_id)?;
            
            let span_id = read_u64(&mut cursor)?;
            let flags = read_u8(&mut cursor)?;
            
            let state_len = read_u16(&mut cursor)? as usize;
            let mut state_bytes = vec![0u8; state_len];
            read_exact(&mut cursor, &mut state_bytes)?;
            let state = String::from_utf8(state_bytes)
                .map_err(|e| RpcError::Deserialization(format!("Invalid UTF-8 in trace state: {}", e)))?;
            
            Some(TraceContext {
                trace_id,
                span_id,
                flags,
                state,
            })
        } else {
            None
        };
        
        // Read payload
        let payload_len = read_u32(&mut cursor)? as usize;
        let mut payload_bytes = vec![0u8; payload_len];
        read_exact(&mut cursor, &mut payload_bytes)?;
        let payload = Bytes::from(payload_bytes);
        
        Ok(Self {
            request_id,
            service_id,
            method_id,
            payload,
            trace_context,
            timeout_ms,
        })
    }
}

impl RpcResponse {
    pub fn success(request_id: RequestId, payload: Bytes) -> Self {
        Self::Success {
            request_id,
            payload,
            trace_context: None,
        }
    }

    pub fn error(request_id: RequestId, error_code: u16, error_message: String) -> Self {
        Self::Error {
            request_id,
            error_code,
            error_message,
            trace_context: None,
        }
    }

    pub fn with_trace_context(self, trace_context: TraceContext) -> Self {
        match self {
            Self::Success { request_id, payload, .. } => Self::Success {
                request_id,
                payload,
                trace_context: Some(trace_context),
            },
            Self::Error { request_id, error_code, error_message, .. } => Self::Error {
                request_id,
                error_code,
                error_message,
                trace_context: Some(trace_context),
            },
        }
    }

    pub fn request_id(&self) -> RequestId {
        match self {
            Self::Success { request_id, .. } => *request_id,
            Self::Error { request_id, .. } => *request_id,
        }
    }

    /// Serialize to Cap'n Proto bytes
    pub fn serialize(&self) -> RpcResult<Bytes> {
        let mut buffer = BytesMut::new();
        
        match self {
            Self::Success { request_id, payload, trace_context } => {
                buffer.put_u8(0x02); // Success response type
                buffer.put_u64(*request_id);
                
                // Trace context (if present)
                if let Some(ref trace_ctx) = trace_context {
                    buffer.put_u8(0x01); // Has trace context
                    buffer.put_u16(trace_ctx.trace_id.len() as u16);
                    buffer.extend_from_slice(&trace_ctx.trace_id);
                    buffer.put_u64(trace_ctx.span_id);
                    buffer.put_u8(trace_ctx.flags);
                    buffer.put_u16(trace_ctx.state.len() as u16);
                    buffer.extend_from_slice(trace_ctx.state.as_bytes());
                } else {
                    buffer.put_u8(0x00); // No trace context
                }
                
                // Payload
                buffer.put_u32(payload.len() as u32);
                buffer.extend_from_slice(payload);
            },
            Self::Error { request_id, error_code, error_message, trace_context } => {
                buffer.put_u8(0x03); // Error response type
                buffer.put_u64(*request_id);
                
                // Trace context (if present)
                if let Some(ref trace_ctx) = trace_context {
                    buffer.put_u8(0x01); // Has trace context
                    buffer.put_u16(trace_ctx.trace_id.len() as u16);
                    buffer.extend_from_slice(&trace_ctx.trace_id);
                    buffer.put_u64(trace_ctx.span_id);
                    buffer.put_u8(trace_ctx.flags);
                    buffer.put_u16(trace_ctx.state.len() as u16);
                    buffer.extend_from_slice(trace_ctx.state.as_bytes());
                } else {
                    buffer.put_u8(0x00); // No trace context
                }
                
                // Error details
                buffer.put_u16(*error_code);
                buffer.put_u16(error_message.len() as u16);
                buffer.extend_from_slice(error_message.as_bytes());
            },
        }
        
        Ok(buffer.freeze())
    }

    /// Deserialize from Cap'n Proto bytes
    pub fn deserialize(data: Bytes) -> RpcResult<Self> {
        let mut cursor = Cursor::new(data);
        
        let msg_type = read_u8(&mut cursor)?;
        let request_id = read_u64(&mut cursor)?;
        
        // Read trace context
        let has_trace = read_u8(&mut cursor)? != 0;
        let trace_context = if has_trace {
            let trace_id_len = read_u16(&mut cursor)? as usize;
            let mut trace_id = vec![0u8; trace_id_len];
            read_exact(&mut cursor, &mut trace_id)?;
            
            let span_id = read_u64(&mut cursor)?;
            let flags = read_u8(&mut cursor)?;
            
            let state_len = read_u16(&mut cursor)? as usize;
            let mut state_bytes = vec![0u8; state_len];
            read_exact(&mut cursor, &mut state_bytes)?;
            let state = String::from_utf8(state_bytes)
                .map_err(|e| RpcError::Deserialization(format!("Invalid UTF-8 in trace state: {}", e)))?;
            
            Some(TraceContext {
                trace_id,
                span_id,
                flags,
                state,
            })
        } else {
            None
        };
        
        match msg_type {
            0x02 => {
                // Success response
                let payload_len = read_u32(&mut cursor)? as usize;
                let mut payload_bytes = vec![0u8; payload_len];
                read_exact(&mut cursor, &mut payload_bytes)?;
                let payload = Bytes::from(payload_bytes);
                
                Ok(Self::Success {
                    request_id,
                    payload,
                    trace_context,
                })
            },
            0x03 => {
                // Error response
                let error_code = read_u16(&mut cursor)?;
                let error_msg_len = read_u16(&mut cursor)? as usize;
                let mut error_msg_bytes = vec![0u8; error_msg_len];
                read_exact(&mut cursor, &mut error_msg_bytes)?;
                let error_message = String::from_utf8(error_msg_bytes)
                    .map_err(|e| RpcError::Deserialization(format!("Invalid UTF-8 in error message: {}", e)))?;
                
                Ok(Self::Error {
                    request_id,
                    error_code,
                    error_message,
                    trace_context,
                })
            },
            _ => Err(RpcError::Deserialization(format!("Invalid response message type: {}", msg_type))),
        }
    }
}

// Helper functions for binary reading
fn read_u8(cursor: &mut Cursor<Bytes>) -> RpcResult<u8> {
    use std::io::Read;
    let mut buf = [0u8; 1];
    cursor.read_exact(&mut buf)
        .map_err(|e| RpcError::Deserialization(format!("Failed to read u8: {}", e)))?;
    Ok(buf[0])
}

fn read_u16(cursor: &mut Cursor<Bytes>) -> RpcResult<u16> {
    use std::io::Read;
    let mut buf = [0u8; 2];
    cursor.read_exact(&mut buf)
        .map_err(|e| RpcError::Deserialization(format!("Failed to read u16: {}", e)))?;
    Ok(u16::from_be_bytes(buf))
}

fn read_u32(cursor: &mut Cursor<Bytes>) -> RpcResult<u32> {
    use std::io::Read;
    let mut buf = [0u8; 4];
    cursor.read_exact(&mut buf)
        .map_err(|e| RpcError::Deserialization(format!("Failed to read u32: {}", e)))?;
    Ok(u32::from_be_bytes(buf))
}

fn read_u64(cursor: &mut Cursor<Bytes>) -> RpcResult<u64> {
    use std::io::Read;
    let mut buf = [0u8; 8];
    cursor.read_exact(&mut buf)
        .map_err(|e| RpcError::Deserialization(format!("Failed to read u64: {}", e)))?;
    Ok(u64::from_be_bytes(buf))
}

fn read_exact(cursor: &mut Cursor<Bytes>, buf: &mut [u8]) -> RpcResult<()> {
    use std::io::Read;
    cursor.read_exact(buf)
        .map_err(|e| RpcError::Deserialization(format!("Failed to read bytes: {}", e)))
}