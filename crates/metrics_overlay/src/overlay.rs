use crate::metrics::FrameMetrics;
use webgpu_compositor::{CompositionLayer, BlendMode};

pub struct MetricsOverlayRenderer;

impl MetricsOverlayRenderer {
    pub fn create_overlay_layer(metrics: &FrameMetrics, z_order: i32) -> CompositionLayer {
        let alpha = if metrics.fps < 30.0 { 0.9 } else { 0.7 };
        let status_color = if metrics.fps >= 55.0 {
            [0.1, 0.8, 0.2, alpha] // Green
        } else if metrics.fps >= 30.0 {
            [0.9, 0.7, 0.1, alpha] // Amber
        } else {
            [0.9, 0.2, 0.2, alpha] // Red
        };

        // Top-left HUD badge in normalized coordinates: [-1.0, 0.85, 0.4, 0.15]
        let mut layer = CompositionLayer::new_color(
            999_999,
            "MetricsHUD",
            [-0.95, 0.85, 0.45, 0.12],
            z_order,
            status_color,
        );
        layer.blend_mode = BlendMode::Premultiplied;
        layer
    }
}
