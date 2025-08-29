use std::os::raw::{c_char, c_void};
use std::ffi::{CStr, CString};

/// Opaque handle to a QuicTransport
#[repr(C)]
pub struct QuicTransportHandle {
    _private: [u8; 0],
}

/// Opaque handle to a QuicConnection
#[repr(C)]
pub struct QuicConnectionHandle {
    _private: [u8; 0],
}

/// Opaque handle to a QuicStream
#[repr(C)]
pub struct QuicStreamHandle {
    _private: [u8; 0],
}

/// Configuration for TLS
#[repr(C)]
pub struct QuicTlsConfig {
    /// Path to certificate file (PEM format)
    pub cert_path: *const c_char,
    /// Path to private key file (PEM format)
    pub key_path: *const c_char,
    /// Path to CA certificate file (PEM format) - optional, can be null
    pub ca_path: *const c_char,
}

/// Connection configuration
#[repr(C)]
pub struct QuicConnectionConfig {
    /// Maximum number of concurrent streams
    pub max_concurrent_streams: u32,
    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u32,
    /// Keep alive interval in milliseconds (0 to disable)
    pub keep_alive_ms: u32,
}

impl Default for QuicConnectionConfig {
    fn default() -> Self {
        Self {
            max_concurrent_streams: 1000,
            connection_timeout_ms: 30000,  // 30 seconds
            keep_alive_ms: 10000,          // 10 seconds
        }
    }
}

/// Callback function type for async operations
/// 
/// Parameters:
/// - user_data: User-provided data pointer
/// - error: Error code (QuicError::Success if no error)
/// - result_data: Operation-specific result data (can be null)
pub type QuicCallback = extern "C" fn(
    user_data: *mut c_void,
    error: crate::error::QuicError,
    result_data: *mut c_void,
);

/// Message data structure for sending/receiving
#[repr(C)]
pub struct QuicMessage {
    /// Pointer to message data
    pub data: *const u8,
    /// Length of message data in bytes
    pub len: usize,
}

/// Helper to convert C string to Rust &str
pub fn c_str_to_str(c_str: *const c_char) -> Result<&'static str, crate::error::QuicError> {
    if c_str.is_null() {
        return Err(crate::error::QuicError::InvalidArgument);
    }
    
    let c_str = unsafe { CStr::from_ptr(c_str) };
    c_str.to_str().map_err(|_| crate::error::QuicError::InvalidArgument)
}

/// Helper to convert Rust str to C string (caller must free)
pub fn str_to_c_str(s: &str) -> *mut c_char {
    match CString::new(s) {
        Ok(c_str) => c_str.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Helper to convert bytes to QuicMessage
pub fn bytes_to_message(data: &[u8]) -> QuicMessage {
    QuicMessage {
        data: data.as_ptr(),
        len: data.len(),
    }
}

/// Helper to convert QuicMessage to bytes (does not take ownership)
pub fn message_to_bytes(msg: &QuicMessage) -> &[u8] {
    if msg.data.is_null() || msg.len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(msg.data, msg.len) }
    }
}