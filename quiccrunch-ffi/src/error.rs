

/// Error codes for QuicCrunch FFI operations
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuicError {
    /// Operation completed successfully
    Success = 0,
    
    /// Generic error occurred
    Error = 1,
    
    /// Invalid argument passed to function
    InvalidArgument = 2,
    
    /// Connection failed
    ConnectionFailed = 3,
    
    /// Operation timed out
    TimeoutError = 4,
    
    /// Serialization/deserialization error
    SerializationError = 5,
    
    /// TLS/certificate error
    TlsError = 6,
    
    /// Address parsing error
    AddressError = 7,
    
    /// Out of memory
    OutOfMemory = 8,
    
    /// Invalid handle/pointer
    InvalidHandle = 9,
    
    /// Operation would block (for non-blocking operations)
    WouldBlock = 10,
    
    /// Stream was closed
    StreamClosed = 11,
    
    /// Not implemented
    NotImplemented = 12,
}

impl From<quiccrunch_core::TransportError> for QuicError {
    fn from(err: quiccrunch_core::TransportError) -> Self {
        use quiccrunch_core::TransportError::*;
        match err {
            ConnectionFailed(_) => QuicError::ConnectionFailed,
            TimeoutError(_) => QuicError::TimeoutError,
            CertificateError(_) => QuicError::TlsError,
            StreamError(_) => QuicError::StreamClosed,
            ConfigError(_) => QuicError::InvalidArgument,
            ProtocolError(_) => QuicError::Error,
            SerializationError(_) => QuicError::SerializationError,
            InvalidMessage(_) => QuicError::InvalidArgument,
            Fast(_) => QuicError::Error,
        }
    }
}

impl From<std::io::Error> for QuicError {
    fn from(err: std::io::Error) -> Self {
        use std::io::ErrorKind::*;
        match err.kind() {
            InvalidInput | InvalidData => QuicError::InvalidArgument,
            TimedOut => QuicError::TimeoutError,
            ConnectionRefused | ConnectionAborted | ConnectionReset => QuicError::ConnectionFailed,
            AddrNotAvailable | AddrInUse => QuicError::AddressError,
            BrokenPipe => QuicError::StreamClosed,
            WouldBlock => QuicError::WouldBlock,
            OutOfMemory => QuicError::OutOfMemory,
            _ => QuicError::Error,
        }
    }
}

impl From<serde_json::Error> for QuicError {
    fn from(_err: serde_json::Error) -> Self {
        QuicError::SerializationError
    }
}

/// Convert a Rust Result to a C-style error code, storing the Ok value in an out parameter
pub fn result_to_c_error<T, E>(result: Result<T, E>, out: *mut T) -> QuicError 
where 
    E: Into<QuicError>
{
    match result {
        Ok(value) => {
            if !out.is_null() {
                unsafe { std::ptr::write(out, value); }
            }
            QuicError::Success
        }
        Err(err) => err.into(),
    }
}

/// Helper to check if a pointer is valid (not null)
pub fn check_ptr<T>(ptr: *const T) -> Result<&'static T, QuicError> {
    if ptr.is_null() {
        Err(QuicError::InvalidHandle)
    } else {
        Ok(unsafe { &*ptr })
    }
}

/// Helper to check if a mutable pointer is valid (not null)
pub fn check_ptr_mut<T>(ptr: *mut T) -> Result<&'static mut T, QuicError> {
    if ptr.is_null() {
        Err(QuicError::InvalidHandle)
    } else {
        Ok(unsafe { &mut *ptr })
    }
}