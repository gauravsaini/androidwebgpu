//! AIDL `ICameraDevice` Interface, Remotable Server Stub, and Client Proxy.

use crate::camera_device_callback::{CameraDeviceCallbackProxy, ICameraDeviceCallback};
use crate::camera_device_session::{
    CameraDeviceSessionProxy, CameraDeviceSessionService, ICameraDeviceSession,
};
use crate::types::CameraMetadata;
use aidl_compat::pointer::{SpIBinder, Strong};
use aidl_compat::status::{
    Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_UNKNOWN_TRANSACTION,
};
use aidl_compat::traits::{IBinder, Interface, Parcelable, Proxy, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, RwLock};

pub const ICAMERA_DEVICE_DESCRIPTOR: &str = "android.hardware.camera.device.ICameraDevice";

pub mod icamera_device_codes {
    use super::FIRST_CALL_TRANSACTION;
    pub const GET_CAMERA_CHARACTERISTICS: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const GET_PHYSICAL_CAMERA_CHARACTERISTICS: u32 = FIRST_CALL_TRANSACTION + 1; // 2
    pub const OPEN: u32 = FIRST_CALL_TRANSACTION + 2; // 3
    pub const SET_TORCH_MODE: u32 = FIRST_CALL_TRANSACTION + 3; // 4
    pub const TURN_ON_TORCH_WITH_STRENGTH_LEVEL: u32 = FIRST_CALL_TRANSACTION + 4; // 5
    pub const GET_TORCH_STRENGTH_LEVEL: u32 = FIRST_CALL_TRANSACTION + 5; // 6
    pub const DUMP_STATE: u32 = FIRST_CALL_TRANSACTION + 6; // 7
}

/// AIDL Interface representing a camera device.
pub trait ICameraDevice: Interface + Send + Sync {
    /// Retrieve static characteristics metadata for this camera.
    fn get_camera_characteristics(&self) -> AidlResult<CameraMetadata>;

    /// Retrieve static characteristics for a physical camera unit.
    fn get_physical_camera_characteristics(&self, physical_id: &str) -> AidlResult<CameraMetadata>;

    /// Open device capture session attached to callback.
    fn open(
        &self,
        callback: Strong<dyn ICameraDeviceCallback>,
    ) -> AidlResult<Strong<dyn ICameraDeviceSession>>;

    /// Enable or disable flashlight torch mode.
    fn set_torch_mode(&self, enabled: bool) -> AidlResult<()>;

    /// Turn on torch with strength level.
    fn turn_on_torch_with_strength_level(&self, torch_strength: i32) -> AidlResult<()>;

    /// Get current torch strength level.
    fn get_torch_strength_level(&self) -> AidlResult<i32>;

    /// Dump internal diagnostic state.
    fn dump_state(&self, fd: i32) -> AidlResult<()>;
}

// -----------------------------------------------------------------------------
// Active Camera Sessions Registry (for local in-process client proxies)
// -----------------------------------------------------------------------------

static ACTIVE_CAMERA_SESSIONS: RwLock<Option<HashMap<String, Arc<CameraDeviceSessionService>>>> = RwLock::new(None);

pub fn register_active_camera_session(device_id: &str, session: Arc<CameraDeviceSessionService>) {
    let mut guard = ACTIVE_CAMERA_SESSIONS.write().unwrap();
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    guard.as_mut().unwrap().insert(device_id.to_string(), session);
}

pub fn get_active_camera_session(device_id: &str) -> Option<Arc<CameraDeviceSessionService>> {
    let guard = ACTIVE_CAMERA_SESSIONS.read().unwrap();
    guard.as_ref().and_then(|map| map.get(device_id).cloned())
}

// -----------------------------------------------------------------------------
// CameraDeviceService Implementation
// -----------------------------------------------------------------------------

pub struct CameraDeviceService {
    camera_id: String,
    characteristics: CameraMetadata,
    torch_enabled: AtomicBool,
    torch_strength: AtomicI32,
    active_session: RwLock<Option<Arc<CameraDeviceSessionService>>>,
}

impl CameraDeviceService {
    pub fn new(camera_id: &str) -> Self {
        Self {
            camera_id: camera_id.to_string(),
            characteristics: CameraMetadata::new_virtual_camera_characteristics(),
            torch_enabled: AtomicBool::new(false),
            torch_strength: AtomicI32::new(1),
            active_session: RwLock::new(None),
        }
    }

    pub fn camera_id(&self) -> &str {
        &self.camera_id
    }

    pub fn get_active_session(&self) -> Option<Arc<CameraDeviceSessionService>> {
        self.active_session.read().unwrap().clone()
    }
}

impl Interface for CameraDeviceService {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::new(CameraDeviceBinder {
            inner: Arc::new(Self {
                camera_id: self.camera_id.clone(),
                characteristics: self.characteristics.clone(),
                torch_enabled: AtomicBool::new(self.torch_enabled.load(Ordering::Relaxed)),
                torch_strength: AtomicI32::new(self.torch_strength.load(Ordering::Relaxed)),
                active_session: RwLock::new(self.active_session.read().unwrap().clone()),
            }),
        })
    }
}

impl ICameraDevice for CameraDeviceService {
    fn get_camera_characteristics(&self) -> AidlResult<CameraMetadata> {
        Ok(self.characteristics.clone())
    }

    fn get_physical_camera_characteristics(
        &self,
        _physical_id: &str,
    ) -> AidlResult<CameraMetadata> {
        Ok(self.characteristics.clone())
    }

    fn open(
        &self,
        callback: Strong<dyn ICameraDeviceCallback>,
    ) -> AidlResult<Strong<dyn ICameraDeviceSession>> {
        let session = Arc::new(CameraDeviceSessionService::new(callback));
        register_active_camera_session(&self.camera_id, Arc::clone(&session));
        *self.active_session.write().unwrap() = Some(Arc::clone(&session));
        Ok(Strong::new(session))
    }

    fn set_torch_mode(&self, enabled: bool) -> AidlResult<()> {
        self.torch_enabled.store(enabled, Ordering::Relaxed);
        Ok(())
    }

    fn turn_on_torch_with_strength_level(&self, torch_strength: i32) -> AidlResult<()> {
        if torch_strength < 1 || torch_strength > 10 {
            return Err(Status::from_status(STATUS_BAD_VALUE));
        }
        self.torch_strength.store(torch_strength, Ordering::Relaxed);
        self.torch_enabled.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn get_torch_strength_level(&self) -> AidlResult<i32> {
        Ok(self.torch_strength.load(Ordering::Relaxed))
    }

    fn dump_state(&self, _fd: i32) -> AidlResult<()> {
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Remotable Binder Stub for CameraDevice
// -----------------------------------------------------------------------------

pub struct CameraDeviceBinder {
    pub inner: Arc<CameraDeviceService>,
}

impl Interface for CameraDeviceBinder {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::new(CameraDeviceBinder {
            inner: Arc::clone(&self.inner),
        })
    }
}

impl Remotable for CameraDeviceBinder {
    fn get_class_descriptor() -> &'static str {
        ICAMERA_DEVICE_DESCRIPTOR
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
            icamera_device_codes::GET_CAMERA_CHARACTERISTICS => {
                let meta = self.inner.get_camera_characteristics()?;
                reply.write_status(&Status::ok()).unwrap();
                meta.write_to_parcel(reply)?;
                Ok(())
            }
            icamera_device_codes::GET_PHYSICAL_CAMERA_CHARACTERISTICS => {
                let pid = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                let meta = self.inner.get_physical_camera_characteristics(&pid)?;
                reply.write_status(&Status::ok()).unwrap();
                meta.write_to_parcel(reply)?;
                Ok(())
            }
            icamera_device_codes::OPEN => {
                let callback_binder = if let Ok(flat) = data.read_binder(&mut offset) {
                    let transport = Arc::new(binder_sys::BinderKernelTransport::new());
                    aidl_compat::RemoteBinder::new_with_transport(
                        flat.handle(),
                        flat.cookie,
                        None,
                        transport,
                    )
                } else {
                    return Err(Status::from_status(STATUS_BAD_VALUE));
                };

                let callback_proxy = Arc::new(CameraDeviceCallbackProxy::new(callback_binder));
                let session = self.inner.open(Strong::new(callback_proxy))?;

                reply.write_status(&Status::ok()).unwrap();
                let handle = session.as_binder().handle().unwrap_or(0);
                reply
                    .write_binder(handle, 0)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            icamera_device_codes::SET_TORCH_MODE => {
                let enabled = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                self.inner.set_torch_mode(enabled)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            icamera_device_codes::TURN_ON_TORCH_WITH_STRENGTH_LEVEL => {
                let strength = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                self.inner.turn_on_torch_with_strength_level(strength)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            icamera_device_codes::GET_TORCH_STRENGTH_LEVEL => {
                let strength = self.inner.get_torch_strength_level()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_i32(strength)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            icamera_device_codes::DUMP_STATE => {
                let fd = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                self.inner.dump_state(fd)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            _ => Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

impl IBinder for CameraDeviceBinder {
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
                    .write_utf16(Some(ICAMERA_DEVICE_DESCRIPTOR))
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
        Some(ICAMERA_DEVICE_DESCRIPTOR)
    }
}

// -----------------------------------------------------------------------------
// Client Proxy
// -----------------------------------------------------------------------------

pub struct CameraDeviceProxy {
    binder: SpIBinder,
    camera_id: String,
}

impl CameraDeviceProxy {
    pub fn new(binder: SpIBinder) -> Self {
        Self {
            binder,
            camera_id: "device@1.0/virtual/0".to_string(),
        }
    }

    pub fn with_camera_id(binder: SpIBinder, camera_id: &str) -> Self {
        Self {
            binder,
            camera_id: camera_id.to_string(),
        }
    }
}

impl Interface for CameraDeviceProxy {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl Proxy for CameraDeviceProxy {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl ICameraDevice for CameraDeviceProxy {
    fn get_camera_characteristics(&self) -> AidlResult<CameraMetadata> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_device_codes::GET_CAMERA_CHARACTERISTICS,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            if let Some(active_dev) = crate::camera_provider::get_active_camera_device(&self.camera_id) {
                return active_dev.get_camera_characteristics();
            }
            return Err(status);
        }

        let mut meta = CameraMetadata::default();
        meta.read_from_parcel_at(&reply, &mut offset)?;
        Ok(meta)
    }

    fn get_physical_camera_characteristics(&self, physical_id: &str) -> AidlResult<CameraMetadata> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_utf8(Some(physical_id))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_device_codes::GET_PHYSICAL_CAMERA_CHARACTERISTICS,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            if let Some(active_dev) = crate::camera_provider::get_active_camera_device(&self.camera_id) {
                return active_dev.get_physical_camera_characteristics(physical_id);
            }
            return Err(status);
        }

        let mut meta = CameraMetadata::default();
        meta.read_from_parcel_at(&reply, &mut offset)?;
        Ok(meta)
    }

    fn open(
        &self,
        callback: Strong<dyn ICameraDeviceCallback>,
    ) -> AidlResult<Strong<dyn ICameraDeviceSession>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let callback_handle = callback.as_binder().handle().unwrap_or(0);
        data.write_binder(callback_handle, 0)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(icamera_device_codes::OPEN, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            if let Some(active_dev) = crate::camera_provider::get_active_camera_device(&self.camera_id) {
                return active_dev.open(callback);
            }
            return Err(status);
        }

        if let Some(active_sess) = get_active_camera_session(&self.camera_id) {
            return Ok(Strong::new(active_sess as Arc<dyn ICameraDeviceSession>));
        }

        let flat = reply
            .read_binder(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let transport = Arc::new(binder_sys::BinderKernelTransport::new());
        let session_binder = aidl_compat::RemoteBinder::new_with_transport(
            flat.handle(),
            flat.cookie,
            Some(crate::camera_device_session::ICAMERA_DEVICE_SESSION_DESCRIPTOR),
            transport,
        );

        let proxy = Arc::new(CameraDeviceSessionProxy::new(session_binder));
        Ok(Strong::new(proxy))
    }

    fn set_torch_mode(&self, enabled: bool) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_bool(enabled)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_device_codes::SET_TORCH_MODE,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            if let Some(active_dev) = crate::camera_provider::get_active_camera_device(&self.camera_id) {
                return active_dev.set_torch_mode(enabled);
            }
            return Err(status);
        }
        Ok(())
    }

    fn turn_on_torch_with_strength_level(&self, torch_strength: i32) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(torch_strength)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_device_codes::TURN_ON_TORCH_WITH_STRENGTH_LEVEL,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            if let Some(active_dev) = crate::camera_provider::get_active_camera_device(&self.camera_id) {
                return active_dev.turn_on_torch_with_strength_level(torch_strength);
            }
            return Err(status);
        }
        Ok(())
    }

    fn get_torch_strength_level(&self) -> AidlResult<i32> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_device_codes::GET_TORCH_STRENGTH_LEVEL,
            0,
            &data,
            &mut reply,
        )?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            if let Some(active_dev) = crate::camera_provider::get_active_camera_device(&self.camera_id) {
                return active_dev.get_torch_strength_level();
            }
            return Err(status);
        }

        reply
            .read_i32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))
    }

    fn dump_state(&self, fd: i32) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_DEVICE_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(fd)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(icamera_device_codes::DUMP_STATE, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            if let Some(active_dev) = crate::camera_provider::get_active_camera_device(&self.camera_id) {
                return active_dev.dump_state(fd);
            }
            return Err(status);
        }
        Ok(())
    }
}
