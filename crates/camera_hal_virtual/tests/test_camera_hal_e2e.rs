//! End-to-End Integration Test for Virtual Camera HAL and ServiceManager Registration.

use aidl_compat::pointer::Strong;
use aidl_compat::stub::Binder;
use aidl_compat::traits::Interface;
use binder_sys::{
    BinderKernelTransport, IPCThreadState, IServiceManager, MockBinderDriver,
    MockServiceManager, ProcessState, ServiceManagerClient, DUMP_FLAG_PRIORITY_DEFAULT,
    SERVICE_MANAGER_DESCRIPTOR,
};
use camera_hal_virtual::{
    BufferStatus, CameraProviderProxy, CameraProviderService, CaptureRequest,
    ICameraDeviceCallback, ICameraProvider, MockCameraDeviceCallback, PixelFormat,
    RequestTemplate, Stream, StreamBuffer, StreamConfiguration, StreamRotation, StreamType,
    ICAMERA_PROVIDER_VIRTUAL_INSTANCE,
};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_camera_provider_proxy_and_device_lifecycle() {
    let provider_service = Arc::new(CameraProviderService::new());
    let provider_proxy = CameraProviderProxy::new(provider_service.as_binder());

    // 1. Enumerate Camera IDs
    let id_list = provider_proxy.get_camera_id_list().expect("Failed to get camera ID list");
    assert_eq!(id_list, vec!["device@1.0/virtual/0".to_string()]);

    // 2. Obtain Camera Device Interface
    let device_proxy = provider_proxy
        .get_camera_device_interface("device@1.0/virtual/0")
        .expect("Failed to obtain camera device interface");

    // 3. Query Characteristics Metadata
    let characteristics = device_proxy
        .get_camera_characteristics()
        .expect("Failed to query characteristics");
    assert_eq!(characteristics.get("android.lens.facing"), Some("1"));
    assert_eq!(characteristics.get("android.sensor.orientation"), Some("90"));
    assert!(characteristics.get("android.scaler.availableStreamConfigurations").is_some());

    // 4. Test Torch Mode
    device_proxy.set_torch_mode(true).expect("Failed to enable torch");
    device_proxy.turn_on_torch_with_strength_level(5).expect("Failed to set torch level");
    assert_eq!(device_proxy.get_torch_strength_level().unwrap(), 5);
    device_proxy.set_torch_mode(false).expect("Failed to disable torch");

    // 5. Open Session with Callback
    let callback = Arc::new(MockCameraDeviceCallback::new());
    let session_strong = device_proxy
        .open(Strong::new(Arc::clone(&callback) as Arc<dyn ICameraDeviceCallback>))
        .expect("Failed to open camera session");

    // 6. Configure Streams
    let stream_config = StreamConfiguration {
        streams: vec![
            Stream {
                id: 1,
                stream_type: StreamType::Output,
                width: 1280,
                height: 720,
                format: PixelFormat::Yuv420888,
                usage: 0x3,
                data_space: 0,
                rotation: StreamRotation::Rotation0,
                physical_camera_id: String::new(),
                buffer_size: 1280 * 720 * 3 / 2,
            },
            Stream {
                id: 2,
                stream_type: StreamType::Output,
                width: 640,
                height: 480,
                format: PixelFormat::Rgba8888,
                usage: 0x3,
                data_space: 0,
                rotation: StreamRotation::Rotation0,
                physical_camera_id: String::new(),
                buffer_size: 640 * 480 * 4,
            },
        ],
        operation_mode: 0,
    };

    let hal_config = session_strong
        .configure_streams(&stream_config)
        .expect("Failed to configure streams");
    assert_eq!(hal_config.streams.len(), 2);
    assert_eq!(hal_config.streams[0].id, 1);
    assert_eq!(hal_config.streams[1].id, 2);

    // 7. Construct Request Settings
    let settings = session_strong
        .construct_default_request_settings(RequestTemplate::Preview)
        .expect("Failed to construct settings");
    assert_eq!(settings.get("android.control.aeTargetFpsRange"), Some("[30, 30]"));

    // 8. Submit Multi-Buffer Capture Request
    let request = CaptureRequest {
        frame_number: 42,
        fmq_settings_size: 0,
        settings,
        input_buffer: None,
        output_buffers: vec![
            StreamBuffer {
                stream_id: 1,
                buffer_id: 201,
                buffer_data: Vec::new(),
                status: BufferStatus::Ok,
            },
            StreamBuffer {
                stream_id: 2,
                buffer_id: 202,
                buffer_data: Vec::new(),
                status: BufferStatus::Ok,
            },
        ],
    };

    let processed = session_strong
        .process_capture_request(&[request])
        .expect("Failed to process capture request");
    assert_eq!(processed, 1);

    // Verify Callback Delivery
    let results = callback.get_results();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].frame_number, 42);
    assert_eq!(results[0].output_buffers.len(), 2);
    assert_eq!(results[0].output_buffers[0].buffer_data.len(), 1280 * 720 * 3 / 2);
    assert_eq!(results[0].output_buffers[1].buffer_data.len(), 640 * 480 * 4);

    let notifs = callback.get_notifications();
    assert_eq!(notifs.len(), 1);

    // 9. Close Session
    session_strong.close().expect("Failed to close session");
}

#[test]
fn test_camera_provider_registration_with_mock_servicemanager() {
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

    // 2. Server Process: verify VINTF declaration before registering
    assert!(vintf_validator::is_declared(ICAMERA_PROVIDER_VIRTUAL_INSTANCE));

    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let sm_client_server = ServiceManagerClient::with_binder(
        aidl_compat::stub::RemoteBinder::new_with_transport(
            0,
            0,
            Some(SERVICE_MANAGER_DESCRIPTOR),
            Arc::new(BinderKernelTransport::with_process(Arc::clone(&server_ps))),
        ),
    );

    let provider = Arc::new(CameraProviderService::new());
    let provider_binder = provider.as_binder();

    sm_client_server
        .add_service(
            ICAMERA_PROVIDER_VIRTUAL_INSTANCE,
            provider_binder,
            false,
            DUMP_FLAG_PRIORITY_DEFAULT,
        )
        .expect("Failed to register Camera Provider service with ServiceManager");

    // 3. Lookup service from ServiceManager
    let looked_up = sm_client_server
        .get_service(ICAMERA_PROVIDER_VIRTUAL_INSTANCE)
        .expect("get_service failed")
        .expect("Camera Provider service not found in ServiceManager");

    assert_eq!(looked_up.handle(), Some(0));
}
