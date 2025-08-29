# QuicCrunch Release Performance Report

**Release Version:** dev  
**Test Date:** 2025-08-29T13:35:35+00:00  
**Environment:** Linux shift-nixos 6.6.54 #1-NixOS SMP PREEMPT_DYNAMIC x86_64 GNU/Linux  
**Rust Version:** rustc 1.89.0 (29483883e 2025-08-04)  

## Performance vs gRPC

# QuicCrunch vs gRPC Performance Comparison Report

**Generated:** 2025-08-29T13:35:35+00:00  
**Test Duration:** Comprehensive latency and throughput benchmark  
**Environment:** NixOS Linux x86_64, 8 CPU cores  
**Git Commit:** f9c9bb4  
**Build Profile:** Release with optimizations  

## Executive Summary

QuicCrunch with tQUIC backend **significantly outperforms gRPC** across all tested message sizes, demonstrating:

- **🏆 12/12 test cases won** by QuicCrunch
- **📉 46.4% average latency reduction** compared to gRPC
- **📈 67.4% average throughput improvement** over gRPC  
- **⚡ 68.0% average requests-per-second improvement** over gRPC

## Test Configuration

- **Message Sizes:** 64B to 8MB (12 test cases)
- **Requests per test:** 500 requests
- **Concurrent clients:** 4 clients
- **Warmup requests:** 25 requests per size
- **Protocol comparison:** QuicCrunch (tQUIC) vs gRPC (HTTP/2)

## Key Performance Results

### Small Messages (64B - 1KB)
- **Average RPS improvement:** 57.3%
- **Average latency reduction:** 47.1%
- **Average throughput gain:** 57.3%

### Medium Messages (4KB - 64KB)  
- **Average RPS improvement:** 79.6%
- **Average latency reduction:** 49.8%
- **Average throughput gain:** 79.9%

### Large Messages (256KB - 8MB)
- **Average RPS improvement:** 70.6%
- **Average latency reduction:** 45.1%
- **Average throughput gain:** 69.3%

## Build Configuration

```toml
features = ["backend-tquic", "grpc-comparison"]
profile = "release"
```

## Test Environment

- **OS:** Linux
- **CPU:** 8 cores
- **Memory:** 32GB
- **Network:** Localhost loopback

## Reproducibility

To reproduce these results:

```bash
./scripts/run_grpc_comparison.sh
```

Or manually:

```bash
cargo run --example grpc_comparison --features="backend-tquic,grpc-comparison" --release
```

## Conclusion

QuicCrunch demonstrates **consistent and significant performance advantages** over gRPC, with improvements ranging from 44% to 90% across different metrics. The tQUIC backend's implementation provides production-ready performance gains that can substantially improve application scalability and reduce infrastructure costs.