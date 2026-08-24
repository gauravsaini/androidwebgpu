pub mod binary;
pub mod bridge;
pub mod command;
pub mod protocol;
pub mod wasm;

pub use binary::{BinaryWireParser, DecodedVirtioCommand};
pub use bridge::VirtioGpuBridge;
pub use command::{CommandResponse, GpuCommand};

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::*;

    #[test]
    fn test_virtio_gpu_protocol_constants() {
        assert_eq!(VIRTIO_GPU_CMD_GET_DISPLAY_INFO, 0x0100);
        assert_eq!(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D, 0x0101);
        assert_eq!(VIRTIO_GPU_CMD_CTX_CREATE, 0x0200);
        assert_eq!(VIRTIO_GPU_RESP_OK_NODATA, 0x1100);
    }

    #[test]
    fn test_binary_decode_create_2d() {
        let cmd = VirtioGpuResourceCreate2d {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
                flags: 0,
                fence_id: 42,
                ctx_id: 0,
                padding: 0,
            },
            resource_id: 101,
            format: VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM,
            width: 800,
            height: 600,
        };

        let bytes = bytemuck::bytes_of(&cmd);
        let parsed = BinaryWireParser::parse_command(bytes).expect("Failed to parse");

        match parsed {
            DecodedVirtioCommand::ResourceCreate2d(res) => {
                assert_eq!(res.resource_id, 101);
                assert_eq!(res.format, VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM);
                assert_eq!(res.width, 800);
                assert_eq!(res.height, 600);
                assert_eq!(res.hdr.fence_id, 42);
            }
            _ => panic!("Expected ResourceCreate2d variant"),
        }
    }

    #[test]
    fn test_flush_subrect_blit() {
        pollster::block_on(async {
            let mut bridge = match VirtioGpuBridge::new(64, 64).await {
                Ok(b) => b,
                Err(_) => return,
            };

            // 1. Create 2D resource
            let create_cmd = VirtioGpuResourceCreate2d {
                hdr: VirtioGpuCtrlHdr {
                    type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
                    flags: 0,
                    fence_id: 1,
                    ctx_id: 0,
                    padding: 0,
                },
                resource_id: 5,
                format: VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM,
                width: 16,
                height: 16,
            };
            bridge.process_binary_wire_command(bytemuck::bytes_of(&create_cmd));

            // 2. Set scanout 0 for resource 5
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
                    width: 16,
                    height: 16,
                },
                scanout_id: 0,
                resource_id: 5,
            };
            bridge.process_binary_wire_command(bytemuck::bytes_of(&scanout_cmd));

            // 3. Transfer subrect (4x4 red pixel at (2, 2))
            let sub_w = 4u32;
            let sub_h = 4u32;
            let red_pixels = vec![255, 0, 0, 255].repeat((sub_w * sub_h) as usize);

            let transfer_hdr = VirtioGpuTransferToHost2d {
                hdr: VirtioGpuCtrlHdr {
                    type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D,
                    flags: 0,
                    fence_id: 3,
                    ctx_id: 0,
                    padding: 0,
                },
                r: VirtioGpuRect {
                    x: 2,
                    y: 2,
                    width: sub_w,
                    height: sub_h,
                },
                offset: 0,
                resource_id: 5,
                padding: 0,
            };

            let mut packet = bytemuck::bytes_of(&transfer_hdr).to_vec();
            packet.extend_from_slice(&red_pixels);
            bridge.process_binary_wire_command(&packet);

            // 4. Flush subrect
            let flush_cmd = VirtioGpuResourceFlush {
                hdr: VirtioGpuCtrlHdr {
                    type_: VIRTIO_GPU_CMD_RESOURCE_FLUSH,
                    flags: 0,
                    fence_id: 4,
                    ctx_id: 0,
                    padding: 0,
                },
                r: VirtioGpuRect {
                    x: 2,
                    y: 2,
                    width: sub_w,
                    height: sub_h,
                },
                resource_id: 5,
                padding: 0,
            };
            bridge.process_binary_wire_command(bytemuck::bytes_of(&flush_cmd));

            // 5. Check scanout framebuffer at (2, 2)
            let fb = bridge.get_scanout_framebuffer(0).expect("FB missing");
            let pixel_off = ((2 * 16 + 2) * 4) as usize;
            assert_eq!(fb[pixel_off], 255, "R must be 255");
            assert_eq!(fb[pixel_off + 1], 0, "G must be 0");
            assert_eq!(fb[pixel_off + 2], 0, "B must be 0");
            assert_eq!(fb[pixel_off + 3], 255, "A must be 255");
        });
    }
}
