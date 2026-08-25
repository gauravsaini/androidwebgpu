//! End-to-end Integration Test for Virtual Sensors HAL and ServiceManager Registration.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::{IBinder, Interface};
use aidl_compat::stub::Binder;
use binder_sys::{
    BinderKernelTransport, IPCThreadState, IServiceManager, MockBinderDriver,
    MockServiceManager, ProcessState, ServiceManagerClient, DUMP_FLAG_PRIORITY_DEFAULT,
    SERVICE_MANAGER_DESCRIPTOR,
};
use sensors_hal_virtual::{
    Event, ISensors, OperationMode, SensorType, SensorsHalService, SensorsProxy,
    ISENSORS_DEFAULT_INSTANCE,
};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_sensors_hal_proxy_query_and_streaming() {
    let service = Arc::new(SensorsHalService::new());
    let proxy = SensorsProxy::new(service.as_binder());

    // 1. Enumerate sensors
    let sensors = proxy.get_sensors_list().expect("Failed to get sensors list");
    assert_eq!(sensors.len(), 2);
    let accel = &sensors[0];
    let gyro = &sensors[1];
    assert_eq!(accel.sensor_handle, 1);
    assert_eq!(accel.sensor_type, SensorType::Accelerometer);
    assert_eq!(gyro.sensor_handle, 2);
    assert_eq!(gyro.sensor_type, SensorType::Gyroscope);

    // 2. Batch and Activate
    proxy.batch(1, 20_000_000, 0).expect("Batch failed");
    proxy.activate(1, true).expect("Activate failed");
    assert!(service.is_sensor_active(1));

    // 3. Flush FIFO
    proxy.flush(1).expect("Flush failed");
    let flush_events = service.poll_events(5);
    assert_eq!(flush_events.len(), 1);
    assert_eq!(flush_events[0].sensor_handle, 1);

    // 4. Data Injection mode
    proxy.set_operation_mode(OperationMode::DataInjection).unwrap();
    let inject_ev = Event::new_accelerometer(1, 2_000_000_000, 0.05, 9.81, -0.02);
    proxy.inject_sensor_data(&inject_ev).unwrap();

    let polled = service.poll_events(5);
    assert_eq!(polled.len(), 1);
    assert_eq!(polled[0].timestamp, 2_000_000_000);
    assert!((polled[0].y() - 9.81).abs() < 1e-4);

    // 5. Deactivate
    proxy.activate(1, false).unwrap();
    assert!(!service.is_sensor_active(1));
}

#[test]
fn test_sensors_service_registration_with_mock_servicemanager() {
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

    // 2. Server Process: register "android.hardware.sensors.ISensors/default" service
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let sm_client_server = ServiceManagerClient::with_binder(
        aidl_compat::stub::RemoteBinder::new_with_transport(
            0,
            0,
            Some(SERVICE_MANAGER_DESCRIPTOR),
            Arc::new(BinderKernelTransport::with_process(Arc::clone(&server_ps))),
        ),
    );

    let service = Arc::new(SensorsHalService::new());
    let service_binder = SpIBinder::from_arc(Arc::clone(&service) as Arc<dyn IBinder>);

    sm_client_server
        .add_service(
            ISENSORS_DEFAULT_INSTANCE,
            service_binder,
            false,
            DUMP_FLAG_PRIORITY_DEFAULT,
        )
        .expect("Failed to register sensors service with ServiceManager");

    // 3. Lookup service from ServiceManager
    let looked_up = sm_client_server
        .get_service(ISENSORS_DEFAULT_INSTANCE)
        .expect("get_service failed")
        .expect("Sensors service not found in ServiceManager");

    assert_eq!(looked_up.handle(), Some(0));
}
