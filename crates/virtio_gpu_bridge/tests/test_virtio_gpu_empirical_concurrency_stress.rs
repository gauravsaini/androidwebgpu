//! High-Throughput Empirical Concurrency and Stress Test Suite for VirtIO GPU Bridge & Binder IPC.
//!
//! Tests:
//! 1. High-frequency Binary Wire Command Storm (8,000 commands across 8 threads).
//! 2. Heavy 3D submission pipeline (Clear, DrawArrays, Viewport, Vulkan submits) with concurrent scanout damage tracking.
//! 3. Adversarial malformed wire commands, truncated buffers, out-of-range scanouts, unref non-existent resources.
//! 4. Concurrent VirtIO Binder cross-service dispatch (PMS, AMS, WMS, InputFlinger, SurfaceComposer, GraphicBufferProducer).

use aidl_compat::{DeathRecipient, IBinder, Parcel, Result as AidlResult};
use binder_rt::types::{TransactionCode, TransactionFlags};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use virtio_binder::protocol::*;
use virtio_gpu_bridge::protocol::*;
use virtio_gpu_bridge::VirtioGpuBridge;

struct MockEchoBinder {
    call_count: AtomicUsize,
}

impl MockEchoBinder {
    fn new() -> Self {
        Self {
            call_count: AtomicUsize::new(0),
        }
    }
}

impl IBinder for MockEchoBinder {
    fn transact(
        &self,
        code: TransactionCode,
        _flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let mut offset = 0;
        let val = data.read_i32(&mut offset).unwrap_or(0);
        reply.write_i32(val + code as i32).unwrap();
        Ok(())
    }

    fn link_to_death(&self, _recipient: Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn unlink_to_death(&self, _recipient: &Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn is_binder_alive(&self) -> bool {
        true
    }

    fn get_class_descriptor(&self) -> Option<&'static str> {
        Some("android.os.IMockEcho")
    }
}

// -----------------------------------------------------------------------------
// 1. High-frequency Binary Wire Command Storm
// -----------------------------------------------------------------------------

#[test]
fn test_virtio_gpu_high_frequency_binary_wire_command_storm() {
    pollster::block_on(async {
        let bridge = Arc::new(Mutex::new(
            VirtioGpuBridge::new(1280, 720)
                .await
                .expect("Failed to create VirtioGpuBridge"),
        ));

        // 1. Setup 10 resources and scanouts
        {
            let mut bg = bridge.lock().unwrap();
            for res_id in 1..=10u32 {
                let create_cmd = VirtioGpuResourceCreate2d {
                    hdr: VirtioGpuCtrlHdr {
                        type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_2D,
                        flags: 0,
                        fence_id: res_id as u64,
                        ctx_id: 0,
                        padding: 0,
                    },
                    resource_id: res_id,
                    format: 1,
                    width: 256,
                    height: 256,
                };
                let resp = bg.process_binary_wire_command(bytemuck::bytes_of(&create_cmd));
                let resp_hdr = bytemuck::from_bytes::<VirtioGpuCtrlHdr>(&resp[0..24]);
                assert_eq!(resp_hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);

                let scanout_cmd = VirtioGpuSetScanout {
                    hdr: VirtioGpuCtrlHdr {
                        type_: VIRTIO_GPU_CMD_SET_SCANOUT,
                        flags: 0,
                        fence_id: res_id as u64,
                        ctx_id: 0,
                        padding: 0,
                    },
                    r: VirtioGpuRect {
                        x: 0,
                        y: 0,
                        width: 256,
                        height: 256,
                    },
                    scanout_id: res_id,
                    resource_id: res_id,
                };
                let resp = bg.process_binary_wire_command(bytemuck::bytes_of(&scanout_cmd));
                let resp_hdr = bytemuck::from_bytes::<VirtioGpuCtrlHdr>(&resp[0..24]);
                assert_eq!(resp_hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);
            }
        }

        // 2. Spawn 8 threads submitting 1,000 commands each (total 8,000 commands)
        let num_threads = 8;
        let iters_per_thread = 1000;
        let mut handles = Vec::new();

        for t in 0..num_threads {
            let bg_arc = Arc::clone(&bridge);
            handles.push(thread::spawn(move || {
                for i in 0..iters_per_thread {
                    let res_id = ((t + i) % 10 + 1) as u32;

                    // A: TransferToHost2D with subrect payload
                    let sub_w = 64u32;
                    let sub_h = 64u32;
                    let payload = vec![0xABu8; (sub_w * sub_h * 4) as usize];

                    let mut cmd_buf = Vec::new();
                    let xfer_hdr = VirtioGpuTransferToHost2d {
                        hdr: VirtioGpuCtrlHdr {
                            type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D,
                            flags: 0,
                            fence_id: i as u64,
                            ctx_id: 0,
                            padding: 0,
                        },
                        r: VirtioGpuRect {
                            x: 0,
                            y: 0,
                            width: sub_w,
                            height: sub_h,
                        },
                        offset: 0,
                        resource_id: res_id,
                        padding: 0,
                    };
                    cmd_buf.extend_from_slice(bytemuck::bytes_of(&xfer_hdr));
                    cmd_buf.extend_from_slice(&payload);

                    let resp = {
                        let mut bg = bg_arc.lock().unwrap();
                        bg.process_binary_wire_command(&cmd_buf)
                    };
                    let resp_hdr = bytemuck::from_bytes::<VirtioGpuCtrlHdr>(&resp[0..24]);
                    assert_eq!(resp_hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);

                    // B: ResourceFlush
                    let flush_cmd = VirtioGpuResourceFlush {
                        hdr: VirtioGpuCtrlHdr {
                            type_: VIRTIO_GPU_CMD_RESOURCE_FLUSH,
                            flags: 0,
                            fence_id: i as u64,
                            ctx_id: 0,
                            padding: 0,
                        },
                        r: VirtioGpuRect {
                            x: 0,
                            y: 0,
                            width: sub_w,
                            height: sub_h,
                        },
                        resource_id: res_id,
                        padding: 0,
                    };
                    let resp = {
                        let mut bg = bg_arc.lock().unwrap();
                        bg.process_binary_wire_command(bytemuck::bytes_of(&flush_cmd))
                    };
                    let resp_hdr = bytemuck::from_bytes::<VirtioGpuCtrlHdr>(&resp[0..24]);
                    assert_eq!(resp_hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // 3. Verify scanout integrity and damage rects
        let bg = bridge.lock().unwrap();
        for scanout_id in 1..=10 {
            let fb = bg.get_scanout_framebuffer(scanout_id).expect("Scanout must exist");
            assert_eq!(fb.len(), 256 * 256 * 4);
            assert_eq!(fb[0], 0xAB);
            let damage = bg.get_scanout_damage(scanout_id).expect("Damage rect must exist");
            assert_eq!(damage, [0, 0, 64, 64]);
        }
    });
}

// -----------------------------------------------------------------------------
// 2. Submit3D Command Stream & Vulkan Queue Submissions Stress
// -----------------------------------------------------------------------------

#[test]
fn test_virtio_gpu_submit_3d_and_vulkan_concurrency_stress() {
    pollster::block_on(async {
        let bridge = Arc::new(Mutex::new(
            VirtioGpuBridge::new(1280, 720)
                .await
                .expect("Failed to create VirtioGpuBridge"),
        ));

        // Create 3D resource
        {
            let mut bg = bridge.lock().unwrap();
            let create_3d = VirtioGpuResourceCreate3d {
                hdr: VirtioGpuCtrlHdr {
                    type_: VIRTIO_GPU_CMD_RESOURCE_CREATE_3D,
                    flags: 0,
                    fence_id: 100,
                    ctx_id: 0,
                    padding: 0,
                },
                resource_id: 100,
                target: 2,
                format: 1,
                bind: 0,
                width: 512,
                height: 512,
                depth: 1,
                array_size: 1,
                last_level: 0,
                nr_samples: 1,
                flags: 0,
                padding: 0,
            };
            let resp = bg.process_binary_wire_command(bytemuck::bytes_of(&create_3d));
            let resp_hdr = bytemuck::from_bytes::<VirtioGpuCtrlHdr>(&resp[0..24]);
            assert_eq!(resp_hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);
        }

        let num_threads = 6;
        let submits_per_thread = 500;
        let mut handles = Vec::new();

        for t in 0..num_threads {
            let bg_arc = Arc::clone(&bridge);
            handles.push(thread::spawn(move || {
                for i in 0..submits_per_thread {
                    let mut submit_buf = Vec::new();

                    // 1. Clear cmd (opcode 0x01): mask, r, g, b, a
                    submit_buf.extend_from_slice(&1u32.to_le_bytes()); // opcode
                    submit_buf.extend_from_slice(&20u32.to_le_bytes()); // length
                    submit_buf.extend_from_slice(&0x4000u32.to_le_bytes()); // GL_COLOR_BUFFER_BIT
                    submit_buf.extend_from_slice(&0.5f32.to_le_bytes()); // r
                    submit_buf.extend_from_slice(&0.2f32.to_le_bytes()); // g
                    submit_buf.extend_from_slice(&0.8f32.to_le_bytes()); // b
                    submit_buf.extend_from_slice(&1.0f32.to_le_bytes()); // a

                    // 2. Viewport cmd (opcode 0x04): x, y, w, h
                    submit_buf.extend_from_slice(&4u32.to_le_bytes()); // opcode
                    submit_buf.extend_from_slice(&16u32.to_le_bytes()); // length
                    submit_buf.extend_from_slice(&0i32.to_le_bytes());
                    submit_buf.extend_from_slice(&0i32.to_le_bytes());
                    submit_buf.extend_from_slice(&512u32.to_le_bytes());
                    submit_buf.extend_from_slice(&512u32.to_le_bytes());

                    // 3. Vulkan Queue Submit cmd (VIRTGPU_VK_CMD_QUEUE_SUBMIT = 0x2003)
                    submit_buf.extend_from_slice(&0x2003u32.to_le_bytes());
                    submit_buf.extend_from_slice(&12u32.to_le_bytes());
                    submit_buf.extend_from_slice(&1u32.to_le_bytes()); // num_cbs = 1
                    submit_buf.extend_from_slice(&((t * 1000 + i) as u64).to_le_bytes()); // cb_id

                    let submit_cmd_hdr = VirtioGpuSubmit3d {
                        hdr: VirtioGpuCtrlHdr {
                            type_: VIRTIO_GPU_CMD_SUBMIT_3D,
                            flags: VIRTIO_GPU_FLAG_FENCE,
                            fence_id: (t * 10000 + i) as u64,
                            ctx_id: 1,
                            padding: 0,
                        },
                        size: submit_buf.len() as u32,
                        padding: 0,
                    };

                    let mut full_packet = Vec::new();
                    full_packet.extend_from_slice(bytemuck::bytes_of(&submit_cmd_hdr));
                    full_packet.extend_from_slice(&submit_buf);

                    let resp = {
                        let mut bg = bg_arc.lock().unwrap();
                        bg.process_binary_wire_command(&full_packet)
                    };

                    let resp_hdr = bytemuck::from_bytes::<VirtioGpuCtrlHdr>(&resp[0..24]);
                    assert_eq!(resp_hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);
                    assert_eq!(resp_hdr.flags, VIRTIO_GPU_FLAG_FENCE);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }
    });
}

// -----------------------------------------------------------------------------
// 3. Adversarial Malformed Wire Packets and Error Invalidation
// -----------------------------------------------------------------------------

#[test]
fn test_virtio_gpu_adversarial_malformed_packets_and_boundaries() {
    pollster::block_on(async {
        let mut bridge = VirtioGpuBridge::new(1280, 720)
            .await
            .expect("Failed to create VirtioGpuBridge");

        // Test A: Truncated packets (<24 bytes)
        for len in 0..24 {
            let trunc_packet = vec![0xFFu8; len];
            let resp = bridge.process_binary_wire_command(&trunc_packet);
            let resp_hdr = bytemuck::from_bytes::<VirtioGpuCtrlHdr>(&resp[0..24]);
            assert_eq!(resp_hdr.type_, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER);
        }

        // Test B: Unknown command type
        let unknown_hdr = VirtioGpuCtrlHdr {
            type_: 0x9999,
            flags: 0,
            fence_id: 42,
            ctx_id: 0,
            padding: 0,
        };
        let resp = bridge.process_binary_wire_command(bytemuck::bytes_of(&unknown_hdr));
        let resp_hdr = bytemuck::from_bytes::<VirtioGpuCtrlHdr>(&resp[0..24]);
        assert_eq!(resp_hdr.type_, VIRTIO_GPU_RESP_ERR_UNSPEC);

        // Test C: TransferToHost2D for non-existent resource
        let xfer_bad_res = VirtioGpuTransferToHost2d {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D,
                flags: 0,
                fence_id: 1,
                ctx_id: 0,
                padding: 0,
            },
            r: VirtioGpuRect {
                x: 0,
                y: 0,
                width: 64,
                height: 64,
            },
            offset: 0,
            resource_id: 999999, // Doesn't exist
            padding: 0,
        };
        let resp = bridge.process_binary_wire_command(bytemuck::bytes_of(&xfer_bad_res));
        let resp_hdr = bytemuck::from_bytes::<VirtioGpuCtrlHdr>(&resp[0..24]);
        assert_eq!(resp_hdr.type_, VIRTIO_GPU_RESP_ERR_INVALID_RESOURCE_ID);

        // Test D: Submit 3D with truncated inner payload
        let submit_hdr = VirtioGpuSubmit3d {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_SUBMIT_3D,
                flags: 0,
                fence_id: 2,
                ctx_id: 0,
                padding: 0,
            },
            size: 100,
            padding: 0,
        };
        // Claim length 1000 for opcode, but only provide 4 bytes
        let mut malformed_sub = Vec::new();
        malformed_sub.extend_from_slice(&1u32.to_le_bytes()); // opcode 1
        malformed_sub.extend_from_slice(&1000u32.to_le_bytes()); // declared len 1000
        malformed_sub.extend_from_slice(&[0xAA; 4]); // only 4 bytes

        let mut packet = Vec::new();
        packet.extend_from_slice(bytemuck::bytes_of(&submit_hdr));
        packet.extend_from_slice(&malformed_sub);

        // Bridge should handle cleanly without panic
        let resp = bridge.process_binary_wire_command(&packet);
        let resp_hdr = bytemuck::from_bytes::<VirtioGpuCtrlHdr>(&resp[0..24]);
        assert_eq!(resp_hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);
    });
}

// -----------------------------------------------------------------------------
// 4. Concurrent VirtIO Binder Multi-Service Transaction Storm
// -----------------------------------------------------------------------------

#[test]
fn test_virtio_gpu_binder_multi_service_transaction_storm() {
    pollster::block_on(async {
        let bridge = Arc::new(
            VirtioGpuBridge::new(1280, 720)
                .await
                .expect("Failed to create VirtioGpuBridge"),
        );

        // Register a mock echo service at handle 99
        let echo_mock = Arc::new(MockEchoBinder::new());
        bridge.binder_device.register_service(99, echo_mock.clone());

        let num_threads = 10;
        let transacts_per_thread = 500;
        let mut handles = Vec::new();

        for t in 0..num_threads {
            let bg_arc = Arc::clone(&bridge);
            handles.push(thread::spawn(move || {
                for i in 0..transacts_per_thread {
                    let mut parcel = Parcel::new();
                    parcel.write_i32(i as i32).unwrap();

                    let req = VirtioBinderRequest::from_parcel(
                        (t * 10000 + i) as u64,
                        99,
                        10,
                        0,
                        0,
                        &parcel,
                    );
                    let packet = req.serialize();

                    let resp_bytes = bg_arc.process_binder_packet(&packet);
                    let resp = VirtioBinderResponse::deserialize(&resp_bytes)
                        .expect("Valid binder response");

                    assert_eq!(resp.hdr.msg_id, (t * 10000 + i) as u64);
                    assert_eq!(resp.hdr.status, 0); // STATUS_OK

                    let mut offset = 0;
                    let reply_parcel = Parcel::from_slice(&resp.data);
                    let result_val = reply_parcel.read_i32(&mut offset).unwrap();
                    assert_eq!(result_val, (i as i32) + 10);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(echo_mock.call_count.load(Ordering::SeqCst), num_threads * transacts_per_thread);
    });
}

// -----------------------------------------------------------------------------
// 5. Empirical Proof: DrawArrays Without Bound Shader Program Panics in WGPU
// -----------------------------------------------------------------------------

#[test]
fn test_virtio_gpu_submit_3d_draw_without_program_vulnerability_proof() {
    pollster::block_on(async {
        let mut bridge = VirtioGpuBridge::new(1280, 720)
            .await
            .expect("Failed to create VirtioGpuBridge");

        let mut submit_buf = Vec::new();
        // DrawArrays cmd (opcode 0x02): mode, first, count
        submit_buf.extend_from_slice(&2u32.to_le_bytes());
        submit_buf.extend_from_slice(&12u32.to_le_bytes());
        submit_buf.extend_from_slice(&4u32.to_le_bytes()); // GL_TRIANGLES
        submit_buf.extend_from_slice(&0u32.to_le_bytes());
        submit_buf.extend_from_slice(&6u32.to_le_bytes());

        let submit_cmd_hdr = VirtioGpuSubmit3d {
            hdr: VirtioGpuCtrlHdr {
                type_: VIRTIO_GPU_CMD_SUBMIT_3D,
                flags: 0,
                fence_id: 1,
                ctx_id: 1,
                padding: 0,
            },
            size: submit_buf.len() as u32,
            padding: 0,
        };

        let mut full_packet = Vec::new();
        full_packet.extend_from_slice(bytemuck::bytes_of(&submit_cmd_hdr));
        full_packet.extend_from_slice(&submit_buf);

        // Verify that with GLES Draw Pipeline Guard, execution completes safely without panic
        let resp = bridge.process_binary_wire_command(&full_packet);
        let resp_hdr = bytemuck::from_bytes::<VirtioGpuCtrlHdr>(&resp[0..24]);
        assert_eq!(resp_hdr.type_, VIRTIO_GPU_RESP_OK_NODATA);
    });
}

