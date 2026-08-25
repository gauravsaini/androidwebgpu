//! # camera_host_rs
//!
//! Host-Side WebRTC/webcam stream bridge and zero-copy shared memory buffer pipeline
//! for AndroidWebGPU Camera HAL.

pub mod bridge;
pub mod buffer_pool;
pub mod frame_generator;

pub use bridge::{CameraHostBridge, CameraHostMode};
pub use buffer_pool::{BufferPoolStats, CameraBufferPool, SharedBufferHandle};
pub use frame_generator::{CameraFrameGenerator, FramePattern};

#[cfg(test)]
mod tests {
    use super::*;
    use camera_hal_virtual::{
        BufferStatus, CameraDeviceService, CaptureRequest, ICameraDevice, ICameraDeviceCallback,
        MockCameraDeviceCallback, PixelFormat, RequestTemplate, Stream, StreamBuffer,
        StreamConfiguration, StreamRotation, StreamType,
    };
    use aidl_compat::pointer::Strong;
    use std::sync::Arc;

    #[test]
    fn test_buffer_pool_recycling_and_stats() {
        let pool = CameraBufferPool::new(4);
        let b1 = pool.acquire(1280, 720, PixelFormat::Yuv420888);
        assert_eq!(b1.data.len(), 1280 * 720 * 3 / 2);
        assert_eq!(pool.get_stats().in_flight, 1);

        let b2 = pool.acquire(1280, 720, PixelFormat::Yuv420888);
        assert_eq!(pool.get_stats().in_flight, 2);

        pool.release(b1);
        assert_eq!(pool.get_stats().in_flight, 1);
        assert_eq!(pool.get_stats().pooled_available, 1);

        // Next acquire reuses pooled allocation
        let b3 = pool.acquire(1280, 720, PixelFormat::Yuv420888);
        assert_eq!(pool.get_stats().in_flight, 2);
        assert_eq!(pool.get_stats().pooled_available, 0);

        pool.release(b2);
        pool.release(b3);
        assert_eq!(pool.get_stats().in_flight, 0);
        assert_eq!(pool.get_stats().pooled_available, 2);
    }

    #[test]
    fn test_camera_host_bridge_synthetic_frame_pump_and_delivery() {
        let device = Arc::new(CameraDeviceService::new("device@1.0/virtual/0"));
        let callback = Arc::new(MockCameraDeviceCallback::new());

        let session_strong = device
            .open(Strong::new(Arc::clone(&callback) as Arc<dyn ICameraDeviceCallback>))
            .expect("Open session failed");

        let session_service = device.get_active_session().expect("Session service must exist");
        let bridge = CameraHostBridge::new(Arc::clone(&session_service));

        // Configure stream
        let stream_config = StreamConfiguration {
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
        session_strong.configure_streams(&stream_config).unwrap();

        // Tick a synthetic ColorBars frame
        bridge.set_pattern(FramePattern::ColorBars);
        let seq = bridge.tick_frame(1, 640, 480, PixelFormat::Yuv420888);
        assert_eq!(seq, 1);

        // Process capture request
        let settings = session_strong
            .construct_default_request_settings(RequestTemplate::Preview)
            .unwrap();
        let req = CaptureRequest {
            frame_number: 1,
            fmq_settings_size: 0,
            settings,
            input_buffer: None,
            output_buffers: vec![StreamBuffer {
                stream_id: 1,
                buffer_id: 101,
                buffer_data: Vec::new(),
                status: BufferStatus::Ok,
            }],
        };

        let processed = session_strong.process_capture_request(&[req]).unwrap();
        assert_eq!(processed, 1);

        let results = callback.get_results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].output_buffers[0].buffer_data.len(), 640 * 480 * 3 / 2);
        // Verify genuine color bar content (not all zeros or 0x80)
        assert!(results[0].output_buffers[0].buffer_data.iter().any(|&b| b == 235 || b == 210 || b == 170));
    }

    #[test]
    fn test_camera_host_bridge_webrtc_injection() {
        let device = Arc::new(CameraDeviceService::new("device@1.0/virtual/0"));
        let callback = Arc::new(MockCameraDeviceCallback::new());
        let _session_strong = device
            .open(Strong::new(Arc::clone(&callback) as Arc<dyn ICameraDeviceCallback>))
            .unwrap();
        let session_service = device.get_active_session().unwrap();
        let bridge = CameraHostBridge::new(Arc::clone(&session_service));

        let mock_rgba = vec![0xfe; 640 * 480 * 4];
        let injected = bridge.inject_frame(2, 640, 480, PixelFormat::Rgba8888, &mock_rgba);
        assert!(injected);
    }
}
