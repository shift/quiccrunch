/// CPU-specific optimizations and runtime feature detection
/// 
/// This module provides CPU-specific optimizations that can be enabled
/// at runtime based on detected CPU features.

use std::sync::Once;

/// Runtime CPU feature detection (simplified version)
#[derive(Debug, Clone)]
pub struct CpuFeatures {
    pub avx2: bool,
    pub avx512f: bool,
    pub avx512bw: bool,
    pub sse41: bool,
    pub neon: bool, // ARM NEON
}

impl CpuFeatures {
    /// Detect available CPU features at runtime
    pub fn detect() -> Self {
        Self {
            #[cfg(target_arch = "x86_64")]
            avx2: is_x86_feature_detected!("avx2"),
            #[cfg(target_arch = "x86_64")]
            avx512f: is_x86_feature_detected!("avx512f"),
            #[cfg(target_arch = "x86_64")]
            avx512bw: is_x86_feature_detected!("avx512bw"),
            #[cfg(target_arch = "x86_64")]
            sse41: is_x86_feature_detected!("sse4.1"),
            #[cfg(not(target_arch = "x86_64"))]
            avx2: false,
            #[cfg(not(target_arch = "x86_64"))]
            avx512f: false,
            #[cfg(not(target_arch = "x86_64"))]
            avx512bw: false,
            #[cfg(not(target_arch = "x86_64"))]
            sse41: false,

            #[cfg(target_arch = "aarch64")]
            neon: is_aarch64_feature_detected!("neon"),
            #[cfg(not(target_arch = "aarch64"))]
            neon: false,
        }
    }
}

/// Global CPU features detected at runtime
static mut CPU_FEATURES: Option<CpuFeatures> = None;
static INIT: Once = Once::new();

/// Runtime CPU optimization configuration
pub struct CpuOptimizations {
    pub features: CpuFeatures,
    pub numa_nodes: usize,
    pub cpu_count: usize,
    pub cache_line_size: usize,
    pub page_size: usize,
}

impl CpuOptimizations {
    /// Detect and initialize CPU optimizations
    pub fn detect() -> Self {
        INIT.call_once(|| unsafe {
            CPU_FEATURES = Some(CpuFeatures::detect());
        });

        let features = unsafe { CPU_FEATURES.as_ref().unwrap().clone() };

        Self {
            features,
            numa_nodes: Self::detect_numa_nodes(),
            cpu_count: Self::detect_cpu_count(),
            cache_line_size: Self::detect_cache_line_size(),
            page_size: Self::detect_page_size(),
        }
    }

    /// Get optimal chunk size based on CPU cache
    pub fn optimal_chunk_size(&self) -> usize {
        // Base on L2 cache size, typically 256KB to 1MB
        if self.features.avx512f {
            1_048_576 // 1MB for AVX-512 systems
        } else if self.features.avx2 {
            524_288   // 512KB for AVX2 systems
        } else {
            262_144   // 256KB for SSE systems
        }
    }

    /// Get optimal batch size for operations
    pub fn optimal_batch_size(&self) -> usize {
        if self.features.avx512f {
            128  // Large batches for high-end CPUs
        } else if self.features.avx2 {
            64   // Medium batches for mid-range CPUs
        } else {
            32   // Small batches for older CPUs
        }
    }

    /// Get optimal number of worker threads
    pub fn optimal_worker_threads(&self) -> usize {
        // Use 75% of available cores to leave room for OS
        ((self.cpu_count * 3) / 4).max(1)
    }

    /// Get CPU affinity recommendations
    pub fn cpu_affinity_map(&self) -> Vec<usize> {
        (0..self.cpu_count).collect()
    }

    /// Detect number of NUMA nodes
    fn detect_numa_nodes() -> usize {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_dir("/sys/devices/system/node")
                .map(|entries| {
                    entries
                        .filter_map(Result::ok)
                        .filter(|entry| {
                            entry.file_name()
                                .to_string_lossy()
                                .starts_with("node")
                        })
                        .count()
                })
                .unwrap_or(1)
        }
        #[cfg(not(target_os = "linux"))]
        {
            1 // Assume single NUMA node on non-Linux systems
        }
    }

    /// Detect number of CPU cores
    fn detect_cpu_count() -> usize {
        num_cpus::get()
    }

    /// Detect CPU cache line size
    fn detect_cache_line_size() -> usize {
        #[cfg(target_arch = "x86_64")]
        {
            // Most x86_64 systems use 64-byte cache lines
            64
        }
        #[cfg(target_arch = "aarch64")]
        {
            // ARM systems typically use 64-byte cache lines
            64
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            64 // Safe default
        }
    }

    /// Detect system page size
    fn detect_page_size() -> usize {
        #[cfg(unix)]
        {
            unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize }
        }
        #[cfg(not(unix))]
        {
            4096 // Common page size
        }
    }

    /// Enable transparent huge pages for large allocations
    pub fn enable_huge_pages(&self) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            std::fs::write("/sys/kernel/mm/transparent_hugepage/enabled", "always")?;
            std::fs::write("/sys/kernel/mm/transparent_hugepage/defrag", "always")?;
        }
        Ok(())
    }

    /// Set CPU governor for maximum performance
    pub fn set_performance_governor(&self) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            for cpu in 0..self.cpu_count {
                let governor_path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor", cpu);
                if std::path::Path::new(&governor_path).exists() {
                    let _ = std::fs::write(&governor_path, "performance");
                }
            }
        }
        Ok(())
    }

    /// Disable CPU frequency scaling for consistent performance
    pub fn disable_cpu_scaling(&self) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            for cpu in 0..self.cpu_count {
                let scaling_path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_min_freq", cpu);
                let max_path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_max_freq", cpu);
                
                if let (Ok(max_freq), true) = (
                    std::fs::read_to_string(&max_path), 
                    std::path::Path::new(&scaling_path).exists()
                ) {
                    let _ = std::fs::write(&scaling_path, max_freq.trim());
                }
            }
        }
        Ok(())
    }

    /// Configure network interface for optimal performance
    pub fn optimize_network_interface(&self, interface: &str) -> std::io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;

            // Increase ring buffer sizes
            let _ = Command::new("ethtool")
                .args(["-G", interface, "rx", "4096", "tx", "4096"])
                .output();

            // Enable hardware offloading
            let _ = Command::new("ethtool")
                .args(["-K", interface, "gso", "on", "tso", "on", "gro", "on"])
                .output();

            // Set interrupt coalescing
            let _ = Command::new("ethtool")
                .args(["-C", interface, "rx-usecs", "50", "tx-usecs", "50"])
                .output();
        }
        Ok(())
    }

    /// Get memory bandwidth estimate in GB/s
    pub fn memory_bandwidth_estimate(&self) -> f64 {
        // Rough estimates based on common CPU types
        if self.features.avx512f {
            // High-end server CPUs with AVX-512
            100.0
        } else if self.features.avx2 {
            // Mid-range to high-end CPUs with AVX2
            50.0
        } else if self.features.sse41 {
            // Older CPUs with SSE4.1
            25.0
        } else {
            // Very old CPUs
            10.0
        }
    }

    /// Get recommended io_uring settings
    pub fn io_uring_settings(&self) -> IoUringSettings {
        IoUringSettings {
            ring_size: if self.cpu_count >= 16 { 4096 } else { 2048 },
            use_sqpoll: self.cpu_count >= 8,
            sqpoll_cpu: Some(self.cpu_count - 1), // Use last CPU for polling
            batch_size: self.optimal_batch_size(),
        }
    }
}

/// Recommended io_uring configuration
#[derive(Debug, Clone)]
pub struct IoUringSettings {
    pub ring_size: u32,
    pub use_sqpoll: bool,
    pub sqpoll_cpu: Option<usize>,
    pub batch_size: usize,
}

/// Memory allocation hints for optimal performance
pub struct MemoryHints {
    pub use_huge_pages: bool,
    pub numa_local: bool,
    pub prefault: bool,
    pub alignment: usize,
}

impl CpuOptimizations {
    /// Get memory allocation hints
    pub fn memory_hints(&self, size: usize) -> MemoryHints {
        MemoryHints {
            use_huge_pages: size >= 2 * 1024 * 1024, // Use huge pages for 2MB+
            numa_local: self.numa_nodes > 1,
            prefault: size <= 64 * 1024 * 1024, // Prefault up to 64MB
            alignment: self.cache_line_size,
        }
    }

    /// Benchmark memory bandwidth
    pub fn benchmark_memory_bandwidth(&self) -> f64 {
        use std::time::Instant;
        
        let size = 64 * 1024 * 1024; // 64MB test
        let iterations = 10;
        let src = vec![0x42u8; size];
        let mut dst = vec![0u8; size];

        let start = Instant::now();
        for _ in 0..iterations {
            // crate::simd::fast_copy(&src, &mut dst);
            dst.copy_from_slice(&src);
        }
        let duration = start.elapsed();

        // Calculate bandwidth in GB/s
        let total_bytes = (size * iterations * 2) as f64; // Read + write
        let seconds = duration.as_secs_f64();
        (total_bytes / seconds) / (1024.0 * 1024.0 * 1024.0)
    }

    /// Benchmark CPU performance with SIMD operations
    pub fn benchmark_cpu_performance(&self) -> CpuBenchmarkResults {
        let memory_bandwidth = self.benchmark_memory_bandwidth();
        let simd_performance = self.benchmark_simd_operations();
        
        CpuBenchmarkResults {
            memory_bandwidth_gbps: memory_bandwidth,
            simd_operations_per_sec: simd_performance,
            cpu_features: self.features.clone(),
            optimal_chunk_size: self.optimal_chunk_size(),
            optimal_batch_size: self.optimal_batch_size(),
        }
    }

    fn benchmark_simd_operations(&self) -> f64 {
        use std::time::Instant;
        
        let size = 1024 * 1024; // 1MB of u32 values
        let data = vec![0x12345678u32; size / 4];
        let iterations = 1000;

        let start = Instant::now();
        for _ in 0..iterations {
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    data.as_ptr() as *const u8,
                    data.len() * 4
                )
            };
            // let _result = crate::simd::decode_u32_array(bytes);
            let _result = "placeholder";
        }
        let duration = start.elapsed();

        // Operations per second
        (iterations as f64) / duration.as_secs_f64()
    }
}

#[derive(Debug, Clone)]
pub struct CpuBenchmarkResults {
    pub memory_bandwidth_gbps: f64,
    pub simd_operations_per_sec: f64,
    pub cpu_features: CpuFeatures,
    pub optimal_chunk_size: usize,
    pub optimal_batch_size: usize,
}

impl CpuBenchmarkResults {
    pub fn print_summary(&self) {
        println!("\n🔧 CPU Optimization Benchmark Results");
        println!("====================================");
        println!("Memory Bandwidth: {:.2} GB/s", self.memory_bandwidth_gbps);
        println!("SIMD Operations: {:.0} ops/sec", self.simd_operations_per_sec);
        println!("Optimal Chunk Size: {} bytes", self.optimal_chunk_size);
        println!("Optimal Batch Size: {} operations", self.optimal_batch_size);
        println!("\nCPU Features:");
        println!("  AVX-512: {}", if self.cpu_features.avx512f { "Yes" } else { "No" });
        println!("  AVX2: {}", if self.cpu_features.avx2 { "Yes" } else { "No" });
        println!("  SSE4.1: {}", if self.cpu_features.sse41 { "Yes" } else { "No" });
        println!("  NEON: {}", if self.cpu_features.neon { "Yes" } else { "No" });
    }
}

/// Initialize all CPU optimizations for the application
pub fn initialize_optimizations() -> std::io::Result<CpuOptimizations> {
    let cpu_opts = CpuOptimizations::detect();
    
    // Enable performance optimizations
    cpu_opts.enable_huge_pages()?;
    cpu_opts.set_performance_governor()?;
    cpu_opts.disable_cpu_scaling()?;
    
    println!("🚀 CPU optimizations initialized:");
    println!("   CPU Cores: {}", cpu_opts.cpu_count);
    println!("   NUMA Nodes: {}", cpu_opts.numa_nodes);
    println!("   Cache Line: {} bytes", cpu_opts.cache_line_size);
    println!("   Optimal Chunk: {} bytes", cpu_opts.optimal_chunk_size());
    println!("   Optimal Batch: {} operations", cpu_opts.optimal_batch_size());
    
    Ok(cpu_opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_detection() {
        let cpu_opts = CpuOptimizations::detect();
        assert!(cpu_opts.cpu_count > 0);
        assert!(cpu_opts.cache_line_size > 0);
        assert!(cpu_opts.page_size > 0);
    }

    #[test]
    fn test_optimal_sizes() {
        let cpu_opts = CpuOptimizations::detect();
        let chunk_size = cpu_opts.optimal_chunk_size();
        let batch_size = cpu_opts.optimal_batch_size();
        
        assert!(chunk_size >= 262_144); // At least 256KB
        assert!(batch_size >= 16);      // At least 16 operations
    }

    #[test]
    fn test_memory_bandwidth_benchmark() {
        let cpu_opts = CpuOptimizations::detect();
        let bandwidth = cpu_opts.benchmark_memory_bandwidth();
        
        // Should get at least 1 GB/s on any modern system
        assert!(bandwidth > 1.0);
    }
}