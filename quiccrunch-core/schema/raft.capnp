@0xd4933954b8c65b49;  # Unique file ID

# Raft RPC contract, version 1.
#
# This is the wire contract for the distri-kv PartitionRaft seam. Every
# request/response is wrapped in an envelope carrying an explicit
# `contractVersion`; a server rejects unknown versions before looking at the
# payload. Variable-length fields are additionally bounded by the constants in
# quiccrunch-core/src/raft/mod.rs and checked on encode and decode.

enum ErrorCode {
    ok               @0;
    unavailable      @1;  # transport/connection failure, retryable
    notLeader        @2;  # leaderHint is populated
    termMismatch     @3;  # caller's term is stale (response.term is current)
    snapshotStale    @4;  # offset/snapshot identity does not match retained state
    frameTooLarge    @5;  # exceeded a documented bound
    invalidRequest   @6;
    deadlineExceeded @7;
    backpressure     @8;  # bounded queue full, retryable
    internal         @9;
    protocol         @10;
}

enum RpcKind {
    appendEntries   @0;
    requestVote     @1;
    installSnapshot @2;
}

struct LeaderHint {
    leaderId   @0 :UInt64;
    leaderAddr @1 :Text;
}

struct LogEntry {
    term  @0 :UInt64;
    index @1 :UInt64;
    data  @2 :Data;
}

struct AppendEntries {
    partitionId   @0 :UInt64;
    term          @1 :UInt64;
    leaderId      @2 :UInt64;
    prevLogIndex  @3 :UInt64;
    prevLogTerm   @4 :UInt64;
    entries       @5 :List(LogEntry);
    leaderCommit  @6 :UInt64;
}

struct AppendEntriesResponse {
    term          @0 :UInt64;
    success       @1 :Bool;
    matchIndex    @2 :UInt64;
    conflictIndex @3 :UInt64;  # first index the follower wants on failure
    conflictTerm  @4 :UInt64;  # term of the conflicting entry, 0 if unknown
}

struct RequestVote {
    partitionId  @0 :UInt64;
    term         @1 :UInt64;
    candidateId  @2 :UInt64;
    lastLogIndex @3 :UInt64;
    lastLogTerm  @4 :UInt64;
}

struct RequestVoteResponse {
    term        @0 :UInt64;
    voteGranted @1 :Bool;
}

# One streaming InstallSnapshot frame. A session is identified by
# (partitionId, lastIndex, lastTerm). `offset` is the absolute byte offset of
# `chunk` in the snapshot body. `done` marks the final frame. The receiver acks
# every frame with the next contiguous offset it has durably accepted, so a
# sender can resume from `nextOffset` after a reconnect.
struct InstallSnapshotChunk {
    partitionId @0 :UInt64;
    term        @1 :UInt64;
    leaderId    @2 :UInt64;
    lastIndex   @3 :UInt64;
    lastTerm    @4 :UInt64;
    offset      @5 :UInt64;
    chunk       @6 :Data;
    done        @7 :Bool;
}

struct InstallSnapshotAck {
    term          @0 :UInt64;
    success       @1 :Bool;
    bytesAccepted @2 :UInt64;  # total contiguous bytes accepted for this snapshot
    nextOffset    @3 :UInt64;  # == bytesAccepted; resume point
    done          @4 :Bool;
}

struct Request {
    contractVersion @0 :UInt16;
    kind            @1 :RpcKind;
    union {
        appendEntries   @2 :AppendEntries;
        requestVote     @3 :RequestVote;
        installSnapshot @4 :InstallSnapshotChunk;
    }
}

struct Response {
    contractVersion @0 :UInt16;
    kind            @1 :RpcKind;
    code            @2 :ErrorCode;
    leaderHint      @3 :LeaderHint;
    retryable       @4 :Bool;
    retryAfterMs    @5 :UInt64;
    message         @6 :Text;
    union {
        appendEntries   @7 :AppendEntriesResponse;
        requestVote     @8 :RequestVoteResponse;
        installSnapshot @9 :InstallSnapshotAck;
    }
}