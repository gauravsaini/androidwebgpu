//! Framebuffer MMIO Device Model.
//!
//! Provides an explicit-state, deterministic device model for a linear memory-mapped
//! framebuffer. The device conforms to the Linux `simple-framebuffer` specification.
//!
//! # Frozen MMIO Contract
//! - Base Address: `0x1000_0000` (256 MiB physical)
//! - Dimensions: 640 × 480
//! - Pixel Format: `a8b8g8r8` (32 bpp, 8-bit R at byte 0, G at byte 1, B at byte 2, A at byte 3)
//! - Stride (Pitch): 2560 bytes (640 × 4; 256-byte aligned for WebGPU `COPY_BYTES_PER_ROW_ALIGNMENT`)
//! - Total Size: 1,228,800 bytes (`0x12_C000`, exactly 300 × 4096-byte pages)

/// Base physical address of the framebuffer in guest MMIO space.
pub const FB_BASE: u64 = 0x1000_0000;

/// Display width in pixels.
pub const FB_WIDTH: u32 = 640;

/// Display height in pixels.
pub const FB_HEIGHT: u32 = 480;

/// Bytes per pixel (32-bit depth / 4 bytes per pixel).
pub const FB_BPP: u32 = 4;

/// Framebuffer pitch / stride in bytes per row (640 * 4 = 2560).
/// Note: 2560 is a multiple of 256, satisfying WebGPU's texture copy alignment constraint.
pub const FB_STRIDE: u32 = FB_WIDTH * FB_BPP;

/// Total memory size of the framebuffer in bytes (1,228,800 bytes / 0x12_C000).
pub const FB_SIZE: u64 = (FB_STRIDE as u64) * (FB_HEIGHT as u64);

/// Linux simple-framebuffer format identifier string.
/// In Linux `simplefb.c`: `a8b8g8r8` means {r: {0,8}, g: {8,8}, b: {16,8}, a: {24,8}}.
/// In little-endian byte ordering: Byte 0 = R, Byte 1 = G, Byte 2 = B, Byte 3 = A.
/// This matches WebGPU `rgba8unorm` format directly with zero pixel swizzling overhead.
pub const FB_FORMAT: &str = "a8b8g8r8";

/// Dirty bounding rectangle in pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    pub min_x: u32,
    pub min_y: u32,
    pub max_x: u32,
    pub max_y: u32,
}

impl DirtyRect {
    /// Construct a new dirty rectangle bounding a single pixel.
    pub fn point(x: u32, y: u32) -> Self {
        Self {
            min_x: x.min(FB_WIDTH - 1),
            min_y: y.min(FB_HEIGHT - 1),
            max_x: x.min(FB_WIDTH - 1),
            max_y: y.min(FB_HEIGHT - 1),
        }
    }

    /// Expand the dirty rectangle to include another pixel.
    pub fn include(&mut self, x: u32, y: u32) {
        let x = x.min(FB_WIDTH - 1);
        let y = y.min(FB_HEIGHT - 1);
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    /// Full screen dirty rect.
    pub fn full_screen() -> Self {
        Self {
            min_x: 0,
            min_y: 0,
            max_x: FB_WIDTH - 1,
            max_y: FB_HEIGHT - 1,
        }
    }
}

/// Explicit-state framebuffer device model.
///
/// Contains pure state only: backing pixel memory, dirty status, and bounding tracking.
/// Performs no side-effects, spawns no threads, and holds no hidden state.
#[derive(Debug, Clone)]
pub struct FramebufferDevice {
    /// Linear backing memory buffer for the framebuffer pixels.
    buffer: Vec<u8>,
    /// Whether any write has occurred since the last clear_dirty.
    dirty: bool,
    /// Bounding rectangle of modified pixels for optimal partial texture uploads.
    dirty_rect: Option<DirtyRect>,
}

impl Default for FramebufferDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl FramebufferDevice {
    /// Allocate a new framebuffer device initialized to all-black (RGBA: 0, 0, 0, 255).
    pub fn new() -> Self {
        let mut dev = Self {
            buffer: vec![0u8; FB_SIZE as usize],
            dirty: true,
            dirty_rect: Some(DirtyRect::full_screen()),
        };
        // Initialize alpha channel to fully opaque (0xFF)
        for y in 0..FB_HEIGHT {
            for x in 0..FB_WIDTH {
                let off = ((y * FB_STRIDE + x * FB_BPP) + 3) as usize;
                dev.buffer[off] = 0xFF;
            }
        }
        dev
    }

    /// Check if a guest physical address falls within the framebuffer MMIO window.
    #[inline]
    pub fn contains(addr: u64) -> bool {
        (FB_BASE..FB_BASE + FB_SIZE).contains(&addr)
    }

    /// Calculate internal buffer byte offset from guest physical address.
    #[inline]
    pub fn offset_of(addr: u64) -> Option<usize> {
        if Self::contains(addr) {
            Some((addr - FB_BASE) as usize)
        } else {
            None
        }
    }

    /// Read an integer (1, 2, 4, or 8 bytes) from a guest physical address.
    /// Returns 0 for out-of-bounds reads.
    pub fn mmio_read(&self, addr: u64, size: usize) -> u64 {
        let Some(offset) = Self::offset_of(addr) else {
            return 0;
        };
        if offset + size > self.buffer.len() {
            return 0;
        }

        let mut val = 0u64;
        for i in 0..size {
            val |= (self.buffer[offset + i] as u64) << (8 * i);
        }
        val
    }

    /// Write an integer (1, 2, 4, or 8 bytes) to a guest physical address.
    /// Updates pixel memory and marks dirty rect.
    pub fn mmio_write(&mut self, addr: u64, size: usize, val: u64) -> bool {
        let Some(offset) = Self::offset_of(addr) else {
            return false;
        };
        if offset + size > self.buffer.len() {
            return false;
        }

        for i in 0..size {
            self.buffer[offset + i] = ((val >> (8 * i)) & 0xFF) as u8;
        }

        self.dirty = true;
        self.mark_dirty_range(offset, offset + size);
        true
    }

    /// Read raw bytes into a slice starting at offset within the framebuffer.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> usize {
        if offset >= self.buffer.len() {
            return 0;
        }
        let count = out.len().min(self.buffer.len() - offset);
        out[..count].copy_from_slice(&self.buffer[offset..offset + count]);
        count
    }

    /// Write raw bytes from a slice into the framebuffer at specified offset.
    pub fn write_bytes(&mut self, offset: usize, data: &[u8]) -> usize {
        if offset >= self.buffer.len() {
            return 0;
        }
        let count = data.len().min(self.buffer.len() - offset);
        self.buffer[offset..offset + count].copy_from_slice(&data[..count]);
        self.dirty = true;
        self.mark_dirty_range(offset, offset + count);
        count
    }

    /// Get pixel RGBA components at `(x, y)`.
    /// Returns `[R, G, B, A]`.
    #[inline]
    pub fn get_pixel(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= FB_WIDTH || y >= FB_HEIGHT {
            return [0, 0, 0, 0];
        }
        let off = (y * FB_STRIDE + x * FB_BPP) as usize;
        [
            self.buffer[off],
            self.buffer[off + 1],
            self.buffer[off + 2],
            self.buffer[off + 3],
        ]
    }

    /// Get pixel as little-endian 32-bit word (`0xAABBGGRR`).
    #[inline]
    pub fn get_pixel_u32(&self, x: u32, y: u32) -> u32 {
        let p = self.get_pixel(x, y);
        u32::from_le_bytes(p)
    }

    /// Set pixel RGBA components at `(x, y)`.
    #[inline]
    pub fn set_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        if x >= FB_WIDTH || y >= FB_HEIGHT {
            return;
        }
        let off = (y * FB_STRIDE + x * FB_BPP) as usize;
        self.buffer[off..off + 4].copy_from_slice(&rgba);
        self.dirty = true;
        if let Some(ref mut rect) = self.dirty_rect {
            rect.include(x, y);
        } else {
            self.dirty_rect = Some(DirtyRect::point(x, y));
        }
    }

    /// Set pixel as little-endian 32-bit word (`0xAABBGGRR`).
    #[inline]
    pub fn set_pixel_u32(&mut self, x: u32, y: u32, val: u32) {
        self.set_pixel(x, y, val.to_le_bytes());
    }

    /// Clear entire framebuffer with specified RGBA color.
    pub fn clear(&mut self, rgba: [u8; 4]) {
        for chunk in self.buffer.chunks_exact_mut(4) {
            chunk.copy_from_slice(&rgba);
        }
        self.dirty = true;
        self.dirty_rect = Some(DirtyRect::full_screen());
    }

    /// Fill a rectangle with specified RGBA color.
    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, rgba: [u8; 4]) {
        let x_end = (x + w).min(FB_WIDTH);
        let y_end = (y + h).min(FB_HEIGHT);
        for row in y..y_end {
            for col in x..x_end {
                let off = (row * FB_STRIDE + col * FB_BPP) as usize;
                self.buffer[off..off + 4].copy_from_slice(&rgba);
            }
        }
        self.dirty = true;
        let rect = DirtyRect {
            min_x: x.min(FB_WIDTH - 1),
            min_y: y.min(FB_HEIGHT - 1),
            max_x: (x_end - 1).min(FB_WIDTH - 1),
            max_y: (y_end - 1).min(FB_HEIGHT - 1),
        };
        if let Some(ref mut existing) = self.dirty_rect {
            existing.min_x = existing.min_x.min(rect.min_x);
            existing.min_y = existing.min_y.min(rect.min_y);
            existing.max_x = existing.max_x.max(rect.max_x);
            existing.max_y = existing.max_y.max(rect.max_y);
        } else {
            self.dirty_rect = Some(rect);
        }
    }

    /// Read-only access to the linear pixel buffer (for WebGPU texture uploads).
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        &self.buffer
    }

    /// Read-only access to the linear pixel buffer as Vec reference.
    #[inline]
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    /// True if any modification occurred since the last call to `clear_dirty`.
    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clear dirty flag and reset dirty rect tracker.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
        self.dirty_rect = None;
    }

    /// Take the current dirty rect and clear it.
    pub fn take_dirty_rect(&mut self) -> Option<DirtyRect> {
        self.dirty = false;
        self.dirty_rect.take()
    }

    /// Mark internal dirty tracking for a modified byte range.
    fn mark_dirty_range(&mut self, start: usize, end: usize) {
        let start_y = (start as u32 / FB_STRIDE).min(FB_HEIGHT - 1);
        let end_y = ((end.saturating_sub(1)) as u32 / FB_STRIDE).min(FB_HEIGHT - 1);
        let start_x = ((start as u32 % FB_STRIDE) / FB_BPP).min(FB_WIDTH - 1);
        let end_x = (((end.saturating_sub(1)) as u32 % FB_STRIDE) / FB_BPP).min(FB_WIDTH - 1);

        let min_x = if start_y == end_y {
            start_x.min(end_x)
        } else {
            0
        };
        let max_x = if start_y == end_y {
            start_x.max(end_x)
        } else {
            FB_WIDTH - 1
        };

        if let Some(ref mut rect) = self.dirty_rect {
            rect.min_y = rect.min_y.min(start_y);
            rect.max_y = rect.max_y.max(end_y);
            rect.min_x = rect.min_x.min(min_x);
            rect.max_x = rect.max_x.max(max_x);
        } else {
            self.dirty_rect = Some(DirtyRect {
                min_x,
                min_y: start_y,
                max_x,
                max_y: end_y,
            });
        }
    }
}
