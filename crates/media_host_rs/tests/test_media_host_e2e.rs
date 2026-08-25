//! End-to-End Integration Test for Framework MediaCodec Service and WebCodecs Bridge.

use aidl_compat::stub::Binder;
use aidl_compat::traits::Interface;
use binder_sys::{
    BinderKernelTransport, IPCThreadState, IServiceManager, MockBinderDriver,
    MockServiceManager, ProcessState, ServiceManagerClient, DUMP_FLAG_PRIORITY_DEFAULT,
    SERVICE_MANAGER_DESCRIPTOR,
};
use media_host_rs::{
    BitstreamParser, BufferInfo, IMediaCodecService, MediaCodecService,
    MediaCodecServiceProxy, MediaFormat, BUFFER_FLAG_CODEC_CONFIG, BUFFER_FLAG_KEY_FRAME,
    MEDIA_CODEC_SERVICE_NAME,
};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_media_codec_full_lifecycle_and_decoding() {
    let service = Arc::new(MediaCodecService::new());
    let proxy = MediaCodecServiceProxy::new(service.as_binder());

    // 1. Enumerate codecs
    let codec_list = proxy.get_codec_list().expect("Failed to get codec list");
    assert!(codec_list.len() >= 4);

    // 2. Create AVC Decoder instance
    let decoder = proxy
        .create_codec_by_type("video/avc", false)
        .expect("Failed to create AVC decoder");

    // 3. Configure
    let mut format = MediaFormat::new_video_format("video/avc", 1920, 1080);
    format.set_string("color-format", "yuv420p");
    decoder.configure(&format, None, 0).expect("Configure failed");

    // 4. Start
    decoder.start().expect("Start failed");

    // 5. Send SPS/PPS Codec Config Packet
    let config_nalu = vec![0x00, 0x00, 0x00, 0x01, 0x67, 0x42, 0x00, 0x28, 0x00, 0x00, 0x00, 0x01, 0x68, 0xce, 0x3c, 0x80];
    let in_slot_0 = decoder.dequeue_input_buffer(0).unwrap() as u32;
    decoder.set_input_buffer(in_slot_0, &config_nalu).unwrap();
    decoder.queue_input_buffer(in_slot_0, 0, config_nalu.len() as u32, 0, BUFFER_FLAG_CODEC_CONFIG).unwrap();

    // 6. Send IDR Keyframe Packet
    let mut idr_nalu = vec![0x00, 0x00, 0x00, 0x01, 0x65, 0x88, 0x84, 0x00];
    idr_nalu.resize(2048, 0x55);
    let in_slot_1 = decoder.dequeue_input_buffer(0).unwrap() as u32;
    decoder.set_input_buffer(in_slot_1, &idr_nalu).unwrap();
    decoder.queue_input_buffer(in_slot_1, 0, idr_nalu.len() as u32, 33_333, BUFFER_FLAG_KEY_FRAME).unwrap();

    // 7. Dequeue Decoded Output Buffer
    let mut out_info = BufferInfo::default();
    let out_slot = decoder.dequeue_output_buffer(&mut out_info, 0).expect("Failed to dequeue output");
    assert!(out_slot >= 0);
    assert_eq!(out_info.presentation_time_us, 33_333);
    assert_eq!(out_info.size, 1920 * 1080 * 3 / 2);
    assert!(out_info.is_key_frame());

    // 8. Release Output Buffer
    decoder.release_output_buffer(out_slot as u32, true, 33_333_000).expect("Release output failed");

    // 9. Flush and Reset
    decoder.flush().expect("Flush failed");
    decoder.reset().expect("Reset failed");
    decoder.release().expect("Release failed");
}

#[test]
fn test_h264_and_h265_bitstream_parsing() {
    // H.264 Annex B Stream (SPS, PPS, IDR Slice, Non-IDR Slice)
    let h264_stream = vec![
        0x00, 0x00, 0x00, 0x01, 0x67, 0x64, 0x00, 0x1f, // SPS
        0x00, 0x00, 0x00, 0x01, 0x68, 0xeb, 0xe3, 0xcb, // PPS
        0x00, 0x00, 0x01, 0x65, 0x88, 0x84, 0x00,       // IDR Slice
        0x00, 0x00, 0x01, 0x41, 0x9a,                   // Non-IDR Slice
    ];

    let nalus = BitstreamParser::find_annex_b_nalus(&h264_stream);
    assert_eq!(nalus.len(), 4);
    assert!(BitstreamParser::is_h264_keyframe(&h264_stream));

    // H.265 Annex B Stream (VPS, SPS, PPS, IDR_W_RADL)
    let h265_stream = vec![
        0x00, 0x00, 0x00, 0x01, 0x40, 0x01, 0x0c, // VPS (NAL unit type 32 -> 0x40 >> 1 = 32)
        0x00, 0x00, 0x00, 0x01, 0x42, 0x01, 0x01, // SPS (NAL unit type 33 -> 0x42 >> 1 = 33)
        0x00, 0x00, 0x00, 0x01, 0x44, 0x01, 0xc0, // PPS (NAL unit type 34 -> 0x44 >> 1 = 34)
        0x00, 0x00, 0x00, 0x01, 0x26, 0x01, 0xaf, // IDR_W_RADL (NAL unit type 19 -> 0x26 >> 1 = 19)
    ];

    let h265_nalus = BitstreamParser::find_annex_b_nalus(&h265_stream);
    assert_eq!(h265_nalus.len(), 4);
    assert!(BitstreamParser::is_h265_keyframe(&h265_stream));
}

#[test]
fn test_media_codec_service_registration_with_mock_servicemanager() {
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

    // 2. Server Process: register "media.codec"
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let sm_client_server = ServiceManagerClient::with_binder(
        aidl_compat::stub::RemoteBinder::new_with_transport(
            0,
            0,
            Some(SERVICE_MANAGER_DESCRIPTOR),
            Arc::new(BinderKernelTransport::with_process(Arc::clone(&server_ps))),
        ),
    );

    let service = Arc::new(MediaCodecService::new());
    let service_binder = service.as_binder();

    sm_client_server
        .add_service(
            MEDIA_CODEC_SERVICE_NAME,
            service_binder,
            false,
            DUMP_FLAG_PRIORITY_DEFAULT,
        )
        .expect("Failed to register media.codec service with ServiceManager");

    // 3. Lookup service from ServiceManager
    let looked_up = sm_client_server
        .get_service(MEDIA_CODEC_SERVICE_NAME)
        .expect("get_service failed")
        .expect("media.codec service not found in ServiceManager");

    assert_eq!(looked_up.handle(), Some(0));
}
