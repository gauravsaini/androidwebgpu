//! Error types for ActivityManagerService and Activity Lifecycle management.

use aidl_compat::status::Status;
use thiserror::Error;
use zygote_client::ZygoteError;

/// Result type for AMS internal operations.
pub type AmsResult<T> = Result<T, AmsError>;

/// Error variants for ActivityManagerService.
#[derive(Debug, Error)]
pub enum AmsError {
    #[error("Invalid lifecycle state transition from {from:?} to {to:?}")]
    InvalidStateTransition {
        from: crate::types::ActivityState,
        to: crate::types::ActivityState,
    },

    #[error("Activity record not found for token {0}")]
    ActivityNotFound(String),

    #[error("Task not found with ID {0}")]
    TaskNotFound(i32),

    #[error("Process not found with PID {0}")]
    ProcessNotFound(u32),

    #[error("Zygote client error: {0}")]
    Zygote(#[from] ZygoteError),

    #[error("AIDL IPC status error: {0:?}")]
    Aidl(#[from] Status),

    #[error("Package manager resolution error: {0}")]
    PmsError(String),

    #[error("Invalid or null binder token")]
    InvalidToken,

    #[error("Application thread IPC failure: {0}")]
    AppThreadError(String),
}

impl From<AmsError> for Status {
    fn from(err: AmsError) -> Self {
        match err {
            AmsError::Aidl(status) => status,
            AmsError::ActivityNotFound(_) | AmsError::TaskNotFound(_) | AmsError::ProcessNotFound(_) => {
                Status::from_status(aidl_compat::status::STATUS_NAME_NOT_FOUND)
            }
            AmsError::InvalidStateTransition { .. } | AmsError::InvalidToken => {
                Status::from_status(aidl_compat::status::STATUS_BAD_VALUE)
            }
            _ => Status::from_status(aidl_compat::status::STATUS_UNKNOWN_ERROR),
        }
    }
}
