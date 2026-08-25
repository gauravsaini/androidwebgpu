//! Error types for VINTF manifest parsing and validation.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum VintfError {
    #[error("XML parse error: {0}")]
    XmlParse(String),

    #[error("I/O error reading manifest: {0}")]
    Io(String),

    #[error("Missing required element or attribute: {0}")]
    MissingField(String),

    #[error("Invalid target level: found {found}, expected at least {required}")]
    InvalidTargetLevel { found: u32, required: u32 },

    #[error("Invalid HAL format: {0}")]
    InvalidHalFormat(String),

    #[error("Service is not declared in VINTF manifest: {0}")]
    ServiceNotDeclared(String),
}
