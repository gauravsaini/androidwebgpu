//! Error definitions for the `inputflinger_rs` InputFlinger crate.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum InputFlingerError {
    #[error("No focused window or input channel registered")]
    NoFocusedWindow,

    #[error("Channel error: {0}")]
    Channel(String),

    #[error("Evdev reader error: {0}")]
    Evdev(String),

    #[error("Invalid event format: {0}")]
    InvalidEvent(String),

    #[error("Timeout waiting for input ack")]
    Timeout,

    #[error("AIDL error: {0}")]
    Aidl(String),
}

pub type InputFlingerResult<T> = std::result::Result<T, InputFlingerError>;
