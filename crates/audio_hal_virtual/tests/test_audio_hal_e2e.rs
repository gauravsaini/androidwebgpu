//! End-to-end Integration Test for Virtual Audio HAL and ServiceManager Registration.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::stub::Binder;
use aidl_compat::traits::{IBinder, Interface};
use audio_hal_virtual::{
    AudioFormat, AudioModuleProxy, AudioModuleService, IModule, IStreamIn, IStreamOut,
    OpenInputStreamArguments, OpenOutputStreamArguments, StreamInProxy, StreamOutProxy,
    IMODULE_DEFAULT_INSTANCE,
};
use binder_sys::{
    BinderKernelTransport, IPCThreadState, IServiceManager, MockBinderDriver,
    MockServiceManager, ProcessState, ServiceManagerClient, DUMP_FLAG_PRIORITY_DEFAULT,
    SERVICE_MANAGER_DESCRIPTOR,
};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_audio_hal_proxy_streams_and_controls() {
    let service = Arc::new(AudioModuleService::new());
    let proxy = AudioModuleProxy::new(service.as_binder());

    // 1. Check volume and mute
    assert_eq!(proxy.get_master_mute().unwrap(), false);
    assert_eq!(proxy.get_master_volume().unwrap(), 1.0);

    proxy.set_master_volume(0.8).unwrap();
    assert!((proxy.get_master_volume().unwrap() - 0.8).abs() < 1e-4);

    proxy.set_master_mute(true).unwrap();
    assert_eq!(proxy.get_master_mute().unwrap(), true);
    proxy.set_master_mute(false).unwrap();

    // 2. Open Output Stream via proxy
    let out_args = OpenOutputStreamArguments {
        port_config_id: 1,
        buffer_size_frames: 480,
        sample_rate: 48000,
        channel_mask: 2,
        format: AudioFormat::Pcm16Bit,
    };
    let out_res = proxy.open_output_stream(&out_args).unwrap();
    assert_eq!(out_res.sample_rate, 48000);
    assert_eq!(out_res.channel_count, 2);

    let stream_out = service.get_output_stream(out_res.stream_id).expect("Stream must exist");
    let stream_out_proxy = StreamOutProxy::new(stream_out.as_binder());

    assert_eq!(stream_out_proxy.get_sample_rate().unwrap(), 48000);
    assert_eq!(stream_out_proxy.get_channel_count().unwrap(), 2);
    assert_eq!(stream_out_proxy.get_buffer_size_frames().unwrap(), 480);

    // Write 480 frames of 16-bit stereo PCM (480 * 4 = 1920 bytes)
    let pcm_buffer = vec![0x55; 1920];
    let written_frames = stream_out_proxy.write(&pcm_buffer, 480).unwrap();
    assert_eq!(written_frames, 480);

    // 3. Open Input Stream via proxy
    let in_args = OpenInputStreamArguments {
        port_config_id: 2,
        buffer_size_frames: 480,
        sample_rate: 48000,
        channel_mask: 1,
        format: AudioFormat::Pcm16Bit,
    };
    let in_res = proxy.open_input_stream(&in_args).unwrap();
    assert_eq!(in_res.sample_rate, 48000);
    assert_eq!(in_res.channel_count, 1);

    let stream_in = service.get_input_stream(in_res.stream_id).expect("Stream must exist");
    let stream_in_proxy = StreamInProxy::new(stream_in.as_binder());

    assert_eq!(stream_in_proxy.get_sample_rate().unwrap(), 48000);
    assert_eq!(stream_in_proxy.get_channel_count().unwrap(), 1);

    // Feed mic data and read
    stream_in.feed_pcm(&[0x11, 0x22, 0x33, 0x44]);
    let mut in_buffer = vec![0u8; 4];
    let read_frames = stream_in_proxy.read(&mut in_buffer, 2).unwrap();
    assert_eq!(read_frames, 2);
    assert_eq!(in_buffer, vec![0x11, 0x22, 0x33, 0x44]);
}

#[test]
fn test_audio_service_registration_with_mock_servicemanager() {
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

    // 2. Server Process: register "android.hardware.audio.core.IModule/default" service
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let sm_client_server = ServiceManagerClient::with_binder(
        aidl_compat::stub::RemoteBinder::new_with_transport(
            0,
            0,
            Some(SERVICE_MANAGER_DESCRIPTOR),
            Arc::new(BinderKernelTransport::with_process(Arc::clone(&server_ps))),
        ),
    );

    let service = Arc::new(AudioModuleService::new());
    let service_binder = SpIBinder::from_arc(Arc::clone(&service) as Arc<dyn IBinder>);

    sm_client_server
        .add_service(
            IMODULE_DEFAULT_INSTANCE,
            service_binder,
            false,
            DUMP_FLAG_PRIORITY_DEFAULT,
        )
        .expect("Failed to register audio service with ServiceManager");

    // 3. Lookup service from ServiceManager
    let looked_up = sm_client_server
        .get_service(IMODULE_DEFAULT_INSTANCE)
        .expect("get_service failed")
        .expect("Audio service not found in ServiceManager");

    assert_eq!(looked_up.handle(), Some(0));
}
