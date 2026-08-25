//! Error types for Virtual Sensors HAL.

use aidl_compat::status::{Status, STATUS_BAD_VALUE, STATUS_INVALID_OPERATION, STATUS_UNKNOWN_ERROR};
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SensorsHalError {
    #[error("Sensor handle {0} not found")]
    SensorNotFound(i32),

    #[error("Sensor handle {0} is already active")]
    SensorAlreadyActive(i32),

    #[error("Sensor handle {0} is not active")]
    SensorNotActive(i32),

    #[error("Invalid operation mode: {0}")]
    InvalidOperationMode(i32),

    #[error("Operation not permitted in current mode: expected {expected:?}, got {actual:?}")]
    OperationNotPermitted {
        expected: &'static str,
        actual: &'static str,
    },

    #[error("Invalid sampling rate: period={period_ns}ns, latency={latency_ns}ns")]
    InvalidSamplingRate {
        period_ns: i64,
        latency_ns: i64,
    },

    #[error("Event queue is full (capacity {0})")]
    EventQueueFull(usize),

    #[error("Event queue not initialized")]
    EventQueueNotInitialized,

    #[error("Parcel serialization/deserialization failed: {0}")]
    ParcelError(String),

    #[error("Binder transaction error: {0}")]
    BinderError(String),
}

impl From<SensorsHalError> for Status {
    fn from(err: SensorsHalError) -> Self {
        match err {
            SensorsHalError::SensorNotFound(_) => Status::from_status(STATUS_BAD_VALUE),
            SensorsHalError::SensorAlreadyActive(_) => Status::from_status(STATUS_INVALID_OPERATION),
            SensorsHalError::SensorNotActive(_) => Status::from_status(STATUS_INVALID_OPERATION),
            SensorsHalError::InvalidOperationMode(_) => Status::from_status(STATUS_BAD_VALUE),
            SensorsHalError::OperationNotPermitted { .. } => {
                Status::from_status(STATUS_INVALID_OPERATION)
            }
            SensorsHalError::InvalidSamplingRate { .. } => Status::from_status(STATUS_BAD_VALUE),
            SensorsHalError::EventQueueFull(_) => Status::from_status(STATUS_INVALID_OPERATION),
            SensorsHalError::EventQueueNotInitialized => {
                Status::from_status(STATUS_INVALID_OPERATION)
            }
            SensorsHalError::ParcelError(_) => Status::from_status(STATUS_BAD_VALUE),
            SensorsHalError::BinderError(_) => Status::from_status(STATUS_UNKNOWN_ERROR),
        }
    }
}
