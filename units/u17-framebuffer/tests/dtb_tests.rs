use std::path::Path;
use u17_framebuffer::dtb::*;
use u17_framebuffer::model::*;

#[test]
fn test_dts_node_generation() {
    let dts = generate_dts_node();
    assert!(dts.contains("compatible = \"simple-framebuffer\";"));
    assert!(dts.contains("reg = <0x0 0x10000000 0x0 0x0012c000>;"));
    assert!(dts.contains("width = <640>;"));
    assert!(dts.contains("height = <480>;"));
    assert!(dts.contains("stride = <2560>;"));
    assert!(dts.contains("format = \"a8b8g8r8\";"));
    assert!(dts.contains("status = \"okay\";"));
}

#[test]
fn test_fdt_node_builder() {
    let mut builder = SimpleFbFdtBuilder::new();
    let (struct_tokens, string_table) = builder.build_node_tokens();

    assert!(!struct_tokens.is_empty());
    assert!(!string_table.is_empty());
    assert_eq!(struct_tokens.len() % 4, 0, "FDT struct must be 4-byte aligned");

    let strings_str = String::from_utf8_lossy(string_table);
    assert!(strings_str.contains("compatible"));
    assert!(strings_str.contains("reg"));
    assert!(strings_str.contains("width"));
    assert!(strings_str.contains("height"));
    assert!(strings_str.contains("stride"));
    assert!(strings_str.contains("format"));
}

#[test]
fn test_dtb_injection_and_validation() {
    // Path to the existing minimal-virt.dtb in the repo
    let dtb_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("guest-image/minimal-virt.dtb");

    if !dtb_path.exists() {
        eprintln!("minimal-virt.dtb not found at {:?}, skipping file-based test", dtb_path);
        return;
    }

    let orig_dtb = std::fs::read(&dtb_path).expect("Failed to read minimal-virt.dtb");
    assert!(orig_dtb.len() >= 40, "Original DTB too small");

    // Before injection: should fail validation (no simple-framebuffer)
    let pre_check = validate_dtb(&orig_dtb);
    assert!(
        pre_check.is_err(),
        "Original DTB should not have simple-framebuffer yet"
    );

    // Inject simple-framebuffer node
    let patched_dtb = inject_simple_framebuffer_into_dtb(&orig_dtb)
        .expect("Failed to inject simple-framebuffer node into DTB");

    assert!(patched_dtb.len() > orig_dtb.len());

    // Validate the patched DTB
    let props = validate_dtb(&patched_dtb)
        .expect("Patched DTB must pass validation");

    assert_eq!(props.compatible, "simple-framebuffer");
    assert_eq!(props.base_addr, FB_BASE, "Base must match 0x1000_0000");
    assert_eq!(props.size, FB_SIZE, "Size must match 0x12_C000");
    assert_eq!(props.width, FB_WIDTH, "Width must match 640");
    assert_eq!(props.height, FB_HEIGHT, "Height must match 480");
    assert_eq!(props.stride, FB_STRIDE, "Stride must match 2560");
    assert_eq!(props.format, FB_FORMAT, "Format must match a8b8g8r8");
    assert_eq!(props.status, "okay");
}
