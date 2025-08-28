@0xd4933954b8c65b48;  # Unique file ID

# Common types used across messages
struct LogEntry {
    term @0 :UInt64;
    index @1 :UInt64;
    data @2 :Data;
}

# Request vote RPC
struct RequestVoteRequest {
    term @0 :UInt64;
    candidateId @1 :Text;
    lastLogIndex @2 :UInt64;
    lastLogTerm @3 :UInt64;
}

struct RequestVoteResponse {
    term @0 :UInt64;
    voteGranted @1 :Bool;
}

# Append entries RPC
struct AppendEntriesRequest {
    term @0 :UInt64;
    leaderId @1 :Text;
    prevLogIndex @2 :UInt64;
    prevLogTerm @3 :UInt64;
    entries @4 :List(LogEntry);
    leaderCommit @5 :UInt64;
}

struct AppendEntriesResponse {
    term @0 :UInt64;
    success @1 :Bool;
    lastMatchIndex @2 :UInt64;
}

# Install snapshot RPC
struct InstallSnapshotRequest {
    term @0 :UInt64;
    leaderId @1 :Text;
    lastIncludedIndex @2 :UInt64;
    lastIncludedTerm @3 :UInt64;
    offset @4 :UInt64;
    data @5 :Data;
    done @6 :Bool;
}

struct InstallSnapshotResponse {
    term @0 :UInt64;
    success @1 :Bool;
}

# Message wrapper for all RPC types
struct Message {
    union {
        requestVoteRequest @0 :RequestVoteRequest;
        requestVoteResponse @1 :RequestVoteResponse;
        appendEntriesRequest @2 :AppendEntriesRequest;
        appendEntriesResponse @3 :AppendEntriesResponse;
        installSnapshotRequest @4 :InstallSnapshotRequest;
        installSnapshotResponse @5 :InstallSnapshotResponse;
    }
}