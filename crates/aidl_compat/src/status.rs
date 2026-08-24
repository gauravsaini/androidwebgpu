//! Status codes and AIDL exception representations.

pub use binder_rt::status::{
    ExceptionCode, Status, StatusCode, EX_BAD_PARCELABLE, EX_HAS_REPLY_HEADER, EX_ILLEGAL_ARGUMENT,
    EX_ILLEGAL_STATE, EX_NETWORK_MAIN_THREAD, EX_NONE, EX_NULL_POINTER, EX_PARCELABLE, EX_SECURITY,
    EX_SERVICE_SPECIFIC, EX_TRANSACTION_FAILED, EX_UNSUPPORTED_OPERATION, STATUS_ALREADY_EXISTS,
    STATUS_BAD_TYPE, STATUS_BAD_VALUE, STATUS_DEAD_OBJECT, STATUS_FAILED_TRANSACTION,
    STATUS_INVALID_OPERATION, STATUS_NAME_NOT_FOUND, STATUS_NO_MEMORY, STATUS_OK,
    STATUS_PERMISSION_DENIED, STATUS_TIMED_OUT, STATUS_UNKNOWN_ERROR, STATUS_UNKNOWN_TRANSACTION,
};

/// Type alias for Results carrying AIDL Status errors.
pub type Result<T> = binder_rt::Result<T>;

/// Helper to create an `Exception` Status.
#[allow(non_snake_case)]
pub fn Exception(code: ExceptionCode, msg: impl Into<String>) -> Status {
    let msg_str = msg.into();
    Status::new_exception(code, Some(&msg_str))
}

/// Helper to create a service-specific exception Status.
#[allow(non_snake_case)]
pub fn ServiceSpecificException(code: i32, msg: impl Into<String>) -> Status {
    let msg_str = msg.into();
    Status::new_service_specific_error(code, Some(&msg_str))
}
