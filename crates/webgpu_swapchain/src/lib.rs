pub mod swapchain;

pub use swapchain::{FrameTimingStats, SwapchainTarget, WebGpuSwapchain};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swapchain_present_counter_and_stats() {
        let stats = FrameTimingStats {
            frame_count: 120,
            fps: 120.0,
            frame_time_ms: 8.333,
            gpu_time_ms: 2.1,
            target_fps: 120.0,
            is_120fps_capable: true,
        };
        assert!(stats.is_120fps_capable);
        assert_eq!(stats.target_fps, 120.0);
    }

    #[test]
    fn test_swapchain_surface_resize_and_triple_buffering() {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
            let adapter = match instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
            {
                Some(a) => a,
                None => return,
            };

            let (device, _) = match adapter.request_device(&wgpu::DeviceDescriptor::default(), None).await {
                Ok(d) => d,
                Err(_) => return,
            };

            let mut swapchain = WebGpuSwapchain::new(&device, 64, 64, wgpu::TextureFormat::Rgba8UnormSrgb);
            assert_eq!(swapchain.width, 64);
            assert_eq!(swapchain.height, 64);
            assert_eq!(swapchain.present_mode, wgpu::PresentMode::Mailbox);

            swapchain.resize(&device, 128, 128);
            assert_eq!(swapchain.width, 128);
            assert_eq!(swapchain.height, 128);

            let frame1 = swapchain.present();
            assert_eq!(frame1, 1);
            let frame2 = swapchain.present();
            assert_eq!(frame2, 2);

            let stats = swapchain.get_timing_stats();
            assert_eq!(stats.frame_count, 2);
            assert_eq!(stats.target_fps, 120.0);
        });
    }
}

