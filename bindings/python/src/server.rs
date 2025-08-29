use crate::{QuicCrunchError, map_ffi_error, RUNTIME};
use pyo3::prelude::*;
use pyo3_asyncio::tokio::future_into_py;
use std::sync::Arc;
use tokio::sync::Mutex;

#[pyclass]
pub struct QuicServer {
    handle: Arc<Mutex<Option<*mut quiccrunch_ffi::QuicTransportHandle>>>,
}

unsafe impl Send for QuicServer {}
unsafe impl Sync for QuicServer {}

#[pymethods]
impl QuicServer {
    #[new]
    fn new() -> PyResult<Self> {
        let handle = unsafe { quiccrunch_ffi::quiccrunch_server_new() };
        if handle.is_null() {
            return Err(QuicCrunchError::new(
                "Failed to create server".to_string(),
                1
            ).into());
        }
        
        Ok(QuicServer {
            handle: Arc::new(Mutex::new(Some(handle))),
        })
    }
    
    /// Initialize the server
    fn init(&self) -> PyResult<()> {
        let handle = RUNTIME.block_on(async {
            self.handle.lock().await.clone()
        });
        
        match handle {
            Some(h) => {
                let result = unsafe { quiccrunch_ffi::quiccrunch_server_init(h) };
                map_ffi_error(result).map_err(|e| e.into())
            }
            None => Err(QuicCrunchError::new(
                "Server handle is null".to_string(),
                4
            ).into())
        }
    }
    
    /// Start listening on an address
    fn listen(&self, address: String) -> PyResult<()> {
        let handle = RUNTIME.block_on(async {
            self.handle.lock().await.clone()
        });
        
        match handle {
            Some(h) => {
                let addr_cstr = std::ffi::CString::new(address)
                    .map_err(|_| QuicCrunchError::new("Invalid address string".to_string(), 1))?;
                
                let result = unsafe {
                    quiccrunch_ffi::quiccrunch_server_listen(h, addr_cstr.as_ptr())
                };
                
                map_ffi_error(result).map_err(|e| e.into())
            }
            None => Err(QuicCrunchError::new(
                "Server handle is null".to_string(),
                4
            ).into())
        }
    }
    
    /// Run the server (blocking)
    fn serve(&self) -> PyResult<()> {
        let handle = RUNTIME.block_on(async {
            self.handle.lock().await.clone()
        });
        
        match handle {
            Some(h) => {
                let result = unsafe { quiccrunch_ffi::quiccrunch_server_serve(h) };
                map_ffi_error(result).map_err(|e| e.into())
            }
            None => Err(QuicCrunchError::new(
                "Server handle is null".to_string(),
                4
            ).into())
        }
    }
    
    /// Run the server (async)
    fn serve_async(&self, py: Python) -> PyResult<&PyAny> {
        let handle = self.handle.clone();
        
        future_into_py(py, async move {
            let h = handle.lock().await.clone();
            
            match h {
                Some(handle_ptr) => {
                    // For now, we'll use the blocking version wrapped in async
                    let result = tokio::task::spawn_blocking(move || {
                        unsafe { quiccrunch_ffi::quiccrunch_server_serve(handle_ptr) }
                    }).await.map_err(|_| QuicCrunchError::new("Task join error".to_string(), 7))?;
                    
                    map_ffi_error(result)
                }
                None => Err(QuicCrunchError::new(
                    "Server handle is null".to_string(),
                    4
                ))
            }
        })
    }
    
    /// Get connection count
    fn connection_count(&self) -> PyResult<u32> {
        let handle = RUNTIME.block_on(async {
            self.handle.lock().await.clone()
        });
        
        match handle {
            Some(h) => {
                let count = unsafe {
                    quiccrunch_ffi::quiccrunch_server_connection_count(h)
                };
                Ok(count)
            }
            None => Err(QuicCrunchError::new(
                "Server handle is null".to_string(),
                4
            ).into())
        }
    }
}

impl Drop for QuicServer {
    fn drop(&mut self) {
        if let Ok(handle) = self.handle.try_lock() {
            if let Some(h) = *handle {
                unsafe {
                    quiccrunch_ffi::quiccrunch_server_free(h);
                }
            }
        }
    }
}