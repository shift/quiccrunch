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

//! Comprehensive benchmark suite for comparing QuicCrunch and gRPC performance
//! across all streaming patterns with accurate performance measurement.

pub mod metrics;
pub mod grpc_client;
pub mod grpc_server;
pub mod quiccrunch_client;
pub mod quiccrunch_server;

use std::time::Duration;
use serde::{Serialize, Deserialize};

/// Message size test matrix covering small, medium and large messages
pub const MESSAGE_SIZES: &[usize] = &[
    16, 32, 64, 128, 256, 512, 768,           // Small (B)
    1024, 2048, 4096, 8192,                  // Small (KB)  
    16384, 32768, 65536, 131072,             // Medium (KB)
    1048576, 2097152, 4194304                // Large (MB)
];

/// Connection count test matrix for concurrent testing
pub const CONNECTION_COUNTS: &[usize] = &[1, 10, 100, 1000];

/// Benchmark configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub message_size: usize,
    pub streaming_pattern: StreamingPattern,
    pub concurrent_connections: usize,
    pub duration_seconds: u64,
    pub warmup_seconds: u64,
    pub target_qps: Option<u64>, // For rate limiting
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            message_size: 1024,
            streaming_pattern: StreamingPattern::Unary,
            concurrent_connections: 10,
            duration_seconds: 30,
            warmup_seconds: 5,
            target_qps: None,
        }
    }
}

/// Streaming pattern enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamingPattern {
    Unary,
    ClientStreaming { message_count: usize },
    ServerStreaming { message_count: usize },
    Bidirectional { message_count: usize },
}

/// Performance metrics structure
#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub latency_percentiles: LatencyMetrics,
    pub throughput: ThroughputMetrics,
    pub resource_usage: ResourceMetrics,
    pub error_metrics: ErrorMetrics,
}

/// Latency measurement metrics
#[derive(Debug, Serialize, Deserialize)]
pub struct LatencyMetrics {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub mean_ms: f64,
    pub std_dev_ms: f64,
}

/// Throughput measurement metrics
#[derive(Debug, Serialize, Deserialize)]
pub struct ThroughputMetrics {
    pub messages_per_second: f64,
    pub bytes_per_second: f64,
    pub mbps: f64,
}

/// Resource usage metrics
#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceMetrics {
    pub cpu_usage_percent: f64,
    pub memory_usage_bytes: u64,
    pub network_io_bytes: u64,
}

/// Error tracking metrics
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorMetrics {
    pub connection_failures: u64,
    pub message_failures: u64,
    pub timeout_failures: u64,
    pub total_errors: u64,
}

/// Benchmark comparison result
#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkComparison {
    pub timestamp: String,
    pub config: BenchmarkConfig,
    pub quiccrunch_results: PerformanceMetrics,
    pub grpc_results: PerformanceMetrics,
    pub comparison: ComparisonMetrics,
}

/// Performance comparison metrics
#[derive(Debug, Serialize, Deserialize)]
pub struct ComparisonMetrics {
    pub throughput_ratio: f64,
    pub latency_ratio: f64,
    pub winner: String,
}

/// Generate test data with realistic patterns
pub fn generate_test_message(size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size);
    
    // Mix of random data and patterns to simulate real traffic
    for i in 0..size {
        match i % 4 {
            0 => data.push((i % 256) as u8),
            1 => data.push(0xFF),
            2 => data.push(0x00),
            3 => data.push(rand::random::<u8>()),
            _ => unreachable!(),
        }
    }
    
    data
}

/// Generate unique message ID
pub fn generate_message_id(pattern: &str, sequence: u32) -> String {
    format!("{}:{:08x}:{}", pattern, sequence, chrono::Utc::now().timestamp_micros())
}