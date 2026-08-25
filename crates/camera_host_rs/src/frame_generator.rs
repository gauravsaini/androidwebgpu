//! Synthetic Camera Frame Generator for WebRTC/Webcam Simulation.

use camera_hal_virtual::PixelFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePattern {
    ColorBars,
    Checkerboard,
    SolidRgb(u8, u8, u8),
    Gradient,
}

pub struct CameraFrameGenerator {
    pattern: FramePattern,
    tick_counter: u64,
}

impl Default for CameraFrameGenerator {
    fn default() -> Self {
        Self::new(FramePattern::ColorBars)
    }
}

impl CameraFrameGenerator {
    pub fn new(pattern: FramePattern) -> Self {
        Self {
            pattern,
            tick_counter: 0,
        }
    }

    pub fn set_pattern(&mut self, pattern: FramePattern) {
        self.pattern = pattern;
    }

    /// Render pattern into the provided destination buffer.
    pub fn generate_frame(
        &mut self,
        width: u32,
        height: u32,
        format: PixelFormat,
        output_buffer: &mut [u8],
    ) {
        self.tick_counter = self.tick_counter.wrapping_add(1);

        match format {
            PixelFormat::Rgba8888 | PixelFormat::Rgbx8888 => {
                self.generate_rgba(width, height, output_buffer);
            }
            PixelFormat::Yuv420888 | PixelFormat::YV12 => {
                self.generate_yuv420(width, height, output_buffer);
            }
            _ => {
                // Fallback fill
                output_buffer.fill(0x80);
            }
        }
    }

    fn generate_rgba(&self, width: u32, height: u32, buf: &mut [u8]) {
        let expected = (width * height * 4) as usize;
        if buf.len() < expected {
            return;
        }

        match self.pattern {
            FramePattern::ColorBars => {
                let colors: [[u8; 4]; 8] = [
                    [255, 255, 255, 255], // White
                    [255, 255, 0, 255],   // Yellow
                    [0, 255, 255, 255],   // Cyan
                    [0, 255, 0, 255],     // Green
                    [255, 0, 255, 255],   // Magenta
                    [255, 0, 0, 255],     // Red
                    [0, 0, 255, 255],     // Blue
                    [0, 0, 0, 255],       // Black
                ];
                let bar_width = width.max(1) / 8;
                for y in 0..height {
                    for x in 0..width {
                        let bar_idx = (x / bar_width.max(1)).min(7) as usize;
                        let offset = ((y * width + x) * 4) as usize;
                        buf[offset..offset + 4].copy_from_slice(&colors[bar_idx]);
                    }
                }
            }
            FramePattern::Checkerboard => {
                let block_size = 32u32;
                let shift = (self.tick_counter % 32) as u32;
                for y in 0..height {
                    for x in 0..width {
                        let cx = (x + shift) / block_size;
                        let cy = y / block_size;
                        let is_white = (cx + cy) % 2 == 0;
                        let val = if is_white { 240 } else { 30 };
                        let offset = ((y * width + x) * 4) as usize;
                        buf[offset..offset + 4].copy_from_slice(&[val, val, val, 255]);
                    }
                }
            }
            FramePattern::SolidRgb(r, g, b) => {
                for chunk in buf[..expected].chunks_exact_mut(4) {
                    chunk.copy_from_slice(&[r, g, b, 255]);
                }
            }
            FramePattern::Gradient => {
                for y in 0..height {
                    let r = ((y * 255) / height.max(1)) as u8;
                    for x in 0..width {
                        let g = ((x * 255) / width.max(1)) as u8;
                        let offset = ((y * width + x) * 4) as usize;
                        buf[offset..offset + 4].copy_from_slice(&[r, g, 128, 255]);
                    }
                }
            }
        }
    }

    fn generate_yuv420(&self, width: u32, height: u32, buf: &mut [u8]) {
        let y_size = (width * height) as usize;
        let uv_size = (width * height / 4) as usize;
        let total_size = y_size + uv_size * 2;

        if buf.len() < total_size {
            return;
        }

        let (y_plane, uv_planes) = buf.split_at_mut(y_size);
        let (u_plane, v_plane) = uv_planes.split_at_mut(uv_size);

        match self.pattern {
            FramePattern::ColorBars => {
                // YUV equivalents of 8 standard color bars:
                // White (235, 128, 128), Yellow (210, 16, 146), Cyan (170, 166, 16),
                // Green (145, 54, 34), Magenta (106, 202, 222), Red (81, 90, 240),
                // Blue (41, 240, 110), Black (16, 128, 128)
                let yuv_bars: [(u8, u8, u8); 8] = [
                    (235, 128, 128),
                    (210, 16, 146),
                    (170, 166, 16),
                    (145, 54, 34),
                    (106, 202, 222),
                    (81, 90, 240),
                    (41, 240, 110),
                    (16, 128, 128),
                ];
                let bar_width = width.max(1) / 8;
                for y in 0..height {
                    for x in 0..width {
                        let bar_idx = (x / bar_width.max(1)).min(7) as usize;
                        let (y_val, _, _) = yuv_bars[bar_idx];
                        y_plane[(y * width + x) as usize] = y_val;
                    }
                }
                let half_w = width / 2;
                let half_h = height / 2;
                let uv_bar_width = half_w.max(1) / 8;
                for y in 0..half_h {
                    for x in 0..half_w {
                        let bar_idx = (x / uv_bar_width.max(1)).min(7) as usize;
                        let (_, u_val, v_val) = yuv_bars[bar_idx];
                        let idx = (y * half_w + x) as usize;
                        u_plane[idx] = u_val;
                        v_plane[idx] = v_val;
                    }
                }
            }
            FramePattern::SolidRgb(r, g, b) => {
                let y_val = ((66 * r as i32 + 129 * g as i32 + 25 * b as i32 + 128) >> 8) + 16;
                let u_val = ((-38 * r as i32 - 74 * g as i32 + 112 * b as i32 + 128) >> 8) + 128;
                let v_val = ((112 * r as i32 - 94 * g as i32 - 18 * b as i32 + 128) >> 8) + 128;
                y_plane.fill(y_val.clamp(16, 235) as u8);
                u_plane.fill(u_val.clamp(16, 240) as u8);
                v_plane.fill(v_val.clamp(16, 240) as u8);
            }
            _ => {
                y_plane.fill(128);
                u_plane.fill(128);
                v_plane.fill(128);
            }
        }
    }
}
