//! # vintf_validator
//!
//! Android 13 VINTF Device Manifest (`device_manifest.xml`) Parser and Target-Level 7 AIDL HAL Validator
//! for AndroidWebGPU.
//!
//! Validates declared HAL services including:
//! - `android.hardware.sensors.ISensors/default` (v1)
//! - `android.hardware.audio.core.IModule/default` (v1)
//! - `android.hardware.audio.core.IConfig/default` (v1)
//! - `android.hardware.camera.provider.ICameraProvider/virtual/0` (v1)

pub mod error;
pub mod manifest;
pub mod parser;

pub use error::VintfError;
pub use manifest::{HalFormat, HalManifestEntry, VintfManifest};

/// Embedded default device manifest XML content for target-level 7.
pub const DEFAULT_DEVICE_MANIFEST_XML: &str = r#"<manifest version="5.0" type="device" target-level="7">
    <hal format="aidl">
        <name>android.hardware.sensors</name>
        <version>1</version>
        <fqname>ISensors/default</fqname>
    </hal>
    <hal format="aidl">
        <name>android.hardware.audio.core</name>
        <version>1</version>
        <fqname>IModule/default</fqname>
    </hal>
    <hal format="aidl">
        <name>android.hardware.audio.core</name>
        <version>1</version>
        <fqname>IConfig/default</fqname>
    </hal>
    <hal format="aidl">
        <name>android.hardware.camera.provider</name>
        <version>1</version>
        <fqname>ICameraProvider/virtual/0</fqname>
    </hal>
</manifest>"#;

/// Load the default VINTF manifest.
pub fn load_default_manifest() -> Result<VintfManifest, VintfError> {
    // Attempt to load from relative guest path if running within repo workspace, else use embedded
    let guest_path = "guest/etc/vintf/device_manifest.xml";
    if let Ok(manifest) = VintfManifest::from_file(guest_path) {
        return Ok(manifest);
    }
    VintfManifest::from_xml_str(DEFAULT_DEVICE_MANIFEST_XML)
}

/// Global helper verifying whether a service name is declared in the VINTF manifest.
pub fn is_declared(service_name: &str) -> bool {
    let manifest = load_default_manifest().unwrap_or_else(|_| {
        VintfManifest::from_xml_str(DEFAULT_DEVICE_MANIFEST_XML).expect("Embedded manifest must be valid")
    });
    manifest.is_declared(service_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_default_embedded_manifest() {
        let manifest = VintfManifest::from_xml_str(DEFAULT_DEVICE_MANIFEST_XML)
            .expect("Failed to parse embedded manifest");
        assert_eq!(manifest.version, "5.0");
        assert_eq!(manifest.manifest_type, "device");
        assert_eq!(manifest.target_level, 7);
        assert_eq!(manifest.hals.len(), 4);

        manifest.validate_target_level(7).expect("Target level 7 check failed");
    }

    #[test]
    fn test_is_declared_for_all_required_hals() {
        assert!(is_declared("android.hardware.sensors.ISensors/default"));
        assert!(is_declared("android.hardware.audio.core.IModule/default"));
        assert!(is_declared("android.hardware.audio.core.IConfig/default"));
        assert!(is_declared("android.hardware.camera.provider.ICameraProvider/virtual/0"));

        // Negative checks
        assert!(!is_declared("android.hardware.nfc.INfc/default"));
        assert!(!is_declared("android.hardware.biometrics.fingerprint.IFingerprint/default"));
        assert!(!is_declared("android.hardware.camera.provider.ICameraProvider/legacy/0"));
    }

    #[test]
    fn test_manifest_from_guest_file() {
        let manifest = load_default_manifest().expect("Failed to load default manifest");
        assert!(manifest.is_declared("android.hardware.camera.provider.ICameraProvider/virtual/0"));
        let declared = manifest.declared_services();
        assert!(declared.contains(&"android.hardware.sensors.ISensors/default".to_string()));
        assert!(declared.contains(&"android.hardware.camera.provider.ICameraProvider/virtual/0".to_string()));
    }

    #[test]
    fn test_malformed_xml_handling() {
        assert!(VintfManifest::from_xml_str("").is_err());
        assert!(VintfManifest::from_xml_str("<invalid></invalid>").is_err());
        assert!(VintfManifest::from_xml_str("<manifest><hal><name>foo</name></hal></manifest>").is_err());
    }
}
