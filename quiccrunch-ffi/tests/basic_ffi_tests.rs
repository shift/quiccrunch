use quiccrunch_ffi::*;
use std::ffi::CString;
use std::ptr;

#[test]
fn test_ffi_init_and_shutdown() {
    let result = unsafe { quiccrunch_init() };
    assert_eq!(result, QuicError::Success);

    let result = unsafe { quiccrunch_shutdown() };
    assert_eq!(result, QuicError::Success);
}

#[test]
fn test_version() {
    let version_ptr = unsafe { quiccrunch_version() };
    assert!(!version_ptr.is_null());

    let version_cstr = unsafe { std::ffi::CStr::from_ptr(version_ptr) };
    let version = version_cstr.to_str().expect("Invalid UTF-8 in version");

    // Should be a valid version string
    assert!(!version.is_empty());
    println!("QuicCrunch version: {}", version);

    unsafe { quiccrunch_string_free(version_ptr) };
}

#[test]
fn test_client_lifecycle() {
    unsafe {
        // Initialize library
        assert_eq!(quiccrunch_init(), QuicError::Success);

        // Create client
        let client = quiccrunch_client_new();
        assert!(!client.is_null());

        // Initialize client
        let result = quiccrunch_client_init(client);
        assert_eq!(result, QuicError::Success);

        // Check connection count (should be 0)
        let count = quiccrunch_client_connection_count(client);
        assert_eq!(count, 0);

        // Free client
        quiccrunch_client_free(client);

        // Shutdown library
        assert_eq!(quiccrunch_shutdown(), QuicError::Success);
    }
}

#[test]
fn test_server_lifecycle() {
    unsafe {
        // Initialize library
        assert_eq!(quiccrunch_init(), QuicError::Success);

        // Create server
        let server = quiccrunch_server_new();
        assert!(!server.is_null());

        // Initialize server
        let result = quiccrunch_server_init(server);
        assert_eq!(result, QuicError::Success);

        // Check connection count (should be 0)
        let count = quiccrunch_server_connection_count(server);
        assert_eq!(count, 0);

        // Free server
        quiccrunch_server_free(server);

        // Shutdown library
        assert_eq!(quiccrunch_shutdown(), QuicError::Success);
    }
}

#[test]
fn test_error_handling() {
    unsafe {
        // Test with null pointers
        let result = quiccrunch_client_init(ptr::null_mut());
        assert_eq!(result, QuicError::InvalidHandle);

        let result = quiccrunch_server_init(ptr::null_mut());
        assert_eq!(result, QuicError::InvalidHandle);

        let count = quiccrunch_client_connection_count(ptr::null_mut());
        assert_eq!(count, 0);

        let count = quiccrunch_server_connection_count(ptr::null_mut());
        assert_eq!(count, 0);
    }
}

#[test]
fn test_string_handling() {
    unsafe {
        // Test string allocation and deallocation
        let version_ptr = quiccrunch_version();
        assert!(!version_ptr.is_null());

        // This should not crash
        quiccrunch_string_free(version_ptr);

        // Test with null pointer (should not crash)
        quiccrunch_string_free(ptr::null_mut());
    }
}
