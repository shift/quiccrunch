# Task: Debug and Fix Performance Regression

## Objective
Investigate and fix the 4x performance regression where optimized code (lock-free + SIMD) runs at 14,066 msg/sec vs baseline 56,370 msg/sec.

## Acceptance Criteria
- [ ] Root cause identified and documented
- [ ] Performance restored to at least baseline (56,370 msg/sec)
- [ ] Optimizations show measurable improvement over baseline
- [ ] No correctness regressions introduced
- [ ] Benchmarks pass consistently

## Current State

### Baseline (Working)
- **Throughput**: 56,370 msg/sec
- **Concurrency**: 20
- **Worker threads**: 8
- **Features**: Default (no lock-free, no SIMD)

### Current Optimized (Regressed)
- **Throughput**: 14,066 msg/sec (4x slower!)
- **Features**: `lock-free-channels`, `simd-optimizations`
- **Suspected cause**: Threading model mismatch - optimizations serialized parallel operations

## Investigation Plan

### 1. Profile Current Implementation
```bash
# Run with profiling
cargo bench --features "backend-tquic,lock-free-channels,simd-optimizations" -- --profile-time=30

# Generate flamegraph
cargo flamegraph --features "backend-tquic,lock-free-channels,simd-optimizations" --bench benchmark_suite
```

### 2. Compare Threading Models
```rust
// Baseline: Multi-threaded worker pool (8 threads)
// Optimized: Lock-free channels may force single-threaded execution

// Check worker_pool.rs - how are workers spawned?
// Check lockfree.rs - are channels creating bottlenecks?
```

### 3. Key Areas to Investigate

#### A. Lock-free Channel Contention
```rust
// In lockfree.rs - ConnectionHandle uses dedicated channels
// But if all operations go through single StreamProcessor...
// May serialize what was previously parallel

// Check: Are we using crossbeam MPSC correctly?
// Check: Is batch processing actually batching or serializing?
```

#### B. SIMD Overhead on Small Messages
```rust
// SIMD optimized for 64KB+ workloads
// Baseline test may use small messages (<8KB)
// SIMD setup/teardown overhead > benefit

// Check: Adaptive strategy selection in adaptive.rs
// Check: Message size detection in stream operations
```

#### C. Worker Pool vs Lock-free Architecture Mismatch
```rust
// Baseline: 8 worker threads each with own tQUIC endpoint
// Optimized: Single lock-free channel system?

// Check: How many threads actually running in optimized mode?
// Check: Is io_uring single-threaded by design?
```

#### D. Memory Pool Contention
```rust
// Memory pools use thread-local storage
// But if operations moved to single thread...
// Contention on global pool?

// Check: memory.rs - FastBuffer allocation patterns
```

## Implementation Plan

### Phase 1: Profiling & Root Cause (2-3 hours)
1. Run benchmarks with `perf` and `cargo flamegraph`
2. Compare thread utilization (baseline vs optimized)
3. Identify hot paths and bottlenecks
4. Document findings

### Phase 2: Fix Threading Model (2-3 hours)
1. Ensure lock-free channels don't serialize parallel work
2. Configure worker count appropriately for lock-free mode
3. Verify SIMD only activates for appropriate message sizes
4. Test adaptive strategy selection

### Phase 3: Validate & Benchmark (1-2 hours)
1. Run full benchmark suite
2. Compare against baseline
3. Verify no correctness regressions
4. Document performance characteristics

## Key Files to Investigate

| File | Suspected Issue |
|------|-----------------|
| `src/transport/worker_pool.rs` | Worker spawning/count |
| `src/lockfree.rs` | Channel architecture, batch processing |
| `src/adaptive.rs` | Strategy selection logic |
| `src/memory.rs` | Pool contention |
| `src/stream_optimized.rs` | Message size handling |
| `src/io_uring_advanced.rs` | Threading model |

## Hypothesis: Thread Count Mismatch

The baseline uses 8 worker threads (configured in test). The lock-free optimization may have inadvertently reduced this to 1 thread, or the lock-free channels create a bottleneck where all work funnels through a single processor.

**Test**: Add logging to count active worker threads in both modes.

## Testing Strategy

```bash
# Baseline benchmark
cargo bench --bench benchmark_suite --features backend-tquic 2>&1 | tee baseline.log

# Optimized benchmark  
cargo bench --bench benchmark_suite --features "backend-tquic,lock-free-channels,simd-optimizations" 2>&1 | tee optimized.log

# Compare results
```

## Validation Checklist
- [ ] Thread count matches configuration in both modes
- [ ] Lock-free channels show parallel processing (not serialized)
- [ ] SIMD activates only for large messages (>64KB)
- [ ] Adaptive strategy selects correct optimization
- [ ] Throughput >= 56,370 msg/sec (baseline)
- [ ] Ideally > 100k msg/sec with optimizations

## Dependencies
- Working tQUIC integration (Phase 4 of fix-tquic-integration-gaps)
- Benchmark infrastructure functional

## Estimated Effort
5-8 hours investigation + fix

---
*Task Definition for: Debug and Fix Performance Regression*
*Rust Performance Optimization Task*
*Last updated: 2025-01-16*