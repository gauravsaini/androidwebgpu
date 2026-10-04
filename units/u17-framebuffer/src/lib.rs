//! U17 `framebuffer` — Simple-Framebuffer Device Model + WebGPU Host Display.
//!
//! Track D frozen deliverable:
//! 1. Framebuffer MMIO layout (`0x1000_0000`, 640×480, `a8b8g8r8` 32bpp, stride 2560).
//! 2. Linux Device Tree `simple-framebuffer` node generator and FDT validator.
//! 3. Bare-metal test pattern generator and AArch64 machine code drawing program.
//! 4. Browser HTML + WebGPU presentation pipeline using WGSL fullscreen quad sampling.
//!
//! # Memory Map Contract
//! | Property | Value | Notes |
//! |---|---|---|
//! | Physical Base | `0x1000_0000` | 256 MiB physical, clear of GIC/UART/virtio and RAM |
//! | Resolution | 640 × 480 | VGA standard, matching existing Path N display surface |
//! | Format | `a8b8g8r8` | 32bpp, Little-Endian: R at [0], G at [1], B at [2], A at [3] |
//! | Stride | 2560 bytes | 640 × 4; 256-byte aligned for WebGPU texture copy |
//! | Buffer Size | 1,228,800 bytes | `0x12_C000`, exactly 300 × 4 KiB pages |
//!
//! # Swarm Rules Conformance
//! - Purity: EXPLICIT-STATE. All device state lives in [`FramebufferDevice`].
//! - Zero external dependencies: 100% Rust standard library.
//! - One-line workspace registration in root `Cargo.toml`.

pub mod baremetal;
pub mod dtb;
pub mod model;
pub mod webgpu;

// Re-export primary types and constants
pub use baremetal::{
    assemble_baremetal_draw_program, disassemble_program, draw_test_pattern_to_device,
    generate_test_pattern, verify_pattern_pins, PixelCheckPin, COLOR_BLACK, COLOR_BLUE, COLOR_CYAN,
    COLOR_GREEN, COLOR_MAGENTA, COLOR_RED, COLOR_WHITE, COLOR_YELLOW, TEST_BAR_COLORS,
    VERIFICATION_PINS,
};
pub use dtb::{
    generate_dts_node, inject_simple_framebuffer_into_dtb, validate_dtb, SimpleFbFdtBuilder,
    SimpleFbProperties,
};
pub use model::{
    DirtyRect, FramebufferDevice, FB_BASE, FB_BPP, FB_FORMAT, FB_HEIGHT, FB_SIZE, FB_STRIDE,
    FB_WIDTH,
};
pub use webgpu::{encode_base64, generate_html_viewer, WebGpuConfig, WGSL_SHADERS};

/// Trait for routing framebuffer MMIO accesses in an emulator or orchestrator.
pub trait FramebufferMmioHandler {
    /// Return Some(value) if `addr` is within the framebuffer MMIO window, None otherwise.
    fn handle_fb_read(&self, addr: u64, size: usize) -> Option<u64>;

    /// Return true and process the write if `addr` is within the framebuffer MMIO window.
    fn handle_fb_write(&mut self, addr: u64, size: usize, val: u64) -> bool;
}

impl FramebufferMmioHandler for FramebufferDevice {
    #[inline]
    fn handle_fb_read(&self, addr: u64, size: usize) -> Option<u64> {
        if FramebufferDevice::contains(addr) {
            Some(self.mmio_read(addr, size))
        } else {
            None
        }
    }

    #[inline]
    fn handle_fb_write(&mut self, addr: u64, size: usize, val: u64) -> bool {
        self.mmio_write(addr, size, val)
    }
}
