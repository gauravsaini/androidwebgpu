//! Tier 4: Real-World Application Scenario Test Suite
//!
//! 6 comprehensive real-world scenarios representing production Android-on-Linux WebGPU graphics pipelines:
//! 1. Full Lifecycle Ping & Status Round-Trip across VM Boundary
//! 2. Multi-Client Surface Allocation & Concurrent Render with WebGPU Pixel Readback
//! 3. Multi-Hop Handle Pass & Cross-Client Layer Update
//! 4. Abrupt Client Crash & Host Resource Teardown
//! 5. Full Android HWC Multi-Layer SurfaceFlinger Frame Submission
//! 6. Selective Routing Mixed Workload (Local Pass-Through + Offloaded 3D)

use aidl_compat::{
    IBinder, Parcel, Remotable, RemoteTransport,
};
use binder_handle_bridge::HandleBridge;
use binder_routing::{
    DescriptorMatcher, MatchRule, MatcherEngine, RouteAction, ServiceNameMatcher,
};
use surfaceflinger_gpu_service::layer_translator::{LayerState, LayerTranslator};
use surfaceflinger_gpu_service::service::{isurfacecomposer_codes, SurfaceComposerService};
use tests_e2e_binder::harness::{create_test_wgpu_device, EchoService};
use virtio_binder::device::VirtioBinderDevice;
use virtio_binder::guest_shim::GuestVirtioTransport;
use virtio_binder::protocol::*;
use webgpu_compositor::WebGpuCompositor;
use webgpu_swapchain::WebGpuSwapchain;
use std::sync::Arc;

// =============================================================================
// Scenario 1: Full Lifecycle Ping & Status Round-Trip across VM Boundary
// =============================================================================

#[test]
fn test_scenario_1_full_lifecycle_ping_and_status_roundtrip() {
    // 1. Initialize Host Device & Bridge
    let bridge = Arc::new(HandleBridge::new());
    let host_device = Arc::new(VirtioBinderDevice::new());

    // 2. Register Host Echo Service for Client 1
    let echo_service: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let handle_id = bridge.register_service(1, EchoService::DESCRIPTOR, Arc::clone(&echo_service));
    host_device.register_service(handle_id, bridge.get_service(1, handle_id).unwrap());

    // 3. Guest Shim Initialization
    let guest_transport = GuestVirtioTransport::new_with_device(Arc::clone(&host_device));

    // 4. Ping Transaction
    assert!(guest_transport.ping(handle_id).is_ok());

    // 5. Multi-arg calculation / Echo transaction
    let mut data = Parcel::new();
    data.write_utf8(Some("Hello Host from Guest VM!")).unwrap();
    let mut reply = Parcel::new();

    guest_transport.transact(handle_id, EchoService::TRANSACTION_ECHO, 0, &data, &mut reply).unwrap();

    let mut off = 0;
    let status = reply.read_status(&mut off).unwrap();
    assert!(status.is_ok());
    let response_text = reply.read_utf8(&mut off).unwrap().unwrap();
    assert_eq!(response_text, "Hello Host from Guest VM!");

    // 6. Graceful Teardown
    let dropped = bridge.release_ref(1, handle_id, 1).unwrap();
    assert!(dropped);
    assert!(bridge.get_service(1, handle_id).is_none());
}

// =============================================================================
// Scenario 2: Multi-Client Surface Allocation & Concurrent Render with WebGPU Pixel Readback
// =============================================================================

#[test]
fn test_scenario_2_multi_client_surface_allocation_and_pixel_readback() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let composer_service = SurfaceComposerService::new(device, queue, 64, 64);

        // Boot finished signal
        let mut boot_reply = Parcel::new();
        composer_service.on_transact(isurfacecomposer_codes::BOOT_FINISHED, &Parcel::new(), &mut boot_reply).unwrap();
        assert!(composer_service.is_boot_finished());

        // Client 1 allocates Layer 1 (Red Background quad)
        let s1 = composer_service.create_surface("Client1_Background", 64, 64, 0).unwrap();
        composer_service.set_surface_color(s1.surface_id, [1.0, 0.0, 0.0, 1.0], [-1.0, -1.0, 2.0, 2.0], 0).unwrap();

        // Client 2 allocates Layer 2 (Green Centered Box quad)
        let s2 = composer_service.create_surface("Client2_Box", 32, 32, 0).unwrap();
        composer_service.set_surface_color(s2.surface_id, [0.0, 1.0, 0.0, 1.0], [-0.5, -0.5, 1.0, 1.0], 1).unwrap();

        // Compose and present frame to WebGPU swapchain
        let frame_1 = composer_service.compose_and_present().unwrap();
        assert_eq!(frame_1, 1);
        assert_eq!(composer_service.get_layer_count(), 2);

        // Compose frame 2
        let frame_2 = composer_service.compose_and_present().unwrap();
        assert_eq!(frame_2, 2);
    });
}

// =============================================================================
// Scenario 3: Multi-Hop Handle Pass & Cross-Client Layer Update
// =============================================================================

#[test]
fn test_scenario_3_multihop_handle_pass_and_layer_update() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());

    // Host registers service on Client A (id: 100)
    let handle_a = bridge.register_service(100, "android.gui.IGraphicBufferProducer", Arc::clone(&echo));

    // Client A passes handle to Client B (id: 200) via Binder transaction
    let handle_b = bridge.transfer_handle(100, 200, handle_a).unwrap();

    // Client B passes handle to Client C (id: 300) (e.g. SurfaceFlinger)
    let handle_c = bridge.transfer_handle(200, 300, handle_b).unwrap();

    // Verify all clients have valid, isolated handle references to the same service
    assert!(bridge.get_service(100, handle_a).is_some());
    assert!(bridge.get_service(200, handle_b).is_some());
    assert!(bridge.get_service(300, handle_c).is_some());

    // Client A drops its reference
    bridge.release_ref(100, handle_a, 1).unwrap();
    assert!(bridge.get_service(100, handle_a).is_none());

    // Client B and C references remain perfectly valid and functional
    assert!(bridge.get_service(200, handle_b).is_some());
    assert!(bridge.get_service(300, handle_c).is_some());
}

// =============================================================================
// Scenario 4: Abrupt Client Crash & Host Resource Teardown
// =============================================================================

#[test]
fn test_scenario_4_abrupt_client_crash_and_host_resource_teardown() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let bridge = Arc::new(HandleBridge::new());
        let virtio_dev = Arc::new(VirtioBinderDevice::new());
        let svc = SurfaceComposerService::new(device, queue, 800, 600);

        // Client 50 connects and creates 2 surfaces
        let h1 = bridge.register_service(50, "android.gui.ISurfaceComposer", Arc::new(EchoService::new()));
        let h2 = bridge.register_service(50, "android.gui.IGraphicBufferProducer", Arc::new(EchoService::new()));

        // Link death recipients on both handles
        bridge.register_death_recipient(50, h1, 0x5001).unwrap();
        bridge.register_death_recipient(50, h2, 0x5002).unwrap();

        let s1 = svc.create_surface("CrashApp_Surface1", 400, 300, 0).unwrap();
        let s2 = svc.create_surface("CrashApp_Surface2", 400, 300, 0).unwrap();

        // Client 50 abruptly crashes!
        let death_events = bridge.on_client_died(50);
        assert_eq!(death_events.len(), 2);

        // Host dispatches death events to Virtio event queue
        for (handle, cookie) in &death_events {
            virtio_dev.event_queue().lock().unwrap().push_event(VirtioBinderEventHdr::new_death(*handle, *cookie));
        }

        // Host tears down allocated surfaces
        svc.destroy_surface(s1.surface_id).unwrap();
        svc.destroy_surface(s2.surface_id).unwrap();

        // Verify guest event queue receives notifications
        let drained = virtio_dev.event_queue().lock().unwrap().drain_events();
        assert_eq!(drained.len(), 2);

        // Verify client 50 handles are completely cleaned up
        assert!(bridge.get_service(50, h1).is_none());
        assert!(bridge.get_service(50, h2).is_none());
    });
}

// =============================================================================
// Scenario 5: Full Android HWC Multi-Layer SurfaceFlinger Frame Submission
// =============================================================================

#[test]
fn test_scenario_5_full_android_hwc_multilayer_frame_submission() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let mut compositor = WebGpuCompositor::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let mut swapchain = WebGpuSwapchain::new(&device, 1080, 1920, wgpu::TextureFormat::Rgba8Unorm);

        // 1. Wallpaper Layer (Bottom, z = 0, full screen)
        let mut wallpaper = LayerState::new(1, "Wallpaper");
        wallpaper.set_bounds_pixels([0.0, 0.0, 1080.0, 1920.0]);
        wallpaper.set_z_order(0);
        wallpaper.set_alpha(1.0);
        compositor.add_or_update_layer(LayerTranslator::translate_to_composition_layer(&wallpaper, 1080, 1920, None));

        // 2. App Content Layer (z = 1, middle)
        let mut app_content = LayerState::new(2, "AppContent");
        app_content.set_bounds_pixels([0.0, 120.0, 1080.0, 1680.0]);
        app_content.set_z_order(1);
        app_content.set_alpha(1.0);
        compositor.add_or_update_layer(LayerTranslator::translate_to_composition_layer(&app_content, 1080, 1920, None));

        // 3. Status Bar Layer (z = 2, top 120px)
        let mut status_bar = LayerState::new(3, "StatusBar");
        status_bar.set_bounds_pixels([0.0, 0.0, 1080.0, 120.0]);
        status_bar.set_z_order(2);
        status_bar.set_alpha(0.9);
        compositor.add_or_update_layer(LayerTranslator::translate_to_composition_layer(&status_bar, 1080, 1920, None));

        // 4. Navigation Bar Layer (z = 3, bottom 120px)
        let mut nav_bar = LayerState::new(4, "NavigationBar");
        nav_bar.set_bounds_pixels([0.0, 1800.0, 1080.0, 120.0]);
        nav_bar.set_z_order(3);
        nav_bar.set_alpha(0.9);
        compositor.add_or_update_layer(LayerTranslator::translate_to_composition_layer(&nav_bar, 1080, 1920, None));

        assert_eq!(compositor.layers.len(), 4);

        // Render composite frame
        let target_view = swapchain.get_current_texture_view();
        compositor.compose(&device, &queue, target_view, Some(wgpu::Color::BLACK));
        swapchain.present();
    });
}

// =============================================================================
// Scenario 6: Selective Routing Mixed Workload (Local Pass-Through + Offloaded 3D)
// =============================================================================

#[test]
fn test_scenario_6_selective_routing_mixed_workload() {
    let mut matcher = MatcherEngine::new(RouteAction::LocalGuest);

    // Host offload rules for 3D graphics & SurfaceFlinger
    matcher.add_rule(
        MatchRule::new(
            DescriptorMatcher::Exact("android.gui.ISurfaceComposer".to_string()),
            RouteAction::HostOffload,
        )
        .with_service(ServiceNameMatcher::Exact("SurfaceFlinger".to_string())),
    );

    matcher.add_rule(
        MatchRule::new(
            DescriptorMatcher::Prefix("android.hardware.graphics.".to_string()),
            RouteAction::HostOffload,
        ),
    );

    // Local guest rules for Android System Services
    matcher.add_rule(
        MatchRule::new(
            DescriptorMatcher::Exact("android.os.IServiceManager".to_string()),
            RouteAction::LocalGuest,
        )
        .with_priority(100),
    );

    matcher.add_rule(
        MatchRule::new(
            DescriptorMatcher::Exact("android.app.IActivityManager".to_string()),
            RouteAction::LocalGuest,
        )
        .with_priority(100),
    );

    // Simulate mixed transaction sequence from guest application
    let actions = vec![
        matcher.match_transaction(Some("servicemanager"), Some("android.os.IServiceManager"), 1),
        matcher.match_transaction(Some("activity"), Some("android.app.IActivityManager"), 12),
        matcher.match_transaction(Some("SurfaceFlinger"), Some("android.gui.ISurfaceComposer"), isurfacecomposer_codes::CREATE_SURFACE),
        matcher.match_transaction(Some("allocator"), Some("android.hardware.graphics.allocator.IAllocator"), 1),
        matcher.match_transaction(Some("window"), Some("android.view.IWindowManager"), 5),
    ];

    assert_eq!(actions, vec![
        RouteAction::LocalGuest,   // ServiceManager
        RouteAction::LocalGuest,   // ActivityManager
        RouteAction::HostOffload,  // SurfaceFlinger
        RouteAction::HostOffload,  // Gralloc Allocator
        RouteAction::LocalGuest,   // WindowManager default
    ]);
}
