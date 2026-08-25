//! VINTF Manifest Data Models and `is_declared` query logic.

use crate::error::VintfError;
use serde::{Deserialize, Serialize};
use std::fmt;

/// HAL format descriptor (AIDL or HIDL).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HalFormat {
    Aidl,
    Hidl,
}

impl fmt::Display for HalFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HalFormat::Aidl => write!(f, "aidl"),
            HalFormat::Hidl => write!(f, "hidl"),
        }
    }
}

/// A declared HAL entry inside `<hal format="...">...</hal>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HalManifestEntry {
    pub format: HalFormat,
    pub name: String,
    pub version: u32,
    pub fqnames: Vec<String>,
}

impl HalManifestEntry {
    /// Return all fully-qualified service names declared by this HAL entry.
    pub fn full_service_names(&self) -> Vec<String> {
        self.fqnames
            .iter()
            .map(|fq| {
                if fq.contains('.') {
                    fq.clone()
                } else {
                    format!("{}.{}", self.name, fq)
                }
            })
            .collect()
    }

    /// Check whether a service name matches this HAL entry.
    pub fn matches_service(&self, service_name: &str) -> bool {
        let trimmed = service_name.trim();
        for full_name in self.full_service_names() {
            if full_name == trimmed {
                return true;
            }
        }
        for fq in &self.fqnames {
            if fq == trimmed {
                return true;
            }
        }
        false
    }
}

/// Complete VINTF Device Manifest representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VintfManifest {
    pub version: String,
    pub manifest_type: String,
    pub target_level: u32,
    pub hals: Vec<HalManifestEntry>,
}

impl VintfManifest {
    /// Parse VINTF manifest from an XML string.
    pub fn from_xml_str(xml: &str) -> Result<Self, VintfError> {
        crate::parser::parse_manifest_xml(xml)
    }

    /// Read and parse VINTF manifest from a file path.
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, VintfError> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| VintfError::Io(format!("{}: {}", path.as_ref().display(), e)))?;
        Self::from_xml_str(&content)
    }

    /// Check if a service name is declared in this manifest.
    pub fn is_declared(&self, service_name: &str) -> bool {
        self.hals.iter().any(|hal| hal.matches_service(service_name))
    }

    /// Retrieve list of all declared fully-qualified service names.
    pub fn declared_services(&self) -> Vec<String> {
        self.hals
            .iter()
            .flat_map(|hal| hal.full_service_names())
            .collect()
    }

    /// Validate target-level satisfies minimum requirement.
    pub fn validate_target_level(&self, required_level: u32) -> Result<(), VintfError> {
        if self.target_level < required_level {
            return Err(VintfError::InvalidTargetLevel {
                found: self.target_level,
                required: required_level,
            });
        }
        Ok(())
    }

    /// Find HAL entry by package name.
    pub fn get_hal(&self, name: &str) -> Option<&HalManifestEntry> {
        self.hals.iter().find(|h| h.name == name)
    }
}
