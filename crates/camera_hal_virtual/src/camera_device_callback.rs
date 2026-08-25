//! AIDL `ICameraDeviceCallback` Interface, Client Proxy, and Mock Receiver.

use crate::types::{CaptureResult, NotifyMsg};
use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_UNKNOWN_TRANSACTION};
use aidl_compat::traits::{IBinder, Interface, Parcelable, Proxy, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use std::sync::{Arc, Mutex};

pub const ICAMERA_DEVICE_CALLBACK_DESCRIPTOR: &str =
    "android.hardware.camera.device.ICameraDeviceCallback";

pub mod icamera_device_callback_codes {
    use super::FIRST_CALL_TRANSACTION;
    pub const PROCESS_CAPTURE_RESULT: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const NOTIFY: u32 = FIRST_CALL_TRANSACTION + 1; // 2
}

/// AIDL Interface for receiving asynchronous camera capture results and notifications.
pub trait ICameraDeviceCallback: Interface + Send + Sync {
    /// Deliver capture results to client.
    fn process_capture_result(&self, results: &[CaptureResult]) -> AidlResult<()>;

    /// Deliver shutter or error notifications to client.
    fn notify(&self, msgs: &[NotifyMsg]) -> AidlResult<()>;
}

// -----------------------------------------------------------------------------
// Client Proxy
// -----------------------------------------------------------------------------

pub struct CameraDeviceCallbackProxy {
    binder: SpIBinder,
}

impl CameraDeviceCallbackProxy {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }
}

impl Interface for CameraDeviceCallbackProxy {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl Proxy for CameraDeviceCallbackProxy {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl ICameraDeviceCallback for CameraDeviceCallbackProxy {
    fn process_capture_result(&self, results: &[CaptureResult]) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_CALLBACK_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(results.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for res in results {
            res.write_to_parcel(&mut data)?;
        }

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_device_callback_codes::PROCESS_CAPTURE_RESULT,
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

    fn notify(&self, msgs: &[NotifyMsg]) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_CALLBACK_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(msgs.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for msg in msgs {
            msg.write_to_parcel(&mut data)?;
        }

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_device_callback_codes::NOTIFY,
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

// -----------------------------------------------------------------------------
// Mock / In-Memory Receiver for Tests & Guest Server Dispatch
// -----------------------------------------------------------------------------

#[derive(Default)]
pub struct MockCameraDeviceCallback {
    received_results: Mutex<Vec<CaptureResult>>,
    received_notifications: Mutex<Vec<NotifyMsg>>,
}

impl MockCameraDeviceCallback {
    pub fn new() -> Self {
        Self {
            received_results: Mutex::new(Vec::new()),
            received_notifications: Mutex::new(Vec::new()),
        }
    }

    pub fn get_results(&self) -> Vec<CaptureResult> {
        self.received_results.lock().unwrap().clone()
    }

    pub fn get_notifications(&self) -> Vec<NotifyMsg> {
        self.received_notifications.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.received_results.lock().unwrap().clear();
        self.received_notifications.lock().unwrap().clear();
    }
}

impl Interface for MockCameraDeviceCallback {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::new(MockCameraDeviceCallbackBinder {
            inner: Arc::new(Self {
                received_results: Mutex::new(self.received_results.lock().unwrap().clone()),
                received_notifications: Mutex::new(self.received_notifications.lock().unwrap().clone()),
            }),
        })
    }
}

impl ICameraDeviceCallback for MockCameraDeviceCallback {
    fn process_capture_result(&self, results: &[CaptureResult]) -> AidlResult<()> {
        let mut guard = self.received_results.lock().unwrap();
        guard.extend_from_slice(results);
        Ok(())
    }

    fn notify(&self, msgs: &[NotifyMsg]) -> AidlResult<()> {
        let mut guard = self.received_notifications.lock().unwrap();
        guard.extend_from_slice(msgs);
        Ok(())
    }
}

pub struct MockCameraDeviceCallbackBinder {
    pub inner: Arc<MockCameraDeviceCallback>,
}

impl Interface for MockCameraDeviceCallbackBinder {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::new(MockCameraDeviceCallbackBinder {
            inner: Arc::clone(&self.inner),
        })
    }
}

impl Remotable for MockCameraDeviceCallbackBinder {
    fn get_class_descriptor() -> &'static str {
        ICAMERA_DEVICE_CALLBACK_DESCRIPTOR
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
            icamera_device_callback_codes::PROCESS_CAPTURE_RESULT => {
                let count = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let mut results = Vec::with_capacity(count.max(0) as usize);
                for _ in 0..count {
                    let mut res = CaptureResult::default();
                    res.read_from_parcel_at(data, &mut offset)?;
                    results.push(res);
                }
                self.inner.process_capture_result(&results)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            icamera_device_callback_codes::NOTIFY => {
                let count = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let mut msgs = Vec::with_capacity(count.max(0) as usize);
                for _ in 0..count {
                    let mut msg = NotifyMsg::Shutter(crate::types::ShutterMsg { frame_number: 0, timestamp_ns: 0 });
                    msg.read_from_parcel_at(data, &mut offset)?;
                    msgs.push(msg);
                }
                self.inner.notify(&msgs)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            _ => Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

impl IBinder for MockCameraDeviceCallbackBinder {
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
                    .write_utf16(Some(ICAMERA_DEVICE_CALLBACK_DESCRIPTOR))
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
        Some(ICAMERA_DEVICE_CALLBACK_DESCRIPTOR)
    }
}
