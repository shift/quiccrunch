use std::os::raw::{c_char, c_void};
use std::sync::Arc;
use std::collections::HashMap;
use quiccrunch_core::{QuicServer as CoreServer, QuicListener as CoreListener, MessageHandler, EchoHandler};
use crate::error::QuicError;
use crate::types::{QuicCallback, QuicTlsConfig, QuicConnectionConfig, QuicMessage, c_str_to_str, str_to_c_str};
use crate::runtime::FfiRuntime;

/// Message handler callback type
/// 
/// Parameters:
/// - user_data: User-provided data pointer
/// - connection_id: ID of the connection that sent the message
/// - message: The received message
/// - response: Output parameter - set this to the response message
/// 
/// Returns QuicError::Success to send the response, or an error code to close the connection
pub type QuicMessageHandler = extern "C" fn(
    user_data: *mut c_void,
    connection_id: u32,
    message: *const QuicMessage,
    response: *mut QuicMessage,
) -> QuicError;

/// Internal server handle structure
pub struct QuicServerInternal {
    server: Option<CoreServer>,
    listener: Option<CoreListener>,
    message_handler: Option<QuicMessageHandler>,
    user_data: *mut c_void,
    connections: HashMap<u32, String>, // connection_id -> remote_addr
    next_connection_id: u32,
}

impl QuicServerInternal {
    fn new() -> Self {
        Self {
            server: None,
            listener: None,
            message_handler: None,
            user_data: std::ptr::null_mut(),
            connections: HashMap::new(),
            next_connection_id: 1,
        }
    }
    
    fn add_connection(&mut self, remote_addr: String) -> u32 {
        let id = self.next_connection_id;
        self.next_connection_id += 1;
        self.connections.insert(id, remote_addr);
        id
    }
    
    fn remove_connection(&mut self, id: u32) -> bool {
        self.connections.remove(&id).is_some()
    }
}

/// Create a new QUIC server
/// 
/// Returns a handle to the server, or null on error.
/// The handle must be freed with quiccrunch_server_free().
#[no_mangle]
pub extern "C" fn quiccrunch_server_new() -> *mut crate::types::QuicTransportHandle {
    let server_internal = Box::new(QuicServerInternal::new());
    Box::into_raw(server_internal) as *mut crate::types::QuicTransportHandle
}

/// Create a new QUIC server with TLS configuration
/// 
/// # Arguments
/// * `tls_config` - TLS configuration for the server
/// 
/// Returns a handle to the server, or null on error.
#[no_mangle]
pub extern "C" fn quiccrunch_server_new_with_tls(
    tls_config: *const QuicTlsConfig
) -> *mut crate::types::QuicTransportHandle {
    if tls_config.is_null() {
        return std::ptr::null_mut();
    }
    
    let tls_config = unsafe { &*tls_config };
    
    // For now, just create a basic server
    // TODO: Implement actual TLS configuration
    let server_internal = Box::new(QuicServerInternal::new());
    Box::into_raw(server_internal) as *mut crate::types::QuicTransportHandle
}

/// Free a QUIC server handle
/// 
/// # Safety
/// The handle must have been created by quiccrunch_server_new() and
/// must not be used after calling this function.
#[no_mangle]
pub unsafe extern "C" fn quiccrunch_server_free(handle: *mut crate::types::QuicTransportHandle) {
    if !handle.is_null() {
        let _ = Box::from_raw(handle as *mut QuicServerInternal);
    }
}

/// Initialize the server (creates the underlying transport)
/// 
/// This must be called before starting to listen.
/// 
/// # Arguments
/// * `handle` - Server handle
/// 
/// Returns QuicError::Success on success, error code on failure.
#[no_mangle]
pub extern "C" fn quiccrunch_server_init(
    handle: *mut crate::types::QuicTransportHandle
) -> QuicError {
    if handle.is_null() {
        return QuicError::InvalidHandle;
    }
    
    let server_internal = unsafe { &mut *(handle as *mut QuicServerInternal) };
    
    let result = QuicServer::new();
    
    match result {
        Ok(server) => {
            server_internal.server = Some(server);
            QuicError::Success
        }
        Err(err) => err.into(),
    }
}

/// Set a message handler for the server
/// 
/// # Arguments
/// * `handle` - Server handle
/// * `handler` - Message handler callback function
/// * `user_data` - User data to pass to the handler
/// 
/// Returns QuicError::Success on success, error code on failure.
#[no_mangle]
pub extern "C" fn quiccrunch_server_set_message_handler(
    handle: *mut crate::types::QuicTransportHandle,
    handler: QuicMessageHandler,
    user_data: *mut c_void,
) -> QuicError {
    if handle.is_null() {
        return QuicError::InvalidHandle;
    }
    
    let server_internal = unsafe { &mut *(handle as *mut QuicServerInternal) };
    server_internal.message_handler = Some(handler);
    server_internal.user_data = user_data;
    
    QuicError::Success
}

/// Start listening on an address
/// 
/// # Arguments
/// * `handle` - Server handle
/// * `address` - Address to listen on (e.g., "127.0.0.1:50051")
/// 
/// Returns QuicError::Success on success, error code on failure.
#[no_mangle]
pub extern "C" fn quiccrunch_server_listen(
    handle: *mut crate::types::QuicTransportHandle,
    address: *const c_char,
) -> QuicError {
    if handle.is_null() || address.is_null() {
        return QuicError::InvalidHandle;
    }
    
    let address_str = match c_str_to_str(address) {
        Ok(s) => s,
        Err(e) => return e,
    };
    
    let server_internal = unsafe { &mut *(handle as *mut QuicServerInternal) };
    
    let server = match &server_internal.server {
        Some(s) => s.clone(),
        None => return QuicError::InvalidHandle,
    };
    
    let result = FfiRuntime::block_on(async {
        server.listen(address_str).await
    });
    
    match result {
        Ok(listener) => {
            server_internal.listener = Some(listener);
            QuicError::Success
        }
        Err(err) => err.into(),
    }
}

/// Start serving (accept connections and handle messages)
/// 
/// This function runs the server loop. It will block until quiccrunch_server_stop() is called
/// or an error occurs. In a real application, this should be called in a separate thread.
/// 
/// # Arguments
/// * `handle` - Server handle
/// 
/// Returns QuicError::Success when stopped normally, error code on failure.
#[no_mangle]
pub extern "C" fn quiccrunch_server_serve(
    handle: *mut crate::types::QuicTransportHandle,
) -> QuicError {
    if handle.is_null() {
        return QuicError::InvalidHandle;
    }
    
    let server_internal = unsafe { &mut *(handle as *mut QuicServerInternal) };
    
    let listener = match &mut server_internal.listener {
        Some(l) => l,
        None => return QuicError::InvalidHandle,
    };
    
    // For now, use the built-in echo handler
    // TODO: Implement custom message handler support
    let echo_handler = EchoHandler::new();
    
    let result = FfiRuntime::block_on(async {
        loop {
            match listener.accept().await {
                Ok(connection) => {
                    let handler = echo_handler.clone();
                    // Spawn a task to handle this connection
                    tokio::spawn(async move {
                        // Handle the connection
                        // This is a simplified version - real implementation would
                        // integrate with the FFI message handler
                        if let Err(e) = handler.handle_connection(connection).await {
                            eprintln!("Error handling connection: {:?}", e);
                        }
                    });
                }
                Err(e) => {
                    eprintln!("Error accepting connection: {:?}", e);
                    return Err(e.into());
                }
            }
        }
    });
    
    match result {
        Ok(()) => QuicError::Success,
        Err(err) => err,
    }
}

/// Start serving asynchronously
/// 
/// This starts the server in the background and returns immediately.
/// 
/// # Arguments
/// * `handle` - Server handle
/// 
/// Returns QuicError::Success on success, error code on failure.
#[no_mangle]
pub extern "C" fn quiccrunch_server_serve_async(
    handle: *mut crate::types::QuicTransportHandle,
) -> QuicError {
    if handle.is_null() {
        return QuicError::InvalidHandle;
    }
    
    // For async serving, we need to spawn a task that runs the server loop
    // This is complex to implement safely with the current FFI design
    // For now, return not implemented
    QuicError::NotImplemented
}

/// Stop the server
/// 
/// # Arguments
/// * `handle` - Server handle
/// 
/// Returns QuicError::Success on success, error code on failure.
#[no_mangle]
pub extern "C" fn quiccrunch_server_stop(
    handle: *mut crate::types::QuicTransportHandle,
) -> QuicError {
    if handle.is_null() {
        return QuicError::InvalidHandle;
    }
    
    // TODO: Implement server stopping mechanism
    QuicError::NotImplemented
}

/// Get the number of active connections
/// 
/// # Arguments
/// * `handle` - Server handle
/// 
/// Returns the number of active connections, or 0 if handle is invalid.
#[no_mangle]
pub extern "C" fn quiccrunch_server_connection_count(
    handle: *mut crate::types::QuicTransportHandle,
) -> u32 {
    if handle.is_null() {
        return 0;
    }
    
    let server_internal = unsafe { &*(handle as *mut QuicServerInternal) };
    server_internal.connections.len() as u32
}