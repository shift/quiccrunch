use std::sync::Arc;
use tokio::sync::Mutex;

/// Runtime utilities for managing async operations in FFI context
pub struct FfiRuntime;

impl FfiRuntime {
    /// Execute an async operation and block until completion
    pub fn block_on<F, T>(future: F) -> T 
    where 
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        crate::RUNTIME.block_on(future)
    }
    
    /// Spawn an async task and execute a callback when complete
    pub fn spawn_with_callback<F, T>(
        future: F,
        callback: crate::types::QuicCallback,
        user_data: *mut std::os::raw::c_void,
    ) where
        F: std::future::Future<Output = Result<T, crate::error::QuicError>> + Send + 'static,
        T: Send + 'static,
    {
        // Use std::thread::spawn to avoid Send requirement for raw pointers
        std::thread::spawn(move || {
            let result = crate::RUNTIME.block_on(future);
            match result {
                Ok(_) => {
                    callback(user_data, crate::error::QuicError::Success, std::ptr::null_mut());
                }
                Err(error) => {
                    callback(user_data, error, std::ptr::null_mut());
                }
            }
        });
    }
    
    /// Spawn an async task that returns data via callback
    pub fn spawn_with_result_callback<F, T>(
        future: F,
        callback: crate::types::QuicCallback,
        user_data: *mut std::os::raw::c_void,
    ) where
        F: std::future::Future<Output = Result<T, crate::error::QuicError>> + Send + 'static,
        T: Send + 'static,
    {
        crate::RUNTIME.spawn(async move {
            let result = future.await;
            match result {
                Ok(data) => {
                    let boxed = Box::into_raw(Box::new(data));
                    callback(user_data, crate::error::QuicError::Success, boxed as *mut std::os::raw::c_void);
                }
                Err(error) => {
                    callback(user_data, error, std::ptr::null_mut());
                }
            }
        });
    }
}