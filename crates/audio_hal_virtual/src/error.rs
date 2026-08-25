//! Error types for Virtual Audio HAL.

use aidl_compat::status::{Status, STATUS_BAD_VALUE, STATUS_INVALID_OPERATION, STATUS_UNKNOWN_ERROR};
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum AudioHalError {
    #[error("Audio stream id {0} not found")]
    StreamNotFound(i32),

    #[error("Audio port id {0} not found")]
    PortNotFound(i32),

    #[error("Invalid audio configuration: sample_rate={sample_rate}, channels={channels}, format={format:?}")]
    InvalidConfig {
        sample_rate: u32,
        channels: u32,
        format: String,
    },

    #[error("Audio buffer overflow: requested {requested} bytes, available {available} bytes")]
    BufferOverflow {
        requested: usize,
        available: usize,
    },

    #[error("Audio buffer underflow: requested {requested} bytes, available {available} bytes")]
    BufferUnderflow {
        requested: usize,
        available: usize,
    },

    #[error("Stream is in invalid state: {0}")]
    InvalidStreamState(&'static str),

    #[error("Stream is closed")]
    StreamClosed,

    #[error("Parcel serialization/deserialization failed: {0}")]
    ParcelError(String),

    #[error("Binder transaction failed: {0}")]
    BinderError(String),
}

impl From<AudioHalError> for Status {
    fn from(err: AudioHalError) -> Self {
        match err {
            AudioHalError::StreamNotFound(_) => Status::from_status(STATUS_BAD_VALUE),
            AudioHalError::PortNotFound(_) => Status::from_status(STATUS_BAD_VALUE),
            AudioHalError::InvalidConfig { .. } => Status::from_status(STATUS_BAD_VALUE),
            AudioHalError::BufferOverflow { .. } => Status::from_status(STATUS_INVALID_OPERATION),
            AudioHalError::BufferUnderflow { .. } => Status::from_status(STATUS_INVALID_OPERATION),
            AudioHalError::InvalidStreamState(_) => Status::from_status(STATUS_INVALID_OPERATION),
            AudioHalError::StreamClosed => Status::from_status(STATUS_INVALID_OPERATION),
            AudioHalError::ParcelError(_) => Status::from_status(STATUS_BAD_VALUE),
            AudioHalError::BinderError(_) => Status::from_status(STATUS_UNKNOWN_ERROR),
        }
    }
}
