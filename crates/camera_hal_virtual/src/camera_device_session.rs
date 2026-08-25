//! AIDL `ICameraDeviceSession` Interface, Remotable Server Stub, and Client Proxy.

use crate::camera_device_callback::ICameraDeviceCallback;
use crate::error::CameraHalError;
use crate::types::*;
use aidl_compat::pointer::{SpIBinder, Strong};
use aidl_compat::status::{Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_UNKNOWN_TRANSACTION};
use aidl_compat::traits::{IBinder, Interface, Parcelable, Proxy, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock};

pub const ICAMERA_DEVICE_SESSION_DESCRIPTOR: &str =
    "android.hardware.camera.device.ICameraDeviceSession";

pub mod icamera_device_session_codes {
    use super::FIRST_CALL_TRANSACTION;
    pub const CONSTRUCT_DEFAULT_REQUEST_SETTINGS: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const CONFIGURE_STREAMS: u32 = FIRST_CALL_TRANSACTION + 1; // 2
    pub const PROCESS_CAPTURE_REQUEST: u32 = FIRST_CALL_TRANSACTION + 2; // 3
    pub const FLUSH: u32 = FIRST_CALL_TRANSACTION + 3; // 4
    pub const CLOSE: u32 = FIRST_CALL_TRANSACTION + 4; // 5
    pub const REPEATING_REQUEST_END: u32 = FIRST_CALL_TRANSACTION + 5; // 6
}

/// AIDL Interface for an active camera device capture session.
pub trait ICameraDeviceSession: Interface + Send + Sync {
    /// Construct default request settings for template.
    fn construct_default_request_settings(
        &self,
        template: RequestTemplate,
    ) -> AidlResult<CameraMetadata>;

    /// Configure input/output stream pipelines.
    fn configure_streams(
        &self,
        config: &StreamConfiguration,
    ) -> AidlResult<HalStreamConfiguration>;

    /// Process a batch of capture requests.
    fn process_capture_request(&self, requests: &[CaptureRequest]) -> AidlResult<u32>;

    /// Flush all in-flight requests.
    fn flush(&self) -> AidlResult<()>;

    /// Close the session.
    fn close(&self) -> AidlResult<()>;

    /// Notify end of repeating request sequence.
    fn repeating_request_end(&self, frame_number: i32, stream_ids: &[i32]) -> AidlResult<()>;
}

// -----------------------------------------------------------------------------
// CameraDeviceSessionService Implementation
// -----------------------------------------------------------------------------

pub struct CameraDeviceSessionService {
    callback: Strong<dyn ICameraDeviceCallback>,
    stream_config: RwLock<Option<StreamConfiguration>>,
    hal_stream_config: RwLock<Option<HalStreamConfiguration>>,
    is_closed: AtomicBool,
    processed_frames: AtomicU32,
    frame_buffer_cache: Mutex<HashMap<i32, Vec<u8>>>,
}

impl CameraDeviceSessionService {
    pub fn new(callback: Strong<dyn ICameraDeviceCallback>) -> Self {
        Self {
            callback,
            stream_config: RwLock::new(None),
            hal_stream_config: RwLock::new(None),
            is_closed: AtomicBool::new(false),
            processed_frames: AtomicU32::new(0),
            frame_buffer_cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn processed_frames_count(&self) -> u32 {
        self.processed_frames.load(Ordering::Relaxed)
    }

    /// Pre-populate frame data for a specific stream ID.
    pub fn set_stream_frame_data(&self, stream_id: i32, data: &[u8]) {
        let mut guard = self.frame_buffer_cache.lock().unwrap();
        guard.insert(stream_id, data.to_vec());
    }

    /// Check if session is active.
    pub fn is_active(&self) -> bool {
        !self.is_closed.load(Ordering::Relaxed)
    }
}

impl Interface for CameraDeviceSessionService {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::new(CameraDeviceSessionBinder {
            inner: Arc::new(Self {
                callback: self.callback.clone(),
                stream_config: RwLock::new(self.stream_config.read().unwrap().clone()),
                hal_stream_config: RwLock::new(self.hal_stream_config.read().unwrap().clone()),
                is_closed: AtomicBool::new(self.is_closed.load(Ordering::Relaxed)),
                processed_frames: AtomicU32::new(self.processed_frames.load(Ordering::Relaxed)),
                frame_buffer_cache: Mutex::new(self.frame_buffer_cache.lock().unwrap().clone()),
            }),
        })
    }
}

impl ICameraDeviceSession for CameraDeviceSessionService {
    fn construct_default_request_settings(
        &self,
        template: RequestTemplate,
    ) -> AidlResult<CameraMetadata> {
        if self.is_closed.load(Ordering::Relaxed) {
            return Err(CameraHalError::SessionError("Session closed".to_string()).into());
        }
        Ok(CameraMetadata::new_default_request_settings(template))
    }

    fn configure_streams(
        &self,
        config: &StreamConfiguration,
    ) -> AidlResult<HalStreamConfiguration> {
        if self.is_closed.load(Ordering::Relaxed) {
            return Err(CameraHalError::SessionError("Session closed".to_string()).into());
        }
        if config.streams.is_empty() {
            return Err(CameraHalError::InvalidStreamConfig("No streams provided".to_string()).into());
        }

        let mut hal_streams = Vec::new();
        for stream in &config.streams {
            hal_streams.push(HalStream {
                id: stream.id,
                override_format: stream.format,
                producer_usage: 0x3, // GRALLOC_USAGE_SW_READ_OFTEN | GRALLOC_USAGE_SW_WRITE_OFTEN
                consumer_usage: 0x3,
                max_buffers: 4,
                override_data_space: stream.data_space,
            });
        }

        let hal_config = HalStreamConfiguration { streams: hal_streams };
        *self.stream_config.write().unwrap() = Some(config.clone());
        *self.hal_stream_config.write().unwrap() = Some(hal_config.clone());

        Ok(hal_config)
    }

    fn process_capture_request(&self, requests: &[CaptureRequest]) -> AidlResult<u32> {
        if self.is_closed.load(Ordering::Relaxed) {
            return Err(CameraHalError::SessionError("Session closed".to_string()).into());
        }
        if requests.is_empty() {
            return Ok(0);
        }

        let mut count = 0;
        let mut results = Vec::new();
        let mut notify_msgs = Vec::new();

        let cached_buffers = self.frame_buffer_cache.lock().unwrap().clone();

        for req in requests {
            let timestamp_ns = 1_000_000_000 + (req.frame_number as i64 * 33_333_333); // ~30fps step

            // 1. Send Shutter Notify
            notify_msgs.push(NotifyMsg::Shutter(ShutterMsg {
                frame_number: req.frame_number,
                timestamp_ns,
            }));

            // 2. Build Output Buffers
            let mut out_buffers = Vec::new();
            for ob in &req.output_buffers {
                let frame_bytes = if let Some(cached) = cached_buffers.get(&ob.stream_id) {
                    cached.clone()
                } else {
                    let size = self
                        .stream_config
                        .read()
                        .unwrap()
                        .as_ref()
                        .and_then(|cfg| {
                            cfg.streams
                                .iter()
                                .find(|s| s.id == ob.stream_id)
                                .map(|s| s.buffer_size as usize)
                        })
                        .unwrap_or(1280 * 720 * 3 / 2);
                    vec![0x80; size]
                };

                out_buffers.push(StreamBuffer {
                    stream_id: ob.stream_id,
                    buffer_id: ob.buffer_id,
                    buffer_data: frame_bytes,
                    status: BufferStatus::Ok,
                });
            }

            let mut result_meta = req.settings.clone();
            result_meta.set("android.sensor.timestamp", &timestamp_ns.to_string());
            result_meta.set("android.control.aeState", "2"); // CONVERGED
            result_meta.set("android.control.afState", "4"); // FOCUSED_LOCKED

            results.push(CaptureResult {
                frame_number: req.frame_number,
                fmq_result_size: 0,
                result: result_meta,
                output_buffers: out_buffers,
                input_buffer: None,
                partial_result: 1,
            });

            self.processed_frames.fetch_add(1, Ordering::Relaxed);
            count += 1;
        }

        // Deliver results asynchronously to callback
        self.callback.notify(&notify_msgs)?;
        self.callback.process_capture_result(&results)?;

        Ok(count)
    }

    fn flush(&self) -> AidlResult<()> {
        if self.is_closed.load(Ordering::Relaxed) {
            return Err(CameraHalError::SessionError("Session closed".to_string()).into());
        }
        Ok(())
    }

    fn close(&self) -> AidlResult<()> {
        self.is_closed.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn repeating_request_end(&self, _frame_number: i32, _stream_ids: &[i32]) -> AidlResult<()> {
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Remotable Binder Stub for Session
// -----------------------------------------------------------------------------

pub struct CameraDeviceSessionBinder {
    pub inner: Arc<CameraDeviceSessionService>,
}

impl Interface for CameraDeviceSessionBinder {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::new(CameraDeviceSessionBinder {
            inner: Arc::clone(&self.inner),
        })
    }
}

impl Remotable for CameraDeviceSessionBinder {
    fn get_class_descriptor() -> &'static str {
        ICAMERA_DEVICE_SESSION_DESCRIPTOR
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        let mut offset = 0;
        let _ = data.read_utf16(&mut offset);

        match code {
            icamera_device_session_codes::CONSTRUCT_DEFAULT_REQUEST_SETTINGS => {
                let template_val = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let template = RequestTemplate::from_i32(template_val)
                    .ok_or_else(|| Status::from_status(STATUS_BAD_VALUE))?;

                let settings = self.inner.construct_default_request_settings(template)?;
                reply.write_status(&Status::ok()).unwrap();
                settings.write_to_parcel(reply)?;
                Ok(())
            }
            icamera_device_session_codes::CONFIGURE_STREAMS => {
                let mut config = StreamConfiguration::default();
                config.read_from_parcel_at(data, &mut offset)?;

                let hal_config = self.inner.configure_streams(&config)?;
                reply.write_status(&Status::ok()).unwrap();
                hal_config.write_to_parcel(reply)?;
                Ok(())
            }
            icamera_device_session_codes::PROCESS_CAPTURE_REQUEST => {
                let count = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let mut requests = Vec::with_capacity(count.max(0) as usize);
                for _ in 0..count {
                    let mut req = CaptureRequest::default();
                    req.read_from_parcel_at(data, &mut offset)?;
                    requests.push(req);
                }

                let processed = self.inner.process_capture_request(&requests)?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_u32(processed)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            icamera_device_session_codes::FLUSH => {
                self.inner.flush()?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            icamera_device_session_codes::CLOSE => {
                self.inner.close()?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            icamera_device_session_codes::REPEATING_REQUEST_END => {
                let frame_number = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let stream_count = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let mut stream_ids = Vec::new();
                for _ in 0..stream_count {
                    stream_ids.push(
                        data.read_i32(&mut offset)
                            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?,
                    );
                }
                self.inner.repeating_request_end(frame_number, &stream_ids)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            _ => Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

impl IBinder for CameraDeviceSessionBinder {
    fn link_to_death(&self, _recipient: Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn unlink_to_death(&self, _recipient: &Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn ping_binder(&self) -> AidlResult<()> {
        Ok(())
    }

    fn transact(
        &self,
        code: TransactionCode,
        _flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        match code {
            PING_TRANSACTION => {
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            INTERFACE_TRANSACTION => {
                reply
                    .write_utf16(Some(ICAMERA_DEVICE_SESSION_DESCRIPTOR))
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => self.on_transact(code, data, reply),
        }
    }

    fn as_transactable(&self) -> Option<&dyn Remotable> {
        Some(self)
    }

    fn get_class_descriptor(&self) -> Option<&'static str> {
        Some(ICAMERA_DEVICE_SESSION_DESCRIPTOR)
    }
}

// -----------------------------------------------------------------------------
// Client Proxy
// -----------------------------------------------------------------------------

pub struct CameraDeviceSessionProxy {
    binder: SpIBinder,
}

impl CameraDeviceSessionProxy {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }
}

impl Interface for CameraDeviceSessionProxy {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl Proxy for CameraDeviceSessionProxy {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl ICameraDeviceSession for CameraDeviceSessionProxy {
    fn construct_default_request_settings(
        &self,
        template: RequestTemplate,
    ) -> AidlResult<CameraMetadata> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_SESSION_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(template as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_device_session_codes::CONSTRUCT_DEFAULT_REQUEST_SETTINGS,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let mut meta = CameraMetadata::default();
        meta.read_from_parcel_at(&reply, &mut offset)?;
        Ok(meta)
    }

    fn configure_streams(
        &self,
        config: &StreamConfiguration,
    ) -> AidlResult<HalStreamConfiguration> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_SESSION_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        config.write_to_parcel(&mut data)?;

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_device_session_codes::CONFIGURE_STREAMS,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let mut hal_config = HalStreamConfiguration::default();
        hal_config.read_from_parcel_at(&reply, &mut offset)?;
        Ok(hal_config)
    }

    fn process_capture_request(&self, requests: &[CaptureRequest]) -> AidlResult<u32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_SESSION_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(requests.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for req in requests {
            req.write_to_parcel(&mut data)?;
        }

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_device_session_codes::PROCESS_CAPTURE_REQUEST,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        reply
            .read_u32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))
    }

    fn flush(&self) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_SESSION_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_device_session_codes::FLUSH,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn close(&self) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_SESSION_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_device_session_codes::CLOSE,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn repeating_request_end(&self, frame_number: i32, stream_ids: &[i32]) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_SESSION_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(frame_number)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(stream_ids.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for &sid in stream_ids {
            data.write_i32(sid)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_device_session_codes::REPEATING_REQUEST_END,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }
}
