# QuicCrunch Release Performance Report

**Release Version:** dev  
**Test Date:** 2025-08-29T01:06:48+02:00  
**Environment:** Linux x1y 6.16.3 #1-NixOS SMP PREEMPT_DYNAMIC Sat Aug 23 14:49:42 UTC 2025 x86_64 GNU/Linux  
**Rust Version:** rustc 1.89.0 (29483883e 2025-08-04)  

## Performance vs gRPC

# QuicCrunch vs gRPC Performance Report

**Generated:** 2025-08-28T23:06:48.273998972+00:00
**Test Configuration:** 100 requests, 2 concurrent clients

## Results
⚠️ gRPC comparison was not enabled. Build with `--features grpc-comparison` to enable.


## Build Configuration

```toml
features = ["backend-tquic", "grpc-comparison"]
profile = "release"
```

## Test Environment

- **OS:** Linux
- **CPU:** 8 cores
- **Memory:** 15Gi
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
