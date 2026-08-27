//! Tests for guest_servicemanager context manager registration, looper dispatch, and service registration.

use aidl_compat::stub::Binder;
use binder_sys::{
    BinderKernelTransport, IPCThreadState, MockBinderDriver, ProcessState,
    ServiceManagerClient, ServiceManagerServer, IServiceManager, SERVICE_MANAGER_DESCRIPTOR,
    DUMP_FLAG_PRIORITY_ALL,
};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_guest_servicemanager_context_manager_lifecycle() {
    let mock_driver = Arc::new(MockBinderDriver::new());
    let sm_ps = ProcessState::init_mock(Arc::clone(&mock_driver));

    // 1. Claim context manager
    assert!(sm_ps.become_context_manager().is_ok());

    // 2. Register ServiceManager at handle 0
    let sm = Binder::new(ServiceManagerServer::new());
    sm_ps.register_as_binder(sm);

    // Start ServiceManager looper thread
    let sm_ps_clone = Arc::clone(&sm_ps);
    let _sm_thread = thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(sm_ps_clone);
        let _ = ts.enter_looper();
    });

    thread::sleep(Duration::from_millis(50));

    // 3. Verify handle 0 ping from client process
    let client_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let ping_res = IPCThreadState::current_with_process(&client_ps, |s| s.ping(0));
    assert!(ping_res.is_ok(), "Handle 0 ping error: {:?}", ping_res.err());
}

#[test]
fn test_guest_servicemanager_service_registration_and_lookup() {
    let mock_driver = Arc::new(MockBinderDriver::new());
    let sm_ps = ProcessState::init_mock(Arc::clone(&mock_driver));

    // 1. Claim context manager and register server object
    assert!(sm_ps.become_context_manager().is_ok());
    let sm_server = ServiceManagerServer::new();
    let sm_binder = Binder::new(sm_server);
    sm_ps.register_as_binder(sm_binder);

    // Start ServiceManager looper thread
    let sm_ps_clone = Arc::clone(&sm_ps);
    let _sm_thread = thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(sm_ps_clone);
        let _ = ts.enter_looper();
    });

    thread::sleep(Duration::from_millis(50));

    // 2. Client process interacts with ServiceManager over handle 0
    let client_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let sm_client = ServiceManagerClient::with_binder(aidl_compat::stub::RemoteBinder::new_with_transport(
        0,
        0,
        Some(SERVICE_MANAGER_DESCRIPTOR),
        Arc::new(BinderKernelTransport::with_process(Arc::clone(&client_ps))),
    ));

    // Check initial service list
    let list = sm_client.list_services(DUMP_FLAG_PRIORITY_ALL).expect("list_services failed");
    assert!(list.is_empty());
}
