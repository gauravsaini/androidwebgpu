pub mod metrics;
pub mod overlay;

pub use metrics::*;
pub use overlay::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_tracker_draw_and_upload() {
        let mut tracker = MetricsTracker::new();
        tracker.record_draw(120);
        tracker.record_texture_upload(1024 * 1024);
        assert_eq!(tracker.current_draw_calls, 1);
        assert_eq!(tracker.current_triangles, 120);
        assert_eq!(tracker.current_texture_uploads, 1);
        assert_eq!(tracker.current_gpu_mem_bytes, 1024 * 1024);
    }

    #[test]
    fn test_overlay_layer_generation() {
        let metrics = FrameMetrics {
            fps: 60.0,
            frame_time_ms: 16.6,
            draw_calls: 10,
            triangles: 500,
            texture_uploads: 2,
            total_gpu_mem_bytes: 4096,
        };
        let layer = MetricsOverlayRenderer::create_overlay_layer(&metrics, 100);
        assert_eq!(layer.name, "MetricsHUD");
        assert_eq!(layer.z_order, 100);
        assert!(layer.visible);
    }
}
