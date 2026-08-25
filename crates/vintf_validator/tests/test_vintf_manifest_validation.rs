//! Integration Tests for VINTF Manifest Parsing and isDeclared() Validation.

use vintf_validator::{is_declared, HalFormat, VintfError, VintfManifest};

#[test]
fn test_device_manifest_target_level_and_declarations() {
    let manifest = vintf_validator::load_default_manifest().expect("Must load default manifest");
    assert_eq!(manifest.target_level, 7);
    assert_eq!(manifest.manifest_type, "device");

    // All target-level 7 AIDL HALs must be declared
    assert!(manifest.is_declared("android.hardware.sensors.ISensors/default"));
    assert!(manifest.is_declared("android.hardware.audio.core.IModule/default"));
    assert!(manifest.is_declared("android.hardware.audio.core.IConfig/default"));
    assert!(manifest.is_declared("android.hardware.camera.provider.ICameraProvider/virtual/0"));

    // Global helper check
    assert!(is_declared("android.hardware.sensors.ISensors/default"));
    assert!(is_declared("android.hardware.camera.provider.ICameraProvider/virtual/0"));
}

#[test]
fn test_custom_xml_parsing_with_aidl_and_hidl() {
    let custom_xml = r#"
    <!-- Test Manifest -->
    <manifest version="5.0" type="device" target-level="7">
        <hal format="aidl">
            <name>android.hardware.graphics.allocator</name>
            <version>1</version>
            <fqname>IAllocator/default</fqname>
        </hal>
        <hal format="hidl">
            <name>android.hardware.renderscript</name>
            <version>1.0</version>
            <interface>IDevice</interface>
            <instance>default</instance>
        </hal>
    </manifest>
    "#;

    let manifest = VintfManifest::from_xml_str(custom_xml).expect("Failed to parse custom manifest");
    assert_eq!(manifest.hals.len(), 2);
    assert_eq!(manifest.hals[0].format, HalFormat::Aidl);
    assert_eq!(manifest.hals[1].format, HalFormat::Hidl);

    assert!(manifest.is_declared("android.hardware.graphics.allocator.IAllocator/default"));
    assert!(manifest.is_declared("android.hardware.renderscript.IDevice/default"));
}

#[test]
fn test_target_level_validation_failure() {
    let low_level_xml = r#"
    <manifest version="5.0" type="device" target-level="5">
        <hal format="aidl">
            <name>android.hardware.sensors</name>
            <version>1</version>
            <fqname>ISensors/default</fqname>
        </hal>
    </manifest>
    "#;

    let manifest = VintfManifest::from_xml_str(low_level_xml).expect("Parsing succeeded");
    let res = manifest.validate_target_level(7);
    assert!(matches!(
        res,
        Err(VintfError::InvalidTargetLevel {
            found: 5,
            required: 7
        })
    ));
}

#[test]
fn test_invalid_hal_format_failure() {
    let invalid_format_xml = r#"
    <manifest version="5.0" type="device" target-level="7">
        <hal format="corrupted_format">
            <name>android.hardware.test</name>
            <fqname>ITest/default</fqname>
        </hal>
    </manifest>
    "#;

    let res = VintfManifest::from_xml_str(invalid_format_xml);
    assert!(matches!(res, Err(VintfError::InvalidHalFormat(_))));
}
