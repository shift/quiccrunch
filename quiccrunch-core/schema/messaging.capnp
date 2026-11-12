@0xd1a2b3c4d5e6f708;

# QuicCrunch WAF <-> distri-kv backend messaging contract.
#
# Versioning:
#   * `Envelope.contractVersion` is mandatory and currently 1.
#   * New optional fields must use new ordinal numbers; never reuse or
#     change the type of an existing field.
#   * Servers MUST reject a contract version they do not understand.
#   * Every variable-length field is bounded by the constants in
#     `quiccrunch_core::messaging` (MAX_*), enforced before decode/encode.

const contractVersion :UInt16 = 1;
const maxEnvelopeBytes :UInt32 = 4194304;   # 4 MiB
const maxEventsPerIngest :UInt32 = 1024;
const maxBundleBytes :UInt32 = 8388608;     # 8 MiB

enum Method {
  leaseAcquire @0;
  leaseRelease @1;
  policyGet @2;
  policyWatch @3;
  eventIngest @4;
  heartbeat @5;
}

struct Envelope {
  contractVersion @0 :UInt16;
  messageId @1 :UInt64;
  requestId @2 :UInt64;
  timestampMs @3 :UInt64;
  method @4 :Method;

  union {
    request @5 :Request;
    response @6 :Response;
    watchEvent @7 :PolicyWatchEvent;
    streamEnd @8 :StreamEnd;
    error @9 :Error;
  }

  # Optional field added after contract v1. Old v1 decoders ignore it
  # (Cap'n Proto forward compatibility); it is not required for v1.
  traceId @10 :Text;
}

struct Identity {
  spiffeId @0 :Text;
  nodeId @1 :Text;
  roles @2 :List(Text);
}

struct KeyValue {
  key @0 :Text;
  value @1 :Text;
}

struct Request {
  identity @0 :Identity;
  metadata @1 :List(KeyValue);

  leaseAcquire @2 :LeaseAcquireRequest;
  leaseRelease @3 :LeaseReleaseRequest;
  policyGet @4 :PolicyGetRequest;
  policyWatch @5 :PolicyWatchRequest;
  eventIngest @6 :EventIngestRequest;
  heartbeat @7 :HeartbeatRequest;
}

struct Response {
  metadata @0 :List(KeyValue);

  leaseAcquire @1 :LeaseAcquireResponse;
  leaseRelease @2 :LeaseReleaseResponse;
  policyGet @3 :PolicyGetResponse;
  eventIngest @4 :EventIngestResponse;
  heartbeat @5 :HeartbeatResponse;
}

struct LeaseAcquireRequest {
  leaseId @0 :Text;
  resource @1 :Text;
  ttlMs @2 :UInt32;
}

struct LeaseAcquireResponse {
  granted @0 :Bool;
  fencingToken @1 :UInt64;
  expiresAtUnixMs @2 :UInt64;
  holder @3 :Text;
}

struct LeaseReleaseRequest {
  leaseId @0 :Text;
  fencingToken @1 :UInt64;
}

struct LeaseReleaseResponse {
  released @0 :Bool;
}

struct PolicyGetRequest {
  policyId @0 :Text;
  knownVersion @1 :UInt64;
}

struct PolicyGetResponse {
  policyId @0 :Text;
  version @1 :UInt64;
  bundle @2 :Data;
  checksum @3 :Text;
}

struct PolicyWatchRequest {
  policyId @0 :Text;
  sinceVersion @1 :UInt64;
}

struct PolicyWatchEvent {
  policyId @0 :Text;
  version @1 :UInt64;
  bundle @2 :Data;
  checksum @3 :Text;
}

struct EventIngestRequest {
  events @0 :List(Event);
}

struct Event {
  eventId @0 :Text;
  kind @1 :Text;
  occurredAtUnixMs @2 :UInt64;
  payload @3 :Data;
}

struct EventIngestResponse {
  accepted @0 :UInt32;
  duplicate @1 :UInt32;
}

struct HeartbeatRequest {
  nodeId @0 :Text;
  epoch @1 :UInt64;
}

struct HeartbeatResponse {
  serverTimeUnixMs @0 :UInt64;
  epoch @1 :UInt64;
}

struct StreamEnd {
  reason @0 :Text;
}

struct Error {
  code @0 :UInt16;
  message @1 :Text;
  retryAfterMs @2 :UInt32;
  # Added in contract v1 (new ordinals only).
  earliestAvailable @3 :UInt64;
  serverLatest @4 :UInt64;
  retryable @5 :Bool;
}