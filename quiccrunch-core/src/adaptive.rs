/// Adaptive optimization strategy for QuicCrunch
/// Automatically selects the best optimization approach based on workload characteristics

use crate::memory::PooledBuffer;
use crate::simd;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OptimizationStrategy {
    /// Use standard allocator and standard copy (best for small buffers)
    Standard,
    /// Use memory pools but standard copy (best for medium buffers)
    PoolOnly,
    /// Use both memory pools and SIMD (best for large aligned buffers)
    Full,
}

impl OptimizationStrategy {
    /// Select optimal strategy based on buffer size (updated based on performance analysis)
    pub fn for_size(size: usize) -> Self {
        match size {
            // Small buffers: standard allocator is fastest due to pool overhead
            0..=8192 => Self::Standard,           // 0-8KB: Use standard (was 0-8KB)
            // Medium buffers: pools help significantly, but SIMD still has overhead  
            8193..=65536 => Self::PoolOnly,      // 8KB-64KB: Use pools only (was 8KB-128KB)
            // Large buffers: both optimizations help (after SIMD fixes)
            _ => Self::Full,                     // 64KB+: Use full optimization (was 128KB+)
        }
    }
    
    /// Select optimal strategy based on alignment and size (updated thresholds)
    pub fn for_buffer(data: &[u8]) -> Self {
        let size = data.len();
        let aligned = data.as_ptr() as usize % 64 == 0;  // 64-byte alignment for AVX-512
        
        match (size, aligned) {
            // Small buffers always use standard
            (0..=8192, _) => Self::Standard,
            // Medium buffers use pools only 
            (8193..=65536, _) => Self::PoolOnly,
            // Large aligned buffers can use full optimization
            (65537.., true) => Self::Full,
            // Large unaligned buffers avoid SIMD
            (65537.., false) => Self::PoolOnly,
        }
    }
}

/// Optimized buffer allocation that adapts to size
pub fn allocate_optimized(size: usize) -> OptimizedBuffer {
    let strategy = OptimizationStrategy::for_size(size);
    
    match strategy {
        OptimizationStrategy::Standard => {
            OptimizedBuffer::Standard(vec![0u8; size])
        }
        OptimizationStrategy::PoolOnly | OptimizationStrategy::Full => {
            #[cfg(feature = "memory-pools")]
            {
                let mut buffer = PooledBuffer::get(size);
                buffer.resize(size, 0);
                OptimizedBuffer::Pooled(buffer)
            }
            #[cfg(not(feature = "memory-pools"))]
            {
                OptimizedBuffer::Standard(vec![0u8; size])
            }
        }
    }
}

/// Optimized copy operation that adapts to data characteristics
pub fn copy_optimized(src: &[u8], dst: &mut [u8]) {
    assert_eq!(src.len(), dst.len());
    
    let strategy = OptimizationStrategy::for_buffer(src);
    
    match strategy {
        OptimizationStrategy::Standard | OptimizationStrategy::PoolOnly => {
            dst.copy_from_slice(src);
        }
        OptimizationStrategy::Full => {
            #[cfg(feature = "simd-optimizations")]
            {
                simd::fast_copy(src, dst);
            }
            #[cfg(not(feature = "simd-optimizations"))]
            {
                dst.copy_from_slice(src);
            }
        }
    }
}

/// Buffer that adapts its allocation strategy
pub enum OptimizedBuffer {
    Standard(Vec<u8>),
    #[cfg(feature = "memory-pools")]
    Pooled(PooledBuffer),
}

impl OptimizedBuffer {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            OptimizedBuffer::Standard(vec) => vec.as_slice(),
            #[cfg(feature = "memory-pools")]
            OptimizedBuffer::Pooled(buffer) => buffer.as_slice(),
        }
    }
    
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        match self {
            OptimizedBuffer::Standard(vec) => vec.as_mut_slice(),
            #[cfg(feature = "memory-pools")]
            OptimizedBuffer::Pooled(buffer) => buffer.as_mut_slice(),
        }
    }
    
    pub fn len(&self) -> usize {
        match self {
            OptimizedBuffer::Standard(vec) => vec.len(),
            #[cfg(feature = "memory-pools")]
            OptimizedBuffer::Pooled(buffer) => buffer.len(),
        }
    }
    
    pub fn capacity(&self) -> usize {
        match self {
            OptimizedBuffer::Standard(vec) => vec.capacity(),
            #[cfg(feature = "memory-pools")]
            OptimizedBuffer::Pooled(buffer) => buffer.capacity(),
        }
    }
    
    /// Copy data into this buffer
    pub fn copy_from_slice(&mut self, data: &[u8]) {
        match self {
            OptimizedBuffer::Standard(vec) => {
                vec.clear();
                vec.extend_from_slice(data);
            }
            #[cfg(feature = "memory-pools")]
            OptimizedBuffer::Pooled(buffer) => {
                buffer.as_mut_slice()[..data.len()].copy_from_slice(data);
            }
        }
    }
    
    /// Convert into Vec<u8>
    pub fn into_vec(self) -> Vec<u8> {
        match self {
            OptimizedBuffer::Standard(vec) => vec,
            #[cfg(feature = "memory-pools")]
            OptimizedBuffer::Pooled(buffer) => buffer.into_vec(),
        }
    }
}

/// Performance configuration for different workload types
#[derive(Debug, Clone)]
pub struct WorkloadConfig {
    pub typical_message_size: usize,
    pub peak_message_size: usize,
    pub concurrent_streams: usize,
    pub optimization_strategy: OptimizationStrategy,
}

impl WorkloadConfig {
    /// Configuration for small RPC workloads (typical web APIs)
    pub fn small_rpc() -> Self {
        Self {
            typical_message_size: 2048,      // 2KB
            peak_message_size: 8192,         // 8KB  
            concurrent_streams: 100,
            optimization_strategy: OptimizationStrategy::Standard,  // Based on analysis: standard is fastest for <8KB
        }
    }
    
    /// Configuration for medium RPC workloads (file transfers, data sync)
    pub fn medium_rpc() -> Self {
        Self {
            typical_message_size: 32768,     // 32KB
            peak_message_size: 65536,        // 64KB (reduced from 256KB)
            concurrent_streams: 50,
            optimization_strategy: OptimizationStrategy::PoolOnly,  // Pools help significantly in 8KB-64KB range
        }
    }
    
    /// Configuration for large workloads (video streaming, large data)
    pub fn large_rpc() -> Self {
        Self {
            typical_message_size: 262144,    // 256KB (reduced from 512KB)
            peak_message_size: 2097152,      // 2MB (reduced from 4MB)
            concurrent_streams: 10,
            optimization_strategy: OptimizationStrategy::Full,  // Both pools and SIMD help for 64KB+
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_strategy_selection() {
        assert_eq!(OptimizationStrategy::for_size(1024), OptimizationStrategy::Standard);
        assert_eq!(OptimizationStrategy::for_size(16384), OptimizationStrategy::PoolOnly);
        assert_eq!(OptimizationStrategy::for_size(262144), OptimizationStrategy::Full);
    }
    
    #[test]
    fn test_workload_configs() {
        let small = WorkloadConfig::small_rpc();
        assert_eq!(small.optimization_strategy, OptimizationStrategy::Standard);
        
        let medium = WorkloadConfig::medium_rpc();
        assert_eq!(medium.optimization_strategy, OptimizationStrategy::PoolOnly);
        
        let large = WorkloadConfig::large_rpc();
        assert_eq!(large.optimization_strategy, OptimizationStrategy::Full);
    }
}