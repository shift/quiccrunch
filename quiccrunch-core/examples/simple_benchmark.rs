/// Simple Memory and SIMD Performance Test
/// Tests the core optimizations without complex networking

use quiccrunch_core::optimizations::{PooledBuffer, get_pool_stats};
use quiccrunch_core::simd;
use quiccrunch_core::{QuicTransport, TlsConfig};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔬 QuicCrunch Core Optimization Benchmark");
    println!("=========================================\n");

    // Verify backend selection
    verify_backend_selection()?;

    // Test configuration
    const ITERATIONS: usize = 10_000;
    const BUFFER_SIZES: &[usize] = &[1024, 4096, 16384, 65536]; // 1KB, 4KB, 16KB, 64KB

    println!("Testing with {} iterations per size\n", ITERATIONS);

    for &size in BUFFER_SIZES {
        test_buffer_performance(size, ITERATIONS);
        test_copy_performance(size, ITERATIONS / 10); // Fewer iterations for large buffers
        println!();
    }

    // Memory pool statistics
    #[cfg(feature = "memory-pools")]
    {
        println!("📊 Memory Pool Statistics:");
        let stats = get_pool_stats();
        println!("  Total allocations: {}", stats.allocations);
        println!("  Pool hits: {}", stats.pool_hits);
        println!("  Pool misses: {}", stats.pool_misses);
        println!("  Hit rate: {:.2}%", stats.hit_rate());
    }

    Ok(())
}

fn verify_backend_selection() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Backend Verification");
    println!("-----------------------");
    
    // Generate simple test certificates for verification
    let (ca_cert, _ca_key) = generate_ca_cert()?;
    let (cert, key) = generate_node_cert(&ca_cert, "localhost")?;
    let tls_config = TlsConfig { 
        cert: rustls::Certificate(cert), 
        key: rustls::PrivateKey(key), 
        ca_cert: rustls::Certificate(ca_cert) 
    };
    
    let transport = QuicTransport::new(tls_config)?;
    let backend = transport.backend_info();
    
    println!("  Active backend: {}", backend);
    
    if backend == "tQUIC" {
        println!("  ✅ tQUIC backend is active!");
    } else {
        println!("  ⚠️  Expected tQUIC but got: {}", backend);
    }
    
    // Verify features
    #[cfg(feature = "backend-tquic")]
    println!("  ✅ backend-tquic feature enabled");
    #[cfg(not(feature = "backend-tquic"))]
    println!("  ❌ backend-tquic feature NOT enabled");
    
    #[cfg(feature = "backend-quinn")]
    println!("  ⚠️  backend-quinn feature also enabled");
    
    #[cfg(feature = "memory-pools")]
    println!("  ✅ memory-pools feature enabled");
    #[cfg(not(feature = "memory-pools"))]
    println!("  ❌ memory-pools feature NOT enabled");
    
    #[cfg(feature = "simd-optimizations")]
    println!("  ✅ simd-optimizations feature enabled");
    #[cfg(not(feature = "simd-optimizations"))]
    println!("  ❌ simd-optimizations feature NOT enabled");
    
    println!();
    
    Ok(())
}

fn test_buffer_performance(size: usize, iterations: usize) {
    println!("📦 Buffer Allocation Test ({} bytes, {} iterations)", size, iterations);
    
    // Test 1: Standard Vec allocation
    let start = Instant::now();
    let mut buffers = Vec::new();
    for _ in 0..iterations {
        let buffer = vec![0u8; size];
        buffers.push(buffer);
    }
    let std_duration = start.elapsed();
    println!("  Standard Vec::with_capacity: {:?} ({:.2} allocs/sec)", 
             std_duration, iterations as f64 / std_duration.as_secs_f64());
    drop(buffers);

    // Test 2: Pooled allocation (if feature enabled)
    #[cfg(feature = "memory-pools")]
    {
        let start = Instant::now();
        let mut buffers = Vec::new();
        for _ in 0..iterations {
            let buffer = PooledBuffer::get(size);
            buffers.push(buffer);
        }
        let pool_duration = start.elapsed();
        println!("  Pooled allocation: {:?} ({:.2} allocs/sec)", 
                 pool_duration, iterations as f64 / pool_duration.as_secs_f64());
        
        let improvement = (std_duration.as_nanos() as f64 / pool_duration.as_nanos() as f64 - 1.0) * 100.0;
        if improvement > 0.0 {
            println!("  ✅ Pool improvement: {:.1}% faster", improvement);
        } else {
            println!("  ⚠️  Pool overhead: {:.1}% slower", -improvement);
        }
        drop(buffers);
    }

    #[cfg(not(feature = "memory-pools"))]
    {
        println!("  Pooled allocation: disabled (compile with --features memory-pools)");
    }
}

fn test_copy_performance(size: usize, iterations: usize) {
    println!("🔄 Data Copy Test ({} bytes, {} iterations)", size, iterations);
    
    // Create source and destination buffers
    let source = vec![0xAAu8; size];
    let mut dest_std = vec![0u8; size];
    let mut dest_simd = vec![0u8; size];

    // Test 1: Standard memory copy
    let start = Instant::now();
    for _ in 0..iterations {
        dest_std.copy_from_slice(&source);
    }
    let std_duration = start.elapsed();
    let std_throughput = (size * iterations) as f64 / std_duration.as_secs_f64() / 1024.0 / 1024.0;
    println!("  Standard copy_from_slice: {:?} ({:.2} MB/sec)", std_duration, std_throughput);

    // Test 2: SIMD-optimized copy (if size is suitable)
    if size >= 32 {
        #[cfg(feature = "simd-optimizations")]
        {
            let start = Instant::now();
            for _ in 0..iterations {
                simd::fast_copy(&source, &mut dest_simd);
            }
            let simd_duration = start.elapsed();
            let simd_throughput = (size * iterations) as f64 / simd_duration.as_secs_f64() / 1024.0 / 1024.0;
            println!("  SIMD-optimized copy: {:?} ({:.2} MB/sec)", simd_duration, simd_throughput);
            
            let improvement = (simd_throughput / std_throughput - 1.0) * 100.0;
            if improvement > 0.0 {
                println!("  ✅ SIMD improvement: {:.1}% faster", improvement);
            } else {
                println!("  ⚠️  SIMD overhead: {:.1}% slower", -improvement);
            }
        }

        #[cfg(not(feature = "simd-optimizations"))]
        {
            println!("  SIMD-optimized copy: disabled (compile with --features simd-optimizations)");
        }
    } else {
        println!("  SIMD-optimized copy: skipped (size < 32 bytes)");
    }
}

fn generate_ca_cert() -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
    use rcgen::{Certificate, CertificateParams, DistinguishedName};
    
    let mut params = CertificateParams::new(vec!["QuicCrunch-CA".to_string()]);
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(rcgen::DnType::CommonName, "QuicCrunch-CA");
    
    let ca_cert = Certificate::from_params(params)?;
    let ca_cert_der = ca_cert.serialize_der()?;
    let ca_key_der = ca_cert.serialize_private_key_der();
    
    Ok((ca_cert_der, ca_key_der))
}

fn generate_node_cert(ca_cert_der: &[u8], hostname: &str) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
    use rcgen::{Certificate, CertificateParams, DistinguishedName};
    
    // Load CA cert for signing
    let ca_cert_pem = format!("-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----", 
                               base64::encode(ca_cert_der));
    let ca_cert = Certificate::from_pem(&ca_cert_pem)?;
    
    let mut params = CertificateParams::new(vec![hostname.to_string()]);
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(rcgen::DnType::CommonName, hostname);
    
    let cert = Certificate::from_params(params)?;
    let cert_der = cert.serialize_der_with_signer(&ca_cert)?;
    let key_der = cert.serialize_private_key_der();
    
    Ok((cert_der, key_der))
}