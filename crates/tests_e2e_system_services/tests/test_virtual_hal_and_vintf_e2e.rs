//! Virtual HAL and VINTF Manifest End-to-End Subsystem Integration Tests.

use aidl_compat::pointer::Strong;
use aidl_compat::traits::Interface;
use audio_hal_virtual::{
    AudioFormat, AudioModuleProxy, IModule, IStreamIn, IStreamOut, OpenInputStreamArguments,
    OpenOutputStreamArguments, StreamInProxy, StreamOutProxy,
};
use audio_host_rs::MicPattern;
use camera_hal_virtual::{
    BufferStatus, CaptureRequest, ICameraDevice, ICameraDeviceCallback, MockCameraDeviceCallback,
    PixelFormat, RequestTemplate, Stream, StreamBuffer, StreamConfiguration, StreamRotation,
    StreamType, ICAMERA_PROVIDER_VIRTUAL_INSTANCE,
};
use camera_host_rs::{CameraBufferPool, CameraHostBridge, FramePattern};
use media_host_rs::{
    BitstreamParser, BufferInfo, IMediaCodecService, MediaCodecServiceProxy, MediaFormat,
    BUFFER_FLAG_KEY_FRAME,
};
use sensors_hal_virtual::{
    Event, ISensors, OperationMode, SensorType, SensorsProxy, ISENSORS_DEFAULT_INSTANCE,
};
use std::sync::Arc;
use tests_e2e_system_services::SystemServicesHarness;
use vintf_validator::{is_declared, load_default_manifest};

#[test]
fn test_vintf_target_level_7_manifest_declarations() {
    let manifest = load_default_manifest().expect("Default VINTF manifest must load");
    manifest
        .validate_target_level(7)
        .expect("Target level must match Android 13 API level 33 / Level 7");

    // All required HALs
    assert!(is_declared(ISENSORS_DEFAULT_INSTANCE));
    assert!(is_declared("android.hardware.sensors.ISensors/default"));
    assert!(is_declared("android.hardware.audio.core.IModule/default"));
    assert!(is_declared("android.hardware.audio.core.IConfig/default"));
    assert!(is_declared(ICAMERA_PROVIDER_VIRTUAL_INSTANCE));
    assert!(is_declared(
        "android.hardware.camera.provider.ICameraProvider/virtual/0"
    ));

    // Undeclared / non-existent HALs must fail
    assert!(!is_declared("android.hardware.nfc.INfc/default"));
    assert!(!is_declared(
        "android.hardware.biometrics.fingerprint.IFingerprint/default"
    ));
    assert!(!is_declared(
        "android.hardware.camera.provider.ICameraProvider/legacy/0"
    ));
}

#[test]
fn test_virtual_sensors_hal_full_pipeline() {
    let harness = SystemServicesHarness::new();
    let proxy = SensorsProxy::new(Interface::as_binder(&*harness.sensors_service));

    let list = proxy.get_sensors_list().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].sensor_handle, 1);
    assert_eq!(list[0].sensor_type, SensorType::Accelerometer);
    assert_eq!(list[1].sensor_handle, 2);
    assert_eq!(list[1].sensor_type, SensorType::Gyroscope);

    // Batch and activate accelerometer at 100Hz (10ms)
    proxy.batch(1, 10_000_000, 0).unwrap();
    proxy.activate(1, true).unwrap();

    // Stream host motion ticks through SensorHostBridge
    for i in 1..=5 {
        harness.sensor_bridge.tick(i * 10_000_000);
    }
    let polled = harness.sensors_service.poll_events(10);
    assert!(!polled.is_empty());
    assert!(polled.iter().any(|e| e.sensor_handle == 1));

    // Data injection mode
    proxy.set_operation_mode(OperationMode::DataInjection).unwrap();
    let inj = Event::new_accelerometer(1, 1_234_567, 0.5, 9.80, -0.1);
    proxy.inject_sensor_data(&inj).unwrap();
    let polled_inj = harness.sensors_service.poll_events(5);
    assert_eq!(polled_inj.len(), 1);
    assert_eq!(polled_inj[0].timestamp, 1_234_567);
    assert!((polled_inj[0].y() - 9.80).abs() < 1e-3);

    // Flush FIFO
    proxy.flush(1).unwrap();
    proxy.activate(1, false).unwrap();
}

#[test]
fn test_virtual_audio_hal_full_pipeline() {
    let harness = SystemServicesHarness::new();
    let proxy = AudioModuleProxy::new(Interface::as_binder(&*harness.audio_service));

    // Master volume & mute
    proxy.set_master_volume(0.7).unwrap();
    assert!((proxy.get_master_volume().unwrap() - 0.7).abs() < 1e-4);
    proxy.set_master_mute(false).unwrap();
    assert!(!proxy.get_master_mute().unwrap());

    // Playback output stream
    let out_args = OpenOutputStreamArguments {
        port_config_id: 10,
        buffer_size_frames: 480,
        sample_rate: 48000,
        channel_mask: 2,
        format: AudioFormat::Pcm16Bit,
    };
    let out_res = proxy.open_output_stream(&out_args).unwrap();
    let out_proxy = StreamOutProxy::new(out_res.stream.unwrap());

    let test_sample: i16 = 3000;
    let mut pcm_out = Vec::with_capacity(1920);
    for _ in 0..480 {
        pcm_out.extend(&test_sample.to_le_bytes());
        pcm_out.extend(&test_sample.to_le_bytes());
    }
    let written = out_proxy.write(&pcm_out, 480).unwrap();
    assert_eq!(written, 480);

    harness.audio_bridge.process_output_pcm(&pcm_out);
    let mut readback = vec![0u8; 1920];
    let host_bytes = harness.audio_bridge.read_playback_pcm(&mut readback);
    assert_eq!(host_bytes, 1920);
    let scaled = i16::from_le_bytes([readback[0], readback[1]]);
    assert_eq!(scaled, (3000.0 * 0.7) as i16);

    // Capture input stream (microphone)
    let in_args = OpenInputStreamArguments {
        port_config_id: 11,
        buffer_size_frames: 480,
        sample_rate: 48000,
        channel_mask: 1,
        format: AudioFormat::Pcm16Bit,
    };
    let in_res = proxy.open_input_stream(&in_args).unwrap();
    let in_proxy = StreamInProxy::new(in_res.stream.unwrap());

    harness
        .audio_bridge
        .mic_source()
        .set_pattern(MicPattern::SineTone(440.0));
    harness.audio_bridge.pump_mic_input(in_res.stream_id, 480);

    let mut mic_buf = vec![0u8; 960];
    let read_mic = in_proxy.read(&mut mic_buf, 480).unwrap();
    assert_eq!(read_mic, 480);
    assert!(mic_buf.iter().any(|&b| b != 0));
}

#[test]
fn test_virtual_camera_and_mediacodec_decode_pipeline() {
    let harness = SystemServicesHarness::new();

    // 1. Camera Characteristics
    let chars = harness
        .camera_device
        .get_camera_characteristics()
        .unwrap();
    assert_eq!(chars.get("android.lens.facing"), Some("1"));

    // 2. Camera Session & Stream Setup
    let callback = Arc::new(MockCameraDeviceCallback::new());
    let cam_session = harness
        .camera_device
        .open(Strong::new(
            Arc::clone(&callback) as Arc<dyn ICameraDeviceCallback>
        ))
        .unwrap();

    let stream_cfg = StreamConfiguration {
        streams: vec![Stream {
            id: 1,
            stream_type: StreamType::Output,
            width: 640,
            height: 480,
            format: PixelFormat::Yuv420888,
            usage: 0x3,
            data_space: 0,
            rotation: StreamRotation::Rotation0,
            physical_camera_id: String::new(),
            buffer_size: 640 * 480 * 3 / 2,
        }],
        operation_mode: 0,
    };
    cam_session.configure_streams(&stream_cfg).unwrap();

    let active_cam = harness.camera_device.get_active_session().unwrap();
    let host_cam = CameraHostBridge::new(active_cam);
    host_cam.set_pattern(FramePattern::ColorBars);
    host_cam.tick_frame(1, 640, 480, PixelFormat::Yuv420888);

    let cam_settings = cam_session
        .construct_default_request_settings(RequestTemplate::Preview)
        .unwrap();

    let req = CaptureRequest {
        frame_number: 1,
        fmq_settings_size: 0,
        settings: cam_settings,
        input_buffer: None,
        output_buffers: vec![StreamBuffer {
            stream_id: 1,
            buffer_id: 101,
            buffer_data: Vec::new(),
            status: BufferStatus::Ok,
        }],
    };
    cam_session.process_capture_request(&[req]).unwrap();

    let cam_results = callback.get_results();
    assert_eq!(cam_results.len(), 1);
    let yuv_frame = &cam_results[0].output_buffers[0].buffer_data;
    assert_eq!(yuv_frame.len(), 640 * 480 * 3 / 2);

    // 3. MediaCodec H.264 Video Decoder Pipeline
    let media_proxy = MediaCodecServiceProxy::new(Interface::as_binder(&*harness.media_service));
    let codec_list = media_proxy.get_codec_list().unwrap();
    assert!(codec_list.iter().any(|c| c.name == "c2.webcodecs.avc.decoder"));

    let decoder = media_proxy
        .create_codec_by_name("c2.webcodecs.avc.decoder")
        .unwrap();
    let mut fmt = MediaFormat::new_video_format("video/avc", 640, 480);
    fmt.set_string("color-format", "yuv420p");
    decoder.configure(&fmt, None, 0).unwrap();
    decoder.start().unwrap();

    let in_idx = decoder.dequeue_input_buffer(0).unwrap();
    assert!(in_idx >= 0);

    let mut h264_bitstream = vec![0x00, 0x00, 0x00, 0x01, 0x65, 0x88, 0x84, 0x00];
    h264_bitstream.resize(1024, 0xbb);
    assert!(BitstreamParser::is_h264_keyframe(&h264_bitstream));

    decoder
        .set_input_buffer(in_idx as u32, &h264_bitstream)
        .unwrap();
    decoder
        .queue_input_buffer(
            in_idx as u32,
            0,
            h264_bitstream.len() as u32,
            16_666,
            BUFFER_FLAG_KEY_FRAME,
        )
        .unwrap();

    let mut out_info = BufferInfo::default();
    let out_idx = decoder.dequeue_output_buffer(&mut out_info, 0).unwrap();
    assert!(out_idx >= 0);
    assert_eq!(out_info.presentation_time_us, 16_666);
    assert_eq!(out_info.size, 640 * 480 * 3 / 2);

    let decoded_buf = decoder.get_output_buffer(out_idx as u32).unwrap();
    assert_eq!(decoded_buf.len(), 640 * 480 * 3 / 2);

    decoder
        .release_output_buffer(out_idx as u32, true, 16_666_000)
        .unwrap();
    decoder.stop().unwrap();
    decoder.release().unwrap();
}

#[test]
fn test_camera_buffer_pool_concurrency() {
    let pool = CameraBufferPool::new(4);
    let mut handles = Vec::new();

    for _ in 0..4 {
        let b = pool.acquire(1280, 720, PixelFormat::Yuv420888);
        assert_eq!(b.data.len(), 1280 * 720 * 3 / 2);
        handles.push(b);
    }
    assert_eq!(pool.get_stats().in_flight, 4);

    for b in handles {
        pool.release(b);
    }
    assert_eq!(pool.get_stats().in_flight, 0);
    assert_eq!(pool.get_stats().pooled_available, 4);
}
