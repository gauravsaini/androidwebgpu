//! Tier 3: Pairwise Subsystem Interaction Test Suite
//!
//! Combinatorial pairwise integration tests across major subsystems:
//! 1. Virtio-Binder + Handle Bridge
//! 2. Binder Routing + SurfaceFlinger GPU Service
//! 3. AIDL Macros + Virtio Transport
//! 4. Parcel Wire Codec + Flat Binder Objects / FDs
//! 5. WebGPU Compositor + Swapchain
//! 6. Handle Bridge + Death Notification Cascade
//! 7. Matcher Engine + Routing Policy Hierarchy
//! 8. AIDL Status + Error Propagation
//! 9. SurfaceFlinger Service + Layer Translation + WebGPU Swapchain
//! 10. Virtio Event Queue + Transport Backends

use aidl_compat::{
    BinderFeatures, IBinder,
    Parcel, Parcelable, ParcelableHolder, Remotable, RemoteTransport,
    Result as AidlResult, SpIBinder, Status, StatusCode,
    STATUS_FAILED_TRANSACTION, STATUS_UNKNOWN_TRANSACTION,
};
use binder_handle_bridge::HandleBridge;
use binder_routing::{
    DescriptorMatcher, MatchRule, MatcherEngine, RouteAction, RoutingPolicy,
    RoutingRule, ServiceNameMatcher,
};
use surfaceflinger_gpu_service::layer_translator::{ComposerState, LayerState, LayerTranslator};
use surfaceflinger_gpu_service::service::{isurfacecomposer_codes, DisplayInfo, SurfaceComposerService};
use tests_e2e_binder::harness::{create_test_wgpu_device, EchoService};
use virtio_binder::device::VirtioBinderDevice;
use virtio_binder::guest_shim::{DirectDeviceBackend, GuestVirtioTransport, TransportBackend, VirtqueueChainBackend};
use virtio_binder::protocol::*;
use webgpu_compositor::{CompositionLayer, WebGpuCompositor};
use webgpu_swapchain::WebGpuSwapchain;
use std::sync::Arc;

// =============================================================================
// AIDL Interface Definition for Pairwise Tests
// =============================================================================

pub trait ICalculator: aidl_compat::Interface + Send + Sync {
    fn add(&self, a: i32, b: i32) -> AidlResult<i32>;
    fn multiply(&self, a: i32, b: i32) -> AidlResult<i32>;
}

pub struct CalculatorService;

impl aidl_compat::Interface for CalculatorService {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::new(EchoService::new())
    }
}

impl ICalculator for CalculatorService {
    fn add(&self, a: i32, b: i32) -> AidlResult<i32> {
        Ok(a + b)
    }

    fn multiply(&self, a: i32, b: i32) -> AidlResult<i32> {
        Ok(a * b)
    }
}

fn calc_on_transact(
    service: &dyn ICalculator,
    code: aidl_compat::TransactionCode,
    data: &aidl_compat::Parcel,
    reply: &mut aidl_compat::Parcel,
) -> aidl_compat::Result<()> {
    match code {
        1 => {
            let mut off = 0;
            let a = data.read_i32(&mut off).map_err(|_| Status::from_status(STATUS_FAILED_TRANSACTION))?;
            let b = data.read_i32(&mut off).map_err(|_| Status::from_status(STATUS_FAILED_TRANSACTION))?;
            match service.add(a, b) {
                Ok(res) => {
                    reply.write_status(&Status::ok()).unwrap();
                    reply.write_i32(res).unwrap();
                    Ok(())
                }
                Err(err) => {
                    reply.write_status(&err).unwrap();
                    Ok(())
                }
            }
        }
        2 => {
            let mut off = 0;
            let a = data.read_i32(&mut off).map_err(|_| Status::from_status(STATUS_FAILED_TRANSACTION))?;
            let b = data.read_i32(&mut off).map_err(|_| Status::from_status(STATUS_FAILED_TRANSACTION))?;
            match service.multiply(a, b) {
                Ok(res) => {
                    reply.write_status(&Status::ok()).unwrap();
                    reply.write_i32(res).unwrap();
                    Ok(())
                }
                Err(err) => {
                    reply.write_status(&err).unwrap();
                    Ok(())
                }
            }
        }
        _ => Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION)),
    }
}

aidl_compat::declare_binder_interface! {
    ICalculator ["android.os.ICalculator"] {
        native: BnCalculator(calc_on_transact),
        proxy: BpCalculator,
    }
}

impl ICalculator for BpCalculator {
    fn add(&self, a: i32, b: i32) -> AidlResult<i32> {
        let mut data = Parcel::new();
        data.write_i32(a).unwrap();
        data.write_i32(b).unwrap();
        let mut reply = Parcel::new();
        aidl_compat::Interface::as_binder(self).transact(1, 0, &data, &mut reply)?;
        let mut off = 0;
        let status = reply.read_status(&mut off).map_err(|_| Status::from_status(STATUS_FAILED_TRANSACTION))?;
        if !status.is_ok() {
            return Err(status);
        }
        reply.read_i32(&mut off).map_err(|_| Status::from_status(STATUS_FAILED_TRANSACTION))
    }

    fn multiply(&self, a: i32, b: i32) -> AidlResult<i32> {
        let mut data = Parcel::new();
        data.write_i32(a).unwrap();
        data.write_i32(b).unwrap();
        let mut reply = Parcel::new();
        aidl_compat::Interface::as_binder(self).transact(2, 0, &data, &mut reply)?;
        let mut off = 0;
        let status = reply.read_status(&mut off).map_err(|_| Status::from_status(STATUS_FAILED_TRANSACTION))?;
        if !status.is_ok() {
            return Err(status);
        }
        reply.read_i32(&mut off).map_err(|_| Status::from_status(STATUS_FAILED_TRANSACTION))
    }
}

// =============================================================================
// Subsystem Pair 1: Virtio-Binder + Handle Bridge (Tests 1..3)
// =============================================================================

#[test]
fn test_pairwise_virtio_and_handle_bridge_cross_client_call() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h1 = bridge.register_service(1, "android.os.IEchoService", Arc::clone(&echo));

    // Transfer handle from Client 1 to Client 2
    let h2 = bridge.transfer_handle(1, 2, h1).unwrap();
    assert!(bridge.get_service(1, h1).is_some());
    assert!(bridge.get_service(2, h2).is_some());

    // Client 2 uses Virtio transport hooked to the bridge service
    let device = Arc::new(VirtioBinderDevice::new());
    let client2_service = bridge.get_service(2, h2).unwrap();
    device.register_service(h2, client2_service);

    let transport = GuestVirtioTransport::new_with_device(device);
    let mut data = Parcel::new();
    data.write_i32(100).unwrap();
    data.write_i32(200).unwrap();
    let mut reply = Parcel::new();

    transport.transact(h2, EchoService::TRANSACTION_ADD, 0, &data, &mut reply).unwrap();
    let mut off = 0;
    let status = reply.read_status(&mut off).unwrap();
    assert!(status.is_ok());
    assert_eq!(reply.read_i32(&mut off).unwrap(), 300);
}

#[test]
fn test_pairwise_virtio_and_handle_bridge_multi_client_isolation() {
    let bridge = HandleBridge::new();
    let echo1: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let echo2: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let echo3: Arc<dyn IBinder> = Arc::new(EchoService::new());

    let h_c1 = bridge.register_service(1, "desc1", echo1);
    let h_c2_1 = bridge.register_service(2, "desc2_1", echo2);
    let h_c2_2 = bridge.register_service(2, "desc2_2", echo3);

    // Verify handle isolation: Client 1 only has 1 handle, so handle 2 does not exist for Client 1
    assert_eq!(h_c1, 1);
    assert_eq!(h_c2_1, 1);
    assert_eq!(h_c2_2, 2);

    assert!(bridge.get_service(1, 2).is_none());
    assert!(bridge.get_service(2, 2).is_some());
    assert_eq!(bridge.get_descriptor(1, 1), Some("desc1".to_string()));
    assert_eq!(bridge.get_descriptor(2, 1), Some("desc2_1".to_string()));
}

#[test]
fn test_pairwise_virtio_and_handle_bridge_death_cleanup() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h = bridge.register_service(1, "desc", echo);
    bridge.register_death_recipient(1, h, 0xCAFE_BABE).unwrap();

    let device = Arc::new(VirtioBinderDevice::new());
    // Client 1 dies: Bridge reclaims resources and yields death events
    let death_events = bridge.on_client_died(1);
    assert_eq!(death_events.len(), 1);
    assert_eq!(death_events[0].1, 0xCAFE_BABE);

    // Push to Virtio device event queue for guest notification
    for (handle, cookie) in death_events {
        device.event_queue().lock().unwrap().push_event(VirtioBinderEventHdr::new_death(handle, cookie));
    }

    let drained = device.event_queue().lock().unwrap().drain_events();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].cookie, 0xCAFE_BABE);
}

// =============================================================================
// Subsystem Pair 2: Binder Routing + SurfaceFlinger GPU Service (Tests 4..6)
// =============================================================================

#[test]
fn test_pairwise_routing_and_surfaceflinger_offload_composer() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let mut policy = RoutingPolicy::new_default_local();
        policy.allow_host_offload("android.gui.ISurfaceComposer");

        let route = policy.route("android.gui.ISurfaceComposer", isurfacecomposer_codes::GET_DISPLAY_INFO);
        assert_eq!(route, RouteAction::HostOffload);

        let composer_service = SurfaceComposerService::new(device, queue, 1920, 1080);
        let data = Parcel::new();
        let mut reply = Parcel::new();

        composer_service.on_transact(isurfacecomposer_codes::GET_DISPLAY_INFO, &data, &mut reply).unwrap();
        let mut off = 0;
        let status = reply.read_status(&mut off).unwrap();
        assert!(status.is_ok());
        let mut info = DisplayInfo::default();
        info.read_from_parcel_at(&reply, &mut off).unwrap();
        assert_eq!(info.width, 1920);
        assert_eq!(info.height, 1080);
    });
}

#[test]
fn test_pairwise_routing_and_surfaceflinger_create_surface_offload() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let mut policy = RoutingPolicy::new_default_local();
        policy.allow_host_offload("android.gui.ISurfaceComposer");

        let route = policy.route("android.gui.ISurfaceComposer", isurfacecomposer_codes::CREATE_SURFACE);
        assert_eq!(route, RouteAction::HostOffload);

        let composer_service = SurfaceComposerService::new(device, queue, 1024, 768);
        let mut data = Parcel::new();
        data.write_utf8(Some("MainWindow")).unwrap();
        data.write_u32(1024).unwrap();
        data.write_u32(768).unwrap();
        data.write_i32(1).unwrap(); // format
        let mut reply = Parcel::new();

        composer_service.on_transact(isurfacecomposer_codes::CREATE_SURFACE, &data, &mut reply).unwrap();
        let mut off = 0;
        let status = reply.read_status(&mut off).unwrap();
        assert!(status.is_ok());
        let surface_id = reply.read_u64(&mut off).unwrap();
        assert_eq!(surface_id, 1);
    });
}

#[test]
fn test_pairwise_routing_and_surfaceflinger_local_guest_fallback() {
    let policy = RoutingPolicy::new_default_local();
    // Default is LocalGuest
    let route = policy.route("android.os.ICustomService", 1);
    assert_eq!(route, RouteAction::LocalGuest);
}

// =============================================================================
// Subsystem Pair 3: AIDL Macros + Virtio Transport (Tests 7..8)
// =============================================================================

#[test]
fn test_pairwise_aidl_macros_and_virtio_transport_proxy_dispatch() {
    let device = Arc::new(VirtioBinderDevice::new());
    let calc = BnCalculator::new_binder(CalculatorService, BinderFeatures::default());
    device.register_service(1, calc.into_arc());

    let transport = Arc::new(GuestVirtioTransport::new_with_device(device));
    let remote = aidl_compat::stub::RemoteBinder::new_raw_with_transport(1, 0, Some("android.os.ICalculator"), transport);
    let proxy = BpCalculator::new(SpIBinder::new(remote));

    let sum = proxy.add(15, 27).unwrap();
    assert_eq!(sum, 42);

    let product = proxy.multiply(6, 7).unwrap();
    assert_eq!(product, 42);
}

#[test]
fn test_pairwise_aidl_macros_and_virtio_transport_one_way_dispatch() {
    let device = Arc::new(VirtioBinderDevice::new());
    let echo = Arc::new(EchoService::new());
    device.register_service(1, echo);

    let transport = Arc::new(GuestVirtioTransport::new_with_device(device));
    let remote = aidl_compat::stub::RemoteBinder::new_raw_with_transport(1, 0, None, transport);

    let mut data = Parcel::new();
    data.write_i32(50).unwrap();
    data.write_i32(50).unwrap();
    let mut reply = Parcel::new();

    // TF_ONE_WAY transaction
    let res = remote.transact(EchoService::TRANSACTION_ADD, 1 /* TF_ONE_WAY */, &data, &mut reply);
    assert!(res.is_ok());
}

// =============================================================================
// Subsystem Pair 4: Parcel Wire Codec + Flat Binder Objects / FDs (Tests 9..10)
// =============================================================================

#[test]
fn test_pairwise_parcel_wire_codec_and_flat_binder_handles() {
    let mut p = Parcel::new();
    p.write_i32(1001).unwrap();
    p.write_binder(42, 0x8888).unwrap();
    p.write_i32(2002).unwrap();

    assert_eq!(p.offsets().len(), 1);
    assert_eq!(p.offsets()[0], 4); // After i32 (4 bytes)

    let mut off = 0;
    assert_eq!(p.read_i32(&mut off).unwrap(), 1001);
    let obj = p.read_binder_object(&mut off).unwrap();
    assert_eq!(obj.handle(), 42);
    assert_eq!(obj.cookie, 0x8888);
    assert_eq!(p.read_i32(&mut off).unwrap(), 2002);
}

#[test]
fn test_pairwise_parcel_wire_codec_and_file_descriptors() {
    let mut p = Parcel::new();
    p.write_file_descriptor(10, 0x1111).unwrap();
    p.write_file_descriptor(20, 0x2222).unwrap();

    assert_eq!(p.offsets().len(), 2);
    let mut off = 0;
    assert_eq!(p.read_file_descriptor(&mut off).unwrap(), 10);
    assert_eq!(p.read_file_descriptor(&mut off).unwrap(), 20);
}

// =============================================================================
// Subsystem Pair 5: WebGPU Compositor + Swapchain (Tests 11..12)
// =============================================================================

#[test]
fn test_pairwise_webgpu_compositor_and_swapchain_render() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let mut compositor = WebGpuCompositor::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let mut swapchain = WebGpuSwapchain::new(&device, 256, 256, wgpu::TextureFormat::Rgba8Unorm);

        let bg = CompositionLayer::new_color(1, "Background", [-1.0, -1.0, 2.0, 2.0], 0, [0.1, 0.2, 0.3, 1.0]);
        let fg = CompositionLayer::new_color(2, "Foreground", [-0.5, -0.5, 1.0, 1.0], 1, [0.9, 0.1, 0.1, 0.8]);

        compositor.add_or_update_layer(bg);
        compositor.add_or_update_layer(fg);

        let target_view = swapchain.get_current_texture_view();
        compositor.compose(&device, &queue, target_view, None);
        swapchain.present();
    });
}

#[test]
fn test_pairwise_webgpu_compositor_and_layer_translator() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let mut state = LayerState::new(10, "TranslatedLayer");
        state.set_bounds_pixels([100.0, 100.0, 200.0, 200.0]);
        state.set_alpha(0.85);
        state.set_z_order(5);

        let comp_layer = LayerTranslator::translate_to_composition_layer(&state, 800, 600, None);
        assert_eq!(comp_layer.id, 10);
        assert_eq!(comp_layer.z_order, 5);

        let mut compositor = WebGpuCompositor::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        compositor.add_or_update_layer(comp_layer);

        let swapchain = WebGpuSwapchain::new(&device, 800, 600, wgpu::TextureFormat::Rgba8Unorm);
        let target_view = swapchain.get_current_texture_view();
        compositor.compose(&device, &queue, target_view, None);
    });
}

// =============================================================================
// Subsystem Pair 6: Handle Bridge + Death Recipients Cascade (Tests 13..14)
// =============================================================================

#[test]
fn test_pairwise_handle_bridge_and_death_recipients_cascade() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h1 = bridge.register_service(1, "desc", echo);

    // Client 1 passes to Client 2
    let h2 = bridge.transfer_handle(1, 2, h1).unwrap();
    // Client 2 passes to Client 3
    let h3 = bridge.transfer_handle(2, 3, h2).unwrap();

    // Client 3 links death recipient on h3
    bridge.register_death_recipient(3, h3, 0x3333).unwrap();

    // Client 3 dies -> death recipient dispatched
    let events = bridge.on_client_died(3);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1, 0x3333);

    // Client 1 & 2 handles remain valid
    assert!(bridge.get_service(1, h1).is_some());
    assert!(bridge.get_service(2, h2).is_some());
}

#[test]
fn test_pairwise_handle_bridge_and_refcount_lifecycle() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h1 = bridge.register_service(1, "desc", echo);
    let h2 = bridge.transfer_handle(1, 2, h1).unwrap();

    // Increment references on client 2
    bridge.acquire_ref(2, h2, 3).unwrap();
    assert_eq!(bridge.get_strong_count(2, h2), Some(4));

    // Release all client 2 references
    let dropped2 = bridge.release_ref(2, h2, 4).unwrap();
    assert!(dropped2);
    assert!(bridge.get_service(2, h2).is_none());

    // Client 1 is still alive and holding 1 ref
    assert!(bridge.get_service(1, h1).is_some());
    let dropped1 = bridge.release_ref(1, h1, 1).unwrap();
    assert!(dropped1);
    assert!(bridge.get_service(1, h1).is_none());
}

// =============================================================================
// Subsystem Pair 7: Matcher Engine + Routing Policy Hierarchy (Tests 15..16)
// =============================================================================

#[test]
fn test_pairwise_matcher_engine_and_routing_policy_hierarchical() {
    let mut engine = MatcherEngine::new(RouteAction::LocalGuest);

    // Host offload for SurfaceFlinger ISurfaceComposer
    engine.add_rule(
        MatchRule::new(
            DescriptorMatcher::Prefix("android.gui.".to_string()),
            RouteAction::HostOffload,
        )
        .with_service(ServiceNameMatcher::Exact("SurfaceFlinger".to_string())),
    );

    // Local guest for ISurfaceComposerClient
    engine.add_rule(
        MatchRule::new(
            DescriptorMatcher::Exact("android.gui.ISurfaceComposerClient".to_string()),
            RouteAction::LocalGuest,
        )
        .with_priority(10),
    );

    assert_eq!(
        engine.match_transaction(Some("SurfaceFlinger"), Some("android.gui.ISurfaceComposer"), 1),
        RouteAction::HostOffload
    );
    assert_eq!(
        engine.match_transaction(Some("SurfaceFlinger"), Some("android.gui.ISurfaceComposerClient"), 1),
        RouteAction::LocalGuest
    );
    assert_eq!(
        engine.match_transaction(Some("OtherService"), Some("android.gui.ISurfaceComposer"), 1),
        RouteAction::LocalGuest
    );
}

#[test]
fn test_pairwise_matcher_engine_and_routing_policy_priority_override() {
    let mut policy = RoutingPolicy::new_default_local();
    // Wildcard low priority rule
    policy.add_rule(RoutingRule::new("android.gui.*", RouteAction::HostOffload).with_priority(1));
    // Exact high priority override
    policy.add_rule(RoutingRule::new("android.gui.ILocalService", RouteAction::LocalGuest).with_priority(100));

    assert_eq!(policy.route("android.gui.ISurfaceComposer", 1), RouteAction::HostOffload);
    assert_eq!(policy.route("android.gui.ILocalService", 1), RouteAction::LocalGuest);
}

// =============================================================================
// Subsystem Pair 8: AIDL Status + Error Propagation (Tests 17..18)
// =============================================================================

#[test]
fn test_pairwise_aidl_status_and_virtio_error_propagation_service_specific() {
    struct FailingCalc;
    impl aidl_compat::Interface for FailingCalc {
        fn as_binder(&self) -> SpIBinder {
            SpIBinder::new(EchoService::new())
        }
    }
    impl ICalculator for FailingCalc {
        fn add(&self, _a: i32, _b: i32) -> AidlResult<i32> {
            Err(Status::new_service_specific_error(404, Some("Calculation not found")))
        }
        fn multiply(&self, _a: i32, _b: i32) -> AidlResult<i32> {
            Ok(0)
        }
    }

    let device = Arc::new(VirtioBinderDevice::new());
    let calc = BnCalculator::new_binder(FailingCalc, BinderFeatures::default());
    device.register_service(1, calc.into_arc());

    let transport = Arc::new(GuestVirtioTransport::new_with_device(device));
    let remote = aidl_compat::stub::RemoteBinder::new_raw_with_transport(1, 0, None, transport);
    let proxy = BpCalculator::new(SpIBinder::new(remote));

    let err = proxy.add(1, 2).unwrap_err();
    assert_eq!(err.service_specific_error(), Some(404));
    assert_eq!(err.message(), Some("Calculation not found"));
}

#[test]
fn test_pairwise_aidl_status_and_virtio_error_propagation_remote_dead() {
    let device = Arc::new(VirtioBinderDevice::new());
    // No service registered at handle 9999
    let transport = Arc::new(GuestVirtioTransport::new_with_device(device));
    let remote = aidl_compat::stub::RemoteBinder::new_raw_with_transport(9999, 0, None, transport);
    let proxy = BpCalculator::new(SpIBinder::new(remote));

    let err = proxy.add(1, 2).unwrap_err();
    assert_eq!(err.status, StatusCode::DeadObject);
}

// =============================================================================
// Subsystem Pair 9: SurfaceFlinger Service + Layer State + Swapchain (Tests 19..20)
// =============================================================================

#[test]
fn test_pairwise_surfaceflinger_service_and_webgpu_swapchain() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 640, 480);
        let handle = svc.create_surface("TestSurface", 640, 480, 0).unwrap();
        assert_eq!(handle.surface_id, 1);

        // Submit transaction state
        let mut layer_state = LayerState::new(1, "TestSurface");
        layer_state.set_alpha(1.0);
        layer_state.set_z_order(0);
        let comp_state = ComposerState::new(1, layer_state);

        let mut data = Parcel::new();
        data.write_i32(1).unwrap(); // count = 1
        comp_state.write_to_parcel(&mut data).unwrap();
        let mut reply = Parcel::new();

        svc.on_transact(isurfacecomposer_codes::SET_TRANSACTION_STATE, &data, &mut reply).unwrap();
        let mut off = 0;
        let status = reply.read_status(&mut off).unwrap();
        assert!(status.is_ok());
    });
}

#[test]
fn test_pairwise_virtio_event_queue_and_direct_backend() {
    let device = Arc::new(VirtioBinderDevice::new());
    let backend = DirectDeviceBackend::new(Arc::clone(&device));

    device.event_queue().lock().unwrap().push_event(VirtioBinderEventHdr::new_death(42, 0x1234));
    let drained = backend.drain_events();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].target_handle, 42);
    assert_eq!(drained[0].cookie, 0x1234);
}

// =============================================================================
// Subsystem Pair 10: Virtio Event Queue + Virtqueue Backend (Tests 21..22)
// =============================================================================

#[test]
fn test_pairwise_virtio_event_queue_and_virtqueue_backend() {
    let device = Arc::new(VirtioBinderDevice::new());
    let backend = VirtqueueChainBackend::new(Arc::clone(&device), 512);

    for i in 1..=5 {
        device.event_queue().lock().unwrap().push_event(VirtioBinderEventHdr::new_death(i, i as u64 * 100));
    }

    let drained = backend.drain_events();
    assert_eq!(drained.len(), 5);
    for (i, item) in drained.iter().enumerate() {
        assert_eq!(item.target_handle, (i + 1) as u32);
        assert_eq!(item.cookie, (i + 1) as u64 * 100);
    }
}

#[test]
fn test_pairwise_spibinder_downgrade_and_virtio_transport() {
    let device = Arc::new(VirtioBinderDevice::new());
    device.register_service(1, Arc::new(EchoService::new()));
    let transport = Arc::new(GuestVirtioTransport::new_with_device(device));

    let remote = aidl_compat::stub::RemoteBinder::new_raw_with_transport(1, 0, None, transport);
    let sp = SpIBinder::new(remote);
    let wp = sp.downgrade();

    let upgraded = wp.upgrade().unwrap();
    let mut data = Parcel::new();
    data.write_i32(11).unwrap();
    data.write_i32(22).unwrap();
    let mut reply = Parcel::new();

    upgraded.transact(EchoService::TRANSACTION_ADD, 0, &data, &mut reply).unwrap();
    let mut off = 0;
    let status = reply.read_status(&mut off).unwrap();
    assert!(status.is_ok());
    assert_eq!(reply.read_i32(&mut off).unwrap(), 33);
}

// =============================================================================
// Subsystem Pair 11: ParcelableHolder + Wire Transfer (Tests 23..24)
// =============================================================================

#[test]
fn test_pairwise_parcelable_holder_and_parcel_wire_transfer() {
    let mut holder = ParcelableHolder::new(1);
    holder.set_parcelable(&4242i32, "CustomValue").unwrap();

    let mut p = Parcel::new();
    holder.write_to_parcel(&mut p).unwrap();

    let mut decoded_holder = ParcelableHolder::new(0);
    decoded_holder.read_from_parcel(&p).unwrap();

    assert_eq!(decoded_holder.get_parcelable_name(), Some("CustomValue"));
    assert_eq!(decoded_holder.get_parcelable::<i32>().unwrap(), Some(4242));
}

#[test]
fn test_pairwise_composer_state_batch_update_and_translation() {
    let mut states = Vec::new();
    for i in 1..=5 {
        let mut ls = LayerState::new(i, &format!("Layer_{}", i));
        ls.set_bounds_pixels([i as f32 * 10.0, i as f32 * 10.0, 100.0, 100.0]);
        ls.set_alpha(0.5);
        ls.set_z_order(i as i32);
        states.push(ComposerState::new(i, ls));
    }

    assert_eq!(states.len(), 5);

    let mut p = Parcel::new();
    for cs in &states {
        cs.write_to_parcel(&mut p).unwrap();
    }

    let mut off = 0;
    for (i, _expected_cs) in states.iter().enumerate() {
        let mut decoded = ComposerState::new(0, LayerState::default());
        decoded.read_from_parcel_at(&p, &mut off).unwrap();
        assert_eq!(decoded.surface_id, (i + 1) as u64);
    }
}

// =============================================================================
// Subsystem Pair 12: Selective Routing Mixed Workload & Teardown (Tests 25..28)
// =============================================================================

#[test]
fn test_pairwise_selective_routing_mixed_workload() {
    let mut policy = RoutingPolicy::new_default_local();
    policy.allow_host_offload("android.gui.ISurfaceComposer");
    policy.allow_host_offload("android.gui.IGraphicBufferProducer");

    // Local guest routes
    assert_eq!(policy.route("android.os.IServiceManager", 1), RouteAction::LocalGuest);
    assert_eq!(policy.route("android.app.IActivityManager", 1), RouteAction::LocalGuest);
    // Host offload routes
    assert_eq!(policy.route("android.gui.ISurfaceComposer", 1), RouteAction::HostOffload);
    assert_eq!(policy.route("android.gui.IGraphicBufferProducer", 1), RouteAction::HostOffload);
}

#[test]
fn test_pairwise_virtio_transport_reconnect_and_handle_rebind() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h = bridge.register_service(1, "desc", Arc::clone(&echo));

    // Device 1
    let dev1 = Arc::new(VirtioBinderDevice::new());
    dev1.register_service(h, bridge.get_service(1, h).unwrap());
    let t1 = GuestVirtioTransport::new_with_device(dev1);
    assert!(t1.ping(h).is_ok());

    // Rebind to Device 2
    let dev2 = Arc::new(VirtioBinderDevice::new());
    dev2.register_service(h, bridge.get_service(1, h).unwrap());
    let t2 = GuestVirtioTransport::new_with_device(dev2);
    assert!(t2.ping(h).is_ok());
}

#[test]
fn test_pairwise_layer_translator_crop_and_scaling() {
    let mut state = LayerState::new(1, "CropLayer");
    state.set_bounds_pixels([100.0, 100.0, 400.0, 300.0]);
    state.set_source_crop([0.0, 0.0, 1920.0, 1080.0]);

    let comp_layer = LayerTranslator::translate_to_composition_layer(&state, 1920, 1080, None);
    assert_eq!(comp_layer.id, 1);
    assert_eq!(comp_layer.source_crop, [0.0, 0.0, 1920.0, 1080.0]);
}

#[test]
fn test_pairwise_surfaceflinger_boot_finished_and_first_frame() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 1280, 720);
        assert!(!svc.is_boot_finished());

        // Boot finished signal
        let data = Parcel::new();
        let mut reply = Parcel::new();
        svc.on_transact(isurfacecomposer_codes::BOOT_FINISHED, &data, &mut reply).unwrap();
        assert!(svc.is_boot_finished());

        // Create surface & submit frame
        let handle = svc.create_surface("FirstFrameSurface", 1280, 720, 0).unwrap();
        assert_eq!(handle.surface_id, 1);
    });
}
