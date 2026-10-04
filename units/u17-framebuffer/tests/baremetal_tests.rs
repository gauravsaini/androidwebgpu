use u17_framebuffer::baremetal::*;
use u17_framebuffer::model::*;

#[test]
fn test_pattern_generation_and_pins() {
    let mut dev = FramebufferDevice::new();
    draw_test_pattern_to_device(&mut dev);

    // Verify all 12 pins
    let pin_results = verify_pattern_pins(&dev);
    assert_eq!(pin_results.len(), 12);

    for (name, pass, exp, actual) in pin_results {
        assert!(
            pass,
            "Pin '{}' failed: expected {:?}, got {:?}",
            name, exp, actual
        );
    }
}

#[test]
fn test_pattern_dimensions_and_alpha() {
    let pattern = generate_test_pattern();
    assert_eq!(pattern.len(), FB_SIZE as usize);

    // Verify alpha channel is 255 for all pixels
    for y in 0..FB_HEIGHT {
        for x in 0..FB_WIDTH {
            let alpha_off = (y * FB_STRIDE + x * FB_BPP + 3) as usize;
            assert_eq!(
                pattern[alpha_off], 255,
                "Pixel ({}, {}) alpha channel must be 255 (opaque)",
                x, y
            );
        }
    }
}

#[test]
fn test_baremetal_asm_assembly_and_disassembly() {
    let words = assemble_baremetal_draw_program();
    assert!(!words.is_empty(), "Assembly must produce machine instructions");
    assert!(words.len() >= 10, "Program must contain loop and control flow");

    let disasm = disassemble_program(&words);
    assert_eq!(disasm.len(), words.len());

    let disasm_joined = disasm.join("\n");
    assert!(disasm_joined.contains("movz x0, #0x1000, lsl #16")); // Base pointer setup
    assert!(disasm_joined.contains("wfi")); // Park at end
    assert!(disasm_joined.contains("str w3, [x0], #4")); // Post-index store word
}

#[test]
fn test_pattern_determinism_checksum() {
    let p1 = generate_test_pattern();
    let p2 = generate_test_pattern();
    assert_eq!(p1, p2, "Test pattern generation must be 100% deterministic");

    // Simple 64-bit FNV-1a checksum of the 1.2MB frame
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in &p1 {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    assert_ne!(hash, 0, "Checksum must be non-zero");

    println!("Deterministic Pattern FNV-1a Hash: 0x{:016x}", hash);
}

#[test]
fn test_export_webgpu_html_viewer() {
    let pattern = generate_test_pattern();
    let b64 = u17_framebuffer::encode_base64(&pattern);
    let html = u17_framebuffer::generate_html_viewer(Some(&b64));

    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("Track D: Framebuffer WebGPU Display"));
    assert!(html.contains("rgba8unorm"));

    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let u17_www = manifest_dir.join("www");
    std::fs::create_dir_all(&u17_www).expect("Failed to create u17 www directory");
    let u17_html_path = u17_www.join("framebuffer.html");
    std::fs::write(&u17_html_path, &html).expect("Failed to write u17 framebuffer.html");
    assert!(u17_html_path.exists());

    // Also export to root www/ directory for easy browser testing
    let root_www = manifest_dir.parent().unwrap().parent().unwrap().join("www");
    if root_www.exists() {
        let root_html_path = root_www.join("framebuffer.html");
        std::fs::write(&root_html_path, &html).expect("Failed to write root www/framebuffer.html");
        assert!(root_html_path.exists());
    }
}
