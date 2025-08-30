use std::os::raw::{c_char, c_void};
use std::sync::Arc;
use std::collections::HashMap;
use quiccrunch_core::{QuicClient as CoreClient, QuicConnection as CoreConnection};
use crate::error::QuicError;
use crate::types::{QuicCallback, QuicTlsConfig, QuicConnectionConfig, QuicMessage, c_str_to_str, str_to_c_str};
use crate::runtime::FfiRuntime;

/// Internal client handle structure
pub struct QuicClientInternal {
    client: Option<CoreClient>,
    connections: HashMap<u32, Arc<CoreConnection>>,
    next_connection_id: u32,
}

impl QuicClientInternal {
    fn new() -> Self {
        Self {
            client: None,
            connections: HashMap::new(),
            next_connection_id: 1,
        }
    }
    
    fn add_connection(&mut self, conn: CoreConnection) -> u32 {
        let id = self.next_connection_id;
        self.next_connection_id += 1;
        self.connections.insert(id, Arc::new(conn));
        id
    }
    
    fn get_connection(&self, id: u32) -> Option<Arc<CoreConnection>> {
        self.connections.get(&id).cloned()
    }
    
    fn remove_connection(&mut self, id: u32) -> bool {
        self.connections.remove(&id).is_some()
    }
}

/// Create a new QUIC client
/// 
/// Returns a handle to the client, or null on error.
/// The handle must be freed with quiccrunch_client_free().
#[no_mangle]
pub extern "C" fn quiccrunch_client_new() -> *mut crate::types::QuicTransportHandle {
    let client_internal = Box::new(QuicClientInternal::new());
    Box::into_raw(client_internal) as *mut crate::types::QuicTransportHandle
}

/// Create a new QUIC client with TLS configuration
/// 
/// # Arguments
/// * `tls_config` - TLS configuration for the client
/// 
/// Returns a handle to the client, or null on error.
#[no_mangle]
pub extern "C" fn quiccrunch_client_new_with_tls(
    tls_config: *const QuicTlsConfig
) -> *mut crate::types::QuicTransportHandle {
    if tls_config.is_null() {
        return std::ptr::null_mut();
    }
    
    let tls_config = unsafe { &*tls_config };
    
    // For now, just create a basic client
    // TODO: Implement actual TLS configuration
    let client_internal = Box::new(QuicClientInternal::new());
    Box::into_raw(client_internal) as *mut crate::types::QuicTransportHandle
}

/// Free a QUIC client handle
/// 
/// # Safety
/// The handle must have been created by quiccrunch_client_new() and
/// must not be used after calling this function.
#[no_mangle]
pub unsafe extern "C" fn quiccrunch_client_free(handle: *mut crate::types::QuicTransportHandle) {
    if !handle.is_null() {
        let _ = Box::from_raw(handle as *mut QuicClientInternal);
    }
}

/// Initialize the client (creates the underlying transport)
/// 
/// This must be called before connecting to servers.
/// 
/// # Arguments
/// * `handle` - Client handle
/// 
/// Returns QuicError::Success on success, error code on failure.
#[no_mangle]
pub extern "C" fn quiccrunch_client_init(
    handle: *mut crate::types::QuicTransportHandle
) -> QuicError {
    if handle.is_null() {
        return QuicError::InvalidHandle;
    }
    
    let client_internal = unsafe { &mut *(handle as *mut QuicClientInternal) };
    
    let result = CoreClient::new();
    
    match result {
        Ok(client) => {
            client_internal.client = Some(client);
            QuicError::Success
        }
        Err(err) => err.into(),
    }
}

/// Connect to a server asynchronously
/// 
/// # Arguments
/// * `handle` - Client handle
/// * `address` - Server address (e.g., "127.0.0.1:50051")
/// * `callback` - Callback function to call when connection completes
/// * `user_data` - User data to pass to callback
/// 
/// The callback will be called with:
/// - user_data: The user_data parameter
/// - error: QuicError::Success on success, error code on failure  
/// - result_data: Connection ID (u32*) on success, null on error
#[no_mangle]
pub extern "C" fn quiccrunch_client_connect_async(
    handle: *mut crate::types::QuicTransportHandle,
    address: *const c_char,
    callback: QuicCallback,
    user_data: *mut c_void,
) -> QuicError {
    if handle.is_null() || address.is_null() {
        return QuicError::InvalidHandle;
    }
    
    let address_str = match c_str_to_str(address) {
        Ok(s) => s.to_string(),
        Err(e) => return e,
    };
    
    let client_internal = unsafe { &mut *(handle as *mut QuicClientInternal) };
    
    let client = match &client_internal.client {
        Some(c) => c.clone(),
        None => return QuicError::InvalidHandle,
    };
    
    // We need to move the client_internal reference to capture the connection
    // This is tricky with the current approach - let's use a simpler method for now
    FfiRuntime::spawn_with_callback(
        async move {
            let connection = client.connect(&address_str).await?;
            // TODO: Store connection and return ID
            Ok(())
        },
        callback,
        user_data,
    );
    
    QuicError::Success
}

/// Connect to a server synchronously (blocks until connected)
/// 
/// # Arguments
/// * `handle` - Client handle
/// * `address` - Server address (e.g., "127.0.0.1:50051")
/// * `connection_id` - Output parameter for connection ID
/// 
/// Returns QuicError::Success on success, error code on failure.
#[no_mangle]
pub extern "C" fn quiccrunch_client_connect(
    handle: *mut crate::types::QuicTransportHandle,
    address: *const c_char,
    connection_id: *mut u32,
) -> QuicError {
    if handle.is_null() || address.is_null() {
        return QuicError::InvalidHandle;
    }
    
    let address_str = match c_str_to_str(address) {
        Ok(s) => s,
        Err(e) => return e,
    };
    
    let client_internal = unsafe { &mut *(handle as *mut QuicClientInternal) };
    
    let client = match &client_internal.client {
        Some(c) => c.clone(),
        None => return QuicError::InvalidHandle,
    };
    
    let result = FfiRuntime::block_on(async {
        client.connect(address_str).await
    });
    
    match result {
        Ok(connection) => {
            let id = client_internal.add_connection(connection);
            if !connection_id.is_null() {
                unsafe { *connection_id = id; }
            }
            QuicError::Success
        }
        Err(err) => err.into(),
    }
}

/// Disconnect from a server
/// 
/// # Arguments
/// * `handle` - Client handle
/// * `connection_id` - Connection ID to disconnect
/// 
/// Returns QuicError::Success on success, error code on failure.
#[no_mangle]
pub extern "C" fn quiccrunch_client_disconnect(
    handle: *mut crate::types::QuicTransportHandle,
    connection_id: u32,
) -> QuicError {
    if handle.is_null() {
        return QuicError::InvalidHandle;
    }
    
    let client_internal = unsafe { &mut *(handle as *mut QuicClientInternal) };
    
    if client_internal.remove_connection(connection_id) {
        QuicError::Success
    } else {
        QuicError::InvalidArgument
    }
}

/// Send a message on a connection
/// 
/// # Arguments
/// * `handle` - Client handle
/// * `connection_id` - Connection ID
/// * `message` - Message to send
/// 
/// Returns QuicError::Success on success, error code on failure.
#[no_mangle]
pub extern "C" fn quiccrunch_client_send_message(
    handle: *mut crate::types::QuicTransportHandle,
    connection_id: u32,
    message: *const QuicMessage,
) -> QuicError {
    if handle.is_null() || message.is_null() {
        return QuicError::InvalidHandle;
    }
    
    let client_internal = unsafe { &*(handle as *mut QuicClientInternal) };
    let message = unsafe { &*message };
    
    let connection = match client_internal.get_connection(connection_id) {
        Some(conn) => conn,
        None => return QuicError::InvalidArgument,
    };
    
    let data = crate::types::message_to_bytes(message).to_vec();
    
    let result = FfiRuntime::block_on(async move {
        let mut stream = connection.open_stream().await?;
        stream.send_message(Bytes::from(data)).await?;
        Ok(())
    });
    
    match result {
        Ok(()) => QuicError::Success,
        Err(err) => err.into(),
    }
}

/// Get the number of active connections
/// 
/// # Arguments
/// * `handle` - Client handle
/// 
/// Returns the number of active connections, or 0 if handle is invalid.
#[no_mangle]
pub extern "C" fn quiccrunch_client_connection_count(
    handle: *mut crate::types::QuicTransportHandle,
) -> u32 {
    if handle.is_null() {
        return 0;
    }
    
    let client_internal = unsafe { &*(handle as *mut QuicClientInternal) };
    client_internal.connections.len() as u32
}