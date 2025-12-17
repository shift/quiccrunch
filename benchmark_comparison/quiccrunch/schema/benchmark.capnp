@0x8513d1a6e1d1b1c7;

struct SimpleRequest {
    id :Text;
    timestamp :UInt64;
    payload :Data;
    testType :Text;
    sequenceNumber :UInt32;
}

struct SimpleResponse {
    id :Text;
    timestamp :UInt64;
    payload :Data;
    status :Text;
    metrics :Group {
        concurrentConnections :UInt32;
        totalMessages :UInt64;
        cpuUsage :Float32;
        memoryUsage :UInt64;
    }
}

struct BenchmarkRequest {
    id :Text;
    timestamp :UInt64;
    payload :Data;
    testType :Text;
    sequenceNumber :UInt32;
}

struct BenchmarkResponse {
    id :Text;
    timestamp :UInt64;
    payload :Data;
    status :Text;
    metrics :Group {
        concurrentConnections :UInt32;
        totalMessages :UInt64;
        cpuUsage :Float32;
        memoryUsage :UInt64;
    }
}