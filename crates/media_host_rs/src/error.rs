//! Error types for Media Codec Service and WebCodecs Bridge.

use aidl_compat::status::{Status, STATUS_BAD_VALUE, STATUS_INVALID_OPERATION, STATUS_UNKNOWN_ERROR};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum MediaCodecError {
    #[error("Codec not found for name or MIME type: {0}")]
    CodecNotFound(String),

    #[error("Invalid codec state: {0}")]
    InvalidState(String),

    #[error("Invalid buffer index {0}")]
    InvalidBufferIndex(u32),

    #[error("Buffer queue full or unavailable")]
    BufferUnavailable,

    #[error("Bitstream parsing error: {0}")]
    BitstreamParseError(String),

    #[error("Surface binding or WebGPU render error: {0}")]
    SurfaceError(String),

    #[error("AIDL error: {0}")]
    Aidl(String),
}

impl From<MediaCodecError> for Status {
    fn from(err: MediaCodecError) -> Self {
        match err {
            MediaCodecError::CodecNotFound(_) => Status::from_status(STATUS_BAD_VALUE),
            MediaCodecError::InvalidState(_) => Status::from_status(STATUS_INVALID_OPERATION),
            MediaCodecError::InvalidBufferIndex(_) => Status::from_status(STATUS_BAD_VALUE),
            MediaCodecError::BufferUnavailable => Status::from_status(STATUS_INVALID_OPERATION),
            MediaCodecError::BitstreamParseError(_) => Status::from_status(STATUS_BAD_VALUE),
            MediaCodecError::SurfaceError(_) => Status::from_status(STATUS_INVALID_OPERATION),
            MediaCodecError::Aidl(_) => Status::from_status(STATUS_UNKNOWN_ERROR),
        }
    }
}
