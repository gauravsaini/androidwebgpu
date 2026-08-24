use apk_gpu_analyzer::{ApkGpuAnalyzer, EngineType};
use std::fs;
use virtio_gpu_bridge::protocol::*;
use virtio_gpu_bridge::VirtioGpuBridge;

#[test]
fn test_apk_real_unity_and_godot_virtio_flight() {
    pollster::block_on(async {
        // 1. Load Real Godot GLES2 APK Fixture
        let godot_bytes = fs::read("fixtures/godot_gles2.apk")
            .or_else(|_| fs::read("../../fixtures/godot_gles2.apk"))
            .expect("Failed to read fixtures/godot_gles2.apk");
        let godot_profile = ApkGpuAnalyzer::analyze_apk_bytes(&godot_bytes)
            .expect("Failed to analyze godot_gles2.apk");

        assert_eq!(godot_profile.engine, EngineType::Godot);
        assert_eq!(godot_profile.package_name, "org.godotengine.gles2game");
        assert_eq!(godot_profile.min_gles_version, 0x00020000);
        assert!(godot_profile.native_libraries.contains(&"libgodot_android.so".to_string()));
        assert!(godot_profile.supported_texture_formats.contains(&"ETC2".to_string()));

        // 2. Load Real Unity Cube Vulkan APK Fixture
        let unity_bytes = fs::read("fixtures/unity_cube.vulkan.apk")
            .or_else(|_| fs::read("../../fixtures/unity_cube.vulkan.apk"))
            .or_else(|_| fs::read("fixtures/unity_cube.apk"))
            .or_else(|_| fs::read("../../fixtures/unity_cube.apk"))
            .expect("Failed to read fixtures/unity_cube.vulkan.apk");
        let unity_profile = ApkGpuAnalyzer::analyze_apk_bytes(&unity_bytes)
            .expect("Failed to analyze unity_cube.vulkan.apk");

        assert_eq!(unity_profile.engine, EngineType::Unity);
        assert!(unity_profile.package_name.contains("unity"));
        assert!(unity_profile.requires_vulkan);
        assert!(unity_profile.native_libraries.contains(&"libunity.so".to_string()));
        assert!(unity_profile.native_libraries.contains(&"libmain.so".to_string()));

        // 3. Initialize 720p Virtio-GPU Bridge for Real Game Flight
        let width = 1280u32;
        let height = 720u32;
        let mut bridge = match VirtioGpuBridge::new(width, height).await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("GPU adapter unavailable in test runner, skipping hardware pass: {:?}", e);
                return;
            }
        };

        // Create 720p Scanout Resource
        let create_cmd = VirtioGpuResourceCreate2d {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
                flags: 0,
                fence_id: 10,
                ctx_id: 0,
                padding: 0,
            },
            resource_id: 1,
            format: VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM,
            width,
            height,
        };
        let resp = bridge.process_binary_wire_command(bytemuck::bytes_of(&create_cmd));
        assert_eq!(resp.len(), std::mem::size_of::<VirtioGpuCtrlHdr>());

        // Bind Resource 1 to Scanout 0
        let scanout_cmd = VirtioGpuSetScanout {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_SET_SCANOUT,
                flags: 0,
                fence_id: 11,
                ctx_id: 0,
                padding: 0,
            },
            r: VirtioGpuRect {
                x: 0,
                y: 0,
                width,
                height,
            },
            scanout_id: 0,
            resource_id: 1,
        };
        let scanout_resp = bridge.process_binary_wire_command(bytemuck::bytes_of(&scanout_cmd));
        assert_eq!(scanout_resp.len(), std::mem::size_of::<VirtioGpuCtrlHdr>());

        // 4. Submit 3D Engine Frame Commands (Clear + Viewport + Draw)
        let mut submit_payload = Vec::new();

        // 0x04: VIEWPORT (x=0, y=0, w=1280, h=720)
        submit_payload.extend_from_slice(&4u32.to_le_bytes()); // Opcode VIEWPORT
        submit_payload.extend_from_slice(&16u32.to_le_bytes()); // Payload Len
        submit_payload.extend_from_slice(&0i32.to_le_bytes());
        submit_payload.extend_from_slice(&0i32.to_le_bytes());
        submit_payload.extend_from_slice(&width.to_le_bytes());
        submit_payload.extend_from_slice(&height.to_le_bytes());

        // 0x01: CLEAR (mask=0x4000 (Color), r=0.2, g=0.4, b=0.8, a=1.0)
        submit_payload.extend_from_slice(&1u32.to_le_bytes()); // Opcode CLEAR
        submit_payload.extend_from_slice(&20u32.to_le_bytes()); // Payload Len
        submit_payload.extend_from_slice(&0x00004000u32.to_le_bytes());
        submit_payload.extend_from_slice(&0.2f32.to_le_bytes());
        submit_payload.extend_from_slice(&0.4f32.to_le_bytes());
        submit_payload.extend_from_slice(&0.8f32.to_le_bytes());
        submit_payload.extend_from_slice(&1.0f32.to_le_bytes());

        let submit_hdr = VirtioGpuSubmit3d {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_SUBMIT_3D,
                flags: VIRTIO_GPU_FLAG_FENCE,
                fence_id: 12,
                ctx_id: 1,
                padding: 0,
            },
            size: submit_payload.len() as u32,
            padding: 0,
        };

        let mut full_submit_packet = bytemuck::bytes_of(&submit_hdr).to_vec();
        full_submit_packet.extend_from_slice(&submit_payload);

        let submit_resp = bridge.process_binary_wire_command(&full_submit_packet);
        assert_eq!(submit_resp.len(), std::mem::size_of::<VirtioGpuCtrlHdr>());

        // 5. Transfer to host and flush resource
        let flush_cmd = VirtioGpuResourceFlush {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_RESOURCE_FLUSH,
                flags: 0,
                fence_id: 13,
                ctx_id: 0,
                padding: 0,
            },
            r: VirtioGpuRect {
                x: 0,
                y: 0,
                width,
                height,
            },
            resource_id: 1,
            padding: 0,
        };
        let flush_resp = bridge.process_binary_wire_command(bytemuck::bytes_of(&flush_cmd));
        assert_eq!(flush_resp.len(), std::mem::size_of::<VirtioGpuCtrlHdr>());

        // 6. Assert Output Scanout Pixels
        let scanout_fb = bridge.get_scanout_framebuffer(0).expect("Scanout 0 must have active framebuffer");
        assert_eq!(scanout_fb.len(), (width * height * 4) as usize);

        // Center pixel inspection (x=640, y=360)
        let center_idx = ((360 * width + 640) * 4) as usize;
        let r = scanout_fb[center_idx];
        let g = scanout_fb[center_idx + 1];
        let b = scanout_fb[center_idx + 2];
        let a = scanout_fb[center_idx + 3];

        assert_eq!(r, 51, "Red channel must match clear color 0.2 * 255");
        assert_eq!(g, 102, "Green channel must match clear color 0.4 * 255");
        assert_eq!(b, 204, "Blue channel must match clear color 0.8 * 255");
        assert_eq!(a, 255, "Alpha channel must be 255");
    });
}
