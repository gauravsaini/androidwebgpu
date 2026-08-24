//! # surfaceflinger_gpu_service
//!
//! Host-side Offloaded SurfaceFlinger Compositor Service for AndroidWebGPU.
//!
//! Implements AOSP `android.gui.ISurfaceComposer` and `android.gui.IGraphicBufferProducer`,
//! receiving layer transaction state updates across the VM boundary and rendering frames
//! via the host WebGPU pipeline and Swapchain.

pub mod buffer_queue;
pub mod layer_translator;
pub mod service;

pub use buffer_queue::{igraphicbufferproducer_codes, BufferQueueError, GraphicBufferProducerService};
pub use layer_translator::{layer_change_flags, ComposerState, LayerState, LayerTranslator};
pub use service::{isurfacecomposer_codes, CompositorServiceError, DisplayInfo, SurfaceComposerService, SurfaceHandle};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_state_marshaling_roundtrip() {
        use aidl_compat::{Parcel, Parcelable};

        let mut original = LayerState::new(42, "StatusBar");
        original.set_bounds_ndc([-0.8, -0.8, 1.6, 0.2]);
        original.set_z_order(10);
        original.set_alpha(0.85);
        original.set_color([0.2, 0.4, 0.6, 0.85]);

        let mut p = Parcel::new();
        original.write_to_parcel(&mut p).unwrap();

        let mut decoded = LayerState::default();
        let mut offset = 0;
        decoded.read_from_parcel_at(&p, &mut offset).unwrap();

        assert_eq!(decoded.surface_id, 42);
        assert_eq!(decoded.name, "StatusBar");
        assert_eq!(decoded.bounds, [-0.8, -0.8, 1.6, 0.2]);
        assert_eq!(decoded.z_order, 10);
        assert_eq!(decoded.alpha, 0.85);
        assert_eq!(decoded.color, Some([0.2, 0.4, 0.6, 0.85]));
    }

    #[test]
    fn test_screen_to_ndc_translation() {
        let pixel_bounds = [0.0, 0.0, 1280.0, 720.0];
        let ndc = LayerTranslator::screen_coords_to_ndc(pixel_bounds, 1280, 720);
        assert_eq!(ndc, [-1.0, -1.0, 2.0, 2.0]);

        // Top-left quad of 640x360
        let quad_pixels = [0.0, 0.0, 640.0, 360.0];
        let quad_ndc = LayerTranslator::screen_coords_to_ndc(quad_pixels, 1280, 720);
        assert_eq!(quad_ndc[0], -1.0);
        assert_eq!(quad_ndc[1], 0.0);
        assert_eq!(quad_ndc[2], 1.0);
        assert_eq!(quad_ndc[3], 1.0);
    }
}
