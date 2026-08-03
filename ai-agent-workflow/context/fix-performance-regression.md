# Performance Regression Debugging - Context

## Objective
Systematically identify and fix the 4x performance regression in optimized mode.

## Investigation Methodology

### 1. Enable Detailed Profiling

```bash
# CPU profiling with perf
perf record -g -- cargo bench --bench benchmark_suite --features "backend-tquic,lock-free-channels" 2>&1 | tee perf.log
perf report --stdio > perf_report.txt

# Flamegraph
cargo install flamegraph
cargo flamegraph --bench benchmark_suite --features "backend-tquic,lock-free-channels" 2>&1 | tee flamegraph.log

# Thread analysis
perf stat -e cycles,instructions,cache-references,cache-misses -- cargo bench --bench benchmark_suite --features "backend-tquic,lock-free-channels"
```

### 2. Compare Thread Utilization

Add temporary logging to count active threads:

```rust
// In worker_pool.rs - worker spawn
eprintln!("Worker {} started on thread {:?}", worker_id, std::thread::current().id());

// In lockfree.rs - StreamProcessor
eprintln!("StreamProcessor running on thread {:?}", std::thread::current().id());

// Count unique thread IDs during benchmark
```

**Expected**: Baseline shows 8+ unique thread IDs. Regressed may show 1-2.

### 3. Benchmark Configuration Analysis

Check what the benchmark actually tests:

```rust
// In benches/benchmark_suite.rs or similar
// What message sizes? What concurrency?
// Is it testing the same workload in both modes?
```

### 4. Key Code Paths to Profile

| Component | File | What to Check |
|-----------|------|---------------|
| Worker Pool | `worker_pool.rs` | Thread count, work distribution |
| Lock-free Channels | `lockfree.rs` | `StreamProcessor` batch size, channel capacity |
| Adaptive Strategy | `adaptive.rs` | Strategy selection for test workload |
| Memory Pools | `memory.rs` | Allocation patterns, thread-local vs global |
| Stream Optimized | `stream_optimized.rs` | Message size handling, SIMD activation |

## Likely Root Causes

### Hypothesis 1: Single-Threaded Lock-free Processor
```rust
// In lockfree.rs - StreamProcessor::run()
pub async fn run(&mut self) {
    loop {
        // Processes ALL commands from single channel
        // If multiple workers send to same processor -> serialization!
        let batch = self.receive_batch().await;
        self.process_batch(batch).await;
    }
}

// Fix: One StreamProcessor per worker, not global
```

### Hypothesis 2: SIMD on Small Messages
```rust
// In adaptive.rs - should select strategy based on message size
pub fn select_strategy(&self, msg_size: usize) -> OptimizationStrategy {
    match msg_size {
        0..=8192 => OptimizationStrategy::Standard,      // No SIMD
        8193..=65536 => OptimizationStrategy::MemoryPools,
        _ => OptimizationStrategy::FullOptimizations,     // SIMD + all
    }
}

// Check: Is this being called? Is message size detected correctly?
```

### Hypothesis 3: Channel Contention
```rust
// In lockfree.rs - Crossbeam channels with bounded capacity
// If capacity too small -> backpressure -> serialization
// If all workers share single channel -> contention

// Check: Channel capacities in ConnectionHandle::new()
// Check: Are channels per-connection or global?
```

### Hypothesis 4: Worker Count Mismatch
```rust
// Baseline test: 8 worker threads explicitly
// Optimized: May default to 1 or num_cpus()

// Check: WorkerPool::new() default worker count
// Check: Feature flags affecting worker spawn
```

## Debugging Commands

```bash
# 1. Run baseline with thread logging
RUST_LOG=quiccrunch=trace cargo test --features backend-tquic -- --nocapture 2>&1 | grep -E "(Worker|thread)" | sort -u

# 2. Run optimized with thread logging  
RUST_LOG=quiccrunch=trace cargo test --features "backend-tquic,lock-free-channels" -- --nocapture 2>&1 | grep -E "(Worker|thread|StreamProcessor)" | sort -u

# 3. Benchmark with timing
cargo bench --bench benchmark_suite --features backend-tquic 2>&1 | grep -E "(throughput|msg/sec|time)"

# 4. Check actual feature compilation
cargo build --features "backend-tquic,lock-free-channels,simd-optimizations" -v 2>&1 | grep -E "(lockfree|simd|adaptive)"
```

## Fix Strategies

### If Thread Count Issue:
```rust
// In worker_pool.rs - ensure worker count respected
pub async fn new(config: WorkerPoolConfig) -> Result<Self> {
    let worker_count = config.worker_count.unwrap_or_else(num_cpus::get);
    // Spawn exactly worker_count workers
    for i in 0..worker_count {
        // Each worker gets own StreamProcessor
    }
}
```

### If SIMD on Small Messages:
```rust
// In stream_optimized.rs or adaptive.rs
fn send_optimized(&self, data: &[u8]) -> Result<()> {
    let strategy = self.adaptive.select_strategy(data.len());
    match strategy {
        OptimizationStrategy::Standard => self.send_standard(data),
        OptimizationStrategy::MemoryPools => self.send_with_pools(data),
        OptimizationStrategy::FullOptimizations => self.send_simd(data),
    }
}
```

### If Channel Contention:
```rust
// In lockfree.rs - increase capacity or use per-worker channels
pub fn new(config: ChannelConfig) -> Self {
    let (write_tx, write_rx) = crossbeam_channel::bounded(config.write_capacity * worker_count);
    // Or: one channel per worker
}
```

## Validation Plan

1. **After each fix**: Run benchmark, compare to baseline
2. **Thread verification**: Confirm 8+ threads active
3. **Strategy verification**: Log selected strategy per message size
4. **Channel health**: Monitor channel capacity utilization
5. **Regression test**: Full test suite passes

## Expected Outcomes

| Scenario | Expected Throughput |
|----------|---------------------|
| Baseline (fixed) | ~56,000 msg/sec |
| Lock-free only | ~80,000 msg/sec (1.4x) |
| SIMD (large msgs) | ~150,000 msg/sec (2.5x) |
| All optimizations | ~200,000+ msg/sec (3.5x+) |

## Documentation

Record findings in:
- `PERFORMANCE_REGRESSION_ANALYSIS.md` (new)
- Update `TODO.md` with root cause
- Update benchmark documentation

---
*Context for: Debug and Fix Performance Regression*
*Rust Performance Investigation Context*
*Last updated: 2025-01-16*