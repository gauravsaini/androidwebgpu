//! Challenger 2: Empirical Stress Test Suite for M3 & M4
//! Focus: SurfaceFlinger DRM Ioctls, Scanout 0 Binding, Damage Rects & Framebuffer Entropy.
//! ASD-STE100 Simplified Technical English
//! /ponytail /caveman

use std::collections::HashMap;
use virtio_gpu_bridge::protocol::*;
use virtio_gpu_bridge::VirtioGpuBridge;

/// Calculates Shannon entropy H of a RGBA byte slice
fn calculate_entropy(rgba: &[u8]) -> f64 {
    if rgba.is_empty() {
        return 0.0;
    }
    let total_pixels = rgba.len() / 4;
    let mut freq: HashMap<u32, usize> = HashMap::new();

    for chunk in rgba.chunks_exact(4) {
        let color = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        *freq.entry(color).or_insert(0) += 1;
    }

    let mut entropy = 0.0f64;
    for &count in freq.values() {
        let p = count as f64 / total_pixels as f64;
        if p > 0.0 {
            entropy -= p * p.log2();
        }
    }
    entropy
}

#[test]
fn test_surfaceflinger_drm_resource_create_and_scanout0_binding() {
    pollster::block_on(async {
        let mut bridge = match VirtioGpuBridge::new(720, 1440).await {
            Ok(b) => b,
            Err(_) => return,
        };

        // 1. Emulate DRM_IOCTL_VIRTGPU_RESOURCE_CREATE (res_id=1, 720x1440 R8G8B8A8)
        let create_cmd = VirtioGpuResourceCreate2d {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
                flags: 0,
                fence_id: 1,
                ctx_id: 0,
                padding: 0,
            },
            resource_id: 1,
            format: VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM,
            width: 720,
            height: 1440,
        };
        let resp = bridge.process_binary_wire_command(bytemuck::bytes_of(&create_cmd));
        let hdr: &VirtioGpuCtrlHdr = bytemuck::try_from_bytes(&resp).unwrap();
        assert_eq!(hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);
        assert!(bridge.resources.contains_key(&1));

        // 2. Emulate Display Modeset -> Bind Scanout 0 to Resource 1
        let scanout_cmd = VirtioGpuSetScanout {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_SET_SCANOUT,
                flags: 0,
                fence_id: 2,
                ctx_id: 0,
                padding: 0,
            },
            r: VirtioGpuRect {
                x: 0,
                y: 0,
                width: 720,
                height: 1440,
            },
            scanout_id: 0,
            resource_id: 1,
        };
        let resp = bridge.process_binary_wire_command(bytemuck::bytes_of(&scanout_cmd));
        let hdr: &VirtioGpuCtrlHdr = bytemuck::try_from_bytes(&resp).unwrap();
        assert_eq!(hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);
        assert!(bridge.scanouts.contains_key(&0));

        // 3. Emulate DRM_IOCTL_VIRTGPU_EXECBUFFER (TRANSFER_TO_HOST_2D + RESOURCE_FLUSH)
        // Generate authentic UI pattern with header, body items, and nav bar
        let mut pixels = vec![0u8; 720 * 1440 * 4];
        for y in 0..1440usize {
            for x in 0..720usize {
                let idx = (y * 720 + x) * 4;
                if y < 130 {
                    // Header Bar (Dark Slate)
                    pixels[idx] = 15;
                    pixels[idx + 1] = 23;
                    pixels[idx + 2] = 42;
                    pixels[idx + 3] = 255;
                } else if y > 1380 {
                    // Navigation Bar (Deep Charcoal)
                    pixels[idx] = 10;
                    pixels[idx + 1] = 15;
                    pixels[idx + 2] = 30;
                    pixels[idx + 3] = 255;
                } else {
                    // App List cards
                    let row = (y - 130) / 100;
                    let in_card = x > 20 && x < 700 && ((y - 130) % 100) > 10;
                    if in_card {
                        if x < 100 {
                            // Icon
                            pixels[idx] = ((row * 37 + x) % 256) as u8;
                            pixels[idx + 1] = ((row * 73 + y) % 256) as u8;
                            pixels[idx + 2] = ((row * 109 + x + y) % 256) as u8;
                            pixels[idx + 3] = 255;
                        } else {
                            // Card text
                            let is_text = x % 7 == 0 || y % 5 == 0;
                            pixels[idx] = if is_text { 240 } else { 30 };
                            pixels[idx + 1] = if is_text { 245 } else { 41 };
                            pixels[idx + 2] = if is_text { 250 } else { 59 };
                            pixels[idx + 3] = 255;
                        }
                    } else {
                        // Background
                        pixels[idx] = 18;
                        pixels[idx + 1] = 24;
                        pixels[idx + 2] = 38;
                        pixels[idx + 3] = 255;
                    }
                }
            }
        }

        // Send TRANSFER_TO_HOST_2D
        let xfer_hdr = VirtioGpuTransferToHost2d {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D,
                flags: 0,
                fence_id: 3,
                ctx_id: 0,
                padding: 0,
            },
            r: VirtioGpuRect {
                x: 0,
                y: 0,
                width: 720,
                height: 1440,
            },
            offset: 0,
            resource_id: 1,
            padding: 0,
        };
        let mut xfer_pkt = bytemuck::bytes_of(&xfer_hdr).to_vec();
        xfer_pkt.extend_from_slice(&pixels);

        let resp = bridge.process_binary_wire_command(&xfer_pkt);
        let hdr: &VirtioGpuCtrlHdr = bytemuck::try_from_bytes(&resp).unwrap();
        assert_eq!(hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);

        // Send RESOURCE_FLUSH
        let flush_cmd = VirtioGpuResourceFlush {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_RESOURCE_FLUSH,
                flags: VIRTIO_GPU_FLAG_FENCE,
                fence_id: 4,
                ctx_id: 0,
                padding: 0,
            },
            r: VirtioGpuRect {
                x: 0,
                y: 0,
                width: 720,
                height: 1440,
            },
            resource_id: 1,
            padding: 0,
        };
        let resp = bridge.process_binary_wire_command(bytemuck::bytes_of(&flush_cmd));
        let hdr: &VirtioGpuCtrlHdr = bytemuck::try_from_bytes(&resp).unwrap();
        assert_eq!(hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);
        assert_eq!(hdr.fence_id, 4);

        // 4. Retrieve Scanout 0 Framebuffer and verify Shannon Entropy H >= 1.0
        let fb = bridge.get_scanout_framebuffer(0).expect("Scanout 0 FB must exist");
        assert_eq!(fb.len(), 720 * 1440 * 4);

        let entropy = calculate_entropy(&fb);
        assert!(entropy >= 1.0, "Scanout 0 entropy must be >= 1.0 (got {})", entropy);

        // Verify Damage Rect
        let damage = bridge.get_scanout_damage(0).expect("Damage rect must exist");
        assert_eq!(damage, [0, 0, 720, 1440]);

        bridge.clear_scanout_damage(0);
        assert_eq!(bridge.get_scanout_damage(0), None);
    });
}

#[test]
fn test_multi_scanout_isolation_and_error_handling() {
    pollster::block_on(async {
        let mut bridge = match VirtioGpuBridge::new(720, 1440).await {
            Ok(b) => b,
            Err(_) => return,
        };

        // Create resource 10 for scanout 0
        let c1 = VirtioGpuResourceCreate2d {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
                flags: 0,
                fence_id: 1,
                ctx_id: 0,
                padding: 0,
            },
            resource_id: 10,
            format: VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM,
            width: 100,
            height: 100,
        };
        bridge.process_binary_wire_command(bytemuck::bytes_of(&c1));

        let s0 = VirtioGpuSetScanout {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_SET_SCANOUT,
                flags: 0,
                fence_id: 2,
                ctx_id: 0,
                padding: 0,
            },
            r: VirtioGpuRect { x: 0, y: 0, width: 100, height: 100 },
            scanout_id: 0,
            resource_id: 10,
        };
        bridge.process_binary_wire_command(bytemuck::bytes_of(&s0));

        // Create resource 20 for scanout 1
        let c2 = VirtioGpuResourceCreate2d {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
                flags: 0,
                fence_id: 3,
                ctx_id: 0,
                padding: 0,
            },
            resource_id: 20,
            format: VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM,
            width: 200,
            height: 200,
        };
        bridge.process_binary_wire_command(bytemuck::bytes_of(&c2));

        let s1 = VirtioGpuSetScanout {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_SET_SCANOUT,
                flags: 0,
                fence_id: 4,
                ctx_id: 0,
                padding: 0,
            },
            r: VirtioGpuRect { x: 0, y: 0, width: 200, height: 200 },
            scanout_id: 1,
            resource_id: 20,
        };
        bridge.process_binary_wire_command(bytemuck::bytes_of(&s1));

        // Verify scanouts 0 and 1 are isolated
        assert_eq!(bridge.scanouts.get(&0).unwrap().width, 100);
        assert_eq!(bridge.scanouts.get(&1).unwrap().width, 200);

        // Test querying non-existent scanout 99
        assert_eq!(bridge.get_scanout_framebuffer(99), None);
        assert_eq!(bridge.get_scanout_damage(99), None);
    });
}
