use std::sync::Arc;
use std::os::unix::io::AsRawFd;
use io_uring::{IoUring, opcode, types, Probe};
use socket2::{Socket, Domain, Type, Protocol};
use bytes::Bytes;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Advanced io_uring client with optimized buffer management and polling
pub struct IoUringOptimizedClient {
    ring: IoUring,
    socket: Socket,
    buffer_ring: BufferRing,
    sq_poll_thread: Option<std::thread::JoinHandle<()>>,
    cq_poll_thread: Option<std::thread::JoinHandle<()>>,
    stats: Arc<ClientStats>,
}

/// Buffer ring for zero-copy operations
struct BufferRing {
    buffers: Vec<Arc<RegisteredBuffer>>,
    provided: AtomicUsize,
    consumed: AtomicUsize,
    group_id: u16,
}

/// Pre-registered buffer with io_uring
struct RegisteredBuffer {
    data: Box<[u8]>,
    bid: u16,
    len: usize,
}

#[derive(Default)]
struct ClientStats {
    submissions: AtomicUsize,
    completions: AtomicUsize,
    buffer_hits: AtomicUsize,
    buffer_misses: AtomicUsize,
}

impl IoUringOptimizedClient {
    pub fn new(
        ring_size: u32,
        buffer_size: usize,
        buffer_count: usize,
        use_polling: bool,
    ) -> std::io::Result<Self> {
        // Create ring with SQPOLL if requested
        let mut ring = if use_polling {
            IoUring::builder()
                .setup_sqpoll(100) // 100ms timeout
                .build(ring_size)?
        } else {
            IoUring::new(ring_size)?
        };

        // Check for required features
        let mut probe = Probe::new();
        ring.submitter().register_probe(&mut probe)?;
        
        if !probe.is_supported(opcode::SendMsg::CODE) ||
           !probe.is_supported(opcode::RecvMsg::CODE) ||
           !probe.is_supported(opcode::ProvideBuffers::CODE) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Required io_uring operations not supported"
            ));
        }

        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_nonblocking(true)?;

        // Setup buffer ring
        let buffer_ring = BufferRing::new(&ring, buffer_size, buffer_count)?;

        let stats = Arc::new(ClientStats::default());
        let client = Self {
            ring,
            socket,
            buffer_ring,
            sq_poll_thread: None,
            cq_poll_thread: None,
            stats: stats.clone(),
        };

        // Start polling threads if requested
        if use_polling {
            client.start_polling_threads()?;
        }

        Ok(client)
    }

    fn start_polling_threads(&mut self) -> std::io::Result<()> {
        let stats = self.stats.clone();
        let ring_fd = self.ring.as_raw_fd();

        // Submission queue polling thread
        let sq_stats = stats.clone();
        self.sq_poll_thread = Some(std::thread::spawn(move || {
            let mut sqe_batch = Vec::with_capacity(32);
            loop {
                // Direct SQ polling
                unsafe {
                    let sq = io_uring_sys::io_uring_sq::from_raw_fd(ring_fd);
                    while (*sq.sqe_head != *sq.sqe_tail) {
                        sqe_batch.push(*sq.sqes.offset(*sq.sqe_head as isize));
                        *sq.sqe_head = (*sq.sqe_head + 1) % *sq.ring_entries;
                    }
                }

                if !sqe_batch.is_empty() {
                    sq_stats.submissions.fetch_add(sqe_batch.len(), Ordering::Relaxed);
                    sqe_batch.clear();
                }

                std::thread::sleep(std::time::Duration::from_micros(1));
            }
        }));

        // Completion queue polling thread
        let cq_stats = stats.clone();
        self.cq_poll_thread = Some(std::thread::spawn(move || {
            loop {
                // Direct CQ polling
                unsafe {
                    let cq = io_uring_sys::io_uring_cq::from_raw_fd(ring_fd);
                    while (*cq.head != *cq.tail) {
                        let cqe = *cq.cqes.offset(*cq.head as isize);
                        if cqe.res >= 0 {
                            cq_stats.completions.fetch_add(1, Ordering::Relaxed);
                        }
                        *cq.head = (*cq.head + 1) % *cq.ring_entries;
                    }
                }

                std::thread::sleep(std::time::Duration::from_micros(1));
            }
        }));

        Ok(())
    }

    pub async fn send_message_multishot(&mut self, data: &[u8]) -> std::io::Result<()> {
        let buf = self.buffer_ring.acquire()?;
        buf.data[..data.len()].copy_from_slice(data);

        // Setup multishot send
        let msghdr = unsafe {
            let mut msg: libc::msghdr = std::mem::zeroed();
            let mut iov: libc::iovec = std::mem::zeroed();
            iov.iov_base = buf.data.as_ptr() as *mut _;
            iov.iov_len = data.len();
            msg.msg_iov = &mut iov as *mut _;
            msg.msg_iovlen = 1;
            msg
        };

        // Prepare multishot submission
        let mut sqe = opcode::SendMsg::new(
            types::Fd(self.socket.as_raw_fd()),
            &msghdr as *const _
        )
        .build()
        .flags(io_uring::squeue::Flags::MULTISHOT);

        sqe.user_data() = buf.bid as u64;

        unsafe {
            self.ring.submission()
                .push(&sqe)
                .map_err(|_| std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to push to submission queue"
                ))?;
        }

        self.ring.submit_and_wait(1)?;

        Ok(())
    }

    pub async fn receive_message_fixed(&mut self) -> std::io::Result<Bytes> {
        let buf = self.buffer_ring.acquire()?;

        // Setup fixed buffer receive
        let mut sqe = opcode::RecvMsg::new(
            types::Fd(self.socket.as_raw_fd()),
            &mut [] as *mut [u8]
        )
        .build();

        sqe.flags |= io_uring::squeue::Flags::FIXED_BUF;
        sqe.buf_index = buf.bid;

        unsafe {
            self.ring.submission()
                .push(&sqe)
                .map_err(|_| std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to push to submission queue"
                ))?;
        }

        let cqe = self.ring.completion().next()
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::Other,
                "No completion entry"
            ))?;

        if cqe.result() < 0 {
            return Err(std::io::Error::from_raw_os_error(-cqe.result()));
        }

        let len = cqe.result() as usize;
        Ok(Bytes::copy_from_slice(&buf.data[..len]))
    }

    pub fn get_stats(&self) -> ClientStats {
        ClientStats {
            submissions: AtomicUsize::new(self.stats.submissions.load(Ordering::Relaxed)),
            completions: AtomicUsize::new(self.stats.completions.load(Ordering::Relaxed)),
            buffer_hits: AtomicUsize::new(self.stats.buffer_hits.load(Ordering::Relaxed)),
            buffer_misses: AtomicUsize::new(self.stats.buffer_misses.load(Ordering::Relaxed)),
        }
    }
}

impl BufferRing {
    fn new(ring: &IoUring, buffer_size: usize, count: usize) -> std::io::Result<Self> {
        let mut buffers = Vec::with_capacity(count);
        let group_id = 1; // Use consistent group ID

        // Register buffers with io_uring
        for bid in 0..count as u16 {
            let mut data = vec![0u8; buffer_size].into_boxed_slice();
            
            // Provide buffer to io_uring
            let provide_op = opcode::ProvideBuffers::new(
                data.as_mut_ptr(),
                buffer_size as i32,
                1,
                group_id,
                bid
            );

            unsafe {
                ring.submission()
                    .push(&provide_op.build())
                    .map_err(|_| std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Failed to provide buffer"
                    ))?;
            }

            buffers.push(Arc::new(RegisteredBuffer {
                data,
                bid,
                len: buffer_size,
            }));
        }

        ring.submit_and_wait(count)?;

        Ok(Self {
            buffers,
            provided: AtomicUsize::new(count),
            consumed: AtomicUsize::new(0),
            group_id,
        })
    }

    fn acquire(&self) -> std::io::Result<Arc<RegisteredBuffer>> {
        let current = self.consumed.fetch_add(1, Ordering::Relaxed);
        let idx = current % self.buffers.len();
        
        if current >= self.provided.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "No buffers available"
            ));
        }

        Ok(self.buffers[idx].clone())
    }
}