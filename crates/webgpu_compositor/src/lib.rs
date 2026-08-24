pub mod compositor;
pub mod layer;
pub mod pipeline;

pub use compositor::WebGpuCompositor;
pub use layer::{BlendMode, CompositionLayer};
pub use pipeline::{CompositorPipeline, LayerUniform, QuadVertex};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_creation() {
        let color_layer = CompositionLayer::new_color(
            1,
            "BackgroundLayer",
            [-1.0, -1.0, 2.0, 2.0],
            0,
            [0.1, 0.2, 0.3, 1.0],
        );
        assert_eq!(color_layer.id, 1);
        assert_eq!(color_layer.name, "BackgroundLayer");
        assert_eq!(color_layer.z_order, 0);
        assert!(color_layer.visible);
        assert_eq!(color_layer.blend_mode, BlendMode::Premultiplied);
    }
}
