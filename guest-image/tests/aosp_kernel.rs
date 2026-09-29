//! Integration test verifying guest-image against the real prebuilt AOSP ARM64 kernel.

use guest_image::{
    build_kernel_image, initial_kernel_cpu_state, parse_kernel_header, Arm64Header,
    KernelManifest, ARM64_HEADER_LEN, ARM64_KERNEL_MAGIC,
};
use std::fs;
use std::path::Path;

const KERNEL_PATH: &str = "/mnt/sdb1/aosp/Image";

#[test]
fn test_aosp_kernel_header_and_packaging() {
    let path = Path::new(KERNEL_PATH);
    if !path.exists() {
        eprintln!("AOSP kernel not found at {KERNEL_PATH}, skipping real-image test");
        return;
    }

    let file_bytes = fs::read(path).expect("failed to read AOSP kernel image");
    assert!(file_bytes.len() >= ARM64_HEADER_LEN);

    let header: Arm64Header =
        parse_kernel_header(&file_bytes[..ARM64_HEADER_LEN]).expect("AOSP kernel header must be valid");

    assert_eq!(
        u32::from_le_bytes(file_bytes[0x38..0x3c].try_into().unwrap()),
        ARM64_KERNEL_MAGIC,
        "magic must be ARM\\x64"
    );
    assert_eq!(header.text_offset, 0x80000, "text_offset must be 512 KiB");
    assert_eq!(
        header.image_size, 0x0166d000,
        "image_size must match AOSP build 6640132 header"
    );
    assert_eq!(header.flags & 1, 0, "must be little-endian");

    // Package a slice of the real kernel with manifest
    let manifest = KernelManifest {
        name: "aosp-kernel-4.19".to_string(),
        version: 1,
        load_addr: 0x4008_0000,
        ramdisk_offset: Some(0x0200_0000),
        dtb_offset: Some(0x0100_0000),
    };

    let sample_kernel_slice = &file_bytes[..0x1000];
    let (image, sbom_json) =
        build_kernel_image(&manifest, sample_kernel_slice, Some(b"mock_ramdisk"), Some(b"mock_dtb"));

    assert!(!image.is_empty());
    assert!(sbom_json.contains("pathn-sbom"));
    assert!(sbom_json.contains("kernel-guest-image"));

    let cpu = initial_kernel_cpu_state(&manifest);
    assert_eq!(cpu.pc, 0x4008_0000);
    assert_eq!(cpu.regs[0], 0x4100_0000); // DTB at x0
    assert_eq!(cpu.regs[1], 0);
}
