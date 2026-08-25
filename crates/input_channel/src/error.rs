//! Error types for the `input_channel` crate.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum InputChannelError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Channel closed or disconnected")]
    ChannelClosed,

    #[error("Buffer would block (EAGAIN / EWOULDBLOCK)")]
    WouldBlock,

    #[error("Invalid message size: expected at least {expected}, got {actual}")]
    InvalidMessageSize { expected: usize, actual: usize },

    #[error("Unknown input message type: {0}")]
    UnknownMessageType(u32),

    #[error("Sequence number mismatch: expected {expected}, got {actual}")]
    SequenceMismatch { expected: u32, actual: u32 },

    #[error("AIDL error: {0}")]
    Aidl(String),
}

pub type Result<T> = std::result::Result<T, InputChannelError>;
