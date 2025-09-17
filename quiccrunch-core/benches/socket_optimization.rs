use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use socket2::{Domain, Protocol, Socket, Type};

/// Benchmark SO_REUSEPORT socket performance characteristics
fn bench_so_reuseport_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("so_reuseport");
    
    // Test socket creation overhead with SO_REUSEPORT
    group.bench_function("socket_creation_with_reuseport", |b| {
        b.iter(|| {
            let socket = create_optimized_socket("127.0.0.1:0".parse().unwrap(), true);
            black_box(socket)
        })
    });
    
    group.bench_function("socket_creation_without_reuseport", |b| {
        b.iter(|| {
            let socket = create_optimized_socket("127.0.0.1:0".parse().unwrap(), false);
            black_box(socket)
        })
    });
    
    group.finish();
}

/// Benchmark socket buffer optimization impact
fn bench_socket_buffer_optimization(c: &mut Criterion) {
    let mut group = c.benchmark_group("socket_buffers");
    
    // Test different buffer sizes
    for buffer_size in [64 * 1024, 512 * 1024, 2 * 1024 * 1024].iter() {
        group.bench_function(format!("buffer_size_{}", buffer_size), |b| {
            b.iter(|| {
                let socket = create_socket_with_buffers(*buffer_size);
                black_box(socket)
            })
        });
    }
    
    group.finish();
}

/// Benchmark load distribution fairness across workers
fn bench_load_distribution_fairness(c: &mut Criterion) {
    let mut group = c.benchmark_group("load_distribution");
    group.throughput(Throughput::Elements(1000));
    
    // Test with different worker counts
    for worker_count in [2, 4, 8, 16].iter() {
        group.bench_function(format!("workers_{}", worker_count), |b| {
            b.iter(|| {
                let distribution = simulate_packet_distribution(*worker_count, 1000);
                let fairness = calculate_fairness_score(&distribution);
                black_box(fairness)
            })
        });
    }
    
    group.finish();
}

/// Benchmark cross-platform socket optimizations
fn bench_platform_optimizations(c: &mut Criterion) {
    let mut group = c.benchmark_group("platform_optimizations");
    
    group.bench_function("linux_optimizations", |b| {
        b.iter(|| {
            #[cfg(target_os = "linux")]
            {
                let socket = create_socket_with_linux_optimizations();
                black_box(socket)
            }
            #[cfg(not(target_os = "linux"))]
            {
                black_box(0)
            }
        })
    });
    
    group.bench_function("macos_optimizations", |b| {
        b.iter(|| {
            #[cfg(target_os = "macos")]
            {
                let socket = create_socket_with_macos_optimizations();
                black_box(socket)
            }
            #[cfg(not(target_os = "macos"))]
            {
                black_box(0)
            }
        })
    });
    
    group.finish();
}

/// Benchmark packet processing latency with different socket configurations
fn bench_packet_processing_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("packet_latency");
    group.throughput(Throughput::Bytes(1350)); // QUIC packet size
    
    let test_data = vec![0u8; 1350]; // Typical QUIC packet
    
    group.bench_function("optimized_socket_send_recv", |b| {
        let (sender, receiver) = create_optimized_socket_pair();
        
        b.iter(|| {
            let start = Instant::now();
            
            // Simulate packet send/receive
            let _ = sender.send_to(&test_data, receiver.local_addr().unwrap());
            
            let mut buf = [0u8; 2048];
            let _ = receiver.recv_from(&mut buf);
            
            black_box(start.elapsed())
        })
    });
    
    group.bench_function("standard_socket_send_recv", |b| {
        let (sender, receiver) = create_standard_socket_pair();
        
        b.iter(|| {
            let start = Instant::now();
            
            // Simulate packet send/receive
            let _ = sender.send_to(&test_data, receiver.local_addr().unwrap());
            
            let mut buf = [0u8; 2048];
            let _ = receiver.recv_from(&mut buf);
            
            black_box(start.elapsed())
        })
    });
    
    group.finish();
}

/// Benchmark worker thread efficiency with CPU affinity
fn bench_cpu_affinity_impact(c: &mut Criterion) {
    let mut group = c.benchmark_group("cpu_affinity");
    
    group.bench_function("with_cpu_affinity", |b| {
        b.iter(|| {
            #[cfg(target_os = "linux")]
            {
                let result = simulate_worker_with_affinity(0);
                black_box(result)
            }
            #[cfg(not(target_os = "linux"))]
            {
                black_box(0)
            }
        })
    });
    
    group.bench_function("without_cpu_affinity", |b| {
        b.iter(|| {
            let result = simulate_worker_without_affinity();
            black_box(result)
        })
    });
    
    group.finish();
}

/// Helper function to create optimized socket
fn create_optimized_socket(addr: SocketAddr, use_reuseport: bool) -> Result<Socket, Box<dyn std::error::Error>> {
    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    
    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
    
    if use_reuseport {
        #[cfg(unix)]
        socket.set_reuse_port(true)?;
    }
    
    // Set socket buffer optimizations
    socket.set_recv_buffer_size(2 * 1024 * 1024)?; // 2MB
    socket.set_send_buffer_size(1024 * 1024)?; // 1MB
    
    socket.bind(&socket2::SockAddr::from(addr))?;
    socket.set_nonblocking(true)?;
    
    Ok(socket)
}

/// Helper function to create socket with specific buffer sizes
fn create_socket_with_buffers(buffer_size: usize) -> Result<Socket, Box<dyn std::error::Error>> {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    
    socket.set_recv_buffer_size(buffer_size)?;
    socket.set_send_buffer_size(buffer_size)?;
    
    socket.bind(&socket2::SockAddr::from(addr))?;
    
    Ok(socket)
}

/// Simulate packet distribution across workers
fn simulate_packet_distribution(worker_count: usize, packet_count: usize) -> Vec<usize> {
    let mut distribution = vec![0; worker_count];
    
    // Simulate SO_REUSEPORT kernel distribution
    // In reality, this would be more complex and depend on hash functions
    for i in 0..packet_count {
        let worker_id = i % worker_count;
        distribution[worker_id] += 1;
    }
    
    distribution
}

/// Calculate fairness score for load distribution
fn calculate_fairness_score(distribution: &[usize]) -> f64 {
    let total: usize = distribution.iter().sum();
    let worker_count = distribution.len();
    let expected_per_worker = total as f64 / worker_count as f64;
    
    let mut variance = 0.0;
    for &count in distribution {
        let deviation = count as f64 - expected_per_worker;
        variance += deviation * deviation;
    }
    
    let standard_deviation = (variance / worker_count as f64).sqrt();
    
    // Fairness score: 1.0 = perfectly fair, lower = less fair
    1.0 - (standard_deviation / expected_per_worker).min(1.0)
}

/// Create optimized socket pair for testing
fn create_optimized_socket_pair() -> (UdpSocket, UdpSocket) {
    let sender = create_optimized_socket("127.0.0.1:0".parse().unwrap(), false)
        .unwrap()
        .into();
    let receiver = create_optimized_socket("127.0.0.1:0".parse().unwrap(), false)
        .unwrap()
        .into();
    
    (sender, receiver)
}

/// Create standard socket pair for comparison
fn create_standard_socket_pair() -> (UdpSocket, UdpSocket) {
    let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
    let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
    
    (sender, receiver)
}

/// Linux-specific socket optimizations
#[cfg(target_os = "linux")]
fn create_socket_with_linux_optimizations() -> Result<Socket, Box<dyn std::error::Error>> {
    let socket = create_optimized_socket("127.0.0.1:0".parse().unwrap(), true)?;
    
    // Additional Linux-specific optimizations could go here
    // For now, just return the basic optimized socket
    
    Ok(socket)
}

/// macOS-specific socket optimizations
#[cfg(target_os = "macos")]
fn create_socket_with_macos_optimizations() -> Result<Socket, Box<dyn std::error::Error>> {
    let socket = create_optimized_socket("127.0.0.1:0".parse().unwrap(), true)?;
    
    // Additional macOS-specific optimizations could go here
    // For now, just return the basic optimized socket
    
    Ok(socket)
}

/// Simulate worker with CPU affinity
#[cfg(target_os = "linux")]
fn simulate_worker_with_affinity(worker_id: usize) -> usize {
    // In a real benchmark, this would set CPU affinity and measure performance
    // For now, just simulate some work
    let mut sum = 0;
    for i in 0..1000 {
        sum += i * worker_id;
    }
    sum
}

/// Simulate worker without CPU affinity
fn simulate_worker_without_affinity() -> usize {
    let mut sum = 0;
    for i in 0..1000 {
        sum += i;
    }
    sum
}

criterion_group!(
    socket_optimization_benches,
    bench_so_reuseport_efficiency,
    bench_socket_buffer_optimization,
    bench_load_distribution_fairness,
    bench_platform_optimizations,
    bench_packet_processing_latency,
    bench_cpu_affinity_impact
);
criterion_main!(socket_optimization_benches);