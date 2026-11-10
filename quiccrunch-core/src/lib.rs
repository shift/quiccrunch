// Copyright (c) 2025 QuicCrunch Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! QuicCrunch: A high-performance QUIC benchmarking framework with advanced optimizations
//!
//! This library provides comprehensive QUIC protocol benchmarking capabilities with support for
//! multiple backends (Quinn and tQUIC), performance metrics collection, visualization tools,
//! and cutting-edge performance optimizations including:
//!
//! ## Performance Features
//! - **SIMD-accelerated operations** - Vectorized memory operations using AVX2/AVX-512
//! - **Zero-copy I/O** - Memory-efficient data handling with buffer pools
//! - **Lock-free channels** - High-concurrency stream multiplexing
//! - **io_uring integration** - Direct kernel I/O bypass on Linux
//! - **CPU-specific optimizations** - Runtime feature detection and tuning
//! - **Memory pools** - Thread-local buffer management
//!
//! ## Usage Examples
//!
//! ### Basic benchmarking:
//! ```rust,no_run
//! use quiccrunch_core::{QuicClient, QuicServer};
//!
//! // Start with optimized defaults
//! let client = QuicClient::new()?;
//! let server = QuicServer::new()?;
//! ```
//!
//! ### High-performance configuration:
//! ```rust,no_run
//! use quiccrunch_core::optimizations::{CpuOptimizations, enable_high_performance};
//!
//! // Initialize all optimizations
//! let cpu_opts = CpuOptimizations::detect();
//! enable_high_performance(&cpu_opts)?;
//! ```

mod error;
pub mod transport;
//pub mod connection_pool;
//pub mod crypto_accel;
//pub mod congestion;
//mod stream;
mod stream;
mod stream_splitter; // Stream splitting for concurrent RPC operations
                     //pub mod test_utils;
                     //pub mod metrics;
                     //pub mod power;
pub mod config;
//pub mod client;
//pub mod server;
pub mod rpc;
pub mod production;
pub mod raft;

// Interceptor/Middleware Framework for RPC
#[cfg(feature = "auth")]
pub mod interceptor;

// Benchmark suite module
#[cfg(feature = "benchmark-suite")]
pub mod benchmark;

// Performance optimization modules
pub mod adaptive; // New adaptive optimization module
pub mod cpu_optimizations;
#[cfg(feature = "io-uring-advanced")]
pub mod io_uring_advanced;
#[cfg(feature = "io-uring-advanced")]
pub mod io_uring_optimized;
pub mod lockfree;
pub mod memory;
pub mod optimization_validation;
pub mod simd;
pub mod stream_optimized;

// Zero-allocation components
pub use memory::{
    get_fast_buffer, return_fast_buffer, FastBuffer, FastHeaderParser, FastMetrics,
    FastMetricsSnapshot,
};

// Convenience module for all optimizations
pub mod optimizations {
    pub use crate::adaptive::{
        allocate_optimized, copy_optimized, OptimizationStrategy, OptimizedBuffer, WorkloadConfig,
    };
    pub use crate::cpu_optimizations::{
        initialize_optimizations, CpuBenchmarkResults, CpuOptimizations,
    };
    #[cfg(feature = "io-uring-advanced")]
    pub use crate::io_uring_advanced::{
        IoUringAdvancedTransport, TransportConfig, TransportStatsSnapshot,
    };
    pub use crate::lockfree::{
        ConnectionHandle, ConnectionStatsSnapshot, StreamHandle, StreamStatsSnapshot,
    };
    pub use crate::memory::{get_pool_stats, MessageBuffer, PoolStats, PooledBuffer};
    pub use crate::simd::{decode_u32_array, fast_copy, fast_zero, CpuFeatures};
    pub use crate::stream_optimized::{BenchmarkResults, OptimizedQuicStream, StreamConfig};

    // Re-export optimization validation functions
    pub use crate::optimization_validation::{
        quick_performance_test, validate_optimizations, AdaptiveStrategyValidation,
        AllocationPerformance, CopyPerformance, OptimizationResults,
    };

    /// Enable all high-performance optimizations
    pub fn enable_high_performance(cpu_opts: &CpuOptimizations) -> std::io::Result<()> {
        cpu_opts.enable_huge_pages()?;
        cpu_opts.set_performance_governor()?;
        cpu_opts.disable_cpu_scaling()?;
        println!("🚀 High-performance mode enabled!");
        Ok(())
    }

    /// Quick setup for maximum performance (combines optimization detection and enablement)
    pub fn enable_maximum_performance() -> std::io::Result<CpuOptimizations> {
        let cpu_opts = CpuOptimizations::detect();
        enable_high_performance(&cpu_opts)?;
        println!(
            "💪 Maximum performance mode: SIMD={}, Memory pools={}, CPU optimizations=enabled",
            cfg!(feature = "simd-optimizations"),
            cfg!(feature = "memory-pools")
        );
        Ok(cpu_opts)
    }

    /// Quick benchmark of system capabilities
    pub fn benchmark_system() -> CpuBenchmarkResults {
        let cpu_opts = CpuOptimizations::detect();
        let results = cpu_opts.benchmark_cpu_performance();
        results.print_summary();
        results
    }
}

pub use config::{Backend, ClientConfig, CongestionControl, QuicTransportSettings, ServerConfig};
pub use error::{FastError, Result, TransportError};
pub use transport::{QuicTransport, Transport};
//pub use connection_pool::{ConnectionPool, ConnectionPoolConfig, ConnectionPoolStats, EndpointStats, ConnectionHealth};
//pub use crypto_accel::{HardwareCrypto, CryptoAccelConfig, HardwareCapabilities, CryptoStats, CryptoStatsSnapshot, BatchCryptoProcessor, CipherSuiteOptimizer};
//pub use congestion::{AdaptiveCongestionControl, CongestionControlConfig, NetworkMetrics, CongestionAlgorithm, NetworkCondition, CongestionControlStats};
//pub use stream::QuicStream;
pub use stream::QuicStream;
pub use stream_splitter::{SplitStream, Splittable, StreamReader, StreamWriter}; // Stream splitting support
                                                                                //pub use metrics::{Metrics, Operation};
                                                                                //pub use client::{QuicClient, QuicConnection};
                                                                                //pub use server::{QuicServer, QuicListener, MessageHandler, EchoHandler, run_echo_server, run_server_with_handler};
                                                                                pub use rpc::{RpcService, RpcServer, RpcClient, PooledRpcClient, RpcConnectionPool, RpcError, RpcResult, RpcContext, RpcStreamSink, RpcStreamSource};

// Re-export optimized components for high-performance use cases
pub use optimizations::{benchmark_system, enable_high_performance, enable_maximum_performance};
pub use stream_optimized::OptimizedQuicStream;

pub mod schema {
    pub mod frame_capnp {
        include!(concat!(env!("OUT_DIR"), "/schema/frame_capnp.rs"));
    }

    pub mod rpc_capnp {
        include!(concat!(env!("OUT_DIR"), "/schema/rpc_capnp.rs"));
    }

    pub mod raft_capnp {
        include!(concat!(env!("OUT_DIR"), "/schema/raft_capnp.rs"));
    }

    // Re-export types
    pub use frame_capnp::{message_header, trace_context};

    pub use rpc_capnp::{
        endpoint, method_info, rpc_error, rpc_message, rpc_request, rpc_response, service_info,
        ErrorCode, HealthStatus, LoadBalancingStrategy, MessageType, StreamingType,
    };
}

// Convenience re-exports for generated code
pub use schema::frame_capnp::{frame, message_header, trace_context};

pub use schema::rpc_capnp::{
    endpoint, method_info, rpc_error, rpc_message, rpc_request, rpc_response, service_info,
    ErrorCode, HealthStatus, LoadBalancingStrategy, MessageType, StreamingType,
};
