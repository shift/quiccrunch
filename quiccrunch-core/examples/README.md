# QuicCrunch Examples

## Bidirectional Streaming Example

### ⚠️ KNOWN ISSUE: Server Mode Non-Functional

**The server in this example does not work.** When you run it, you'll see continuous spurious output like:
```
📥 New connection accepted
🔌 Stream closed by client (received 0 messages)
✅ Connection handled successfully
```

This happens even with no clients connected.

### Root Cause

The `TquicConnection` API is incomplete - it lacks an `accept_stream()` method to receive client-initiated streams. The server code incorrectly calls `connection.open_stream()` which:
1. Creates an outbound stream (wrong direction)
2. Immediately closes because there's no peer
3. Repeats in a loop

### What's Missing

The transport layer has stream event handlers (`on_stream_created`, `on_stream_readable` in `TquicWorker`), but these are not exposed through the high-level API.

To fix properly requires:
1. Add `AcceptStream` command to `TquicCommand` enum
2. Implement stream acceptance in worker pool  
3. Add `accept_stream()` method to `TquicConnection`
4. Map incoming stream events from worker to adapter layer

### Current Status

**Client code:** Structurally correct but untestable without working server
**Server code:** Fundamentally broken due to API limitation

Do not use this example until stream acceptance is implemented.
