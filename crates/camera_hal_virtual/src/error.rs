//! Error types for Virtual Camera HAL.

use aidl_compat::status::{Status, STATUS_BAD_VALUE, STATUS_INVALID_OPERATION, STATUS_UNKNOWN_ERROR};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum CameraHalError {
    #[error("Camera device not found: {0}")]
    DeviceNotFound(String),

    #[error("Camera device session already open or not configured")]
    SessionError(String),

    #[error("Invalid stream configuration: {0}")]
    InvalidStreamConfig(String),

    #[error("Capture request error: {0}")]
    CaptureRequestError(String),

    #[error("VINTF validation failed: service {0} not declared in manifest")]
    VintfNotDeclared(String),

    #[error("AIDL serialization/deserialization error: {0}")]
    AidlError(String),
}

impl From<CameraHalError> for Status {
    fn from(err: CameraHalError) -> Self {
        match err {
            CameraHalError::DeviceNotFound(_) => Status::from_status(STATUS_BAD_VALUE),
            CameraHalError::SessionError(_) => Status::from_status(STATUS_INVALID_OPERATION),
            CameraHalError::InvalidStreamConfig(_) => Status::from_status(STATUS_BAD_VALUE),
            CameraHalError::CaptureRequestError(_) => Status::from_status(STATUS_BAD_VALUE),
            CameraHalError::VintfNotDeclared(_) => Status::from_status(STATUS_BAD_VALUE),
            CameraHalError::AidlError(_) => Status::from_status(STATUS_UNKNOWN_ERROR),
        }
    }
}
