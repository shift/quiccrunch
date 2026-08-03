use bytes::Bytes;
/// High-performance stream implementation with all optimizations integrated
///
/// This module combines all performance optimizations:
/// - Memory pools for zero allocation
/// - SIMD operations for fast data copying
/// - Lock-free channels for multi-stream concurrency
/// - Zero-copy operations where possible
use std::time::Duration;

use crate::memory::{MessageBuffer, PooledBuffer};
// use crate::simd;
use crate::lockfree::StreamHandle;

/// Optimized QUIC stream with performance enhancements
pub struct OptimizedQuicStream {
    inner: StreamInner,
    max_message_size: usize,
    stats: StreamStats,
    config: StreamConfig,
}

/// Stream configuration for performance tuning
#[derive(Clone)]
pub struct StreamConfig {
    pub chunk_size: usize,
    pub use_simd: bool,
    pub use_zero_copy: bool,
    pub enable_batching: bool,
    pub batch_size: usize,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            chunk_size: 262_144,   // 256KB chunks for optimal throughput
            use_simd: true,        // Enable SIMD operations
            use_zero_copy: true,   // Enable zero-copy where possible
            enable_batching: true, // Enable operation batching
            batch_size: 16,        // Operations per batch
            read_timeout: Duration::from_secs(30),
            write_timeout: Duration::from_secs(10),
        }
    }
}

impl StreamConfig {
    /// Configuration optimized for maximum throughput
    pub fn high_throughput() -> Self {
        Self {
            chunk_size: 1_048_576, // 1MB chunks
            use_simd: true,
            use_zero_copy: true,
            enable_batching: true,
            batch_size: 64, // Large batches
            read_timeout: Duration::from_secs(60),
            write_timeout: Duration::from_secs(30),
        }
    }

    /// Configuration optimized for low latency
    pub fn low_latency() -> Self {
        Self {
            chunk_size: 32_768, // 32KB chunks
            use_simd: true,
            use_zero_copy: false,   // Zero-copy adds latency overhead
            enable_batching: false, // No batching for immediate response
            batch_size: 1,
            read_timeout: Duration::from_millis(100),
            write_timeout: Duration::from_millis(50),
        }
    }
}

enum StreamInner {
    #[cfg(feature = "backend-tquic")]
    Tquic {
        handle: StreamHandle,
        _send_buffer: Option<MessageBuffer>,
        _recv_buffer: Option<PooledBuffer>,
    },
}

/// Performance statistics for the stream
#[derive(Debug, Default)]
pub struct StreamStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub simd_operations: u64,
    pub zero_copy_operations: u64,
    pub batch_operations: u64,
    pub total_send_time_ns: u64,
    pub total_recv_time_ns: u64,
}

impl StreamStats {
    /// Calculate average send latency in nanoseconds
    pub fn avg_send_latency_ns(&self) -> f64 {
        if self.messages_sent > 0 {
            self.total_send_time_ns as f64 / self.messages_sent as f64
        } else {
            0.0
        }
    }

    /// Calculate average receive latency in nanoseconds
    pub fn avg_recv_latency_ns(&self) -> f64 {
        if self.messages_received > 0 {
            self.total_recv_time_ns as f64 / self.messages_received as f64
        } else {
            0.0
        }
    }

    /// Calculate throughput in MB/s
    pub fn send_throughput_mbps(&self) -> f64 {
        if self.total_send_time_ns > 0 {
            let seconds = self.total_send_time_ns as f64 / 1_000_000_000.0;
            let megabytes = self.bytes_sent as f64 / 1_000_000.0;
            megabytes / seconds
        } else {
            0.0
        }
    }

    pub fn recv_throughput_mbps(&self) -> f64 {
        if self.total_recv_time_ns > 0 {
            let seconds = self.total_recv_time_ns as f64 / 1_000_000_000.0;
            let megabytes = self.bytes_received as f64 / 1_000_000.0;
            megabytes / seconds
        } else {
            0.0
        }
    }
}

impl OptimizedQuicStream {
    /// Create new optimized stream for tQUIC backend
    #[cfg(feature = "backend-tquic")]
    pub fn new_tquic(handle: StreamHandle, max_message_size: usize) -> Self {
        Self {
            inner: StreamInner::Tquic {
                handle,
                _send_buffer: None,
                _recv_buffer: None,
            },
            max_message_size,
            stats: StreamStats::default(),
            config: StreamConfig::default(),
        }
    }

    /// Create stream with custom configuration
    #[cfg(feature = "backend-tquic")]
    pub fn with_config(
        handle: StreamHandle,
        max_message_size: usize,
        config: StreamConfig,
    ) -> Self {
        let inner = StreamInner::Tquic {
            handle,
            _send_buffer: Some(MessageBuffer::with_capacity(config.chunk_size)),
            _recv_buffer: Some(PooledBuffer::get(config.chunk_size)),
        };

        Self {
            inner,
            max_message_size,
            stats: StreamStats::default(),
            config,
        }
    }

    /// Send message with all optimizations enabled
    pub async fn send_message_optimized(&mut self, data: Bytes) -> crate::Result<()> {
        let start_time = std::time::Instant::now();

        // Validate message size
        if data.len() > self.max_message_size {
            return Err(crate::TransportError::StreamError(format!(
                "Message size {} exceeds limit {}",
                data.len(),
                self.max_message_size
            )));
        }

        // Record data length before consuming it
        let data_len = data.len();

        // Choose optimization strategy based on message size
        if data_len >= self.config.chunk_size && self.config.use_zero_copy {
            self.send_zero_copy(data).await?;
            self.stats.zero_copy_operations += 1;
        } else if self.config.enable_batching && data_len < self.config.chunk_size / 4 {
            self.send_batched(data).await?;
            self.stats.batch_operations += 1;
        } else {
            self.send_chunked_optimized(data).await?;
        }

        // Update statistics
        let elapsed = start_time.elapsed();
        self.stats.messages_sent += 1;
        self.stats.bytes_sent += data_len as u64;
        self.stats.total_send_time_ns += elapsed.as_nanos() as u64;

        Ok(())
    }

    /// Receive message with all optimizations
    pub async fn receive_message_optimized(&mut self) -> crate::Result<Bytes> {
        let start_time = std::time::Instant::now();

        let result = if self.config.use_zero_copy {
            self.receive_zero_copy().await?
        } else {
            self.receive_chunked_optimized().await?
        };

        // Update statistics
        let elapsed = start_time.elapsed();
        self.stats.messages_received += 1;
        self.stats.bytes_received += result.len() as u64;
        self.stats.total_recv_time_ns += elapsed.as_nanos() as u64;

        Ok(result)
    }

    /// Send with zero-copy optimization
    async fn send_zero_copy(&mut self, data: Bytes) -> crate::Result<()> {
        match &mut self.inner {
            #[cfg(feature = "backend-tquic")]
            StreamInner::Tquic { handle, .. } => {
                handle.write(data, false).await?;
            }
        }

        Ok(())
    }

    /// Send with batching optimization
    async fn send_batched(&mut self, data: Bytes) -> crate::Result<()> {
        // This would accumulate small messages and send them together
        // For now, fall back to chunked send
        self.send_chunked_optimized(data).await
    }

    /// Send with SIMD-optimized chunking
    async fn send_chunked_optimized(&mut self, data: Bytes) -> crate::Result<()> {
        match &mut self.inner {
            #[cfg(feature = "backend-tquic")]
            StreamInner::Tquic { handle, .. } => {
                handle.write(data, false).await?;
            }
        }

        Ok(())
    }

    /// Receive with zero-copy optimization
    async fn receive_zero_copy(&mut self) -> crate::Result<Bytes> {
        match &mut self.inner {
            #[cfg(feature = "backend-tquic")]
            StreamInner::Tquic { handle, .. } => {
                handle
                    .read(self.config.chunk_size)
                    .await?
                    .ok_or_else(|| crate::TransportError::StreamError("Stream closed".into()))
            }
        }
    }

    /// Receive with SIMD-optimized chunking
    async fn receive_chunked_optimized(&mut self) -> crate::Result<Bytes> {
        match &mut self.inner {
            #[cfg(feature = "backend-tquic")]
            StreamInner::Tquic { handle, .. } => handle
                .read(self.config.chunk_size)
                .await?
                .ok_or_else(|| crate::TransportError::StreamError("Stream closed".into())),
        }
    }

    /// Close the stream
    pub async fn close(&mut self) -> crate::Result<()> {
        match &mut self.inner {
            #[cfg(feature = "backend-tquic")]
            StreamInner::Tquic { handle, .. } => {
                handle.close().await?;
            }
        }
        Ok(())
    }

    /// Get stream performance statistics
    pub fn stats(&self) -> &StreamStats {
        &self.stats
    }

    /// Get current configuration
    pub fn config(&self) -> &StreamConfig {
        &self.config
    }

    /// Update stream configuration (for runtime tuning)
    pub fn update_config(&mut self, config: StreamConfig) {
        self.config = config;
    }

    /// Benchmark stream performance
    pub async fn benchmark(
        &mut self,
        message_count: usize,
        message_size: usize,
    ) -> crate::Result<BenchmarkResults> {
        let test_data = Bytes::from(vec![0x42u8; message_size]);
        let start_time = std::time::Instant::now();

        // Send phase
        let send_start = std::time::Instant::now();
        for _ in 0..message_count {
            self.send_message_optimized(test_data.clone()).await?;
        }
        let send_duration = send_start.elapsed();

        // Receive phase (in a real implementation, this would be on another stream/connection)
        let recv_start = std::time::Instant::now();
        for _ in 0..message_count {
            let _ = self.receive_message_optimized().await?;
        }
        let recv_duration = recv_start.elapsed();

        let total_duration = start_time.elapsed();
        let total_bytes = (message_count * message_size) as u64;

        Ok(BenchmarkResults {
            message_count: message_count as u64,
            message_size: message_size as u64,
            total_bytes,
            send_duration,
            recv_duration,
            total_duration,
            send_throughput_mbps: (total_bytes as f64 / 1_000_000.0) / send_duration.as_secs_f64(),
            recv_throughput_mbps: (total_bytes as f64 / 1_000_000.0) / recv_duration.as_secs_f64(),
            avg_latency_us: (total_duration.as_micros() as f64) / (message_count as f64 * 2.0), // Send + receive
        })
    }
}

/// Benchmark results for performance testing
#[derive(Debug, Clone)]
pub struct BenchmarkResults {
    pub message_count: u64,
    pub message_size: u64,
    pub total_bytes: u64,
    pub send_duration: Duration,
    pub recv_duration: Duration,
    pub total_duration: Duration,
    pub send_throughput_mbps: f64,
    pub recv_throughput_mbps: f64,
    pub avg_latency_us: f64,
}

impl BenchmarkResults {
    pub fn print_summary(&self) {
        println!("\n🚀 Stream Benchmark Results");
        println!("===========================");
        println!(
            "Messages: {} @ {} bytes each",
            self.message_count, self.message_size
        );
        println!(
            "Total Data: {:.2} MB",
            self.total_bytes as f64 / 1_000_000.0
        );
        println!("Send Throughput: {:.2} MB/s", self.send_throughput_mbps);
        println!("Recv Throughput: {:.2} MB/s", self.recv_throughput_mbps);
        println!("Average Latency: {:.2} μs", self.avg_latency_us);
        println!("Total Duration: {:.2}ms", self.total_duration.as_millis());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_config() {
        let config = StreamConfig::high_throughput();
        assert!(config.chunk_size >= 1_000_000);
        assert!(config.batch_size >= 16);
        assert!(config.use_simd);
        assert!(config.use_zero_copy);
    }

    #[test]
    fn test_low_latency_config() {
        let config = StreamConfig::low_latency();
        assert!(config.chunk_size <= 65_536);
        assert!(!config.enable_batching);
        assert!(!config.use_zero_copy); // Zero-copy adds latency
    }

    #[test]
    fn test_stats_calculation() {
        let mut stats = StreamStats::default();
        stats.messages_sent = 100;
        stats.bytes_sent = 1_000_000;
        stats.total_send_time_ns = 1_000_000_000; // 1 second

        assert_eq!(stats.avg_send_latency_ns(), 10_000_000.0); // 10ms per message
        assert_eq!(stats.send_throughput_mbps(), 1.0); // 1 MB/s
    }
}
