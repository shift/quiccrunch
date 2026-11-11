//! SIMD-accelerated message parsing for length-prefixed messages
//!
//! Provides SIMD-accelerated parsing of length-prefixed messages for
//! high-performance message framing.

use std::arch::x86_64::*;

/// SIMD-accelerated length prefix parser
pub struct SimdLengthPrefixParser;

impl SimdLengthPrefixParser {
    /// Parse a single 4-byte big-endian length prefix (scalar fallback)
    #[inline]
    pub fn parse_length_scalar(data: &[u8]) -> Option<u32> {
        if data.len() < 4 {
            return None;
        }

        Some(u32::from_be_bytes([data[0], data[1], data[2], data[3]]))
    }

    /// Parse a single 4-byte big-endian length prefix (SIMD-accelerated)
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    #[inline]
    /// # Safety
    /// Caller must ensure the CPU supports AVX2 (this function is gated on x86_64
    /// and uses `#[target_feature]`, but the data pointer must still be valid).
    pub unsafe fn parse_length_simd(data: &[u8]) -> Option<u32> {
        if data.len() < 4 {
            return None;
        }

        // Load 4 bytes into a SIMD register
        let ptr = data.as_ptr() as *const __m128i;
        let raw = _mm_loadu_si128(ptr);

        // Extract bytes and convert to big-endian u32
        let mut bytes = [0u8; 16];
        _mm_storeu_si128(bytes.as_mut_ptr() as *mut __m128i, raw);

        Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Parse multiple 4-byte length prefixes from a buffer (SIMD-accelerated)
    /// Returns a vector of (offset, length) pairs
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    /// # Safety
    /// Caller must ensure the CPU supports AVX2 and that `data` is a valid slice
    /// for reads of up to `count * 4` bytes.
    pub unsafe fn parse_lengths_simd(data: &[u8], count: usize) -> Vec<(usize, u32)> {
        let mut results = Vec::with_capacity(count);
        let mut offset = 0;

        // Process 8 length prefixes at a time using AVX2
        let chunks = count / 8;
        let remaining = count % 8;

        for _ in 0..chunks {
            if offset + 32 > data.len() {
                break;
            }

            // Load 32 bytes (8 length prefixes) into a SIMD register
            let ptr = data[offset..].as_ptr() as *const __m256i;
            let raw = _mm256_loadu_si256(ptr);

            // Store to temporary buffer for extraction
            let mut bytes = [0u8; 32];
            _mm256_storeu_si256(bytes.as_mut_ptr() as *mut __m256i, raw);

            // Extract 8 big-endian u32 values
            for i in 0..8 {
                let idx = i * 4;
                let length = u32::from_be_bytes([
                    bytes[idx],
                    bytes[idx + 1],
                    bytes[idx + 2],
                    bytes[idx + 3],
                ]);
                results.push((offset, length));
                offset += 4;
            }
        }

        // Process remaining length prefixes
        for _ in 0..remaining {
            if offset + 4 > data.len() {
                break;
            }

            let length = Self::parse_length_scalar(&data[offset..]).unwrap_or(0);
            results.push((offset, length));
            offset += 4;
        }

        results
    }

    /// Find the first valid length prefix in a buffer
    /// A valid length prefix has a non-zero length and doesn't exceed max_length
    pub fn find_first_valid_length(data: &[u8], max_length: u32) -> Option<(usize, u32)> {
        if data.len() < 4 {
            return None;
        }

        for i in 0..=data.len().saturating_sub(4) {
            if let Some(length) = Self::parse_length_scalar(&data[i..]) {
                if length > 0 && length <= max_length {
                    return Some((i, length));
                }
            }
        }

        None
    }

    /// Validate a message buffer with length prefix
    pub fn validate_message(data: &[u8], max_length: u32) -> Option<(u32, &[u8])> {
        let length = Self::parse_length_scalar(data)?;

        if length == 0 || length > max_length {
            return None;
        }

        if data.len() < 4 + length as usize {
            return None;
        }

        let message = &data[4..4 + length as usize];
        Some((length, message))
    }

    /// Parse a stream of length-prefixed messages
    pub fn parse_message_stream(data: &[u8], max_length: u32) -> Vec<(u32, &[u8])> {
        let mut messages = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            if let Some((length, message)) = Self::validate_message(&data[offset..], max_length) {
                messages.push((length, message));
                offset += 4 + length as usize;
            } else {
                break;
            }
        }

        messages
    }
}

/// SIMD-accelerated message scanner
pub struct SimdMessageScanner;

impl SimdMessageScanner {
    /// Scan for a delimiter pattern in a buffer
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    /// # Safety
    /// Caller must ensure the CPU supports AVX2 and that `data` is valid for the
    /// duration of the scan.
    pub unsafe fn scan_delimiter_simd(data: &[u8], delimiter: u8) -> Option<usize> {
        if data.is_empty() {
            return None;
        }

        // Broadcast delimiter to all bytes in a 256-bit register
        let delimiter_vec = _mm256_set1_epi8(delimiter as i8);

        let chunks = data.len() / 32;
        let remaining = data.len() % 32;

        for i in 0..chunks {
            let offset = i * 32;
            let ptr = data[offset..].as_ptr() as *const __m256i;
            let chunk = _mm256_loadu_si256(ptr);

            // Compare each byte with delimiter
            let cmp = _mm256_cmpeq_epi8(chunk, delimiter_vec);

            // Get comparison result as bitmask
            let mask = _mm256_movemask_epi8(cmp);

            if mask != 0 {
                // Found at least one match
                let pos = offset + mask.trailing_zeros() as usize;
                return Some(pos);
            }
        }

        // Check remaining bytes
        let offset = chunks * 32;
        for i in 0..remaining {
            if data[offset + i] == delimiter {
                return Some(offset + i);
            }
        }

        None
    }

    /// Scan for a 4-byte pattern in a buffer
    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2")]
    /// # Safety
    /// Caller must ensure the CPU supports AVX2 and that `data` is valid for the
    /// duration of the scan.
    pub unsafe fn scan_pattern_simd(data: &[u8], pattern: [u8; 4]) -> Option<usize> {
        if data.len() < 4 {
            return None;
        }

        // Broadcast pattern to all bytes in a 256-bit register
        let pattern_vec = _mm256_set1_epi32(i32::from_ne_bytes(pattern));

        let chunks = data.len() / 32;
        let remaining = data.len() % 32;

        for i in 0..chunks {
            let offset = i * 32;
            let ptr = data[offset..].as_ptr() as *const __m256i;
            let chunk = _mm256_loadu_si256(ptr);

            // Compare each 4-byte chunk with pattern
            let cmp = _mm256_cmpeq_epi32(chunk, pattern_vec);

            // Get comparison result as bitmask
            let mask = _mm256_movemask_epi8(cmp);

            if mask != 0 {
                // Found at least one match
                let pos = offset + (mask.trailing_zeros() / 4 * 4) as usize;
                return Some(pos);
            }
        }

        // Check remaining bytes
        let offset = chunks * 32;
        for i in 0..remaining.saturating_sub(3) {
            if data[offset + i..offset + i + 4] == pattern {
                return Some(offset + i);
            }
        }

        None
    }
}

/// Scalar fallback implementations
pub mod scalar {
    use super::*;

    /// Parse a single 4-byte big-endian length prefix (scalar)
    #[inline]
    pub fn parse_length(data: &[u8]) -> Option<u32> {
        SimdLengthPrefixParser::parse_length_scalar(data)
    }

    /// Scan for a delimiter pattern (scalar)
    pub fn scan_delimiter(data: &[u8], delimiter: u8) -> Option<usize> {
        data.iter().position(|&b| b == delimiter)
    }

    /// Scan for a 4-byte pattern (scalar)
    pub fn scan_pattern(data: &[u8], pattern: [u8; 4]) -> Option<usize> {
        data.windows(4).position(|w| w == pattern)
    }
}

/// Check if SIMD is available
pub fn is_simd_available() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_length_scalar() {
        let data = [0x00, 0x00, 0x00, 0x0A]; // Length 10
        assert_eq!(SimdLengthPrefixParser::parse_length_scalar(&data), Some(10));

        let data = [0x00, 0x00, 0x01, 0x00]; // Length 256
        assert_eq!(
            SimdLengthPrefixParser::parse_length_scalar(&data),
            Some(256)
        );

        let data = [0xFF, 0xFF, 0xFF, 0xFF]; // Length 4294967295
        assert_eq!(
            SimdLengthPrefixParser::parse_length_scalar(&data),
            Some(4294967295)
        );
    }

    #[test]
    fn test_validate_message() {
        let data = [
            0x00, 0x00, 0x00, 0x05, // Length 5
            0x68, 0x65, 0x6C, 0x6C, 0x6F, // "hello"
        ];

        let result = SimdLengthPrefixParser::validate_message(&data, 100);
        assert!(result.is_some());

        let (length, message) = result.unwrap();
        assert_eq!(length, 5);
        assert_eq!(message, b"hello");
    }

    #[test]
    fn test_parse_message_stream() {
        let data = [
            0x00, 0x00, 0x00, 0x05, // Length 5
            0x68, 0x65, 0x6C, 0x6C, 0x6F, // "hello"
            0x00, 0x00, 0x00, 0x05, // Length 5
            0x77, 0x6F, 0x72, 0x6C, 0x64, // "world"
        ];

        let messages = SimdLengthPrefixParser::parse_message_stream(&data, 100);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].1, b"hello");
        assert_eq!(messages[1].1, b"world");
    }

    #[test]
    fn test_scalar_scan_delimiter() {
        let data = b"hello world";
        assert_eq!(scalar::scan_delimiter(data, b' '), Some(5));
        assert_eq!(scalar::scan_delimiter(data, b'x'), None);
    }

    #[test]
    fn test_scalar_scan_pattern() {
        let data = b"hello world test";
        assert_eq!(
            scalar::scan_pattern(data, [b'w', b'o', b'r', b'l']),
            Some(6)
        );
        assert_eq!(scalar::scan_pattern(data, [b'x', b'y', b'z', b'w']), None);
    }
}
