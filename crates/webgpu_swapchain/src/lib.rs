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

            let mut required_features = wgpu::Features::empty();
            if adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
                required_features |= wgpu::Features::TIMESTAMP_QUERY;
            }

            let (device, queue) = match adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("Swapchain Test Device"),
                        required_features,
                        required_limits: adapter.limits(),
                        memory_hints: wgpu::MemoryHints::default(),
                    },
                    None,
                )
                .await
            {
                Ok(dq) => dq,
                Err(_) => return,
            };

            let mut swapchain = WebGpuSwapchain::new(&device, 64, 64, wgpu::TextureFormat::Rgba8Unorm);
            assert_eq!(swapchain.width, 64);
            assert_eq!(swapchain.height, 64);
            assert_eq!(swapchain.present_mode, wgpu::PresentMode::Mailbox);

            // Verify view 0 is active
            let view0_ptr = swapchain.get_current_texture_view() as *const _;

            swapchain.resize(&device, 128, 128);
            assert_eq!(swapchain.width, 128);
            assert_eq!(swapchain.height, 128);

            let frame1 = swapchain.present();
            assert_eq!(frame1, 1);
            let view1_ptr = swapchain.get_current_texture_view() as *const _;
            assert_ne!(view0_ptr, view1_ptr, "Current view must change on triple-buffer present");

            let frame2 = swapchain.present();
            assert_eq!(frame2, 2);

            let stats = swapchain.get_timing_stats();
            assert_eq!(stats.frame_count, 2);
            assert_eq!(stats.target_fps, 120.0);

            // Readback from active buffer
            let pixels = swapchain.readback_pixels(&device, &queue).await.unwrap();
            assert_eq!(pixels.len(), 128 * 128 * 4);
        });
    }

    #[test]
    fn test_gpu_timestamp_query_resolves() {
        pollster::block_on(async {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
            let adapter = match instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
            {
                Some(a) => a,
                None => return,
            };

            if !adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
                // If hardware does not expose TIMESTAMP_QUERY, skip
                return;
            }

            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("Timestamp Test Device"),
                        required_features: wgpu::Features::TIMESTAMP_QUERY,
                        required_limits: adapter.limits(),
                        memory_hints: wgpu::MemoryHints::default(),
                    },
                    None,
                )
                .await
                .expect("Failed to create device with TIMESTAMP_QUERY");

            let mut swapchain = WebGpuSwapchain::new(&device, 64, 64, wgpu::TextureFormat::Rgba8Unorm);
            assert!(swapchain.query_set.is_some(), "Query set must be created when feature enabled");

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Timestamp Test Encoder"),
            });

            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Timestamp Test Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: swapchain.get_current_texture_view(),
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: swapchain.query_set.as_ref().map(|qs| wgpu::RenderPassTimestampWrites {
                        query_set: qs,
                        beginning_of_pass_write_index: Some(0),
                        end_of_pass_write_index: Some(1),
                    }),
                    occlusion_query_set: None,
                });
            }

            swapchain.resolve_gpu_timestamp_query(&mut encoder);
            queue.submit(Some(encoder.finish()));

            swapchain.poll_gpu_timestamps(&device, &queue);
            let stats = swapchain.get_timing_stats();
            assert!(stats.gpu_time_ms < 10.0, "GPU execution duration must be <10.0ms, got: {}", stats.gpu_time_ms);
        });
    }
}

