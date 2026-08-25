//! # camera_hal_virtual
//!
//! Virtual Android 13 AIDL Camera HAL (`android.hardware.camera.provider.ICameraProvider`,
//! `ICameraDevice`, `ICameraDeviceSession`, `ICameraDeviceCallback`) for AndroidWebGPU.
//!
//! Provides virtual camera provider enumeration, device characteristics query (1280x720/640x480,
//! YUV420/RGBA8888, 30/60fps), stream configuration, frame capture request dispatching,
//! and VINTF validated registration under `"android.hardware.camera.provider.ICameraProvider/virtual/0"`.

pub mod camera_device;
pub mod camera_device_callback;
pub mod camera_device_session;
pub mod camera_provider;
pub mod error;
pub mod types;

// -----------------------------------------------------------------------------
// Top-Level Public Exports
// -----------------------------------------------------------------------------

pub use camera_device::{
    icamera_device_codes, CameraDeviceBinder, CameraDeviceProxy, CameraDeviceService,
    ICameraDevice, ICAMERA_DEVICE_DESCRIPTOR,
};
pub use camera_device_callback::{
    icamera_device_callback_codes, CameraDeviceCallbackProxy, ICameraDeviceCallback,
    MockCameraDeviceCallback, ICAMERA_DEVICE_CALLBACK_DESCRIPTOR,
};
pub use camera_device_session::{
    icamera_device_session_codes, CameraDeviceSessionBinder, CameraDeviceSessionProxy,
    CameraDeviceSessionService, ICameraDeviceSession, ICAMERA_DEVICE_SESSION_DESCRIPTOR,
};
pub use camera_provider::{
    icamera_provider_codes, register_camera_provider_service, CameraProviderBinder,
    CameraProviderProxy, CameraProviderService, ICameraProvider, ICAMERA_PROVIDER_DESCRIPTOR,
    ICAMERA_PROVIDER_VIRTUAL_INSTANCE,
};
pub use error::CameraHalError;
pub use types::{
    BufferStatus, CaptureRequest, CaptureResult, ErrorMsg, HalStream, HalStreamConfiguration,
    NotifyMsg, PixelFormat, RequestTemplate, ShutterMsg, Stream, StreamBuffer,
    StreamConfiguration, StreamRotation, StreamType, VendorTag, VendorTagSection, CameraMetadata,
};

#[cfg(test)]
mod tests {
    use super::*;
    use aidl_compat::pointer::Strong;
    use std::sync::Arc;

    #[test]
    fn test_camera_provider_enumeration_and_characteristics() {
        let provider = Arc::new(CameraProviderService::new());

        // 1. Get Camera ID list
        let id_list = provider.get_camera_id_list().expect("Get camera id list failed");
        assert_eq!(id_list, vec!["device@1.0/virtual/0".to_string()]);

        // 2. Get Camera Device Interface
        let device = provider
            .get_camera_device_interface("device@1.0/virtual/0")
            .expect("Get camera device failed");

        // 3. Query Camera Characteristics
        let chars = device.get_camera_characteristics().expect("Characteristics failed");
        assert_eq!(chars.get("android.lens.facing"), Some("1"));
        assert_eq!(chars.get("android.sensor.orientation"), Some("90"));
        assert!(chars.get("android.scaler.availableStreamConfigurations").is_some());
    }

    #[test]
    fn test_camera_session_stream_configuration_and_capture_pipeline() {
        let device = Arc::new(CameraDeviceService::new("device@1.0/virtual/0"));
        let callback = Arc::new(MockCameraDeviceCallback::new());

        // Open capture session
        let session = device
            .open(Strong::new(Arc::clone(&callback) as Arc<dyn ICameraDeviceCallback>))
            .expect("Open camera session failed");

        // Construct request settings
        let settings = session
            .construct_default_request_settings(RequestTemplate::Preview)
            .expect("Settings failed");
        assert_eq!(settings.get("android.control.aeTargetFpsRange"), Some("[30, 30]"));

        // Configure stream (1280x720 YUV420_888 Output)
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

        let hal_config = session
            .configure_streams(&stream_config)
            .expect("Configure streams failed");
        assert_eq!(hal_config.streams.len(), 1);
        assert_eq!(hal_config.streams[0].id, 1);
        assert_eq!(hal_config.streams[0].override_format, PixelFormat::Yuv420888);

        // Submit capture request
        let request = CaptureRequest {
            frame_number: 101,
            fmq_settings_size: 0,
            settings: settings.clone(),
            input_buffer: None,
            output_buffers: vec![StreamBuffer {
                stream_id: 1,
                buffer_id: 1001,
                buffer_data: Vec::new(),
                status: BufferStatus::Ok,
            }],
        };

        let processed = session
            .process_capture_request(&[request])
            .expect("Capture request processing failed");
        assert_eq!(processed, 1);

        // Verify callback received results and shutter notification
        let results = callback.get_results();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].frame_number, 101);
        assert_eq!(results[0].output_buffers.len(), 1);
        assert_eq!(results[0].output_buffers[0].stream_id, 1);
        assert_eq!(results[0].output_buffers[0].buffer_data.len(), 1280 * 720 * 3 / 2);

        let notifs = callback.get_notifications();
        assert_eq!(notifs.len(), 1);
        match &notifs[0] {
            NotifyMsg::Shutter(s) => {
                assert_eq!(s.frame_number, 101);
                assert!(s.timestamp_ns > 0);
            }
            _ => panic!("Expected Shutter notification"),
        }
    }

    #[test]
    fn test_vintf_registration_verification() {
        assert!(
            vintf_validator::is_declared(ICAMERA_PROVIDER_VIRTUAL_INSTANCE),
            "Camera provider virtual instance must be declared in VINTF manifest"
        );
        assert!(
            !vintf_validator::is_declared("android.hardware.camera.provider.ICameraProvider/unknown/99"),
            "Undeclared instance must fail is_declared check"
        );
    }
}
