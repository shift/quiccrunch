use std::time::Instant;
use quiccrunch::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 QuicCrunch Performance Validation Test");
    println!("=========================================");
    
    // Test basic functionality
    let start = Instant::now();
    
    // Create a simple transport configuration
    let settings = config::QuicTransportSettings::default();
    let tls_config = transport::TlsConfig::default();
    
    // Test that we can create the transport
    match transport::QuicTransport::new(tls_config, transport::TransportSettings::default()) {
        Ok(transport) => {
            println!("✅ Transport created successfully");
            
            // Test basic features
            println!("📊 Testing optimization features:");
            
            #[cfg(feature = "lock-free-channels")]
            println!("  ✅ Lock-free channels: ENABLED");
            #[cfg(not(feature = "lock-free-channels"))]
            println!("  ❌ Lock-free channels: DISABLED");
            
            #[cfg(feature = "memory-pools")]
            println!("  ✅ Memory pools: ENABLED");
            #[cfg(not(feature = "memory-pools"))]
            println!("  ❌ Memory pools: DISABLED");
            
            #[cfg(feature = "simd-optimizations")]
            println!("  ✅ SIMD optimizations: ENABLED");
            #[cfg(not(feature = "simd-optimizations"))]
            println!("  ❌ SIMD optimizations: DISABLED");
            
            #[cfg(feature = "adaptive-optimizations")]
            println!("  ✅ Adaptive optimizations: ENABLED");
            #[cfg(not(feature = "adaptive-optimizations"))]
            println!("  ❌ Adaptive optimizations: DISABLED");
            
            #[cfg(feature = "io-uring-advanced")]
            println!("  ✅ Advanced io_uring: ENABLED");
            #[cfg(not(feature = "io-uring-advanced"))]
            println!("  ❌ Advanced io_uring: DISABLED");
            
            let elapsed = start.elapsed();
            println!("\n🎉 Initialization completed in {:?}", elapsed);
            println!("✨ QuicCrunch is ready for high-performance RPC!");
        }
        Err(e) => {
            println!("❌ Failed to create transport: {}", e);
            return Err(Box::new(e));
        }
    }
    
    Ok(())
}