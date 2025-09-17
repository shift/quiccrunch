use socket2::{Domain, Protocol, Socket, Type};
use std::net::SocketAddr;
use std::time::Instant;

/// Demonstrates SO_REUSEPORT socket optimization for QuicCrunch worker pool
fn main() {
    println!("QuicCrunch SO_REUSEPORT Socket Optimization Demo");
    println!("================================================");
    
    // Test 1: Basic socket creation with optimizations
    test_socket_creation_performance();
    
    // Test 2: Socket buffer optimization
    test_socket_buffer_optimization();
    
    // Test 3: Platform-specific optimizations
    test_platform_optimizations();
    
    // Test 4: Load distribution simulation
    test_load_distribution_simulation();
}

fn test_socket_creation_performance() {
    println!("\n1. Testing Socket Creation Performance:");
    
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    
    // Standard socket creation
    let start = Instant::now();
    for _ in 0..100 {
        let _socket = std::net::UdpSocket::bind(addr).unwrap();
    }
    let standard_time = start.elapsed();
    
    // Optimized socket creation with SO_REUSEPORT
    let start = Instant::now();
    for _ in 0..100 {
        let _socket = create_optimized_socket(addr, true).unwrap();
    }
    let optimized_time = start.elapsed();
    
    println!("  Standard sockets (100x): {:?}", standard_time);
    println!("  Optimized sockets (100x): {:?}", optimized_time);
    println!("  Improvement: {:.2}%", 
        (standard_time.as_nanos() as f64 - optimized_time.as_nanos() as f64) 
        / standard_time.as_nanos() as f64 * 100.0);
}

fn test_socket_buffer_optimization() {
    println!("\n2. Testing Socket Buffer Optimization:");
    
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    
    // Test different buffer sizes
    let buffer_sizes = [64 * 1024, 512 * 1024, 2 * 1024 * 1024]; // 64KB, 512KB, 2MB
    
    for &buffer_size in &buffer_sizes {
        let start = Instant::now();
        let socket = create_socket_with_buffers(addr, buffer_size).unwrap();
        let creation_time = start.elapsed();
        
        // Get actual buffer sizes
        let recv_size = socket.recv_buffer_size().unwrap_or(0);
        let send_size = socket.send_buffer_size().unwrap_or(0);
        
        println!("  Buffer {}: creation {:?}, recv: {}, send: {}", 
            format_bytes(buffer_size), creation_time, 
            format_bytes(recv_size), format_bytes(send_size));
    }
}

fn test_platform_optimizations() {
    println!("\n3. Testing Platform-Specific Optimizations:");
    
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    
    #[cfg(target_os = "linux")]
    {
        println!("  Platform: Linux");
        let socket = create_socket_with_linux_optimizations(addr).unwrap();
        println!("  ✓ Linux optimizations applied successfully");
        
        // Test CPU count detection for worker scaling
        let cpu_count = num_cpus::get();
        println!("  Detected CPUs: {}", cpu_count);
        println!("  Recommended workers for small RPC: {}", cpu_count);
        println!("  Recommended workers for medium RPC: {}", cpu_count * 3 / 2);
        println!("  Recommended workers for large RPC: {}", cpu_count * 2);
    }
    
    #[cfg(target_os = "macos")]
    {
        println!("  Platform: macOS");
        let socket = create_socket_with_macos_optimizations(addr).unwrap();
        println!("  ✓ macOS optimizations applied successfully");
    }
    
    #[cfg(target_os = "windows")]
    {
        println!("  Platform: Windows");
        let socket = create_socket_with_windows_optimizations(addr).unwrap();
        println!("  ✓ Windows optimizations applied successfully");
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        println!("  Platform: Other (using generic optimizations)");
        let socket = create_optimized_socket(addr, true).unwrap();
        println!("  ✓ Generic optimizations applied successfully");
    }
}

fn test_load_distribution_simulation() {
    println!("\n4. Testing Load Distribution Simulation:");
    
    let worker_counts = [2, 4, 8, 16];
    let packet_count = 10000;
    
    for &workers in &worker_counts {
        let distribution = simulate_so_reuseport_distribution(workers, packet_count);
        let fairness = calculate_fairness_score(&distribution);
        
        println!("  {} workers: fairness {:.3}, distribution {:?}", 
            workers, fairness, distribution);
    }
}

/// Create optimized socket with SO_REUSEPORT and buffer optimization
fn create_optimized_socket(addr: SocketAddr, use_reuseport: bool) -> Result<Socket, Box<dyn std::error::Error>> {
    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    
    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
    
    // Enable SO_REUSEPORT for load distribution across workers
    if use_reuseport {
        #[cfg(unix)]
        socket.set_reuse_port(true)?;
    }
    
    // Optimize buffer sizes for QUIC workloads
    socket.set_recv_buffer_size(2 * 1024 * 1024)?; // 2MB
    socket.set_send_buffer_size(1024 * 1024)?; // 1MB
    
    socket.bind(&socket2::SockAddr::from(addr))?;
    socket.set_nonblocking(true)?;
    
    Ok(socket)
}

/// Create socket with specific buffer sizes
fn create_socket_with_buffers(addr: SocketAddr, buffer_size: usize) -> Result<Socket, Box<dyn std::error::Error>> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    
    socket.set_recv_buffer_size(buffer_size)?;
    socket.set_send_buffer_size(buffer_size)?;
    
    socket.bind(&socket2::SockAddr::from(addr))?;
    
    Ok(socket)
}

/// Linux-specific socket optimizations
#[cfg(target_os = "linux")]
fn create_socket_with_linux_optimizations(addr: SocketAddr) -> Result<Socket, Box<dyn std::error::Error>> {
    let socket = create_optimized_socket(addr, true)?;
    
    // Additional Linux-specific optimizations
    use socket2::SockRef;
    let sock_ref = SockRef::from(&socket);
    
    // Set receive timeout to prevent blocking
    let timeout = std::time::Duration::from_millis(1);
    let _ = sock_ref.set_read_timeout(Some(timeout));
    
    Ok(socket)
}

/// macOS-specific socket optimizations
#[cfg(target_os = "macos")]
fn create_socket_with_macos_optimizations(addr: SocketAddr) -> Result<Socket, Box<dyn std::error::Error>> {
    let socket = create_optimized_socket(addr, true)?;
    
    use socket2::SockRef;
    let sock_ref = SockRef::from(&socket);
    
    // Set socket priority for better QoS
    let _ = sock_ref.set_priority(7); // High priority
    
    Ok(socket)
}

/// Windows-specific socket optimizations
#[cfg(target_os = "windows")]
fn create_socket_with_windows_optimizations(addr: SocketAddr) -> Result<Socket, Box<dyn std::error::Error>> {
    let socket = create_optimized_socket(addr, false)?; // SO_REUSEPORT not available on Windows
    
    // Windows-specific optimizations would go here
    // For now, just return the basic optimized socket
    
    Ok(socket)
}

/// Simulate SO_REUSEPORT packet distribution across workers
fn simulate_so_reuseport_distribution(worker_count: usize, packet_count: usize) -> Vec<usize> {
    let mut distribution = vec![0; worker_count];
    
    // Simulate kernel hash-based distribution
    // In reality, this depends on the kernel's hash function
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    for i in 0..packet_count {
        // Simulate packet characteristics that kernel would hash
        let mut hasher = DefaultHasher::new();
        i.hash(&mut hasher);
        let hash = hasher.finish();
        
        let worker_id = (hash as usize) % worker_count;
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

/// Format bytes in human-readable format
fn format_bytes(bytes: usize) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}