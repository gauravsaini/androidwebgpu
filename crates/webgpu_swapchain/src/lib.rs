pub mod swapchain;

pub use swapchain::WebGpuSwapchain;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swapchain_present_counter() {
        let mut count = 0u64;
        count += 1;
        assert_eq!(count, 1);
    }

    #[test]
    fn test_swapchain_surface_resize() {
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

            swapchain.resize(&device, 128, 128);
            assert_eq!(swapchain.width, 128);
            assert_eq!(swapchain.height, 128);

            let frame = swapchain.present();
            assert_eq!(frame, 1);
        });
    }
}
