//! ServiceManager client and registration tests over handle 0.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{Result, Status, STATUS_BAD_VALUE};
use aidl_compat::stub::Binder;
use aidl_compat::traits::{Interface, Remotable};
use binder_rt::types::TransactionCode;
use binder_rt::Parcel;
use binder_sys::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// Sample greeting service
struct GreeterService {
    calls: AtomicU32,
}

impl Interface for GreeterService {
    fn as_binder(&self) -> SpIBinder {
        Binder::new(Self {
            calls: AtomicU32::new(0),
        })
    }
}

impl Remotable for GreeterService {
    fn get_class_descriptor() -> &'static str {
        "android.test.IGreeterService"
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut offset = 0;
        let _ = data.read_utf16(&mut offset);

        match code {
            1 => {
                let name = data.read_utf8(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let greeting = format!("Hello, {}!", name.unwrap_or_default());
                reply.write_utf8(Some(&greeting)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => Err(Status::from_status(aidl_compat::status::STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

#[test]
fn test_service_manager_registration_and_lookup() {
    let mock_driver = Arc::new(MockBinderDriver::new());

    // 1. Setup ServiceManager process as Context Manager (Handle 0)
    let sm_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let sm_stub = MockServiceManager::new();
    let sm_cookie = 0x534D; // "SM"
    sm_ps.register_service_object(sm_cookie, Binder::new(sm_stub));
    mock_driver.set_context_manager(sm_ps.pid(), 0, sm_cookie);

    // Start ServiceManager looper
    let sm_ps_clone = Arc::clone(&sm_ps);
    let _sm_thread = thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(sm_ps_clone);
        let _ = ts.enter_looper();
    });

    // 2. Server Process: registers "greeter" service
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let greeter = Arc::new(GreeterService {
        calls: AtomicU32::new(0),
    });
    let greeter_cookie = 0x6772; // "gr"
    let greeter_binder = Binder::new_with_arc(Arc::clone(&greeter));
    server_ps.register_service_object(greeter_cookie, greeter_binder.clone());

    // Start Server looper
    let server_ps_clone = Arc::clone(&server_ps);
    let _server_thread = thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(server_ps_clone);
        let _ = ts.enter_looper();
    });

    thread::sleep(Duration::from_millis(50));

    // Register service with ServiceManager from server process
    let sm_client_server = ServiceManagerClient::with_binder(aidl_compat::stub::RemoteBinder::new_with_transport(
        0,
        0,
        Some(SERVICE_MANAGER_DESCRIPTOR),
        Arc::new(BinderKernelTransport::with_process(Arc::clone(&server_ps))),
    ));

    sm_client_server
        .add_service("android.test.IGreeterService", greeter_binder, false, DUMP_FLAG_PRIORITY_DEFAULT)
        .expect("add_service failed");

    // 3. Client Process: looks up "android.test.IGreeterService" via ServiceManager
    let client_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let sm_client = ServiceManagerClient::with_binder(aidl_compat::stub::RemoteBinder::new_with_transport(
        0,
        0,
        Some(SERVICE_MANAGER_DESCRIPTOR),
        Arc::new(BinderKernelTransport::with_process(Arc::clone(&client_ps))),
    ));

    let looked_up = sm_client.get_service("android.test.IGreeterService").expect("get_service failed");
    assert!(looked_up.is_some(), "Looked up service should exist");

    // In mock driver environment, register handle to greeter in client handle table
    let greeter_handle = mock_driver.add_handle_for_client(
        &mock_driver.get_client(client_ps.pid()).unwrap(),
        server_ps.pid(),
        0,
        greeter_cookie,
    );
    let greeter_proxy = aidl_compat::stub::RemoteBinder::new_with_transport(
        greeter_handle,
        0,
        Some("android.test.IGreeterService"),
        Arc::new(BinderKernelTransport::with_process(Arc::clone(&client_ps))),
    );

    // Call greeting method on service
    let mut data = Parcel::new();
    data.write_utf16(Some("android.test.IGreeterService")).unwrap();
    data.write_utf8(Some("AndroidWebGPU")).unwrap();

    let mut reply = Parcel::new();
    greeter_proxy.transact(1, 0, &data, &mut reply).expect("Transact on greeter failed");

    let mut offset = 0;
    let greeting = reply.read_utf8(&mut offset).expect("Read greeting failed").unwrap();
    assert_eq!(greeting, "Hello, AndroidWebGPU!");
    assert_eq!(greeter.calls.load(Ordering::SeqCst), 1);

    // Verify list_services and is_declared
    let services_list = sm_client_server.list_services(DUMP_FLAG_PRIORITY_ALL).expect("list_services failed");
    assert!(services_list.contains(&"android.test.IGreeterService".to_string()));

    let declared = sm_client_server.is_declared("android.test.IGreeterService").expect("is_declared failed");
    assert!(declared);
}
