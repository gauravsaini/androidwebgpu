//! Error definitions for the `wms_rs` Window Manager crate.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WmsError {
    #[error("Session {0} not found")]
    SessionNotFound(u32),

    #[error("Window token not found or invalid")]
    WindowNotFound,

    #[error("Surface bridge error: {0}")]
    SurfaceBridge(String),

    #[error("Compositor error: {0}")]
    Compositor(String),

    #[error("Input channel error: {0}")]
    InputChannel(String),

    #[error("AIDL IPC error: {0}")]
    Aidl(String),

    #[error("Invalid layout parameters: {0}")]
    InvalidLayoutParams(String),
}

pub type WmsResult<T> = std::result::Result<T, WmsError>;
