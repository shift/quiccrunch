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
        // Convert raw pointers to usize for thread safety
        let callback_ptr = callback as usize;
        let user_data_ptr = user_data as usize;

        // Use std::thread::spawn to avoid Send requirement for raw pointers
        std::thread::spawn(move || {
            let result = crate::RUNTIME.block_on(future);
            // Convert back to function pointers
            let callback_fn = unsafe { std::mem::transmute::<usize, crate::types::QuicCallback>(callback_ptr) };
            let user_data_ptr = user_data_ptr as *mut std::os::raw::c_void;
            match result {
                Ok(_) => {
                    callback_fn(user_data_ptr, crate::error::QuicError::Success, std::ptr::null_mut());
                }
                Err(error) => {
                    callback_fn(user_data_ptr, error, std::ptr::null_mut());
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
        // Convert raw pointers to usize for thread safety
        let callback_ptr = callback as usize;
        let user_data_ptr = user_data as usize;

        crate::RUNTIME.spawn(async move {
            let result = future.await;
            // Convert back to function pointers
            let callback_fn = unsafe { std::mem::transmute::<usize, crate::types::QuicCallback>(callback_ptr) };
            let user_data_ptr = user_data_ptr as *mut std::os::raw::c_void;
            match result {
                Ok(data) => {
                    let boxed = Box::into_raw(Box::new(data));
                    callback_fn(user_data_ptr, crate::error::QuicError::Success, boxed as *mut std::os::raw::c_void);
                }
                Err(error) => {
                    callback_fn(user_data_ptr, error, std::ptr::null_mut());
                }
            }
        });
    }
}