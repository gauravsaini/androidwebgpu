use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy)]
pub struct PlatformInstant(f64);

#[cfg(target_arch = "wasm32")]
impl PlatformInstant {
    pub fn now() -> Self {
        Self(js_sys::Date::now())
    }

    pub fn elapsed_ms(&self) -> f32 {
        (js_sys::Date::now() - self.0).max(0.0) as f32
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy)]
pub struct PlatformInstant(std::time::Instant);

#[cfg(not(target_arch = "wasm32"))]
impl PlatformInstant {
    pub fn now() -> Self {
        Self(std::time::Instant::now())
    }

    pub fn elapsed_ms(&self) -> f32 {
        self.0.elapsed().as_secs_f32() * 1000.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameMetrics {
    pub fps: f32,
    pub frame_time_ms: f32,
    pub draw_calls: u32,
    pub triangles: u32,
    pub texture_uploads: u32,
    pub total_gpu_mem_bytes: usize,
}

pub struct MetricsTracker {
    last_instant: PlatformInstant,
    frame_count: u32,
    accumulated_time_ms: f32,
    pub current_metrics: FrameMetrics,
    pub current_draw_calls: u32,
    pub current_triangles: u32,
    pub current_texture_uploads: u32,
    pub current_gpu_mem_bytes: usize,
}

impl MetricsTracker {
    pub fn new() -> Self {
        Self {
            last_instant: PlatformInstant::now(),
            frame_count: 0,
            accumulated_time_ms: 0.0,
            current_metrics: FrameMetrics {
                fps: 60.0,
                frame_time_ms: 16.6,
                draw_calls: 0,
                triangles: 0,
                texture_uploads: 0,
                total_gpu_mem_bytes: 0,
            },
            current_draw_calls: 0,
            current_triangles: 0,
            current_texture_uploads: 0,
            current_gpu_mem_bytes: 0,
        }
    }

    pub fn record_draw(&mut self, triangles: u32) {
        self.current_draw_calls += 1;
        self.current_triangles += triangles;
    }

    pub fn record_texture_upload(&mut self, bytes: usize) {
        self.current_texture_uploads += 1;
        self.current_gpu_mem_bytes += bytes;
    }

    pub fn end_frame(&mut self) -> FrameMetrics {
        let duration = self.last_instant.elapsed_ms();
        self.last_instant = PlatformInstant::now();

        self.accumulated_time_ms += duration;
        self.frame_count += 1;


        if self.accumulated_time_ms >= 500.0 {
            let avg_fps = (self.frame_count as f32 * 1000.0) / self.accumulated_time_ms;
            let avg_frame_time = self.accumulated_time_ms / (self.frame_count as f32);

            self.current_metrics = FrameMetrics {
                fps: avg_fps,
                frame_time_ms: avg_frame_time,
                draw_calls: self.current_draw_calls,
                triangles: self.current_triangles,
                texture_uploads: self.current_texture_uploads,
                total_gpu_mem_bytes: self.current_gpu_mem_bytes,
            };

            self.accumulated_time_ms = 0.0;
            self.frame_count = 0;
            self.current_draw_calls = 0;
            self.current_triangles = 0;
            self.current_texture_uploads = 0;
        }

        self.current_metrics.clone()
    }
}

impl Default for MetricsTracker {
    fn default() -> Self {
        Self::new()
    }
}
