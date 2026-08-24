//! Status codes and AIDL exception representations.

use std::fmt;

// -----------------------------------------------------------------------------
// Binder Status Codes (binder_status_t)
// -----------------------------------------------------------------------------

pub const STATUS_OK: i32 = 0;
pub const STATUS_UNKNOWN_ERROR: i32 = -2147483648;
pub const STATUS_NO_MEMORY: i32 = -12;
pub const STATUS_INVALID_OPERATION: i32 = -38;
pub const STATUS_BAD_VALUE: i32 = -22;
pub const STATUS_BAD_TYPE: i32 = -19;
pub const STATUS_NAME_NOT_FOUND: i32 = -2;
pub const STATUS_PERMISSION_DENIED: i32 = -1;
pub const STATUS_DEAD_OBJECT: i32 = -32;
pub const STATUS_FAILED_TRANSACTION: i32 = -129;
pub const STATUS_TIMED_OUT: i32 = -110;
pub const STATUS_ALREADY_EXISTS: i32 = -17;
pub const STATUS_UNKNOWN_TRANSACTION: i32 = -74;

/// Strongly typed Binder status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum StatusCode {
    Ok = STATUS_OK,
    UnknownError = STATUS_UNKNOWN_ERROR,
    NoMemory = STATUS_NO_MEMORY,
    InvalidOperation = STATUS_INVALID_OPERATION,
    BadValue = STATUS_BAD_VALUE,
    BadType = STATUS_BAD_TYPE,
    NameNotFound = STATUS_NAME_NOT_FOUND,
    PermissionDenied = STATUS_PERMISSION_DENIED,
    DeadObject = STATUS_DEAD_OBJECT,
    FailedTransaction = STATUS_FAILED_TRANSACTION,
    TimedOut = STATUS_TIMED_OUT,
    AlreadyExists = STATUS_ALREADY_EXISTS,
    UnknownTransaction = STATUS_UNKNOWN_TRANSACTION,
}

impl From<i32> for StatusCode {
    fn from(val: i32) -> Self {
        match val {
            STATUS_OK => StatusCode::Ok,
            STATUS_NO_MEMORY => StatusCode::NoMemory,
            STATUS_INVALID_OPERATION => StatusCode::InvalidOperation,
            STATUS_BAD_VALUE => StatusCode::BadValue,
            STATUS_BAD_TYPE => StatusCode::BadType,
            STATUS_NAME_NOT_FOUND => StatusCode::NameNotFound,
            STATUS_PERMISSION_DENIED => StatusCode::PermissionDenied,
            STATUS_DEAD_OBJECT => StatusCode::DeadObject,
            STATUS_FAILED_TRANSACTION => StatusCode::FailedTransaction,
            STATUS_TIMED_OUT => StatusCode::TimedOut,
            STATUS_ALREADY_EXISTS => StatusCode::AlreadyExists,
            STATUS_UNKNOWN_TRANSACTION => StatusCode::UnknownTransaction,
            _ => StatusCode::UnknownError,
        }
    }
}

// -----------------------------------------------------------------------------
// AIDL Exception Codes
// -----------------------------------------------------------------------------

pub const EX_NONE: i32 = 0;
pub const EX_SECURITY: i32 = -1;
pub const EX_BAD_PARCELABLE: i32 = -2;
pub const EX_ILLEGAL_ARGUMENT: i32 = -3;
pub const EX_NULL_POINTER: i32 = -4;
pub const EX_ILLEGAL_STATE: i32 = -5;
pub const EX_NETWORK_MAIN_THREAD: i32 = -6;
pub const EX_UNSUPPORTED_OPERATION: i32 = -7;
pub const EX_SERVICE_SPECIFIC: i32 = -8;
pub const EX_PARCELABLE: i32 = -9;
pub const EX_HAS_REPLY_HEADER: i32 = -128;
pub const EX_TRANSACTION_FAILED: i32 = -129;

/// AIDL Exception Code Enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ExceptionCode {
    None = EX_NONE,
    Security = EX_SECURITY,
    BadParcelable = EX_BAD_PARCELABLE,
    IllegalArgument = EX_ILLEGAL_ARGUMENT,
    NullPointer = EX_NULL_POINTER,
    IllegalState = EX_ILLEGAL_STATE,
    NetworkMainThread = EX_NETWORK_MAIN_THREAD,
    UnsupportedOperation = EX_UNSUPPORTED_OPERATION,
    ServiceSpecific = EX_SERVICE_SPECIFIC,
    Parcelable = EX_PARCELABLE,
    HasReplyHeader = EX_HAS_REPLY_HEADER,
    TransactionFailed = EX_TRANSACTION_FAILED,
}

impl From<i32> for ExceptionCode {
    fn from(code: i32) -> Self {
        match code {
            EX_NONE => ExceptionCode::None,
            EX_SECURITY => ExceptionCode::Security,
            EX_BAD_PARCELABLE => ExceptionCode::BadParcelable,
            EX_ILLEGAL_ARGUMENT => ExceptionCode::IllegalArgument,
            EX_NULL_POINTER => ExceptionCode::NullPointer,
            EX_ILLEGAL_STATE => ExceptionCode::IllegalState,
            EX_NETWORK_MAIN_THREAD => ExceptionCode::NetworkMainThread,
            EX_UNSUPPORTED_OPERATION => ExceptionCode::UnsupportedOperation,
            EX_SERVICE_SPECIFIC => ExceptionCode::ServiceSpecific,
            EX_PARCELABLE => ExceptionCode::Parcelable,
            EX_HAS_REPLY_HEADER => ExceptionCode::HasReplyHeader,
            _ => ExceptionCode::TransactionFailed,
        }
    }
}

// -----------------------------------------------------------------------------
// AIDL Status Struct
// -----------------------------------------------------------------------------

/// Detailed status / exception object conforming to AOSP `binder::Status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub exception: ExceptionCode,
    pub service_specific_error: i32,
    pub status: StatusCode,
    pub message: Option<String>,
}

impl Status {
    /// Create a success status.
    pub const fn ok() -> Self {
        Self {
            exception: ExceptionCode::None,
            service_specific_error: 0,
            status: StatusCode::Ok,
            message: None,
        }
    }

    /// Check whether the status represents success.
    pub fn is_ok(&self) -> bool {
        self.exception == ExceptionCode::None && self.status == StatusCode::Ok
    }

    /// Create an exception status with optional descriptive message.
    pub fn new_exception(exception: ExceptionCode, msg: Option<&str>) -> Self {
        Self {
            exception,
            service_specific_error: 0,
            status: StatusCode::Ok,
            message: msg.map(|s| s.to_string()),
        }
    }

    /// Create a service-specific exception status with code and optional message.
    pub fn new_service_specific_error(error_code: i32, msg: Option<&str>) -> Self {
        Self {
            exception: ExceptionCode::ServiceSpecific,
            service_specific_error: error_code,
            status: StatusCode::Ok,
            message: msg.map(|s| s.to_string()),
        }
    }

    /// Create a transport / kernel status error.
    pub fn from_status(status_code: i32) -> Self {
        Self {
            exception: ExceptionCode::TransactionFailed,
            service_specific_error: 0,
            status: StatusCode::from(status_code),
            message: None,
        }
    }

    /// Return exception code.
    pub const fn exception_code(&self) -> ExceptionCode {
        self.exception
    }

    /// Return raw status code.
    pub const fn status_code(&self) -> StatusCode {
        self.status
    }

    /// Return service-specific error code if applicable.
    pub fn service_specific_error(&self) -> Option<i32> {
        if self.exception == ExceptionCode::ServiceSpecific {
            Some(self.service_specific_error)
        } else {
            None
        }
    }

    /// Return error message slice if present.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

impl Default for Status {
    fn default() -> Self {
        Self::ok()
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_ok() {
            write!(f, "Status::Ok")
        } else if self.exception == ExceptionCode::ServiceSpecific {
            write!(
                f,
                "Status::ServiceSpecific(code={}, msg={:?})",
                self.service_specific_error, self.message
            )
        } else if self.exception != ExceptionCode::None {
            write!(
                f,
                "Status::Exception({:?}, msg={:?})",
                self.exception, self.message
            )
        } else {
            write!(f, "Status::Error({:?})", self.status)
        }
    }
}

impl std::error::Error for Status {}
