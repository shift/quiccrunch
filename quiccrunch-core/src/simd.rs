/// SIMD-optimized operations for QuicCrunch
/// 
/// This module provides vectorized implementations of common operations
/// using CPU-specific instruction sets (AVX2, AVX-512) for maximum performance.

use std::mem;

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

/// Runtime CPU feature detection
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

/// Fast memory copying with SIMD optimization
pub fn fast_copy(src: &[u8], dst: &mut [u8]) {
    assert_eq!(src.len(), dst.len());
    
    // For small copies, standard library is faster due to overhead
    // Increased threshold based on performance analysis showing SIMD overhead
    if src.len() < 65536 {  // 64KB threshold (was 128 bytes)
        dst.copy_from_slice(src);
        return;
    }
    
    // Check alignment - SIMD works best with aligned data
    let src_aligned = src.as_ptr() as usize % 64 == 0;  // 64-byte alignment for AVX-512
    let dst_aligned = dst.as_ptr() as usize % 64 == 0;
    
    // Only use SIMD for large, well-aligned transfers
    if !src_aligned || !dst_aligned {
        dst.copy_from_slice(src);
        return;
    }

    #[cfg(target_arch = "x86_64")]
    {
        // Use thread-local storage for safe feature detection caching
        thread_local! {
            static CPU_FEATURES: CpuFeatures = CpuFeatures::detect();
        }
        
        let use_simd = CPU_FEATURES.with(|features| {
            if features.avx512f && features.avx512bw && src.len() >= 1048576 {  // 1MB for AVX-512
                Some(SIMDType::AVX512)
            } else if features.avx2 && src.len() >= 262144 {  // 256KB for AVX2
                Some(SIMDType::AVX2)
            } else if features.sse41 && src.len() >= 131072 {  // 128KB for SSE4.1
                Some(SIMDType::SSE41)
            } else {
                None
            }
        });
        
        match use_simd {
            Some(SIMDType::AVX512) => {
                unsafe { fast_copy_avx512(src, dst) };
                return;
            }
            Some(SIMDType::AVX2) => {
                unsafe { fast_copy_avx2(src, dst) };
                return;
            }
            Some(SIMDType::SSE41) => {
                unsafe { fast_copy_sse41(src, dst) };
                return;
            }
            None => {}
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        thread_local! {
            static NEON_AVAILABLE: bool = is_aarch64_feature_detected!("neon");
        }
        
        let use_neon = NEON_AVAILABLE.with(|&neon| neon && src.len() >= 131072);  // 128KB threshold
        
        if use_neon {
            unsafe { fast_copy_neon(src, dst) };
            return;
        }
    }

    // Fallback to standard copy
    dst.copy_from_slice(src);
}

#[derive(Debug, Clone, Copy)]
enum SIMDType {
    AVX512,
    AVX2,
    SSE41,
}

/// Fast memory zeroing with SIMD
pub fn fast_zero(buffer: &mut [u8]) {
    // Increased threshold based on performance analysis
    if buffer.len() < 65536 {  // 64KB threshold
        buffer.fill(0);
        return;
    }

    #[cfg(target_arch = "x86_64")]
    {
        thread_local! {
            static CPU_FEATURES: CpuFeatures = CpuFeatures::detect();
        }
        
        let use_simd = CPU_FEATURES.with(|features| {
            if features.avx512f && buffer.len() >= 262144 {  // 256KB for AVX-512
                Some(SIMDType::AVX512)
            } else if features.avx2 && buffer.len() >= 131072 {  // 128KB for AVX2
                Some(SIMDType::AVX2)
            } else {
                None
            }
        });
        
        match use_simd {
            Some(SIMDType::AVX512) => {
                unsafe { fast_zero_avx512(buffer) };
                return;
            }
            Some(SIMDType::AVX2) => {
                unsafe { fast_zero_avx2(buffer) };
                return;
            }
            _ => {}
        }
    }

    buffer.fill(0);
}

/// Process multiple u32 values in parallel (for length headers)
pub fn decode_u32_array(data: &[u8]) -> Vec<u32> {
    let count = data.len() / 4;
    let mut result = Vec::with_capacity(count);

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && count >= 8 {
            unsafe { decode_u32_array_avx2(data, &mut result) };
            return result;
        }
    }

    // Fallback implementation
    for chunk in data.chunks_exact(4) {
        result.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    
    result
}

// x86_64 AVX-512 implementations
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
unsafe fn fast_copy_avx512(src: &[u8], dst: &mut [u8]) {
    let len = src.len();
    let mut i = 0;

    // Process 64-byte chunks
    while i + 64 <= len {
        let src_ptr = src.as_ptr().add(i) as *const __m512i;
        let dst_ptr = dst.as_mut_ptr().add(i) as *mut __m512i;
        
        let data = _mm512_loadu_si512(src_ptr);
        _mm512_storeu_si512(dst_ptr, data);
        
        i += 64;
    }

    // Handle remaining bytes with AVX2
    if i < len {
        fast_copy_avx2(&src[i..], &mut dst[i..]);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
unsafe fn fast_zero_avx512(buffer: &mut [u8]) {
    let len = buffer.len();
    let mut i = 0;
    let zero = _mm512_setzero_si512();

    // Process 64-byte chunks
    while i + 64 <= len {
        let ptr = buffer.as_mut_ptr().add(i) as *mut __m512i;
        _mm512_storeu_si512(ptr, zero);
        i += 64;
    }

    // Handle remaining bytes
    if i < len {
        buffer[i..].fill(0);
    }
}

// x86_64 AVX2 implementations
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn fast_copy_avx2(src: &[u8], dst: &mut [u8]) {
    let len = src.len();
    let mut i = 0;

    // Process 32-byte chunks
    while i + 32 <= len {
        let src_ptr = src.as_ptr().add(i) as *const __m256i;
        let dst_ptr = dst.as_mut_ptr().add(i) as *mut __m256i;
        
        let data = _mm256_loadu_si256(src_ptr);
        _mm256_storeu_si256(dst_ptr, data);
        
        i += 32;
    }

    // Process 16-byte chunks with SSE
    while i + 16 <= len {
        let src_ptr = src.as_ptr().add(i) as *const __m128i;
        let dst_ptr = dst.as_mut_ptr().add(i) as *mut __m128i;
        
        let data = _mm_loadu_si128(src_ptr);
        _mm_storeu_si128(dst_ptr, data);
        
        i += 16;
    }

    // Handle remaining bytes
    if i < len {
        dst[i..].copy_from_slice(&src[i..]);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn fast_zero_avx2(buffer: &mut [u8]) {
    let len = buffer.len();
    let mut i = 0;
    let zero = _mm256_setzero_si256();

    // Process 32-byte chunks
    while i + 32 <= len {
        let ptr = buffer.as_mut_ptr().add(i) as *mut __m256i;
        _mm256_storeu_si256(ptr, zero);
        i += 32;
    }

    // Handle remaining bytes
    if i < len {
        buffer[i..].fill(0);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn decode_u32_array_avx2(data: &[u8], result: &mut Vec<u32>) {
    let count = data.len() / 4;
    let mut i = 0;

    // Process 8 u32s (32 bytes) at a time
    while i + 8 <= count {
        let byte_offset = i * 4;
        let src_ptr = data.as_ptr().add(byte_offset) as *const __m256i;
        let data_vec = _mm256_loadu_si256(src_ptr);
        
        // Extract individual u32 values
        let mut temp: [u32; 8] = mem::zeroed();
        _mm256_storeu_si256(temp.as_mut_ptr() as *mut __m256i, data_vec);
        
        // Convert from little-endian if needed
        for &value in &temp {
            result.push(u32::from_le(value));
        }
        
        i += 8;
    }

    // Handle remaining values
    for j in i..count {
        let byte_offset = j * 4;
        let bytes = &data[byte_offset..byte_offset + 4];
        result.push(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
    }
}

// x86_64 SSE4.1 implementations
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse4.1")]
unsafe fn fast_copy_sse41(src: &[u8], dst: &mut [u8]) {
    let len = src.len();
    let mut i = 0;

    // Process 16-byte chunks
    while i + 16 <= len {
        let src_ptr = src.as_ptr().add(i) as *const __m128i;
        let dst_ptr = dst.as_mut_ptr().add(i) as *mut __m128i;
        
        let data = _mm_loadu_si128(src_ptr);
        _mm_storeu_si128(dst_ptr, data);
        
        i += 16;
    }

    // Handle remaining bytes
    if i < len {
        dst[i..].copy_from_slice(&src[i..]);
    }
}

// ARM NEON implementations
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn fast_copy_neon(src: &[u8], dst: &mut [u8]) {
    let len = src.len();
    let mut i = 0;

    // Process 16-byte chunks
    while i + 16 <= len {
        let data = vld1q_u8(src.as_ptr().add(i));
        vst1q_u8(dst.as_mut_ptr().add(i), data);
        i += 16;
    }

    // Handle remaining bytes
    if i < len {
        dst[i..].copy_from_slice(&src[i..]);
    }
}

/// Memory alignment utilities
pub mod align {
    /// Check if pointer is aligned to boundary
    pub fn is_aligned(ptr: *const u8, alignment: usize) -> bool {
        (ptr as usize) % alignment == 0
    }

    /// Align size up to next boundary
    pub fn align_up(size: usize, alignment: usize) -> usize {
        (size + alignment - 1) & !(alignment - 1)
    }

    /// Get aligned slice from buffer
    pub fn get_aligned_slice(buffer: &[u8], alignment: usize) -> &[u8] {
        let ptr = buffer.as_ptr();
        let aligned_ptr = align_up(ptr as usize, alignment);
        let offset = aligned_ptr - (ptr as usize);
        
        if offset >= buffer.len() {
            return &[];
        }
        
        let aligned_len = (buffer.len() - offset) & !(alignment - 1);
        unsafe {
            std::slice::from_raw_parts((ptr as usize + offset) as *const u8, aligned_len)
        }
    }
}

/// Benchmark utilities for performance testing
#[cfg(test)]
pub mod bench {
    use super::*;
    use std::time::Instant;

    pub fn benchmark_copy(size: usize, iterations: usize) {
        let src = vec![0x42u8; size];
        let mut dst = vec![0u8; size];

        // Standard copy benchmark
        let start = Instant::now();
        for _ in 0..iterations {
            dst.copy_from_slice(&src);
        }
        let std_time = start.elapsed();

        // SIMD copy benchmark
        let start = Instant::now();
        for _ in 0..iterations {
            fast_copy(&src, &mut dst);
        }
        let simd_time = start.elapsed();

        println!("Size: {} bytes, Iterations: {}", size, iterations);
        println!("Standard copy: {:?}", std_time);
        println!("SIMD copy: {:?}", simd_time);
        println!("Speedup: {:.2}x", std_time.as_nanos() as f64 / simd_time.as_nanos() as f64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_feature_detection() {
        let features = CpuFeatures::detect();
        println!("CPU Features: {:?}", features);
        // Test passes if no panic occurs
    }

    #[test]
    fn test_fast_copy() {
        let src = vec![0x42u8; 1024];
        let mut dst = vec![0u8; 1024];
        
        fast_copy(&src, &mut dst);
        
        assert_eq!(src, dst);
    }

    #[test]
    fn test_fast_zero() {
        let mut buffer = vec![0x42u8; 1024];
        fast_zero(&mut buffer);
        
        assert!(buffer.iter().all(|&x| x == 0));
    }

    #[test]
    fn test_decode_u32_array() {
        let data = vec![
            0x01, 0x00, 0x00, 0x00, // 1
            0x02, 0x00, 0x00, 0x00, // 2
            0x03, 0x00, 0x00, 0x00, // 3
            0x04, 0x00, 0x00, 0x00, // 4
        ];
        
        let result = decode_u32_array(&data);
        assert_eq!(result, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_alignment() {
        let buffer = vec![0u8; 100];
        let aligned = align::get_aligned_slice(&buffer, 16);
        
        assert!(align::is_aligned(aligned.as_ptr(), 16));
        assert!(aligned.len() % 16 == 0);
    }
}