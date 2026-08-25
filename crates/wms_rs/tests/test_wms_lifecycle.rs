//! Comprehensive integration tests for Native Android Window Manager Service (WMS).

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::IBinder;
use input_channel::InputChannel;
use std::sync::Arc;
use wms_rs::{
    IWindowManager, IWindowSession, InsetsState, LayoutParams, SurfaceControl,
    SurfaceControlTransaction, WindowManagerProxy, WindowManagerService, WindowSessionProxy,
};

#[test]
fn test_multi_session_concurrency_and_isolation() {
    let wms = Arc::new(WindowManagerService::new());
    let wms_binder = SpIBinder::from_arc(Arc::clone(&wms) as Arc<dyn IBinder>);
    let wms_client = WindowManagerProxy::new(wms_binder);

    // App 1: Opens session 1
    let session1_binder = wms_client.open_session(None).expect("App 1 open session failed");
    let session1 = WindowSessionProxy::new(session1_binder);

    // App 2: Opens session 2
    let session2_binder = wms_client.open_session(None).expect("App 2 open session failed");
    let session2 = WindowSessionProxy::new(session2_binder);

    // App 1 adds "App1_Window"
    let mut attrs1 = LayoutParams::default();
    attrs1.title = "App1_Window".to_string();
    let mut insets1 = InsetsState::default();
    let mut channel1 = InputChannel::default();
    let add1 = session1.add_to_display(None, &attrs1, 0, 0, &mut insets1, &mut channel1).unwrap();
    assert_eq!(add1, 0);

    // App 2 adds "App2_Window"
    let mut attrs2 = LayoutParams::default();
    attrs2.title = "App2_Window".to_string();
    let mut insets2 = InsetsState::default();
    let mut channel2 = InputChannel::default();
    let add2 = session2.add_to_display(None, &attrs2, 0, 0, &mut insets2, &mut channel2).unwrap();
    assert_eq!(add2, 0);

    // App 1 relayouts
    let mut sc1 = SurfaceControl::default();
    let relayout1 = session1.relayout(None, &attrs1, 1080, 1920, 0, 0, &mut sc1).unwrap();
    assert_ne!(relayout1, 0);
    assert_eq!(sc1.name, "App1_Window");
    assert_eq!(sc1.width, 1080);
    assert_eq!(sc1.height, 1920);

    // App 2 relayouts
    let mut sc2 = SurfaceControl::default();
    let relayout2 = session2.relayout(None, &attrs2, 720, 1280, 0, 0, &mut sc2).unwrap();
    assert_ne!(relayout2, 0);
    assert_eq!(sc2.name, "App2_Window");
    assert_eq!(sc2.width, 720);
    assert_eq!(sc2.height, 1280);

    // Ensure surface IDs are distinct
    assert_ne!(sc1.layer_id, sc2.layer_id);

    // App 1 finishes drawing with postDrawTransaction
    let mut tx1 = SurfaceControlTransaction::new(sc1.layer_id);
    tx1.set_alpha(0.9).set_z_order(10);
    session1.finish_drawing(None, Some(&tx1)).unwrap();

    // App 2 finishes drawing without transaction
    session2.finish_drawing(None, None).unwrap();

    // Cleanup App 1
    session1.remove(None).unwrap();

    // App 2 is still active
    let mut sc2_new = SurfaceControl::default();
    let relayout2_again = session2.relayout(None, &attrs2, 720, 1280, 0, 0, &mut sc2_new).unwrap();
    assert_ne!(relayout2_again, 0);

    // Cleanup App 2
    session2.remove(None).unwrap();
}

#[test]
fn test_wms_service_registration() {
    use aidl_compat::stub::Binder;
    use binder_sys::{
        BinderKernelTransport, IPCThreadState, IServiceManager, MockBinderDriver,
        MockServiceManager, ProcessState, ServiceManagerClient, DUMP_FLAG_PRIORITY_DEFAULT,
        SERVICE_MANAGER_DESCRIPTOR,
    };
    use std::time::Duration;

    let mock_driver = Arc::new(MockBinderDriver::new());

    // 1. Setup ServiceManager process (Handle 0)
    let sm_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let sm_stub = MockServiceManager::new();
    let sm_cookie = 0x534D;
    sm_ps.register_service_object(sm_cookie, Binder::new(sm_stub));
    mock_driver.set_context_manager(sm_ps.pid(), 0, sm_cookie);

    let sm_ps_clone = Arc::clone(&sm_ps);
    let _sm_thread = std::thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(sm_ps_clone);
        let _ = ts.enter_looper();
    });

    std::thread::sleep(Duration::from_millis(30));

    // 2. Server Process: register "window" service
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let sm_client_server = ServiceManagerClient::with_binder(
        aidl_compat::stub::RemoteBinder::new_with_transport(
            0,
            0,
            Some(SERVICE_MANAGER_DESCRIPTOR),
            Arc::new(BinderKernelTransport::with_process(Arc::clone(&server_ps))),
        ),
    );

    let wms = Arc::new(WindowManagerService::new());
    let wms_binder = SpIBinder::from_arc(Arc::clone(&wms) as Arc<dyn IBinder>);

    sm_client_server
        .add_service("window", wms_binder, false, DUMP_FLAG_PRIORITY_DEFAULT)
        .expect("Failed to register window service with ServiceManager");

    // 3. Lookup service from ServiceManager
    let looked_up = sm_client_server
        .get_service("window")
        .expect("get_service failed")
        .expect("Service not found in ServiceManager");

    assert_eq!(looked_up.handle(), Some(0));
}
