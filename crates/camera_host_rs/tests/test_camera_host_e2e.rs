//! End-to-End Integration Test for Host Camera Stream Bridge and Buffer Pipeline.

use aidl_compat::pointer::Strong;
use camera_hal_virtual::{
    BufferStatus, CameraDeviceService, CaptureRequest, ICameraDevice, ICameraDeviceCallback,
    MockCameraDeviceCallback, PixelFormat, RequestTemplate, Stream, StreamBuffer,
    StreamConfiguration, StreamRotation, StreamType,
};
use camera_host_rs::{
    CameraBufferPool, CameraFrameGenerator, CameraHostBridge, CameraHostMode, FramePattern,
};
use std::sync::Arc;

#[test]
fn test_buffer_pool_multi_resolution_lifecycle() {
    let pool = CameraBufferPool::new(8);

    // 1. Acquire 720p YUV buffer
    let b720 = pool.acquire(1280, 720, PixelFormat::Yuv420888);
    assert_eq!(b720.data.len(), 1280 * 720 * 3 / 2);
    assert_eq!(pool.get_stats().in_flight, 1);

    // 2. Acquire 480p RGBA buffer
    let b480 = pool.acquire(640, 480, PixelFormat::Rgba8888);
    assert_eq!(b480.data.len(), 640 * 480 * 4);
    assert_eq!(pool.get_stats().in_flight, 2);

    // 3. Release both
    pool.release(b720);
    pool.release(b480);
    assert_eq!(pool.get_stats().in_flight, 0);
    assert_eq!(pool.get_stats().pooled_available, 2);

    // 4. Re-acquire matches resolution and format
    let b720_again = pool.acquire(1280, 720, PixelFormat::Yuv420888);
    assert_eq!(b720_again.data.len(), 1280 * 720 * 3 / 2);
    pool.release(b720_again);
}

#[test]
fn test_all_frame_generator_patterns_and_formats() {
    let patterns = [
        FramePattern::ColorBars,
        FramePattern::Checkerboard,
        FramePattern::SolidRgb(100, 150, 200),
        FramePattern::Gradient,
    ];

    for pattern in patterns {
        let mut gen = CameraFrameGenerator::new(pattern);

        // RGBA test
        let mut rgba_buf = vec![0u8; 640 * 480 * 4];
        gen.generate_frame(640, 480, PixelFormat::Rgba8888, &mut rgba_buf);
        assert!(rgba_buf.iter().any(|&b| b != 0), "RGBA buffer must not be empty");

        // YUV test
        let mut yuv_buf = vec![0u8; 640 * 480 * 3 / 2];
        gen.generate_frame(640, 480, PixelFormat::Yuv420888, &mut yuv_buf);
        assert!(yuv_buf.iter().any(|&b| b != 0), "YUV buffer must not be empty");
    }
}

#[test]
fn test_camera_host_bridge_modes_and_pipelining() {
    let device = Arc::new(CameraDeviceService::new("device@1.0/virtual/0"));
    let callback = Arc::new(MockCameraDeviceCallback::new());

    let session_strong = device
        .open(Strong::new(Arc::clone(&callback) as Arc<dyn ICameraDeviceCallback>))
        .expect("Failed to open session");

    let session_service = device.get_active_session().expect("Active session must exist");
    let bridge = CameraHostBridge::new(Arc::clone(&session_service));

    // Configure 720p output stream
    let stream_config = StreamConfiguration {
        streams: vec![Stream {
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
        }],
        operation_mode: 0,
    };
    session_strong.configure_streams(&stream_config).unwrap();

    // Mode 1: Synthetic Generator (Checkerboard)
    bridge.set_mode(CameraHostMode::SyntheticGenerator);
    bridge.set_pattern(FramePattern::Checkerboard);
    let seq1 = bridge.tick_frame(1, 1280, 720, PixelFormat::Yuv420888);
    assert_eq!(seq1, 1);

    // Mode 2: WebRTC injection
    bridge.set_mode(CameraHostMode::WebRtcStream);
    let mock_webrtc_frame = vec![0xAB; 1280 * 720 * 3 / 2];
    let ok = bridge.inject_frame(1, 1280, 720, PixelFormat::Yuv420888, &mock_webrtc_frame);
    assert!(ok);

    // Submit capture request and verify injected data
    let settings = session_strong
        .construct_default_request_settings(RequestTemplate::Preview)
        .unwrap();
    let req = CaptureRequest {
        frame_number: 10,
        fmq_settings_size: 0,
        settings,
        input_buffer: None,
        output_buffers: vec![StreamBuffer {
            stream_id: 1,
            buffer_id: 501,
            buffer_data: Vec::new(),
            status: BufferStatus::Ok,
        }],
    };

    session_strong.process_capture_request(&[req]).unwrap();

    let results = callback.get_results();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].output_buffers[0].buffer_data[0], 0xAB);
}
