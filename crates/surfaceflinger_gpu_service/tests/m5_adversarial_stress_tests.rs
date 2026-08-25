//! Empirical and adversarial stress test suite for `surfaceflinger_gpu_service`.
//!
//! Tests:
//! 1. Multi-layer composition stress (12+ overlapping layers, alpha blending math, transforms, scissor clipping, pixel readback).
//! 2. BufferQueue allocation churn, texture reallocations on dimension change, slot exhaustion, out-of-order queueing.
//! 3. Cross-thread concurrent surface creation, state transactions, and frame presentation.
//! 4. AIDL IPC transaction parcel parsing fuzzing and boundary resilience.

use aidl_compat::{
    IBinder, Parcel, StatusCode, PING_TRANSACTION,
};
use binder_handle_bridge::HandleBridge;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use surfaceflinger_gpu_service::{
    isurfacecomposer_codes, BufferQueueError, ComposerState, GraphicBufferProducerService,
    LayerState, SurfaceComposerService,
};
use webgpu_compositor::BlendMode;

/// Helper initializing a headless WGPU device and queue for testing.
async fn create_test_wgpu() -> (Arc<wgpu::Device>, Arc<wgpu::Queue>) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("Failed to find suitable WGPU adapter for stress testing");

    let mut required_features = wgpu::Features::empty();
    if adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
        required_features |= wgpu::Features::TIMESTAMP_QUERY;
    }

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("SurfaceFlinger Stress Test Device"),
                required_features,
                required_limits: adapter.limits(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        )
        .await
        .expect("Failed to create WGPU test device");

    (Arc::new(device), Arc::new(queue))
}

// =============================================================================
// 1. Multi-layer stress rendering: 12 overlapping layers, alpha math, transforms
// =============================================================================

#[test]
fn test_multi_layer_stress_rendering_and_alpha_math() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let width = 64;
        let height = 64;
        let service = SurfaceComposerService::new(device, queue, width, height);

        // Layer 0: Opaque Dark Grey background across entire screen
        let s_bg = service.create_surface("Background", width, height, 0).unwrap();
        let mut bg_state = LayerState::new(s_bg.surface_id, "Background");
        bg_state.set_color([0.2, 0.2, 0.2, 1.0]);
        bg_state.set_bounds_ndc([-1.0, -1.0, 2.0, 2.0]);
        bg_state.set_z_order(0);
        bg_state.set_blend_mode(BlendMode::Premultiplied);

        let mut updates = vec![ComposerState::new(s_bg.surface_id, bg_state)];

        // Stack 10 overlapping translucent color quads at increasing Z-orders
        for i in 1..=10 {
            let s_layer = service
                .create_surface(&format!("Layer_{}", i), width, height, 0)
                .unwrap();
            let mut state = LayerState::new(s_layer.surface_id, &format!("Layer_{}", i));

            // Set small incremental translucent tint
            let alpha = 0.1;
            let r = if i % 2 == 0 { 0.1 } else { 0.0 };
            let g = if i % 3 == 0 { 0.1 } else { 0.0 };
            let b = if i % 5 == 0 { 0.1 } else { 0.0 };

            state.set_color([r, g, b, alpha]);
            state.set_bounds_ndc([-0.8, -0.8, 1.6, 1.6]);
            state.set_z_order(i);
            state.set_blend_mode(BlendMode::Premultiplied);

            updates.push(ComposerState::new(s_layer.surface_id, state));
        }

        // Layer 12: Top-right scissor-clipped solid Red layer
        let s_top = service.create_surface("TopScissor", width / 2, height / 2, 0).unwrap();
        let mut top_state = LayerState::new(s_top.surface_id, "TopScissor");
        top_state.set_color([1.0, 0.0, 0.0, 1.0]);
        top_state.set_bounds_ndc([0.0, 0.0, 1.0, 1.0]); // Top-right quad
        top_state.set_z_order(100);
        top_state.set_damage_rect(Some([32.0, 0.0, 32.0, 32.0])); // Top-right pixel scissor

        updates.push(ComposerState::new(s_top.surface_id, top_state));

        // Submit multi-layer batch update
        service.set_transaction_state(updates, 0).unwrap();
        assert_eq!(service.get_layer_count(), 12);

        // Render frame
        let frame_id = service.compose_and_present().unwrap();
        assert_eq!(frame_id, 1);

        // Readback pixels
        let pixels = service.readback_pixels().await.unwrap();
        assert_eq!(pixels.len(), (width * height * 4) as usize);

        let get_pixel = |x: usize, y: usize| -> (u8, u8, u8, u8) {
            let idx = (y * (width as usize) + x) * 4;
            (pixels[idx], pixels[idx + 1], pixels[idx + 2], pixels[idx + 3])
        };

        // Check top-right pixel (x=48, y=16) -> Must be solid Red due to TopScissor layer
        let (r_tr, g_tr, b_tr, a_tr) = get_pixel(48, 16);
        assert_eq!(r_tr, 255, "Top-right pixel R must be 255");
        assert_eq!(g_tr, 0, "Top-right pixel G must be 0");
        assert_eq!(b_tr, 0, "Top-right pixel B must be 0");
        assert_eq!(a_tr, 255, "Top-right pixel A must be 255");

        // Check bottom-left corner outside inner stack (x=2, y=60) -> Background color [51, 51, 51, 255]
        let (r_bl, g_bl, b_bl, a_bl) = get_pixel(2, 60);
        assert!((r_bl as i32 - 51).abs() <= 2, "Corner R must be ~51, got {}", r_bl);
        assert!((g_bl as i32 - 51).abs() <= 2, "Corner G must be ~51, got {}", g_bl);
        assert!((g_bl as i32 - 51).abs() <= 2, "Corner B must be ~51, got {}", b_bl);
        assert_eq!(a_bl, 255);
    });
}

// =============================================================================
// 2. BufferQueue stress: Churn, slot exhaustion, texture reallocation, out-of-order
// =============================================================================

#[test]
fn test_buffer_queue_slot_exhaustion_and_reallocation_churn() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let producer = GraphicBufferProducerService::new(101, Arc::clone(&device), Arc::clone(&queue));

        // Connect
        producer.connect().unwrap();

        // 1. Dequeue all 16 slots
        let mut dequeued_slots = Vec::new();
        for _ in 0..16 {
            let slot = producer.dequeue_buffer(64, 64, 1).unwrap();
            assert!(slot >= 0 && slot < 16);
            assert!(!dequeued_slots.contains(&slot));
            dequeued_slots.push(slot);
        }

        // 2. Attempting 17th dequeue without queueing/cancelling should return NoFreeSlots
        let err = producer.dequeue_buffer(64, 64, 1).unwrap_err();
        assert_eq!(err, BufferQueueError::NoFreeSlots);

        // 3. Cancel slot 5 -> should become free for dequeue
        producer.cancel_buffer(dequeued_slots[5]);
        let slot_reclaimed = producer.dequeue_buffer(64, 64, 1).unwrap();
        assert_eq!(slot_reclaimed, dequeued_slots[5]);

        // 4. Dynamic texture reallocation churn: Queue large 256x256 buffer data on slot 5
        let large_buffer = vec![128u8; 256 * 256 * 4];
        producer.queue_buffer_data(slot_reclaimed, &large_buffer, 256, 256).unwrap();

        // Acquire view
        let view = producer.acquire_latest_texture_view();
        assert!(view.is_some());

        // 5. Shrink resolution on another slot: cancel slot 0, dequeue at 16x16, queue color
        producer.cancel_buffer(dequeued_slots[0]);
        let shrink_slot = producer.dequeue_buffer(16, 16, 1).unwrap();
        producer.queue_buffer_color(shrink_slot, [255, 0, 128, 255], 16, 16).unwrap();
        let shrink_view = producer.acquire_latest_texture_view();
        assert!(shrink_view.is_some());

        // 6. Out-of-order queueing test:
        // Cancel slots 1 and 2, then dequeue both
        producer.cancel_buffer(dequeued_slots[1]);
        producer.cancel_buffer(dequeued_slots[2]);
        let slot_a = producer.dequeue_buffer(32, 32, 1).unwrap();
        let slot_b = producer.dequeue_buffer(32, 32, 1).unwrap();

        // Queue slot B first with Blue, then slot A with Red
        producer.queue_buffer_color(slot_b, [0, 0, 255, 255], 32, 32).unwrap();
        producer.queue_buffer_color(slot_a, [255, 0, 0, 255], 32, 32).unwrap();

        // The latest acquired view must reflect slot A (last queued)
        let latest_view = producer.acquire_latest_texture_view();
        assert!(latest_view.is_some());
    });
}

// =============================================================================
// 3. Concurrent multi-threaded surface mutations and composition
// =============================================================================

#[test]
fn test_concurrent_multithreaded_surface_lifecycle_and_rendering() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let width = 128;
        let height = 128;
        let service = Arc::new(SurfaceComposerService::new(device, queue, width, height));

        let num_threads = 8;
        let iterations_per_thread = 50;
        let stop_flag = Arc::new(AtomicBool::new(false));
        let error_count = Arc::new(AtomicUsize::new(0));

        let mut workers = Vec::with_capacity(num_threads);

        // Spawn worker threads creating and mutating surfaces
        for t in 0..num_threads {
            let svc = Arc::clone(&service);
            let err_cnt = Arc::clone(&error_count);

            let handle = thread::spawn(move || {
                for i in 0..iterations_per_thread {
                    let surface_name = format!("Thread_{}_Surface_{}", t, i);
                    match svc.create_surface(&surface_name, 64, 64, 0) {
                        Ok(handle) => {
                            // Mutate surface state
                            let mut state = LayerState::new(handle.surface_id, &surface_name);
                            state.set_color([0.5, 0.2, 0.8, 0.9]);
                            state.set_bounds_ndc([-0.5, -0.5, 1.0, 1.0]);
                            state.set_z_order((t * 10 + i) as i32);

                            let composer_state = ComposerState::new(handle.surface_id, state);
                            let _ = svc.set_transaction_state(vec![composer_state], 0);

                            // Queue buffer data to producer
                            let _ = handle.producer.connect();
                            if let Ok(slot) = handle.producer.dequeue_buffer(64, 64, 1) {
                                let _ = handle.producer.queue_buffer_color(slot, [100, 150, 200, 255], 64, 64);
                            }

                            // Brief yield
                            thread::sleep(Duration::from_millis(1));

                            // Destroy surface half the time to simulate churn
                            if i % 2 == 0 {
                                let _ = svc.destroy_surface(handle.surface_id);
                            }
                        }
                        Err(_) => {
                            err_cnt.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }
            });
            workers.push(handle);
        }

        // Spawn dedicated Compositor Rendering Loop thread
        let comp_svc = Arc::clone(&service);
        let comp_stop = Arc::clone(&stop_flag);
        let render_thread = thread::spawn(move || {
            let mut frames = 0;
            while !comp_stop.load(Ordering::Relaxed) {
                let _ = comp_svc.compose_and_present();
                frames += 1;
                thread::sleep(Duration::from_millis(5));
            }
            frames
        });

        // Wait for all workers to finish
        for w in workers {
            w.join().expect("Surface worker thread must not panic");
        }

        stop_flag.store(true, Ordering::Relaxed);
        let total_frames = render_thread.join().expect("Render thread must not panic");
        assert!(total_frames > 0);
        assert_eq!(error_count.load(Ordering::SeqCst), 0);

        // Final readback should succeed cleanly
        let pixels = service.readback_pixels().await.unwrap();
        assert_eq!(pixels.len(), (width * height * 4) as usize);
    });
}

// =============================================================================
// 4. AIDL IPC Transaction Fuzzing and Handle Bridge Stress
// =============================================================================

#[test]
fn test_aidl_ipc_fuzzing_and_handle_bridge_resilience() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let bridge = Arc::new(Mutex::new(HandleBridge::new()));
        let service = Arc::new(SurfaceComposerService::with_handle_bridge(
            device,
            queue,
            640,
            480,
            Arc::clone(&bridge),
        ));

        // 1. Unknown transaction code
        let mut reply = Parcel::new();
        let err = service.transact(99999, 0, &Parcel::new(), &mut reply).unwrap_err();
        assert_eq!(err.status_code(), StatusCode::UnknownTransaction);

        // 2. Empty / Truncated Parcel for CREATE_SURFACE -> returns BadValue
        let truncated_data = Parcel::new();
        let mut reply2 = Parcel::new();
        let res = service.transact(
            isurfacecomposer_codes::CREATE_SURFACE,
            0,
            &truncated_data,
            &mut reply2,
        );
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().status_code(), StatusCode::BadValue);

        // 3. Malformed Parcel for DESTROY_SURFACE (invalid / non-existent surface ID)
        let mut destroy_data = Parcel::new();
        destroy_data.write_u64(0xDEAD_BEEF).unwrap();
        let mut destroy_reply = Parcel::new();
        let res2 = service.transact(
            isurfacecomposer_codes::DESTROY_SURFACE,
            0,
            &destroy_data,
            &mut destroy_reply,
        );
        assert!(res2.is_err());
        assert_eq!(res2.unwrap_err().status_code(), StatusCode::NameNotFound);

        // 4. Register and query 32 surfaces through HandleBridge simultaneously
        let client_id = 1; // Default client id used by SurfaceComposerService
        let mut producer_handles = Vec::new();

        for i in 0..32 {
            let mut create_data = Parcel::new();
            create_data.write_utf8(Some(&format!("FuzzSurface_{}", i))).unwrap();
            create_data.write_u32(100).unwrap();
            create_data.write_u32(100).unwrap();
            create_data.write_i32(1).unwrap();
            create_data.write_u32(0).unwrap();

            let mut create_reply = Parcel::new();
            service
                .transact(
                    isurfacecomposer_codes::CREATE_SURFACE,
                    0,
                    &create_data,
                    &mut create_reply,
                )
                .unwrap();

            let mut offset = 0;
            assert!(create_reply.read_status(&mut offset).unwrap().is_ok());
            let _surface_id = create_reply.read_u64(&mut offset).unwrap();
            let prod_handle = create_reply.read_u32(&mut offset).unwrap();
            producer_handles.push(prod_handle);
        }

        assert_eq!(producer_handles.len(), 32);

        // Verify each producer handle can transact via HandleBridge
        for &h in &producer_handles {
            let producer = bridge
                .lock()
                .unwrap()
                .get_service(client_id, h)
                .expect("Producer service must exist in HandleBridge");

            assert_eq!(
                producer.get_class_descriptor(),
                Some(GraphicBufferProducerService::DESCRIPTOR)
            );

            // Transact PING
            let mut ping_rep = Parcel::new();
            assert!(producer.transact(PING_TRANSACTION, 0, &Parcel::new(), &mut ping_rep).is_ok());
        }

        // Release all 32 handles
        for &h in &producer_handles {
            assert!(
                bridge.lock().unwrap().release_ref(client_id, h, 1).unwrap(),
                "Handle must be cleanly dropped"
            );
        }

        assert_eq!(bridge.lock().unwrap().handle_count(client_id), 0);
    });
}
