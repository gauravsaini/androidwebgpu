use u17_framebuffer::model::*;

#[test]
fn test_framebuffer_constants() {
    assert_eq!(FB_BASE, 0x1000_0000, "Base address must be 0x1000_0000");
    assert_eq!(FB_WIDTH, 640, "Width must be 640");
    assert_eq!(FB_HEIGHT, 480, "Height must be 480");
    assert_eq!(FB_BPP, 4, "Bytes per pixel must be 4");
    assert_eq!(FB_STRIDE, 2560, "Stride must be 2560 (640 * 4)");
    assert_eq!(FB_SIZE, 1_228_800, "Buffer size must be 1,228,800 bytes");
    assert_eq!(FB_FORMAT, "a8b8g8r8", "Format must be a8b8g8r8");

    // WebGPU texture copy constraint: bytesPerRow must be a multiple of 256
    assert_eq!(
        FB_STRIDE % 256,
        0,
        "FB_STRIDE must be 256-byte aligned for WebGPU queue.writeTexture"
    );

    // Page alignment constraint: size must be 4096-byte page aligned
    assert_eq!(
        FB_SIZE % 4096,
        0,
        "FB_SIZE must be 4096-byte page aligned (exactly 300 pages)"
    );
}

#[test]
fn test_framebuffer_address_bounds() {
    // Within range
    assert!(FramebufferDevice::contains(FB_BASE));
    assert!(FramebufferDevice::contains(FB_BASE + 100));
    assert!(FramebufferDevice::contains(FB_BASE + FB_SIZE - 1));

    // Outside range
    assert!(!FramebufferDevice::contains(FB_BASE - 1));
    assert!(!FramebufferDevice::contains(FB_BASE + FB_SIZE));
    assert!(!FramebufferDevice::contains(0x0900_0000)); // PL011 UART
    assert!(!FramebufferDevice::contains(0x0A00_0000)); // GPU command port
    assert!(!FramebufferDevice::contains(0x4000_0000)); // Guest RAM base

    // Offset calculations
    assert_eq!(FramebufferDevice::offset_of(FB_BASE), Some(0));
    assert_eq!(
        FramebufferDevice::offset_of(FB_BASE + FB_SIZE - 1),
        Some(FB_SIZE as usize - 1)
    );
    assert_eq!(FramebufferDevice::offset_of(FB_BASE - 1), None);
    assert_eq!(FramebufferDevice::offset_of(FB_BASE + FB_SIZE), None);
}

#[test]
fn test_mmio_read_write_granularity() {
    let mut dev = FramebufferDevice::new();

    // 1-byte store and load
    assert!(dev.mmio_write(FB_BASE + 0x10, 1, 0xAB));
    assert_eq!(dev.mmio_read(FB_BASE + 0x10, 1), 0xAB);

    // 2-byte store and load
    assert!(dev.mmio_write(FB_BASE + 0x20, 2, 0x1234));
    assert_eq!(dev.mmio_read(FB_BASE + 0x20, 2), 0x1234);

    // 4-byte store and load (standard 32bpp pixel)
    assert!(dev.mmio_write(FB_BASE + 0x30, 4, 0xDEADBEEF));
    assert_eq!(dev.mmio_read(FB_BASE + 0x30, 4), 0xDEADBEEF);

    // 8-byte store and load (pair of pixels)
    assert!(dev.mmio_write(FB_BASE + 0x40, 8, 0x0123456789ABCDEF));
    assert_eq!(dev.mmio_read(FB_BASE + 0x40, 8), 0x0123456789ABCDEF);

    // Out of bounds write
    assert!(!dev.mmio_write(FB_BASE + FB_SIZE, 4, 0xFFFFFFFF));
    assert_eq!(dev.mmio_read(FB_BASE + FB_SIZE, 4), 0);
}

#[test]
fn test_pixel_get_set() {
    let mut dev = FramebufferDevice::new();

    // Set pixel (10, 20) to Magenta [255, 0, 255, 255]
    dev.set_pixel(10, 20, [255, 0, 255, 255]);
    assert_eq!(dev.get_pixel(10, 20), [255, 0, 255, 255]);

    // Read back via u32 (little endian: [R, G, B, A] -> (A<<24)|(B<<16)|(G<<8)|R)
    let expected_u32 = (255u32 << 24) | (255u32 << 16) | (0u32 << 8) | 255u32;
    assert_eq!(dev.get_pixel_u32(10, 20), expected_u32);

    // Set pixel via set_pixel_u32
    let cyan_u32 = (255u32 << 24) | (255u32 << 16) | (255u32 << 8) | 0u32;
    dev.set_pixel_u32(100, 150, cyan_u32);
    assert_eq!(dev.get_pixel(100, 150), [0, 255, 255, 255]);
}

#[test]
fn test_dirty_rect_tracking() {
    let mut dev = FramebufferDevice::new();
    dev.clear_dirty();
    assert!(!dev.is_dirty());
    assert_eq!(dev.take_dirty_rect(), None);

    // Touch pixel (50, 100)
    dev.set_pixel(50, 100, [255, 255, 255, 255]);
    assert!(dev.is_dirty());

    let rect = dev.take_dirty_rect().expect("dirty rect should be present");
    assert_eq!(rect.min_x, 50);
    assert_eq!(rect.max_x, 50);
    assert_eq!(rect.min_y, 100);
    assert_eq!(rect.max_y, 100);

    // After taking dirty rect, device is clean
    assert!(!dev.is_dirty());

    // Touch two pixels to form a bounding box
    dev.set_pixel(10, 20, [1, 2, 3, 4]);
    dev.set_pixel(200, 300, [5, 6, 7, 8]);
    let rect2 = dev.take_dirty_rect().expect("dirty rect should exist");
    assert_eq!(rect2.min_x, 10);
    assert_eq!(rect2.min_y, 20);
    assert_eq!(rect2.max_x, 200);
    assert_eq!(rect2.max_y, 300);
}

#[test]
fn test_fill_rect_and_clear() {
    let mut dev = FramebufferDevice::new();

    // Fill rect 100x100 at (50, 50) with Yellow
    dev.fill_rect(50, 50, 100, 100, [255, 255, 0, 255]);
    assert_eq!(dev.get_pixel(50, 50), [255, 255, 0, 255]);
    assert_eq!(dev.get_pixel(149, 149), [255, 255, 0, 255]);
    assert_eq!(dev.get_pixel(49, 50), [0, 0, 0, 255]); // initial black
    assert_eq!(dev.get_pixel(150, 150), [0, 0, 0, 255]);

    // Clear whole screen with Blue
    dev.clear([0, 0, 255, 255]);
    assert_eq!(dev.get_pixel(0, 0), [0, 0, 255, 255]);
    assert_eq!(dev.get_pixel(639, 479), [0, 0, 255, 255]);
    assert_eq!(dev.get_pixel(50, 50), [0, 0, 255, 255]);
}
