use crate::{QuicCrunchError, map_ffi_error, RUNTIME};
use pyo3::prelude::*;
use pyo3_asyncio::tokio::future_into_py;
use std::sync::Arc;
use tokio::sync::Mutex;

#[pyclass]
pub struct QuicClient {
    handle: Arc<Mutex<Option<*mut quiccrunch_ffi::QuicTransportHandle>>>,
}

unsafe impl Send for QuicClient {}
unsafe impl Sync for QuicClient {}

#[pymethods]
impl QuicClient {
    #[new]
    fn new() -> PyResult<Self> {
        let handle = unsafe { quiccrunch_ffi::quiccrunch_client_new() };
        if handle.is_null() {
            return Err(QuicCrunchError::new(
                "Failed to create client".to_string(),
                1
            ).into());
        }
        
        Ok(QuicClient {
            handle: Arc::new(Mutex::new(Some(handle))),
        })
    }
    
    /// Initialize the client
    fn init(&self) -> PyResult<()> {
        let handle = RUNTIME.block_on(async {
            self.handle.lock().await.clone()
        });
        
        match handle {
            Some(h) => {
                let result = unsafe { quiccrunch_ffi::quiccrunch_client_init(h) };
                map_ffi_error(result).map_err(|e| e.into())
            }
            None => Err(QuicCrunchError::new(
                "Client handle is null".to_string(),
                4
            ).into())
        }
    }
    
    /// Connect to a server (blocking)
    fn connect(&self, address: String) -> PyResult<u32> {
        let handle = RUNTIME.block_on(async {
            self.handle.lock().await.clone()
        });
        
        match handle {
            Some(h) => {
                let mut connection_id: u32 = 0;
                let addr_cstr = std::ffi::CString::new(address)
                    .map_err(|_| QuicCrunchError::new("Invalid address string".to_string(), 1))?;
                
                let result = unsafe {
                    quiccrunch_ffi::quiccrunch_client_connect(
                        h,
                        addr_cstr.as_ptr(),
                        &mut connection_id
                    )
                };
                
                map_ffi_error(result)?;
                Ok(connection_id)
            }
            None => Err(QuicCrunchError::new(
                "Client handle is null".to_string(),
                4
            ).into())
        }
    }
    
    /// Connect to a server (async)
    fn connect_async(&self, py: Python, address: String) -> PyResult<&PyAny> {
        let handle = self.handle.clone();
        
        future_into_py(py, async move {
            let h = handle.lock().await.clone();
            
            match h {
                Some(handle_ptr) => {
                    let mut connection_id: u32 = 0;
                    let addr_cstr = std::ffi::CString::new(address)
                        .map_err(|_| QuicCrunchError::new("Invalid address string".to_string(), 1))?;
                    
                    // For now, we'll use the blocking version wrapped in async
                    // In a full implementation, this would use the async FFI function
                    let result = tokio::task::spawn_blocking(move || {
                        unsafe {
                            quiccrunch_ffi::quiccrunch_client_connect(
                                handle_ptr,
                                addr_cstr.as_ptr(),
                                &mut connection_id
                            )
                        }
                    }).await.map_err(|_| QuicCrunchError::new("Task join error".to_string(), 7))?;
                    
                    map_ffi_error(result)?;
                    Ok(connection_id)
                }
                None => Err(QuicCrunchError::new(
                    "Client handle is null".to_string(),
                    4
                ))
            }
        })
    }
    
    /// Send a message
    fn send_message(&self, connection_id: u32, data: Vec<u8>) -> PyResult<()> {
        let handle = RUNTIME.block_on(async {
            self.handle.lock().await.clone()
        });
        
        match handle {
            Some(h) => {
                let message = quiccrunch_ffi::QuicMessage {
                    data: data.as_ptr(),
                    len: data.len(),
                };
                
                let result = unsafe {
                    quiccrunch_ffi::quiccrunch_client_send_message(h, connection_id, &message)
                };
                
                map_ffi_error(result).map_err(|e| e.into())
            }
            None => Err(QuicCrunchError::new(
                "Client handle is null".to_string(),
                4
            ).into())
        }
    }
    
    /// Disconnect from server
    fn disconnect(&self, connection_id: u32) -> PyResult<()> {
        let handle = RUNTIME.block_on(async {
            self.handle.lock().await.clone()
        });
        
        match handle {
            Some(h) => {
                let result = unsafe {
                    quiccrunch_ffi::quiccrunch_client_disconnect(h, connection_id)
                };
                
                map_ffi_error(result).map_err(|e| e.into())
            }
            None => Err(QuicCrunchError::new(
                "Client handle is null".to_string(),
                4
            ).into())
        }
    }
    
    /// Get connection count
    fn connection_count(&self) -> PyResult<u32> {
        let handle = RUNTIME.block_on(async {
            self.handle.lock().await.clone()
        });
        
        match handle {
            Some(h) => {
                let count = unsafe {
                    quiccrunch_ffi::quiccrunch_client_connection_count(h)
                };
                Ok(count)
            }
            None => Err(QuicCrunchError::new(
                "Client handle is null".to_string(),
                4
            ).into())
        }
    }
}

impl Drop for QuicClient {
    fn drop(&mut self) {
        if let Ok(handle) = self.handle.try_lock() {
            if let Some(h) = *handle {
                unsafe {
                    quiccrunch_ffi::quiccrunch_client_free(h);
                }
            }
        }
    }
}