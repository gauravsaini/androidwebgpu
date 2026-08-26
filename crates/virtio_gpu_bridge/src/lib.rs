pub mod binary;
pub mod bridge;
pub mod command;
pub mod protocol;
pub mod wasm;

pub use binary::{BinaryWireParser, DecodedVirtioCommand};
pub use bridge::VirtioGpuBridge;
pub use command::{CommandResponse, GpuCommand};
#[cfg(feature = "wasm")]
pub use wasm::WasmVirtioGpuBridge;

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

    #[test]
    fn test_virtio_submit3d_vulkan_pipeline_and_transfer_to_host_3d() {
        pollster::block_on(async {
            let mut bridge = match VirtioGpuBridge::new(64, 64).await {
                Ok(b) => b,
                Err(_) => return,
            };

            // 1. Create 3D resource
            let create_3d_cmd = VirtioGpuResourceCreate3d {
                hdr: VirtioGpuCtrlHdr {
                    type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_3D,
                    flags: 0,
                    fence_id: 10,
                    ctx_id: 1,
                    padding: 0,
                },
                resource_id: 8,
                target: 2,
                format: VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM,
                bind: 0,
                width: 32,
                height: 32,
                depth: 1,
                array_size: 1,
                last_level: 0,
                nr_samples: 0,
                flags: 0,
                padding: 0,
            };
            let resp = bridge.process_binary_wire_command(bytemuck::bytes_of(&create_3d_cmd));
            let hdr: &VirtioGpuCtrlHdr = bytemuck::try_from_bytes(&resp).unwrap();
            assert_eq!(hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);

            // 2. TransferToHost3D
            let transfer_3d = VirtioGpuTransferToHost3d {
                hdr: VirtioGpuCtrlHdr {
                    type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D,
                    flags: 0,
                    fence_id: 11,
                    ctx_id: 1,
                    padding: 0,
                },
                box_: VirtioGpuBox {
                    x: 0,
                    y: 0,
                    z: 0,
                    w: 8,
                    h: 8,
                    d: 1,
                },
                offset: 0,
                resource_id: 8,
                level: 0,
                stride: 32,
                layer_stride: 0,
            };
            let test_pattern = vec![0x33, 0x66, 0x99, 0xFF].repeat(64);
            let mut xfer_pkt = bytemuck::bytes_of(&transfer_3d).to_vec();
            xfer_pkt.extend_from_slice(&test_pattern);
            let xfer_resp = bridge.process_binary_wire_command(&xfer_pkt);
            let xfer_hdr: &VirtioGpuCtrlHdr = bytemuck::try_from_bytes(&xfer_resp).unwrap();
            assert_eq!(xfer_hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);

            // 3. Submit3D with Vulkan Stream
            if let Some(vk) = &mut bridge.vk_device {
                let cb_id = vk.vk_create_command_buffer();
                let img_id = vk.vk_create_image(64, 64, 1, 1, 1, 0, 0x00000010);
                let view_id = vk.vk_create_image_view(img_id, 0);

                let mut stream: Vec<u8> = Vec::new();

                // Opcode 0x0021: BeginRendering
                stream.extend_from_slice(&VIRTGPU_VK_CMD_BEGIN_RENDERING.to_le_bytes());
                stream.extend_from_slice(&44u32.to_le_bytes()); // len
                stream.extend_from_slice(&cb_id.to_le_bytes());
                stream.extend_from_slice(&view_id.to_le_bytes());
                stream.extend_from_slice(&0u64.to_le_bytes()); // depth_view
                stream.extend_from_slice(&0.1f32.to_le_bytes());
                stream.extend_from_slice(&0.2f32.to_le_bytes());
                stream.extend_from_slice(&0.3f32.to_le_bytes());
                stream.extend_from_slice(&1.0f32.to_le_bytes());
                stream.extend_from_slice(&1.0f32.to_le_bytes());

                // Opcode 0x0022: EndRendering
                stream.extend_from_slice(&VIRTGPU_VK_CMD_END_RENDERING.to_le_bytes());
                stream.extend_from_slice(&8u32.to_le_bytes()); // len
                stream.extend_from_slice(&cb_id.to_le_bytes());

                // Opcode 0x0020: QueueSubmit
                stream.extend_from_slice(&VIRTGPU_VK_CMD_QUEUE_SUBMIT.to_le_bytes());
                stream.extend_from_slice(&12u32.to_le_bytes()); // len = 4 + 8
                stream.extend_from_slice(&1u32.to_le_bytes()); // count
                stream.extend_from_slice(&cb_id.to_le_bytes());

                let submit_hdr = VirtioGpuSubmit3d {
                    hdr: VirtioGpuCtrlHdr {
                        type_: VIRTIO_GPU_CMD_SUBMIT_3D,
                        flags: VIRTIO_GPU_FLAG_FENCE,
                        fence_id: 12,
                        ctx_id: 1,
                        padding: 0,
                    },
                    size: stream.len() as u32,
                    padding: 0,
                };
                let mut submit_pkt = bytemuck::bytes_of(&submit_hdr).to_vec();
                submit_pkt.extend_from_slice(&stream);

                let submit_resp = bridge.process_binary_wire_command(&submit_pkt);
                let sub_hdr: &VirtioGpuCtrlHdr = bytemuck::try_from_bytes(&submit_resp).unwrap();
                assert_eq!(sub_hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);
                assert_eq!(sub_hdr.fence_id, 12);
            }
        });
    }

    #[test]
    fn test_virtio_binder_process_packet_ping() {
        pollster::block_on(async {
            let bridge = match VirtioGpuBridge::new(64, 64).await {
                Ok(b) => b,
                Err(_) => return,
            };

            let req = virtio_binder::VirtioBinderRequest::new_ping(1001, 0);
            let req_bytes = req.serialize();

            let resp_bytes = bridge.process_binder_packet(&req_bytes);
            let resp = virtio_binder::VirtioBinderResponse::deserialize(&resp_bytes)
                .expect("Failed to deserialize response");

            assert_eq!(resp.hdr.msg_id, 1001);
            assert!(resp.hdr.is_success());
            assert_eq!(resp.hdr.status, binder_rt::status::STATUS_OK);
        });
    }

    #[test]
    fn test_virtio_binder_process_packet_surface_composer_transact() {
        pollster::block_on(async {
            let bridge = match VirtioGpuBridge::new(64, 64).await {
                Ok(b) => b,
                Err(_) => return,
            };

            if bridge.surface_composer.is_none() {
                return;
            }

            // BOOT_FINISHED transaction (code 1025) to handle 1
            let req = virtio_binder::VirtioBinderRequest::new_transact(
                1002,
                1,
                surfaceflinger_gpu_service::isurfacecomposer_codes::BOOT_FINISHED,
                0,
                0,
                Vec::new(),
                Vec::new(),
            );
            let req_bytes = req.serialize();

            let resp_bytes = bridge.process_binder_packet(&req_bytes);
            let resp = virtio_binder::VirtioBinderResponse::deserialize(&resp_bytes)
                .expect("Failed to deserialize response");

            assert_eq!(resp.hdr.msg_id, 1002);
            assert!(resp.hdr.is_success());

            // Check that boot is finished
            if let Some(sf) = &bridge.surface_composer {
                assert!(sf.is_boot_finished());
            }
        });
    }

    #[test]
    fn test_virtio_binder_process_packet_malformed() {
        pollster::block_on(async {
            let bridge = match VirtioGpuBridge::new(64, 64).await {
                Ok(b) => b,
                Err(_) => return,
            };

            let malformed_bytes = vec![0xFF; 10];
            let resp_bytes = bridge.process_binder_packet(&malformed_bytes);
            let resp = virtio_binder::VirtioBinderResponse::deserialize(&resp_bytes)
                .expect("Deserializing error response should succeed");

            assert!(!resp.hdr.is_success());
            assert_eq!(resp.hdr.status, aidl_compat::STATUS_BAD_VALUE);
        });
    }

    #[test]
    fn test_virtio_binder_process_packet_graphic_buffer_producer_transact() {
        pollster::block_on(async {
            let bridge = match VirtioGpuBridge::new(64, 64).await {
                Ok(b) => b,
                Err(_) => return,
            };

            // 1. Connect transaction to handle 10 (IGraphicBufferProducer)
            let req_conn = virtio_binder::VirtioBinderRequest::new_transact(
                1003,
                10,
                surfaceflinger_gpu_service::igraphicbufferproducer_codes::CONNECT,
                0,
                0,
                Vec::new(),
                Vec::new(),
            );
            let resp_bytes = bridge.process_binder_packet(&req_conn.serialize());
            let resp = virtio_binder::VirtioBinderResponse::deserialize(&resp_bytes)
                .expect("Failed to deserialize response");
            assert_eq!(resp.hdr.msg_id, 1003);
            assert!(resp.hdr.is_success());

            // 2. Dequeue buffer on handle 10
            let mut deq_parcel = binder_rt::Parcel::new();
            deq_parcel.write_u32(64).unwrap();
            deq_parcel.write_u32(64).unwrap();
            deq_parcel.write_u32(1).unwrap();

            let req_deq = virtio_binder::VirtioBinderRequest::new_transact(
                1004,
                10,
                surfaceflinger_gpu_service::igraphicbufferproducer_codes::DEQUEUE_BUFFER,
                0,
                0,
                deq_parcel.data().to_vec(),
                Vec::new(),
            );
            let deq_resp_bytes = bridge.process_binder_packet(&req_deq.serialize());
            let deq_resp = virtio_binder::VirtioBinderResponse::deserialize(&deq_resp_bytes)
                .expect("Failed to deserialize dequeue response");
            assert_eq!(deq_resp.hdr.msg_id, 1004);
            assert!(deq_resp.hdr.is_success());
        });
    }
}
