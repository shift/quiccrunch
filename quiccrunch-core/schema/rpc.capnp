@0xd7a9b1c8e5f6a2b3;  # Unique file ID for RPC schema

using import "frame.capnp".TraceContext;

# RPC Message Types
enum MessageType {
    request @0;
    response @1;
    notification @2;
    error @3;
    streamChunk @4;
    streamEnd @5;
    streamError @6;
}

# RPC Call Message
struct RpcMessage {
    messageType @0 :MessageType;
    requestId @1 :UInt64;
    
    union {
        request @2 :RpcRequest;
        response @3 :RpcResponse;
        notification @4 :RpcNotification;
        error @5 :RpcError;
        streamChunk @7 :RpcStreamChunk;
        streamEnd @8 :RpcStreamEnd;
        streamError @9 :RpcStreamError;
    }
    
    # Optional trace context for observability
    traceContext @6 :TraceContext;
}

# RPC Request
struct RpcRequest {
    service @0 :Text;           # Service name (e.g., "UserService")
    method @1 :Text;            # Method name (e.g., "GetUser")
    parameters @2 :Data;        # Serialized parameters
    timeout @3 :UInt32;         # Timeout in milliseconds
    metadata @4 :List(KeyValue); # Request metadata
    streamingType @5 :StreamingType; # Type of streaming for this request
    streamId @6 :UInt64;        # Stream identifier for multi-message streams
}

# RPC Response
struct RpcResponse {
    result @0 :Data;            # Serialized result
    metadata @1 :List(KeyValue); # Response metadata
    streamId @2 :UInt64;        # Stream identifier for streaming responses
}

# RPC Stream Chunk
struct RpcStreamChunk {
    streamId @0 :UInt64;        # Stream identifier
    sequenceId @1 :UInt64;      # Sequence number for ordering
    data @2 :Data;              # Chunk data
    metadata @3 :List(KeyValue); # Chunk metadata
    isLast @4 :Bool;            # True if this is the last chunk
}

# RPC Stream End
struct RpcStreamEnd {
    streamId @0 :UInt64;        # Stream identifier
    metadata @1 :List(KeyValue); # Final metadata
}

# RPC Stream Error
struct RpcStreamError {
    streamId @0 :UInt64;        # Stream identifier
    error @1 :RpcError;         # Error details
}

# RPC Notification (fire-and-forget)
struct RpcNotification {
    service @0 :Text;
    method @1 :Text;
    parameters @2 :Data;
    metadata @3 :List(KeyValue);
}

# RPC Error
struct RpcError {
    code @0 :ErrorCode;
    message @1 :Text;
    details @2 :Data;           # Optional error details
}

# Error Codes
enum ErrorCode {
    unknown @0;
    invalidRequest @1;
    methodNotFound @2;
    invalidParams @3;
    internalError @4;
    timeout @5;
    serviceUnavailable @6;
    permissionDenied @7;
    rateLimited @8;
}

# Key-Value pair for metadata
struct KeyValue {
    key @0 :Text;
    value @1 :Text;
}

# Service Discovery Message
struct ServiceInfo {
    name @0 :Text;              # Service name
    version @1 :Text;           # Service version
    endpoints @2 :List(Endpoint); # Available endpoints
    methods @3 :List(MethodInfo); # Available methods
    metadata @4 :List(KeyValue); # Service metadata
}

# Service Endpoint
struct Endpoint {
    address @0 :Text;           # Network address
    port @1 :UInt16;            # Port number
    protocol @2 :Text;          # Protocol (e.g., "quic")
    weight @3 :UInt32;          # Load balancing weight
    health @4 :HealthStatus;    # Health status
}

# Health Status
enum HealthStatus {
    unknown @0;
    healthy @1;
    unhealthy @2;
    draining @3;
}

# Method Information
struct MethodInfo {
    name @0 :Text;              # Method name
    inputType @1 :Text;         # Input parameter type
    outputType @2 :Text;        # Output result type
    streaming @3 :StreamingType; # Streaming type
    deprecated @4 :Bool;        # Is deprecated
}

# Streaming Types
enum StreamingType {
    unary @0;                   # Single request, single response
    clientStreaming @1;         # Multiple requests, single response
    serverStreaming @2;         # Single request, multiple responses
    bidirectionalStreaming @3;  # Multiple requests, multiple responses
}

# Load Balancing Strategy
enum LoadBalancingStrategy {
    roundRobin @0;
    leastConnections @1;
    weightedRoundRobin @2;
    random @3;
    consistentHash @4;
}

# Service Registry Message
struct ServiceRegistry {
    action @0 :RegistryAction;
    service @1 :ServiceInfo;
}

enum RegistryAction {
    register @0;
    unregister @1;
    update @2;
    heartbeat @3;
}