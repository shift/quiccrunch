@0xc3822953a7c54a37;  # Unique file ID

struct MessageHeader {
    payloadSize @0 :UInt32;
    
    # Optional trace context for telemetry
    traceContext @1 :TraceContext;
}

struct TraceContext {
    traceId @0 :Data;      # 16-byte trace ID
    spanId @1 :UInt64;     # 8-byte span ID
    flags @2 :UInt8;       # Trace flags
    state @3 :Text;        # Trace state
}

struct Frame {
    length @0 :UInt32;
    traceContext @1 :TraceContext;
    payload @2 :Data;
}