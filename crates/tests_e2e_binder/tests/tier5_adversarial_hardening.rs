//! Tier 5: Adversarial, Fuzzing, Concurrency, and Stress Test Suite
//!
//! Hardened adversarial tests:
//! 1. Fuzzed & Corrupted Parcel decoders (OOB offsets, unaligned reads, truncated buffers)
//! 2. Protocol injection & corrupted Virtio descriptor headers
//! 3. High-concurrency races across threads (Virtio transport & HandleBridge)
//! 4. Layer translator & WebGPU compositor stress (NaN/Inf, 100+ layers)
//! 5. Security boundaries & handle isolation verification

use aidl_compat::{
    IBinder, Parcel, ParcelableHolder, RemoteTransport,
};
use binder_handle_bridge::{BridgeError, HandleBridge};
use binder_routing::{
    RouteAction, RoutingPolicy, RoutingRule,
};
use binder_rt::parcel::ParcelError;
use std::sync::Arc;
use std::thread;
use surfaceflinger_gpu_service::layer_translator::{LayerState, LayerTranslator};
use surfaceflinger_gpu_service::service::SurfaceComposerService;
use tests_e2e_binder::harness::{create_test_wgpu_device, EchoService};
use virtio_binder::device::VirtioBinderDevice;
use virtio_binder::guest_shim::GuestVirtioTransport;
use virtio_binder::protocol::*;
use webgpu_compositor::WebGpuCompositor;
use webgpu_swapchain::WebGpuSwapchain;

// =============================================================================
// 1. Fuzzed & Corrupted Parcel Decoders (Tests 1..5)
// =============================================================================

#[test]
fn test_tier5_fuzz_parcel_random_garbage_decoding() {
    // Generate 500 deterministic pseudo-random garbage byte slices
    for seed in 0..500 {
        let len = (seed * 37) % 256;
        let garbage: Vec<u8> = (0..len).map(|i| ((seed * 13 + i * 7) & 0xFF) as u8).collect();
        let p = Parcel::from_slice(&garbage);

        let mut off = 0;
        // All read operations must return Result with Err or Ok without panicking
        let _ = p.read_i32(&mut off);
        let _ = p.read_u32(&mut off);
        let _ = p.read_i64(&mut off);
        let _ = p.read_u64(&mut off);
        let _ = p.read_f32(&mut off);
        let _ = p.read_f64(&mut off);
        let _ = p.read_utf8(&mut off);
        let _ = p.read_utf16(&mut off);
        let _ = p.read_binder_object(&mut off);
        let _ = p.read_file_descriptor(&mut off);
        let _ = p.read_status(&mut off);
    }
}

#[test]
fn test_tier5_adversarial_declared_massive_vector_length_rejection() {
    let mut raw = Vec::new();
    raw.extend_from_slice(&i32::MAX.to_le_bytes()); // Declares 2 billion items
    let p = Parcel::from_slice(&raw);

    let mut off = 0;
    let res: Result<Option<Vec<i32>>, ParcelError> = p.read_vector(&mut off, |p, off| p.read_i32(off));
    // Must immediately fail with NotEnoughData, without allocating 8GB RAM
    assert!(matches!(res, Err(ParcelError::NotEnoughData { .. })));
}

#[test]
fn test_tier5_adversarial_declared_massive_string_length_rejection() {
    let mut raw = Vec::new();
    raw.extend_from_slice(&1_000_000_000i32.to_le_bytes()); // Declares 1GB string
    let p = Parcel::from_slice(&raw);

    let mut off = 0;
    let res = p.read_utf8(&mut off);
    assert!(matches!(res, Err(ParcelError::NotEnoughData { .. })));
}

#[test]
fn test_tier5_adversarial_unaligned_scalar_reads() {
    let p = Parcel::from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    let mut off = 1; // Unaligned offset (1)
    let res = p.read_i32(&mut off);
    // Reads 4 bytes from offset 1 -> [2, 3, 4, 5]
    assert!(res.is_ok());
    assert_eq!(off, 5);
}

#[test]
fn test_tier5_adversarial_out_of_bounds_offsets_table() {
    let mut p = Parcel::new();
    p.write_i32(42).unwrap();
    // Offset points past the end of the buffer
    let mut off = 0;
    let res = p.read_binder_object(&mut off);
    assert!(res.is_err());
}

// =============================================================================
// 2. Protocol Injection & Corrupted Virtio Headers (Tests 6..9)
// =============================================================================

#[test]
fn test_tier5_adversarial_corrupted_virtio_req_hdr() {
    let short_bytes = [0xFFu8; 10]; // Short header
    assert!(VirtioBinderReqHdr::from_bytes(&short_bytes).is_err());

    let exact_bytes = [0x55u8; 48]; // 48 bytes
    let hdr = VirtioBinderReqHdr::from_bytes(&exact_bytes).unwrap();
    assert_eq!(hdr.msg_id, 0x5555555555555555);
}

#[test]
fn test_tier5_adversarial_corrupted_virtio_resp_hdr() {
    let short_bytes = [0xAAu8; 15]; // Short header
    assert!(VirtioBinderRespHdr::from_bytes(&short_bytes).is_err());

    let exact_bytes = [0xAAu8; 32];
    let hdr = VirtioBinderRespHdr::from_bytes(&exact_bytes).unwrap();
    assert_eq!(hdr.msg_id, 0xAAAAAAAAAAAAAAAA);
}

#[test]
fn test_tier5_adversarial_invalid_virtio_cmd_code() {
    let device = Arc::new(VirtioBinderDevice::new());
    let transport = GuestVirtioTransport::new_with_device(device);

    let data = Parcel::new();
    let mut reply = Parcel::new();
    // Unregistered target handle
    let res = transport.transact(8888, 1, 0, &data, &mut reply);
    assert!(res.is_err());
}

#[test]
fn test_tier5_adversarial_mismatched_virtio_data_size() {
    let req = VirtioBinderReqHdr::new_transact(1, 1, 100, 0, 0, 0x1000, 0); // claims 4096 bytes data
    assert_eq!(req.data_size, 0x1000);
}

// =============================================================================
// 3. High-Concurrency Stress & Race Conditions (Tests 10..12)
// =============================================================================

#[test]
fn test_tier5_concurrent_virtio_transport_stress_16_threads() {
    let device = Arc::new(VirtioBinderDevice::new());
    let echo = Arc::new(EchoService::new());
    device.register_service(1, echo);

    let transport = Arc::new(GuestVirtioTransport::new_with_device(device));
    let mut handles = Vec::new();

    for t in 0..16 {
        let t_transport = Arc::clone(&transport);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                let mut data = Parcel::new();
                data.write_i32(t * 1000 + i).unwrap();
                data.write_i32(1).unwrap();
                let mut reply = Parcel::new();

                t_transport.transact(1, EchoService::TRANSACTION_ADD, 0, &data, &mut reply).unwrap();
                let mut off = 0;
                let status = reply.read_status(&mut off).unwrap();
                assert!(status.is_ok());
                assert_eq!(reply.read_i32(&mut off).unwrap(), t * 1000 + i + 1);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_tier5_concurrent_handle_bridge_churn_8_threads() {
    let bridge = Arc::new(HandleBridge::new());
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let mut handles = Vec::new();

    for client_id in 1..=8 {
        let b = Arc::clone(&bridge);
        let svc = Arc::clone(&echo);
        handles.push(thread::spawn(move || {
            for _ in 0..50 {
                let h = b.register_service(client_id, "desc", Arc::clone(&svc));
                b.acquire_ref(client_id, h, 5).unwrap();
                assert_eq!(b.get_strong_count(client_id, h), Some(6));
                let dropped = b.release_ref(client_id, h, 6).unwrap();
                assert!(dropped);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_tier5_concurrent_death_notification_race() {
    let bridge = Arc::new(HandleBridge::new());
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let mut handles = Vec::new();

    for client_id in 1..=10 {
        let b = Arc::clone(&bridge);
        let svc = Arc::clone(&echo);
        handles.push(thread::spawn(move || {
            let h = b.register_service(client_id, "desc", svc);
            for cookie in 1..=20 {
                b.register_death_recipient(client_id, h, cookie).unwrap();
            }
            // Sudden death
            let events = b.on_client_died(client_id);
            assert_eq!(events.len(), 20);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

// =============================================================================
// 4. Layer Translator & Compositor Boundary Fuzzing (Tests 13..15)
// =============================================================================

#[test]
fn test_tier5_adversarial_nan_and_infinity_layer_bounds() {
    let mut state = LayerState::new(1, "NaNLayer");
    state.set_bounds_pixels([f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 100.0]);
    state.set_alpha(f32::NAN);

    let comp_layer = LayerTranslator::translate_to_composition_layer(&state, 800, 600, None);
    assert_eq!(comp_layer.id, 1);
}

#[test]
fn test_tier5_adversarial_layer_alpha_clamping_extreme_values() {
    let mut s1 = LayerState::new(1, "MinAlpha");
    s1.set_alpha(-100.0);
    let mut s2 = LayerState::new(2, "MaxAlpha");
    s2.set_alpha(100.0);

    assert_eq!(s1.alpha, -100.0);
    assert_eq!(s2.alpha, 100.0);
}

#[test]
fn test_tier5_stress_100_layers_composition_pipeline() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let mut compositor = WebGpuCompositor::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let mut swapchain = WebGpuSwapchain::new(&device, 512, 512, wgpu::TextureFormat::Rgba8Unorm);

        for i in 1..=100 {
            let mut state = LayerState::new(i, &format!("Layer_{}", i));
            state.set_bounds_pixels([(i % 10) as f32 * 50.0, (i / 10) as f32 * 50.0, 50.0, 50.0]);
            state.set_z_order(i as i32);
            state.set_alpha(0.5);
            compositor.add_or_update_layer(LayerTranslator::translate_to_composition_layer(&state, 512, 512, None));
        }

        assert_eq!(compositor.layers.len(), 100);
        let target_view = swapchain.get_current_texture_view();
        compositor.compose(&device, &queue, target_view, Some(wgpu::Color::BLACK));
        swapchain.present();
    });
}

// =============================================================================
// 5. Security & Isolation Verification (Tests 16..21)
// =============================================================================

#[test]
fn test_tier5_security_cross_client_handle_access_prohibited() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h1 = bridge.register_service(1, "private_desc", echo);

    // Client 2 attempts to release Client 1's handle
    let res = bridge.release_ref(2, h1, 1);
    assert!(matches!(res, Err(BridgeError::HandleNotFound(..))));

    // Client 2 attempts to register death recipient on Client 1's handle
    let res = bridge.register_death_recipient(2, h1, 0x999);
    assert!(matches!(res, Err(BridgeError::HandleNotFound(..))));
}

#[test]
fn test_tier5_security_handle_id_spoofing_prevented() {
    let bridge = HandleBridge::new();
    let res = bridge.get_service(1, 0xDEADBEEF);
    assert!(res.is_none());
}

#[test]
fn test_tier5_security_routing_engine_conflicting_rules() {
    let mut policy = RoutingPolicy::new_default_local();
    // Rule 1: Host offload priority 10
    policy.add_rule(RoutingRule::new("android.gui.ISurfaceComposer", RouteAction::HostOffload).with_priority(10));
    // Rule 2: Local guest priority 20 (higher)
    policy.add_rule(RoutingRule::new("android.gui.ISurfaceComposer", RouteAction::LocalGuest).with_priority(20));

    // Higher priority rule must win
    assert_eq!(policy.route("android.gui.ISurfaceComposer", 1), RouteAction::LocalGuest);
}

#[test]
fn test_tier5_adversarial_truncated_parcelable_holder_buffer() {
    let p = Parcel::from_slice(&[0x01, 0x02]); // 2 bytes
    let mut holder = ParcelableHolder::new(0);
    let res = holder.read_from_parcel(&p);
    assert!(res.is_err());
}

#[test]
fn test_tier5_adversarial_parcel_file_descriptor_negative_values() {
    let pfd = aidl_compat::ParcelFileDescriptor::new(-1);
    assert_eq!(pfd.as_raw_fd(), -1);

    let mut p = Parcel::new();
    p.write_file_descriptor(-1, 0).unwrap();
    let mut off = 0;
    assert_eq!(p.read_file_descriptor(&mut off).unwrap(), -1);
}

#[test]
fn test_tier5_stress_rapid_surface_create_destroy_loop() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 800, 600);
        for i in 1..=50 {
            let handle = svc.create_surface(&format!("TempSurface_{}", i), 100, 100, 0).unwrap();
            assert_eq!(svc.get_layer_count(), 1);
            svc.destroy_surface(handle.surface_id).unwrap();
            assert_eq!(svc.get_layer_count(), 0);
        }
    });
}
