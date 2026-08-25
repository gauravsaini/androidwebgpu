use pollster::block_on;
use webgpu_compositor::{CompositionLayer, WebGpuCompositor};
use wgpu::util::DeviceExt;

#[test]
fn test_webgpu_compositor_e2e_rendering_and_readback() {
    block_on(async {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = match instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
        {
            Some(a) => a,
            None => {
                println!("No GPU adapter available in this environment. Skipping hardware test.");
                return;
            }
        };

        let mut required_features = wgpu::Features::empty();
        if adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            required_features |= wgpu::Features::TIMESTAMP_QUERY;
        }

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("E2E Compositor Device"),
                    required_features,
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .expect("Failed to create device");

        let width = 64u32;
        let height = 64u32;
        let format = wgpu::TextureFormat::Rgba8Unorm;

        // 1. Create render target
        let target_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("E2E Compositor Target Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // 2. Create red texture for Layer 1
        let red_pixels = vec![255u8, 0, 0, 255].repeat((width * height) as usize);
        let red_texture = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("Red Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &red_pixels,
        );
        let red_view = red_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // 3. Setup Compositor & Layers
        let mut compositor = WebGpuCompositor::new(&device, format);

        // Layer 1: Red background full screen [-1, -1, 2, 2]
        let layer1 = CompositionLayer::new_textured(
            1,
            "RedBackground",
            [-1.0, -1.0, 2.0, 2.0],
            0,
            1.0,
            red_view,
        );

        // Layer 2: Green overlay on top-right quadrant [0, 0, 1, 1] in NDC
        let layer2 = CompositionLayer::new_color(
            2,
            "GreenOverlay",
            [0.0, 0.0, 1.0, 1.0],
            1,
            [0.0, 1.0, 0.0, 1.0],
        );

        compositor.add_or_update_layer(layer1);
        compositor.add_or_update_layer(layer2);

        // 4. Compose
        compositor.compose(&device, &queue, &target_view, Some(wgpu::Color::BLACK));

        // 5. Read back pixels to verify GPU composition output
        let bytes_per_row = (width * 4 + 255) & !255; // 256-byte aligned
        let buffer_size = (bytes_per_row * height) as wgpu::BufferAddress;
        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Readback Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Readback Encoder"),
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        queue.submit(Some(encoder.finish()));

        // Map buffer to host
        let buffer_slice = readback_buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |v| {
            sender.send(v).unwrap();
        });

        device.poll(wgpu::Maintain::Wait);
        receiver.recv().unwrap().expect("Failed to map readback buffer");

        {
            let data = buffer_slice.get_mapped_range();

            // Bottom-left pixel (x=16, y=48 in texture rows): Covered by Layer 1 (Red)
            let bl_offset = (48 * bytes_per_row + 16 * 4) as usize;
            let bl_r = data[bl_offset];
            let bl_g = data[bl_offset + 1];
            let bl_b = data[bl_offset + 2];
            println!("Bottom-left pixel (x=16, y=48): R={}, G={}, B={}", bl_r, bl_g, bl_b);
            assert!(bl_r > 200, "Expected red > 200, got {}", bl_r);
            assert_eq!(bl_g, 0, "Expected green == 0, got {}", bl_g);

            // Top-right pixel (x=48, y=16 in texture rows): Covered by Layer 2 (Green)
            let tr_offset = (16 * bytes_per_row + 48 * 4) as usize;
            let tr_r = data[tr_offset];
            let tr_g = data[tr_offset + 1];
            let tr_b = data[tr_offset + 2];
            println!("Top-right pixel (x=48, y=16): R={}, G={}, B={}", tr_r, tr_g, tr_b);
            assert!(tr_g > 200, "Expected green > 200, got {}", tr_g);
            assert_eq!(tr_r, 0, "Expected red == 0, got {}", tr_r);
        }

        readback_buffer.unmap();
        println!("WebGPU Compositor E2E check PASSED successfully!");
    });
}
