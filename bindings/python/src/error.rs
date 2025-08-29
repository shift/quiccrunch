use pyo3::prelude::*;
use pyo3::exceptions::PyException;

#[pyclass]
#[derive(Debug, Clone)]
pub struct QuicCrunchError {
    #[pyo3(get)]
    pub message: String,
    #[pyo3(get)]
    pub error_code: i32,
}

#[pymethods]
impl QuicCrunchError {
    #[new]
    fn new(message: String, error_code: i32) -> Self {
        Self { message, error_code }
    }
    
    fn __str__(&self) -> String {
        format!("QuicCrunchError({}): {}", self.error_code, self.message)
    }
    
    fn __repr__(&self) -> String {
        format!("QuicCrunchError(message='{}', error_code={})", self.message, self.error_code)
    }
}

impl std::fmt::Display for QuicCrunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "QuicCrunchError({}): {}", self.error_code, self.message)
    }
}

impl std::error::Error for QuicCrunchError {}

impl From<QuicCrunchError> for PyErr {
    fn from(err: QuicCrunchError) -> PyErr {
        PyException::new_err(err.to_string())
    }
}

pub fn map_ffi_error(error_code: quiccrunch_ffi::QuicError) -> Result<(), QuicCrunchError> {
    match error_code {
        quiccrunch_ffi::QuicError::Success => Ok(()),
        quiccrunch_ffi::QuicError::InvalidArgument => Err(QuicCrunchError::new(
            "Invalid argument provided".to_string(), 
            1
        )),
        quiccrunch_ffi::QuicError::ConnectionFailed => Err(QuicCrunchError::new(
            "Connection failed".to_string(), 
            2
        )),
        quiccrunch_ffi::QuicError::TimeoutError => Err(QuicCrunchError::new(
            "Operation timed out".to_string(), 
            3
        )),
        quiccrunch_ffi::QuicError::InvalidHandle => Err(QuicCrunchError::new(
            "Invalid handle provided".to_string(), 
            4
        )),
        quiccrunch_ffi::QuicError::SerializationError => Err(QuicCrunchError::new(
            "Serialization error".to_string(), 
            5
        )),
        quiccrunch_ffi::QuicError::NetworkError => Err(QuicCrunchError::new(
            "Network error".to_string(), 
            6
        )),
        quiccrunch_ffi::QuicError::InternalError => Err(QuicCrunchError::new(
            "Internal error".to_string(), 
            7
        )),
    }
}