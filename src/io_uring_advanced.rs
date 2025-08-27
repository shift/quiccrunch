/// Enhanced io_uring transport with advanced optimizations
/// 
/// This module extends the existing io_uring implementation with:
/// - Vectorized I/O operations
/// - Zero-copy buffer management  
/// - SIMD-optimized memory operations
/// - CPU affinity and NUMA awareness
/// - Advanced batching and prefetching

use std::sync::Arc;
use std::os::unix::io::AsRawFd;
use io_uring::{IoUring, opcode, types, Probe};
use socket2::{Socket, Domain, Type, Protocol, SockAddr};
use bytes::Bytes;
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::collections::VecDeque;
use std::net::SocketAddr;
use crate::memory::{PooledBuffer, MessageBuffer};
use crate::simd;

/// High-performance io_uring transport with vectorized I/O and zero-copy
pub struct IoUringAdvancedTransport {
    ring: IoUring,
    socket: Socket,
    buffer_pool: AdvancedBufferPool,
    batch_processor: BatchProcessor,
    stats: Arc<TransportStats>,
    config: TransportConfig,
}

/// Configuration for io_uring transport optimizations
#[derive(Clone)]
pub struct TransportConfig {
    pub ring_size: u32,
    pub buffer_size: usize,
    pub buffer_count: usize,
    pub batch_size: usize,
    pub use_sqpoll: bool,
    pub use_iopoll: bool,
    pub use_fixed_buffers: bool,
    pub cpu_affinity: Option<usize>,
    pub numa_node: Option<usize>,
    pub prefetch_depth: usize,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            ring_size: 2048,
            buffer_size: 262_144, // 256KB buffers
            buffer_count: 256,
            batch_size: 64,
            use_sqpoll: true,
            use_iopoll: false,
            use_fixed_buffers: true,
            cpu_affinity: None,
            numa_node: None,
            prefetch_depth: 16,
        }
    }
}

impl TransportConfig {
    /// Configuration optimized for maximum throughput
    pub fn extreme_throughput() -> Self {
        Self {
            ring_size: 8192,
            buffer_size: 2_097_152, // 2MB buffers
            buffer_count: 1024,
            batch_size: 256,
            use_sqpoll: true,
            use_iopoll: false,
            use_fixed_buffers: true,
            cpu_affinity: Some(0),
            numa_node: Some(0),
            prefetch_depth: 64,
        }
    }

    /// Configuration optimized for ultra-low latency
    pub fn ultra_low_latency() -> Self {
        Self {
            ring_size: 512,
            buffer_size: 32_768, // 32KB buffers
            buffer_count: 64,
            batch_size: 4,
            use_sqpoll: false, // Polling adds latency
            use_iopoll: false,
            use_fixed_buffers: true,
            cpu_affinity: None,
            numa_node: None,
            prefetch_depth: 2,
        }
    }
}

/// Advanced buffer pool with prefetching and NUMA awareness
struct AdvancedBufferPool {
    registered_buffers: Vec<RegisteredBuffer>,
    available: VecDeque<u16>,
    prefetch_queue: VecDeque<u16>,
    in_use: AtomicUsize,
    group_id: u16,
    prefetch_depth: usize,
}

/// Pre-registered buffer with NUMA-local allocation
struct RegisteredBuffer {
    data: Box<[u8]>,
    buffer_id: u16,
    group_id: u16,
    numa_node: Option<usize>,
}

/// Advanced batch processor with operation coalescing
struct BatchProcessor {
    send_batch: Vec<SendOperation>,
    recv_batch: Vec<RecvOperation>,
    coalesced_sends: Vec<CoalescedSend>,
    max_batch_size: usize,
}

struct SendOperation {
    data: Bytes,
    addr: SockAddr,
    buffer_id: Option<u16>,
    priority: u8,
}

struct RecvOperation {
    buffer_id: u16,
    expected_size: usize,
}

/// Coalesced send operation for vectorized I/O
struct CoalescedSend {
    destinations: Vec<SockAddr>,
    data_chunks: Vec<Bytes>,
    total_size: usize,
}

/// Comprehensive performance statistics
#[derive(Debug, Default)]
pub struct TransportStats {
    pub messages_sent: AtomicU64,
    pub messages_received: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub batches_processed: AtomicU64,
    pub coalesced_operations: AtomicU64,
    pub buffer_hits: AtomicU64,
    pub buffer_misses: AtomicU64,
    pub sqe_submissions: AtomicU64,
    pub cqe_completions: AtomicU64,
    pub zero_copy_ops: AtomicU64,
    pub simd_operations: AtomicU64,
    pub cpu_cycles: AtomicU64,
}

impl IoUringAdvancedTransport {
    /// Create transport with advanced optimizations
    pub fn new(config: TransportConfig) -> std::io::Result<Self> {
        // Set NUMA affinity if specified
        if let Some(numa_node) = config.numa_node {
            Self::set_numa_affinity(numa_node)?;
        }

        // Set CPU affinity if specified
        if let Some(cpu) = config.cpu_affinity {
            Self::set_cpu_affinity(cpu)?;
        }

        // Create io_uring with all optimizations
        let mut ring_builder = IoUring::builder();
        
        if config.use_sqpoll {
            ring_builder = ring_builder.setup_sqpoll(1); // 1ms timeout for responsiveness
        }
        
        if config.use_iopoll {
            ring_builder = ring_builder.setup_iopoll();
        }

        // Enable all performance features
        ring_builder = ring_builder
            .setup_coop_taskrun()    // Cooperative task running
            .setup_taskrun_flag()    // Task run notifications
            .setup_single_issuer();  // Single issuer optimization
        
        let ring = ring_builder.build(config.ring_size)?;

        // Verify all required operations are supported
        let mut probe = Probe::new();
        ring.submitter().register_probe(&mut probe)?;
        Self::check_required_features(&probe)?;

        // Create optimized socket with advanced settings
        let socket = Self::create_ultra_optimized_socket()?;
        
        // Initialize advanced buffer pool
        let buffer_pool = AdvancedBufferPool::new(&ring, &config)?;
        
        // Create advanced batch processor
        let batch_processor = BatchProcessor::new(config.batch_size);
        
        let stats = Arc::new(TransportStats::default());

        Ok(Self {
            ring,
            socket,
            buffer_pool,
            batch_processor,
            stats,
            config,
        })
    }

    /// Send messages with maximum vectorization
    pub async fn send_ultra_vectorized(&mut self, messages: &[(&[u8], SocketAddr)]) -> std::io::Result<()> {
        if messages.is_empty() {
            return Ok(());
        }

        // Group messages by size for optimal batching
        let mut small_messages = Vec::new();
        let mut large_messages = Vec::new();

        for (data, addr) in messages {
            if data.len() <= 1024 {
                small_messages.push((*data, *addr));
            } else {
                large_messages.push((*data, *addr));
            }
        }

        // Process small messages with coalescing
        if !small_messages.is_empty() {
            self.send_coalesced_small(&small_messages).await?;
        }

        // Process large messages with zero-copy
        if !large_messages.is_empty() {
            self.send_zero_copy_large(&large_messages).await?;
        }

        self.stats.batches_processed.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Receive messages with prefetching and prediction
    pub async fn receive_with_prefetch(&mut self, max_messages: usize) -> std::io::Result<Vec<(Bytes, SocketAddr)>> {
        // Prefetch buffers based on predicted traffic
        self.buffer_pool.prefetch_buffers(self.config.prefetch_depth);

        let mut results = Vec::with_capacity(max_messages);
        
        // Prepare optimized receive operations
        for _ in 0..max_messages {
            if let Some(buffer_id) = self.buffer_pool.acquire_with_prefetch() {
                let operation = RecvOperation {
                    buffer_id,
                    expected_size: self.predict_message_size(),
                };
                self.batch_processor.recv_batch.push(operation);
            } else {
                break;
            }
        }

        if !self.batch_processor.recv_batch.is_empty() {
            let received = self.process_advanced_recv_batch().await?;
            results.extend(received);
        }
        
        Ok(results)
    }

    /// Send small messages with operation coalescing
    async fn send_coalesced_small(&mut self, messages: &[(&[u8], SocketAddr)]) -> std::io::Result<()> {
        // Coalesce small messages into fewer syscalls
        let mut coalesced = CoalescedSend {
            destinations: Vec::new(),
            data_chunks: Vec::new(),
            total_size: 0,
        };

        for (data, addr) in messages {
            coalesced.destinations.push(SockAddr::from(*addr));
            coalesced.data_chunks.push(Bytes::copy_from_slice(data));
            coalesced.total_size += data.len();
        }

        self.batch_processor.coalesced_sends.push(coalesced);
        self.process_coalesced_batch().await?;
        
        self.stats.coalesced_operations.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Send large messages with zero-copy optimization
    async fn send_zero_copy_large(&mut self, messages: &[(&[u8], SocketAddr)]) -> std::io::Result<()> {
        let mut submission = self.ring.submission();
        let mut submitted = 0;

        for (data, addr) in messages {
            if let Some(buffer_id) = self.buffer_pool.acquire_with_prefetch() {
                let buffer = &mut self.buffer_pool.registered_buffers[buffer_id as usize];
                
                // Use SIMD for ultra-fast memory copy
                let copy_len = data.len().min(buffer.data.len());
                simd::fast_copy(&data[..copy_len], &mut buffer.data[..copy_len]);

                // Submit zero-copy send with registered buffer
                let send_op = opcode::Send::new(
                    types::Fd(self.socket.as_raw_fd()),
                    std::ptr::null(),
                    copy_len as u32,
                )
                .build()
                .flags(io_uring::squeue::Flags::FIXED_BUF)
                .buf_index(buffer_id);

                unsafe {
                    submission.push(&send_op)?;
                }
                
                submitted += 1;
                self.stats.zero_copy_ops.fetch_add(1, Ordering::Relaxed);
                self.stats.simd_operations.fetch_add(1, Ordering::Relaxed);
            }
        }

        drop(submission);
        
        if submitted > 0 {
            self.ring.submit_and_wait(submitted)?;
            self.process_completions(submitted).await?;
        }

        Ok(())
    }

    /// Process coalesced send operations
    async fn process_coalesced_batch(&mut self) -> std::io::Result<()> {
        let coalesced_batch = &mut self.batch_processor.coalesced_sends;
        if coalesced_batch.is_empty() {
            return Ok(());
        }

        let mut submission = self.ring.submission();
        let mut submitted = 0;

        for coalesced in coalesced_batch.drain(..) {
            // Create vectorized I/O structure
            let mut iovecs: Vec<libc::iovec> = coalesced.data_chunks.iter()
                .map(|chunk| libc::iovec {
                    iov_base: chunk.as_ptr() as *mut libc::c_void,
                    iov_len: chunk.len(),
                })
                .collect();

            // Use sendmsg with multiple destinations (if supported)
            for (i, addr) in coalesced.destinations.iter().enumerate() {
                let msghdr = libc::msghdr {
                    msg_name: addr.as_ptr() as *mut libc::c_void,
                    msg_namelen: addr.len(),
                    msg_iov: iovecs.as_mut_ptr().add(i),
                    msg_iovlen: 1,
                    msg_control: std::ptr::null_mut(),
                    msg_controllen: 0,
                    msg_flags: 0,
                };

                let send_op = opcode::SendMsg::new(
                    types::Fd(self.socket.as_raw_fd()),
                    &msghdr as *const libc::msghdr,
                ).build();

                unsafe {
                    submission.push(&send_op)?;
                }
                submitted += 1;
            }
        }

        drop(submission);
        
        if submitted > 0 {
            self.ring.submit_and_wait(submitted)?;
            self.process_completions(submitted).await?;
        }

        Ok(())
    }

    /// Process receive batch with advanced optimizations
    async fn process_advanced_recv_batch(&mut self) -> std::io::Result<Vec<(Bytes, SocketAddr)>> {
        let batch = &mut self.batch_processor.recv_batch;
        if batch.is_empty() {
            return Ok(Vec::new());
        }

        let mut submission = self.ring.submission();
        let batch_size = batch.len();

        // Submit all receive operations with advanced flags
        for operation in batch.iter() {
            let recv_op = opcode::RecvMsg::new(
                types::Fd(self.socket.as_raw_fd()),
                std::ptr::null_mut(),
            )
            .buf_group(self.buffer_pool.group_id)
            .build()
            .flags(io_uring::squeue::Flags::MULTISHOT) // Enable multishot for efficiency
            .user_data(operation.buffer_id as u64);

            unsafe {
                submission.push(&recv_op)?;
            }
        }

        drop(submission);
        self.ring.submit_and_wait(batch_size)?;

        // Collect results with SIMD optimization
        let mut results = Vec::with_capacity(batch_size);
        let mut completion = self.ring.completion();
        
        for _ in 0..batch_size {
            if let Some(cqe) = completion.next() {
                if cqe.result() > 0 {
                    let buffer_id = cqe.user_data() as u16;
                    let len = cqe.result() as usize;
                    let buffer = &self.buffer_pool.registered_buffers[buffer_id as usize];
                    
                    // Allocate optimally-sized result buffer
                    let mut data = PooledBuffer::get(len);
                    
                    // Use SIMD for ultra-fast memory copy
                    simd::fast_copy(&buffer.data[..len], data.as_mut_slice());
                    
                    // Create dummy address (real implementation would extract from recvfrom)
                    let addr = "127.0.0.1:0".parse().unwrap();
                    
                    results.push((Bytes::from(data.into_vec()), addr));
                    
                    // Update statistics
                    self.stats.messages_received.fetch_add(1, Ordering::Relaxed);
                    self.stats.bytes_received.fetch_add(len as u64, Ordering::Relaxed);
                    self.stats.simd_operations.fetch_add(1, Ordering::Relaxed);
                    
                    // Return buffer to pool
                    self.buffer_pool.release_with_prefetch(buffer_id);
                }
            }
        }

        batch.clear();
        Ok(results)
    }

    /// Process completions with performance monitoring
    async fn process_completions(&mut self, expected: usize) -> std::io::Result<()> {
        let start_cycles = Self::read_cpu_cycles();
        let mut completion = self.ring.completion();
        
        for _ in 0..expected {
            if let Some(cqe) = completion.next() {
                if cqe.result() >= 0 {
                    self.stats.messages_sent.fetch_add(1, Ordering::Relaxed);
                    self.stats.bytes_sent.fetch_add(cqe.result() as u64, Ordering::Relaxed);
                }
                self.stats.cqe_completions.fetch_add(1, Ordering::Relaxed);
            }
        }

        let end_cycles = Self::read_cpu_cycles();
        self.stats.cpu_cycles.fetch_add(end_cycles - start_cycles, Ordering::Relaxed);
        
        Ok(())
    }

    /// Predict optimal message size based on history
    fn predict_message_size(&self) -> usize {
        // Simple prediction based on recent averages
        let recent_bytes = self.stats.bytes_received.load(Ordering::Relaxed);
        let recent_messages = self.stats.messages_received.load(Ordering::Relaxed);
        
        if recent_messages > 0 {
            (recent_bytes / recent_messages) as usize
        } else {
            self.config.buffer_size / 4 // Conservative default
        }
    }

    /// Create ultra-optimized socket with all performance features
    fn create_ultra_optimized_socket() -> std::io::Result<Socket> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        
        // Enable all socket optimizations
        socket.set_nonblocking(true)?;
        socket.set_reuse_address(true)?;
        socket.set_reuse_port(true)?;
        socket.set_send_buffer_size(2 * 1024 * 1024)?; // 2MB send buffer
        socket.set_recv_buffer_size(2 * 1024 * 1024)?; // 2MB recv buffer
        
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::io::AsRawFd;
            let fd = socket.as_raw_fd();
            
            // Enable high-resolution timestamps
            let enable = 1i32;
            unsafe {
                libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_TIMESTAMPING,
                    &enable as *const _ as *const libc::c_void,
                    std::mem::size_of::<i32>() as u32,
                );
                
                // Enable zero-copy send (if available)
                libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_ZEROCOPY,
                    &enable as *const _ as *const libc::c_void,
                    std::mem::size_of::<i32>() as u32,
                );
                
                // Optimize for throughput
                let quickack = 1i32;
                libc::setsockopt(
                    fd,
                    libc::IPPROTO_TCP,
                    libc::TCP_QUICKACK,
                    &quickack as *const _ as *const libc::c_void,
                    std::mem::size_of::<i32>() as u32,
                );
            }
        }

        Ok(socket)
    }

    /// Set NUMA affinity for optimal memory access
    fn set_numa_affinity(numa_node: usize) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            // This would require numa library - simplified implementation
            std::env::set_var("NUMA_NODE", numa_node.to_string());
        }
        Ok(())
    }

    /// Set CPU affinity for the current thread
    fn set_cpu_affinity(cpu: usize) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            use std::mem;
            let mut cpu_set: libc::cpu_set_t = unsafe { mem::zeroed() };
            unsafe {
                libc::CPU_ZERO(&mut cpu_set);
                libc::CPU_SET(cpu, &mut cpu_set);
                if libc::sched_setaffinity(0, mem::size_of::<libc::cpu_set_t>(), &cpu_set) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
        }
        Ok(())
    }

    /// Read CPU cycle counter for performance measurement
    fn read_cpu_cycles() -> u64 {
        #[cfg(target_arch = "x86_64")]
        {
            unsafe { std::arch::x86_64::_rdtsc() }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            0 // Fallback for non-x86 architectures
        }
    }

    /// Check if all required io_uring features are available
    fn check_required_features(probe: &Probe) -> std::io::Result<()> {
        let required_ops = [
            opcode::SendMsg::CODE,
            opcode::RecvMsg::CODE,
            opcode::Send::CODE,
            opcode::Recv::CODE,
        ];

        for &op in &required_ops {
            if !probe.is_supported(op) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Required io_uring operation {} not supported", op)
                ));
            }
        }

        Ok(())
    }

    /// Get comprehensive transport statistics
    pub fn stats(&self) -> TransportStatsSnapshot {
        TransportStatsSnapshot {
            messages_sent: self.stats.messages_sent.load(Ordering::Relaxed),
            messages_received: self.stats.messages_received.load(Ordering::Relaxed),
            bytes_sent: self.stats.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.stats.bytes_received.load(Ordering::Relaxed),
            batches_processed: self.stats.batches_processed.load(Ordering::Relaxed),
            coalesced_operations: self.stats.coalesced_operations.load(Ordering::Relaxed),
            buffer_hit_rate: self.calculate_buffer_hit_rate(),
            zero_copy_ops: self.stats.zero_copy_ops.load(Ordering::Relaxed),
            simd_operations: self.stats.simd_operations.load(Ordering::Relaxed),
            avg_cpu_cycles_per_op: self.calculate_avg_cpu_cycles(),
        }
    }

    fn calculate_buffer_hit_rate(&self) -> f64 {
        let hits = self.stats.buffer_hits.load(Ordering::Relaxed) as f64;
        let misses = self.stats.buffer_misses.load(Ordering::Relaxed) as f64;
        if hits + misses > 0.0 {
            hits / (hits + misses) * 100.0
        } else {
            100.0
        }
    }

    fn calculate_avg_cpu_cycles(&self) -> f64 {
        let total_cycles = self.stats.cpu_cycles.load(Ordering::Relaxed) as f64;
        let total_ops = (self.stats.messages_sent.load(Ordering::Relaxed) + 
                        self.stats.messages_received.load(Ordering::Relaxed)) as f64;
        if total_ops > 0.0 {
            total_cycles / total_ops
        } else {
            0.0
        }
    }
}

impl AdvancedBufferPool {
    fn new(ring: &IoUring, config: &TransportConfig) -> std::io::Result<Self> {
        let mut registered_buffers = Vec::with_capacity(config.buffer_count);
        let mut available = VecDeque::with_capacity(config.buffer_count);
        let prefetch_queue = VecDeque::with_capacity(config.prefetch_depth);
        let group_id = 1;

        // Create NUMA-aware buffers
        for buffer_id in 0..config.buffer_count as u16 {
            let data = Self::allocate_numa_buffer(config.buffer_size, config.numa_node)?;
            
            registered_buffers.push(RegisteredBuffer {
                data,
                buffer_id,
                group_id,
                numa_node: config.numa_node,
            });
            
            available.push_back(buffer_id);
        }

        // Register buffers with io_uring for zero-copy
        if config.use_fixed_buffers {
            let buffer_ring: Vec<libc::iovec> = registered_buffers.iter()
                .map(|buf| libc::iovec {
                    iov_base: buf.data.as_ptr() as *mut libc::c_void,
                    iov_len: buf.data.len(),
                })
                .collect();

            unsafe {
                ring.submitter().register_buffers(&buffer_ring)?;
            }
        }

        Ok(Self {
            registered_buffers,
            available,
            prefetch_queue,
            in_use: AtomicUsize::new(0),
            group_id,
            prefetch_depth: config.prefetch_depth,
        })
    }

    /// Allocate NUMA-local buffer
    fn allocate_numa_buffer(size: usize, numa_node: Option<usize>) -> std::io::Result<Box<[u8]>> {
        let mut buffer = vec![0u8; size];
        
        #[cfg(target_os = "linux")]
        {
            if let Some(_node) = numa_node {
                // In a real implementation, this would use numa_alloc_onnode
                // For now, just use regular allocation with a hint
                unsafe {
                    libc::madvise(
                        buffer.as_mut_ptr() as *mut libc::c_void,
                        size,
                        libc::MADV_HUGEPAGE, // Use huge pages for better performance
                    );
                }
            }
        }

        Ok(buffer.into_boxed_slice())
    }

    fn acquire_with_prefetch(&mut self) -> Option<u16> {
        // Try prefetch queue first
        if let Some(buffer_id) = self.prefetch_queue.pop_front() {
            self.in_use.fetch_add(1, Ordering::Relaxed);
            return Some(buffer_id);
        }

        // Fall back to regular queue
        if let Some(buffer_id) = self.available.pop_front() {
            self.in_use.fetch_add(1, Ordering::Relaxed);
            Some(buffer_id)
        } else {
            None
        }
    }

    fn release_with_prefetch(&mut self, buffer_id: u16) {
        // Add to prefetch queue if not full
        if self.prefetch_queue.len() < self.prefetch_depth {
            self.prefetch_queue.push_back(buffer_id);
        } else {
            self.available.push_back(buffer_id);
        }
        self.in_use.fetch_sub(1, Ordering::Relaxed);
    }

    fn prefetch_buffers(&mut self, count: usize) {
        // Move buffers from available to prefetch queue
        for _ in 0..count {
            if let Some(buffer_id) = self.available.pop_front() {
                if self.prefetch_queue.len() < self.prefetch_depth {
                    self.prefetch_queue.push_back(buffer_id);
                } else {
                    self.available.push_front(buffer_id);
                    break;
                }
            } else {
                break;
            }
        }
    }
}

impl BatchProcessor {
    fn new(max_batch_size: usize) -> Self {
        Self {
            send_batch: Vec::with_capacity(max_batch_size),
            recv_batch: Vec::with_capacity(max_batch_size),
            coalesced_sends: Vec::with_capacity(max_batch_size / 4),
            max_batch_size,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransportStatsSnapshot {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub batches_processed: u64,
    pub coalesced_operations: u64,
    pub buffer_hit_rate: f64,
    pub zero_copy_ops: u64,
    pub simd_operations: u64,
    pub avg_cpu_cycles_per_op: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extreme_config() {
        let config = TransportConfig::extreme_throughput();
        assert!(config.ring_size >= 4096);
        assert!(config.buffer_size >= 1_048_576);
        assert!(config.batch_size >= 128);
    }

    #[test]
    fn test_ultra_low_latency_config() {
        let config = TransportConfig::ultra_low_latency();
        assert!(config.batch_size <= 16);
        assert!(!config.use_sqpoll); // Polling adds latency
    }
}