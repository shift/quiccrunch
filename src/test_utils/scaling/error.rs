use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum BenchError {
    Io(std::io::Error),
    System(String),
}

impl fmt::Display for BenchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BenchError::Io(err) => write!(f, "I/O error: {}", err),
            BenchError::System(msg) => write!(f, "System error: {}", msg),
        }
    }
}

impl Error for BenchError {}

impl From<std::io::Error> for BenchError {
    fn from(err: std::io::Error) -> Self {
        BenchError::Io(err)
    }
}

pub type Result<T> = std::result::Result<T, BenchError>;