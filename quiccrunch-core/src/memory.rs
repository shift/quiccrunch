/// High-performance memory management for QuicCrunch
/// 
/// This module provides thread-local buffer pools and zero-copy operations
/// to eliminate allocation overhead in hot paths.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use bytes::{Bytes, BytesMut};

thread_local! {
    static BUFFER_POOL: RefCell<BufferPool> = RefCell::new(BufferPool::new());
    static POOL_STATS: RefCell<PoolStats> = RefCell::new(PoolStats::default());
}

/// Size-based buffer pools to minimize allocation overhead
struct BufferPool {
    small_buffers: VecDeque<Vec<u8>>,    // < 4KB
    medium_buffers: VecDeque<Vec<u8>>,   // 4KB - 64KB
    large_buffers: VecDeque<Vec<u8>>,    // 64KB - 1MB
    huge_buffers: VecDeque<Vec<u8>>,     // > 1MB
}

impl BufferPool {
    const SMALL_THRESHOLD: usize = 8 * 1024;        // 8KB (was 4KB)
    const MEDIUM_THRESHOLD: usize = 64 * 1024;      // 64KB (unchanged)
    const LARGE_THRESHOLD: usize = 1024 * 1024;     // 1MB (unchanged)
    const MAX_POOL_SIZE: usize = 32;                // Max buffers per pool (increased from 16)

    fn new() -> Self {
        Self {
            small_buffers: VecDeque::new(),
            medium_buffers: VecDeque::new(),
            large_buffers: VecDeque::new(),
            huge_buffers: VecDeque::new(),
        }
    }

    fn get_buffer(&mut self, size: usize) -> Vec<u8> {
        let pool = match size {
            s if s <= Self::SMALL_THRESHOLD => &mut self.small_buffers,
            s if s <= Self::MEDIUM_THRESHOLD => &mut self.medium_buffers,
            s if s <= Self::LARGE_THRESHOLD => &mut self.large_buffers,
            _ => &mut self.huge_buffers,
        };

        // Try to find a suitable buffer in the pool
        for i in 0..pool.len() {
            if let Some(buffer) = pool.get(i) {
                if buffer.capacity() >= size {
                    // Found a suitable buffer - remove it and mark as hit
                    let mut reused_buffer = pool.remove(i).unwrap();
                    reused_buffer.clear();
                    reused_buffer.resize(size, 0);
                    
                    POOL_STATS.with(|stats| {
                        stats.borrow_mut().pool_hits += 1;
                    });
                    return reused_buffer;
                }
            }
        }

        // No suitable buffer found - allocate new buffer (miss)
        POOL_STATS.with(|stats| {
            let mut s = stats.borrow_mut();
            s.pool_misses += 1;
            s.allocations += 1;
        });
        
        // Allocate with appropriate capacity for the size category
        let capacity = match size {
            s if s <= Self::SMALL_THRESHOLD => Self::SMALL_THRESHOLD,
            s if s <= Self::MEDIUM_THRESHOLD => Self::MEDIUM_THRESHOLD,
            s if s <= Self::LARGE_THRESHOLD => Self::LARGE_THRESHOLD,
            _ => size.max(Self::LARGE_THRESHOLD),
        };
        
        let mut buffer = Vec::with_capacity(capacity);
        buffer.resize(size, 0);
        buffer
    }

    fn return_buffer(&mut self, mut buffer: Vec<u8>) {
        POOL_STATS.with(|stats| {
            stats.borrow_mut().deallocations += 1;
        });

        // Clear the buffer but retain capacity
        buffer.clear();

        // Determine correct pool based on capacity (should match allocation strategy)
        let pool = match buffer.capacity() {
            c if c <= Self::SMALL_THRESHOLD => &mut self.small_buffers,
            c if c <= Self::MEDIUM_THRESHOLD => &mut self.medium_buffers,
            c if c <= Self::LARGE_THRESHOLD => &mut self.large_buffers,
            _ => &mut self.huge_buffers,
        };

        // Only keep the buffer if we have room in the pool
        if pool.len() < Self::MAX_POOL_SIZE {
            pool.push_back(buffer);
        }
        // Otherwise, let buffer drop to free memory
    }
}

/// RAII wrapper for pooled buffers
pub struct PooledBuffer {
    buffer: Vec<u8>,
}

impl PooledBuffer {
    /// Get a buffer from the thread-local pool
    pub fn get(size: usize) -> Self {
        let buffer = BUFFER_POOL.with(|pool| {
            pool.borrow_mut().get_buffer(size)
        });
        Self { buffer }
    }

    /// Get the underlying buffer
    pub fn as_slice(&self) -> &[u8] {
        &self.buffer
    }

    /// Get mutable access to the buffer
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    /// Convert to Vec<u8>, consuming the wrapper
    pub fn into_vec(mut self) -> Vec<u8> {
        std::mem::take(&mut self.buffer)
    }

    /// Get current length
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Get capacity
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }

    /// Resize the buffer
    pub fn resize(&mut self, new_len: usize, value: u8) {
        self.buffer.resize(new_len, value);
    }

    /// Extend from slice
    pub fn extend_from_slice(&mut self, slice: &[u8]) {
        self.buffer.extend_from_slice(slice);
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        let buffer = std::mem::take(&mut self.buffer);
        BUFFER_POOL.with(|pool| {
            pool.borrow_mut().return_buffer(buffer);
        });
    }
}

/// Zero-copy message buffer for high-performance I/O
pub struct MessageBuffer {
    data: BytesMut,
    header_written: bool,
}

impl MessageBuffer {
    /// Create new message buffer with capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: BytesMut::with_capacity(capacity + 4), // +4 for header
            header_written: false,
        }
    }

    /// Write message header (length)
    pub fn write_header(&mut self, length: u32) {
        if !self.header_written {
            self.data.extend_from_slice(&length.to_le_bytes());
            self.header_written = true;
        }
    }

    /// Write data to the buffer
    pub fn write_data(&mut self, data: &[u8]) {
        self.data.extend_from_slice(data);
    }

    /// Get the complete message as Bytes (zero-copy)
    pub fn into_bytes(self) -> Bytes {
        self.data.freeze()
    }

    /// Get current data length (excluding header)
    pub fn data_len(&self) -> usize {
        if self.header_written {
            self.data.len() - 4
        } else {
            self.data.len()
        }
    }

    /// Reserve additional capacity
    pub fn reserve(&mut self, additional: usize) {
        self.data.reserve(additional);
    }
}

/// Statistics for memory pool usage
#[derive(Debug, Default, Clone)]
pub struct PoolStats {
    pub allocations: u64,
    pub deallocations: u64,
    pub pool_hits: u64,
    pub pool_misses: u64,
}

impl PoolStats {
    /// Get hit rate as percentage
    pub fn hit_rate(&self) -> f64 {
        if self.pool_hits + self.pool_misses == 0 {
            0.0
        } else {
            (self.pool_hits as f64) / ((self.pool_hits + self.pool_misses) as f64) * 100.0
        }
    }
}

/// Get thread-local pool statistics
pub fn get_pool_stats() -> PoolStats {
    POOL_STATS.with(|stats| stats.borrow().clone())
}

/// Reset thread-local pool statistics
pub fn reset_pool_stats() {
    POOL_STATS.with(|stats| {
        *stats.borrow_mut() = PoolStats::default();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool_reuse() {
        let buffer1 = PooledBuffer::get(1024);
        let ptr1 = buffer1.as_slice().as_ptr();
        drop(buffer1);

        let buffer2 = PooledBuffer::get(1024);
        let ptr2 = buffer2.as_slice().as_ptr();
        
        // Should reuse the same buffer
        assert_eq!(ptr1, ptr2);
    }

    #[test]
    fn test_message_buffer() {
        let mut msg = MessageBuffer::with_capacity(1024);
        msg.write_header(100);
        msg.write_data(b"Hello, World!");
        
        assert_eq!(msg.data_len(), 13);
        
        let bytes = msg.into_bytes();
        assert_eq!(bytes.len(), 17); // 4 bytes header + 13 bytes data
    }

    #[test]
    fn test_size_categories() {
        let small = PooledBuffer::get(1024);
        let medium = PooledBuffer::get(32 * 1024);
        let large = PooledBuffer::get(512 * 1024);

        assert!(small.capacity() >= 1024);
        assert!(medium.capacity() >= 32 * 1024);
        assert!(large.capacity() >= 512 * 1024);
    }
}

/// Zero-allocation buffer system for hot paths
/// Pre-allocated buffer for zero-allocation operations
#[derive(Clone)]
pub struct FastBuffer {
    data: [u8; 4096], // 4KB fixed-size buffer
    len: usize,
}

impl FastBuffer {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            data: [0u8; 4096],
            len: 0,
        }
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.len = 0;
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[u8] {
        &self.data[..self.len]
    }

    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data[..]
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline(always)]
    pub fn capacity(&self) -> usize {
        self.data.len()
    }

    #[inline(always)]
    pub fn extend_from_slice(&mut self, slice: &[u8]) -> Result<(), ()> {
        if self.len + slice.len() > self.data.len() {
            return Err(()); // Buffer overflow
        }
        self.data[self.len..self.len + slice.len()].copy_from_slice(slice);
        self.len += slice.len();
        Ok(())
    }

    #[inline(always)]
    pub fn push(&mut self, byte: u8) -> Result<(), ()> {
        if self.len >= self.data.len() {
            return Err(()); // Buffer overflow
        }
        self.data[self.len] = byte;
        self.len += 1;
        Ok(())
    }
}

thread_local! {
    static FAST_BUFFER_POOL: RefCell<Vec<FastBuffer>> = RefCell::new(Vec::new());
}

/// Get a fast buffer from the thread-local pool
#[inline(always)]
pub fn get_fast_buffer() -> FastBuffer {
    FAST_BUFFER_POOL.with(|pool| {
        if let Some(mut buffer) = pool.borrow_mut().pop() {
            buffer.clear();
            buffer
        } else {
            FastBuffer::new()
        }
    })
}

/// Return a fast buffer to the thread-local pool
#[inline(always)]
pub fn return_fast_buffer(buffer: FastBuffer) {
    FAST_BUFFER_POOL.with(|pool| {
        if pool.borrow().len() < 32 { // Limit pool size
            pool.borrow_mut().push(buffer);
        }
        // Drop buffer if pool is full
    })
}

/// Zero-allocation header parsing
pub struct FastHeaderParser {
    buffer: FastBuffer,
}

impl FastHeaderParser {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            buffer: get_fast_buffer(),
        }
    }

    #[inline(always)]
    pub fn parse_length_prefix(&mut self, data: &[u8]) -> Result<(usize, usize), crate::error::FastError> {
        if data.len() < 4 {
            return Err(crate::error::FastError::invalid_message());
        }

        // Parse 32-bit big-endian length prefix
        let length = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;

        if length > 1024 * 1024 { // 1MB limit
            return Err(crate::error::FastError::buffer_overflow());
        }

        Ok((length, 4)) // Return (length, bytes_consumed)
    }

    #[inline(always)]
    pub fn parse_varint(&mut self, data: &[u8]) -> Result<(u64, usize), crate::error::FastError> {
        if data.is_empty() {
            return Err(crate::error::FastError::invalid_message());
        }

        let mut value: u64 = 0;
        let mut bytes_consumed = 0;

        for (i, &byte) in data.iter().enumerate() {
            if i >= 10 { // Varint max 10 bytes
                return Err(crate::error::FastError::invalid_message());
            }

            value |= ((byte & 0x7F) as u64) << (i * 7);
            bytes_consumed += 1;

            if (byte & 0x80) == 0 {
                break;
            }
        }

        Ok((value, bytes_consumed))
    }
}

impl Drop for FastHeaderParser {
    fn drop(&mut self) {
        return_fast_buffer(self.buffer.clone());
    }
}

/// Zero-allocation metrics collection
pub struct FastMetrics {
    messages_sent: std::sync::atomic::AtomicU64,
    messages_received: std::sync::atomic::AtomicU64,
    bytes_sent: std::sync::atomic::AtomicU64,
    bytes_received: std::sync::atomic::AtomicU64,
    errors_count: std::sync::atomic::AtomicU64,
}

impl FastMetrics {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            messages_sent: std::sync::atomic::AtomicU64::new(0),
            messages_received: std::sync::atomic::AtomicU64::new(0),
            bytes_sent: std::sync::atomic::AtomicU64::new(0),
            bytes_received: std::sync::atomic::AtomicU64::new(0),
            errors_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    #[inline(always)]
    pub fn record_message_sent(&self, size: usize) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(size as u64, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn record_message_received(&self, size: usize) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received.fetch_add(size as u64, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn record_error(&self) {
        self.errors_count.fetch_add(1, Ordering::Relaxed);
    }

    #[inline(always)]
    pub fn get_stats(&self) -> FastMetricsSnapshot {
        FastMetricsSnapshot {
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            errors_count: self.errors_count.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of fast metrics for reporting
#[derive(Debug, Clone)]
pub struct FastMetricsSnapshot {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub errors_count: u64,
}

impl FastMetricsSnapshot {
    #[inline(always)]
    pub fn messages_per_second(&self, elapsed_seconds: f64) -> f64 {
        if elapsed_seconds > 0.0 {
            (self.messages_sent + self.messages_received) as f64 / elapsed_seconds
        } else {
            0.0
        }
    }

    #[inline(always)]
    pub fn throughput_mbps(&self, elapsed_seconds: f64) -> f64 {
        if elapsed_seconds > 0.0 {
            let total_bytes = self.bytes_sent + self.bytes_received;
            (total_bytes as f64 * 8.0) / (elapsed_seconds * 1024.0 * 1024.0) // Convert to Mbps
        } else {
            0.0
        }
    }
}