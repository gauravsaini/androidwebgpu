//! Comprehensive integration tests for host-side SurfaceFlinger GPU service and compositor.

use aidl_compat::{IBinder, Parcel, Parcelable, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_handle_bridge::HandleBridge;
use std::sync::{Arc, Mutex};
use surfaceflinger_gpu_service::{
    igraphicbufferproducer_codes, isurfacecomposer_codes, ComposerState, DisplayInfo,
    GraphicBufferProducerService, LayerState, SurfaceComposerService,
};
use webgpu_compositor::BlendMode;

/// Helper initializing a headless WGPU device and queue for testing.
async fn create_test_wgpu() -> (Arc<wgpu::Device>, Arc<wgpu::Queue>) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("Failed to find suitable WGPU adapter");

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("SurfaceFlinger Test Device"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        )
        .await
        .expect("Failed to create WGPU test device");

    (Arc::new(device), Arc::new(queue))
}

#[test]
fn test_surface_creation_and_lifecycle() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let service = SurfaceComposerService::new(device, queue, 1280, 720);

        assert_eq!(service.get_layer_count(), 0);
        assert!(!service.is_boot_finished());

        // Create surface 1: Background
        let s1 = service
            .create_surface("Wallpaper", 1280, 720, 0)
            .expect("Failed to create Wallpaper surface");
        assert_eq!(s1.surface_id, 1);
        assert_eq!(s1.name, "Wallpaper");
        assert_eq!(service.get_layer_count(), 1);

        // Create surface 2: AppView
        let s2 = service
            .create_surface("MainActivity", 1280, 720, 0)
            .expect("Failed to create MainActivity surface");
        assert_eq!(s2.surface_id, 2);
        assert_eq!(s2.name, "MainActivity");
        assert_eq!(service.get_layer_count(), 2);

        // Query buffer producers
        let p1 = service.get_surface_producer(s1.surface_id);
        assert!(p1.is_some());
        let p2 = service.get_surface_producer(s2.surface_id);
        assert!(p2.is_some());

        // Destroy surface 1
        service
            .destroy_surface(s1.surface_id)
            .expect("Destroy surface 1 failed");
        assert_eq!(service.get_layer_count(), 1);
        assert!(service.get_surface_producer(s1.surface_id).is_none());

        // Destroy surface 2
        service
            .destroy_surface(s2.surface_id)
            .expect("Destroy surface 2 failed");
        assert_eq!(service.get_layer_count(), 0);
    });
}

#[test]
fn test_set_transaction_state_and_pixel_readback() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let width = 64;
        let height = 64;
        let service = SurfaceComposerService::new(device, queue, width, height);

        // Surface 1: Solid Red background across entire screen
        let s_bg = service.create_surface("BgRed", width, height, 0).unwrap();
        let mut bg_state = LayerState::new(s_bg.surface_id, "BgRed");
        bg_state.set_color([1.0, 0.0, 0.0, 1.0]); // Red
        bg_state.set_bounds_ndc([-1.0, -1.0, 2.0, 2.0]);
        bg_state.set_z_order(0);

        // Surface 2: Top-right Solid Blue quad (NDC: x in [0.0, 1.0], y in [0.0, 1.0])
        let s_fg = service.create_surface("FgBlue", width / 2, height / 2, 0).unwrap();
        let mut fg_state = LayerState::new(s_fg.surface_id, "FgBlue");
        fg_state.set_color([0.0, 0.0, 1.0, 1.0]); // Blue
        fg_state.set_bounds_ndc([0.0, 0.0, 1.0, 1.0]);
        fg_state.set_z_order(10);

        let updates = vec![
            ComposerState::new(s_bg.surface_id, bg_state),
            ComposerState::new(s_fg.surface_id, fg_state),
        ];
        service.set_transaction_state(updates, 0).unwrap();

        // Render frame
        let frame_id = service.compose_and_present().unwrap();
        assert_eq!(frame_id, 1);

        // Readback rendered frame pixels
        let pixels = service.readback_pixels().await.unwrap();
        assert_eq!(pixels.len(), (width * height * 4) as usize);

        // In WGPU offscreen rendering:
        // Top-right quadrant (x > 32, y < 32 in screen space) should be Blue [0, 0, 255, 255]
        // Bottom-left quadrant (x < 32, y > 32 in screen space) should be Red [255, 0, 0, 255]
        let get_pixel = |x: usize, y: usize| -> (u8, u8, u8, u8) {
            let idx = (y * (width as usize) + x) * 4;
            (pixels[idx], pixels[idx + 1], pixels[idx + 2], pixels[idx + 3])
        };

        // Check top-right pixel (x=48, y=16) -> Blue
        let (r_tr, _g_tr, b_tr, a_tr) = get_pixel(48, 16);
        assert!(b_tr > 200, "Top-right must be blue, got B={}", b_tr);
        assert_eq!(r_tr, 0, "Top-right R must be 0, got R={}", r_tr);
        assert_eq!(a_tr, 255);

        // Check bottom-left pixel (x=16, y=48) -> Red
        let (r_bl, _g_bl, b_bl, a_bl) = get_pixel(16, 48);
        assert!(r_bl > 200, "Bottom-left must be red, got R={}", r_bl);
        assert_eq!(b_bl, 0, "Bottom-left B must be 0, got B={}", b_bl);
        assert_eq!(a_bl, 255);
    });
}

#[test]
fn test_buffer_queue_texture_queue_and_presentation() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let width = 64;
        let height = 64;
        let service = SurfaceComposerService::new(device, queue, width, height);

        let surface = service.create_surface("TextureLayer", width, height, 0).unwrap();
        let producer = surface.producer;

        // Connect producer
        producer.connect().unwrap();

        // Dequeue buffer slot
        let slot = producer.dequeue_buffer(width, height, 1).unwrap();
        assert!(slot >= 0);

        // Queue full green RGBA buffer
        producer
            .queue_buffer_color(slot, [0, 255, 0, 255], width, height)
            .unwrap();

        // Configure layer to display full screen
        let mut state = LayerState::new(surface.surface_id, "TextureLayer");
        state.set_bounds_ndc([-1.0, -1.0, 2.0, 2.0]);
        state.set_blend_mode(BlendMode::None);

        service
            .set_transaction_state(vec![ComposerState::new(surface.surface_id, state)], 0)
            .unwrap();

        // Compose and present
        let frame_id = service.compose_and_present().unwrap();
        assert_eq!(frame_id, 1);

        // Readback and assert pixels are green
        let pixels = service.readback_pixels().await.unwrap();
        for y in 0..height as usize {
            for x in 0..width as usize {
                let idx = (y * (width as usize) + x) * 4;
                assert_eq!(pixels[idx], 0, "R must be 0 at ({},{})", x, y);
                assert!(pixels[idx + 1] > 200, "G must be >200 at ({},{})", x, y);
                assert_eq!(pixels[idx + 2], 0, "B must be 0 at ({},{})", x, y);
                assert_eq!(pixels[idx + 3], 255, "A must be 255 at ({},{})", x, y);
            }
        }
    });
}

#[test]
fn test_binder_transact_ipc_roundtrip() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let service = SurfaceComposerService::new(device, queue, 1920, 1080);

        // 1. PING_TRANSACTION
        let mut ping_reply = Parcel::new();
        assert!(service
            .transact(PING_TRANSACTION, 0, &Parcel::new(), &mut ping_reply)
            .is_ok());

        // 2. INTERFACE_TRANSACTION
        let mut iface_reply = Parcel::new();
        service
            .transact(INTERFACE_TRANSACTION, 0, &Parcel::new(), &mut iface_reply)
            .unwrap();
        let mut offset = 0;
        let descriptor = iface_reply.read_utf16(&mut offset).unwrap();
        assert_eq!(descriptor, Some(SurfaceComposerService::DESCRIPTOR.to_string()));

        // 3. GET_DISPLAY_INFO (1010)
        let mut disp_reply = Parcel::new();
        service
            .transact(
                isurfacecomposer_codes::GET_DISPLAY_INFO,
                0,
                &Parcel::new(),
                &mut disp_reply,
            )
            .unwrap();
        let mut offset = 0;
        let status = disp_reply.read_status(&mut offset).unwrap();
        assert!(status.is_ok());
        let mut disp_info = DisplayInfo::default();
        disp_info.read_from_parcel_at(&disp_reply, &mut offset).unwrap();
        assert_eq!(disp_info.width, 1920);
        assert_eq!(disp_info.height, 1080);
        assert_eq!(disp_info.fps, 120.0);

        // 4. CREATE_SURFACE (1006)
        let mut create_data = Parcel::new();
        create_data.write_utf8(Some("TransactSurface")).unwrap();
        create_data.write_u32(800).unwrap();
        create_data.write_u32(600).unwrap();
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
        let surface_id = create_reply.read_u64(&mut offset).unwrap();
        assert_eq!(surface_id, 1);
        let _producer_handle = create_reply.read_u32(&mut offset).unwrap();

        // 5. SET_TRANSACTION_STATE (1020)
        let mut state_data = Parcel::new();
        state_data.write_i32(1).unwrap(); // 1 layer state
        let mut lstate = LayerState::new(surface_id, "TransactSurface");
        lstate.set_color([1.0, 1.0, 0.0, 1.0]);
        let cstate = ComposerState::new(surface_id, lstate);
        cstate.write_to_parcel(&mut state_data).unwrap();
        state_data.write_u32(0).unwrap(); // flags

        let mut state_reply = Parcel::new();
        service
            .transact(
                isurfacecomposer_codes::SET_TRANSACTION_STATE,
                0,
                &state_data,
                &mut state_reply,
            )
            .unwrap();
        let mut offset = 0;
        assert!(state_reply.read_status(&mut offset).unwrap().is_ok());

        // 6. BOOT_FINISHED (1025)
        assert!(!service.is_boot_finished());
        let mut boot_reply = Parcel::new();
        service
            .transact(
                isurfacecomposer_codes::BOOT_FINISHED,
                0,
                &Parcel::new(),
                &mut boot_reply,
            )
            .unwrap();
        let mut offset = 0;
        assert!(boot_reply.read_status(&mut offset).unwrap().is_ok());
        assert!(service.is_boot_finished());

        // 7. DESTROY_SURFACE (1007)
        let mut destroy_data = Parcel::new();
        destroy_data.write_u64(surface_id).unwrap();
        let mut destroy_reply = Parcel::new();
        service
            .transact(
                isurfacecomposer_codes::DESTROY_SURFACE,
                0,
                &destroy_data,
                &mut destroy_reply,
            )
            .unwrap();
        let mut offset = 0;
        assert!(destroy_reply.read_status(&mut offset).unwrap().is_ok());
        assert_eq!(service.get_layer_count(), 0);
    });
}

#[test]
fn test_handle_bridge_service_registration() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let bridge = Arc::new(Mutex::new(HandleBridge::new()));
        let service = SurfaceComposerService::with_handle_bridge(
            device,
            queue,
            640,
            480,
            Arc::clone(&bridge),
        );

        // Register SurfaceComposer service in HandleBridge
        let composer_handle = bridge.lock().unwrap().register_service(
            1, // client_id
            SurfaceComposerService::DESCRIPTOR,
            Arc::new(service) as Arc<dyn IBinder>,
        );

        // Retrieve service via handle bridge
        let retrieved_composer = bridge
            .lock()
            .unwrap()
            .get_service(1, composer_handle)
            .expect("Composer service not found in bridge");

        // Dispatch CREATE_SURFACE through the bridge service instance
        let mut create_data = Parcel::new();
        create_data.write_utf8(Some("BridgeSurface")).unwrap();
        create_data.write_u32(320).unwrap();
        create_data.write_u32(240).unwrap();
        create_data.write_i32(1).unwrap();
        create_data.write_u32(0).unwrap();

        let mut create_reply = Parcel::new();
        retrieved_composer
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
        let producer_handle = create_reply.read_u32(&mut offset).unwrap();

        // Retrieve GraphicBufferProducer service from handle bridge using producer_handle
        let retrieved_producer = bridge
            .lock()
            .unwrap()
            .get_service(1, producer_handle)
            .expect("GraphicBufferProducer service not found in bridge");

        assert_eq!(
            retrieved_producer.get_class_descriptor(),
            Some(GraphicBufferProducerService::DESCRIPTOR)
        );

        // Dispatch DEQUEUE_BUFFER across producer handle
        let mut dequeue_data = Parcel::new();
        dequeue_data.write_u32(320).unwrap();
        dequeue_data.write_u32(240).unwrap();
        dequeue_data.write_u32(1).unwrap();

        let mut dequeue_reply = Parcel::new();
        retrieved_producer
            .transact(
                igraphicbufferproducer_codes::DEQUEUE_BUFFER,
                0,
                &dequeue_data,
                &mut dequeue_reply,
            )
            .unwrap();

        let mut offset = 0;
        assert!(dequeue_reply.read_status(&mut offset).unwrap().is_ok());
        let slot = dequeue_reply.read_i32(&mut offset).unwrap();
        assert!(slot >= 0);

        // Acquire and release ref on producer handle
        assert!(bridge.lock().unwrap().acquire_ref(1, producer_handle, 1).is_ok());
        assert!(
            bridge.lock().unwrap().release_ref(1, producer_handle, 2).unwrap(), // Dropped when count reaches 0
        );
    });
}

#[test]
fn test_alpha_blending_transparency_pixel_math() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let width = 64;
        let height = 64;
        let service = SurfaceComposerService::new(device, queue, width, height);

        // Layer 1: Solid Blue opaque background [0, 0, 1.0, 1.0]
        let s_bg = service.create_surface("BlueBg", width, height, 0).unwrap();
        let mut bg_state = LayerState::new(s_bg.surface_id, "BlueBg");
        bg_state.set_color([0.0, 0.0, 1.0, 1.0]);
        bg_state.set_bounds_ndc([-1.0, -1.0, 2.0, 2.0]);
        bg_state.set_z_order(0);
        bg_state.set_blend_mode(BlendMode::Premultiplied);

        // Layer 2: 50% transparent Red overlay [1.0, 0.0, 0.0, 0.5]
        let s_fg = service.create_surface("RedOverlay", width, height, 0).unwrap();
        let mut fg_state = LayerState::new(s_fg.surface_id, "RedOverlay");
        // Premultiplied alpha: RGB is multiplied by alpha (0.5 * 1.0 = 0.5)
        fg_state.set_color([0.5, 0.0, 0.0, 0.5]);
        fg_state.set_bounds_ndc([-1.0, -1.0, 2.0, 2.0]);
        fg_state.set_z_order(1);
        fg_state.set_blend_mode(BlendMode::Premultiplied);

        service
            .set_transaction_state(
                vec![
                    ComposerState::new(s_bg.surface_id, bg_state),
                    ComposerState::new(s_fg.surface_id, fg_state),
                ],
                0,
            )
            .unwrap();

        // Readback pixels
        let pixels = service.readback_pixels().await.unwrap();

        // Check center pixel: Should have both Red and Blue components blended
        let idx = (32 * (width as usize) + 32) * 4;
        let r = pixels[idx];
        let g = pixels[idx + 1];
        let b = pixels[idx + 2];
        let a = pixels[idx + 3];

        println!("Blended center pixel: R={}, G={}, B={}, A={}", r, g, b, a);
        assert_eq!(r, 128, "R must be 128 (0.5 * 255)");
        assert_eq!(g, 0, "G must be 0");
        assert_eq!(b, 191, "B must be 191 (0.75 * 255 under premultiplied alpha)");
        assert_eq!(a, 255, "A must be 255");
    });
}

#[test]
fn test_damage_rect_and_hwc_transforms() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let width = 64;
        let height = 64;
        let service = SurfaceComposerService::new(device, queue, width, height);

        let surface = service.create_surface("TransformLayer", width, height, 0).unwrap();
        let mut state = LayerState::new(surface.surface_id, "TransformLayer");
        state.set_color([1.0, 0.5, 0.2, 1.0]);
        state.set_bounds_ndc([-0.5, -0.5, 1.0, 1.0]);
        state.set_hwc_transform(4); // ROT_90
        state.set_source_crop([0.1, 0.1, 0.9, 0.9]);
        state.set_damage_rect(Some([8.0, 8.0, 48.0, 48.0])); // 8..56 scissor range

        service
            .set_transaction_state(vec![ComposerState::new(surface.surface_id, state)], 0)
            .unwrap();

        let frame = service.compose_and_present().unwrap();
        assert_eq!(frame, 1);

        let pixels = service.readback_pixels().await.unwrap();
        assert_eq!(pixels.len(), (width * height * 4) as usize);
    });
}

#[test]
fn test_error_paths_and_invalid_operations() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let service = SurfaceComposerService::new(device, queue, 100, 100);

        // Destroy non-existent surface
        let err = service.destroy_surface(9999).unwrap_err();
        match err {
            surfaceflinger_gpu_service::CompositorServiceError::SurfaceNotFound(id) => {
                assert_eq!(id, 9999);
            }
            _ => panic!("Expected SurfaceNotFound error"),
        }

        // Buffer producer error handling
        let s = service.create_surface("TestProducer", 100, 100, 0).unwrap();
        let producer = s.producer;

        // Queue without dequeue should fail with SlotInUse
        let err2 = producer.queue_buffer_color(0, [255, 0, 0, 255], 100, 100).unwrap_err();
        assert_eq!(err2, surfaceflinger_gpu_service::BufferQueueError::SlotInUse(0));

        // Invalid slot index
        let err3 = producer.queue_buffer_color(99, [255, 0, 0, 255], 100, 100).unwrap_err();
        assert_eq!(err3, surfaceflinger_gpu_service::BufferQueueError::InvalidSlot(99));
    });
}

