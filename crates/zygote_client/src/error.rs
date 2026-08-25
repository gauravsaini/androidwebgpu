//! Error types for Zygote client communications and process management.

use thiserror::Error;

/// Result type for Zygote client operations.
pub type ZygoteResult<T> = Result<T, ZygoteError>;

/// Errors occurring during Zygote IPC, command encoding, or process management.
#[derive(Debug, Error)]
pub enum ZygoteError {
    #[error("I/O error communicating with Zygote socket: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to connect to Zygote socket at '{0}': {1}")]
    ConnectionFailed(String, String),

    #[error("Zygote protocol violation: {0}")]
    ProtocolViolation(String),

    #[error("Zygote fork failed with error PID/code {pid}: {message}")]
    ForkFailed { pid: i32, message: String },

    #[error("Invalid PID response: expected 4 bytes, received {0} bytes")]
    InvalidPidResponse(usize),

    #[error("Process not found with PID {0}")]
    ProcessNotFound(u32),

    #[error("Process already registered with PID {0}")]
    ProcessAlreadyExists(u32),

    #[error("Mock Zygote failure: {0}")]
    MockFailure(String),
}
