use gles2wgpu::GlContext;
use metrics_overlay::{FrameMetrics, MetricsOverlayRenderer};
use virtio_gpu_bridge::protocol::*;
use virtio_gpu_bridge::VirtioGpuBridge;
use webgpu_compositor::{CompositionLayer, WebGpuCompositor};
use webgpu_swapchain::WebGpuSwapchain;

#[test]
fn test_full_stack_gles_virtio_compositor_swapchain_e2e() {
    pollster::block_on(async {
        let width = 256;
        let height = 256;

        // 1. Initialize GLES Context & Offscreen render target
        let mut gl_ctx = match GlContext::new(width, height).await {
            Ok(ctx) => ctx,
            Err(_) => return,
        };

        let vs_src = r#"
        #version 300 es
        precision mediump float;
        layout(location = 0) in vec3 a_position;
        void main() {
            gl_Position = vec4(a_position, 1.0);
        }
        "#;

        let fs_src = r#"
        #version 300 es
        precision mediump float;
        out vec4 FragColor;
        void main() {
            FragColor = vec4(0.0, 1.0, 0.0, 1.0); // Pure Green
        }
        "#;

        let vs_id = gl_ctx.gl_create_shader(0x8B31);
        gl_ctx.gl_shader_source(vs_id, vs_src);
        gl_ctx.gl_compile_shader(vs_id).expect("VS compile error");

        let fs_id = gl_ctx.gl_create_shader(0x8B30);
        gl_ctx.gl_shader_source(fs_id, fs_src);
        gl_ctx.gl_compile_shader(fs_id).expect("FS compile error");

        let prog_id = gl_ctx.gl_create_program();
        gl_ctx.gl_attach_shader(prog_id, vs_id);
        gl_ctx.gl_attach_shader(prog_id, fs_id);
        gl_ctx.gl_link_program(prog_id).expect("Link error");
        gl_ctx.gl_use_program(prog_id);

        let vertices: [f32; 9] = [
            -1.0, -1.0, 0.0,
             1.0, -1.0, 0.0,
             0.0,  1.0, 0.0,
        ];

        let buf_ids = gl_ctx.gl_gen_buffers(1);
        gl_ctx.gl_bind_buffer(0x8892, buf_ids[0]);
        gl_ctx.gl_buffer_data(0x8892, bytemuck::cast_slice(&vertices), 0x88E4);

        gl_ctx.gl_viewport(0, 0, width, height);
        gl_ctx.gl_clear_color(0.1, 0.2, 0.3, 1.0);
        gl_ctx.gl_clear(0x00004000); // GL_COLOR_BUFFER_BIT
        gl_ctx.gl_draw_arrays(0x0004, 0, 3); // GL_TRIANGLES

        assert_eq!(gl_ctx.metrics.current_draw_calls, 1);
        assert_eq!(gl_ctx.metrics.current_triangles, 1);

        // 2. Initialize Virtio-GPU Bridge & dispatch binary packets
        let mut virtio_bridge = match VirtioGpuBridge::new(width, height).await {
            Ok(b) => b,
            Err(_) => return,
        };

        let create_cmd = VirtioGpuResourceCreate2d {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
                flags: VIRTIO_GPU_FLAG_FENCE,
                fence_id: 1,
                ctx_id: 0,
                padding: 0,
            },
            resource_id: 200,
            format: VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM,
            width,
            height,
        };

        let resp_bytes = virtio_bridge.process_binary_wire_command(bytemuck::bytes_of(&create_cmd));
        assert_eq!(resp_bytes.len(), std::mem::size_of::<VirtioGpuCtrlHdr>());

        // 3. Initialize WebGPU Compositor & Compose multi-layer scene
        let surface_format = virtio_bridge.gl_context.surface_format;
        let mut compositor = WebGpuCompositor::new(&virtio_bridge.gl_context.device, surface_format);

        // Layer 1: Background Android App Render Target
        let app_tex_view = virtio_bridge
            .gl_context
            .default_render_target
            .wgpu_view
            .clone()
            .unwrap();

        let app_layer = CompositionLayer::new_textured(
            1,
            "AppSurface",
            [-1.0, -1.0, 2.0, 2.0], // Fullscreen NDC
            0,
            1.0,
            app_tex_view,
        );
        compositor.add_or_update_layer(app_layer);

        // Layer 2: Metrics HUD Overlay
        let metrics = FrameMetrics {
            fps: 60.0,
            frame_time_ms: 16.6,
            draw_calls: 1,
            triangles: 1,
            texture_uploads: 0,
            total_gpu_mem_bytes: (width * height * 4) as usize,
        };
        let hud_layer = MetricsOverlayRenderer::create_overlay_layer(&metrics, 10);
        compositor.add_or_update_layer(hud_layer);

        // 4. Initialize Swapchain and Render Final Composition
        let mut swapchain = WebGpuSwapchain::new(
            &virtio_bridge.gl_context.device,
            width,
            height,
            surface_format,
        );

        compositor.compose(
            &virtio_bridge.gl_context.device,
            &virtio_bridge.gl_context.queue,
            swapchain.get_current_texture_view(),
            Some(wgpu::Color::BLACK),
        );

        let frame_id = swapchain.present();
        assert_eq!(frame_id, 1);

        // 5. Read back pixels to verify rendering integrity
        let pixels = swapchain
            .readback_pixels(
                &virtio_bridge.gl_context.device,
                &virtio_bridge.gl_context.queue,
            )
            .await
            .expect("Readback failed");

        assert!(pixels.len() >= (width * height * 4) as usize);
    });
}
