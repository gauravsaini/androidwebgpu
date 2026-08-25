//! AIDL `ICameraProvider` Interface, Remotable Server Stub, Client Proxy, and Service Registration.

use crate::camera_device::{CameraDeviceProxy, CameraDeviceService, ICameraDevice};
use crate::error::CameraHalError;
use crate::types::*;
use aidl_compat::pointer::{SpIBinder, Strong};
use aidl_compat::status::{
    Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_PERMISSION_DENIED,
    STATUS_UNKNOWN_TRANSACTION,
};
use aidl_compat::traits::{IBinder, Interface, Proxy, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub const ICAMERA_PROVIDER_DESCRIPTOR: &str =
    "android.hardware.camera.provider.ICameraProvider";
pub const ICAMERA_PROVIDER_VIRTUAL_INSTANCE: &str =
    "android.hardware.camera.provider.ICameraProvider/virtual/0";

pub mod icamera_provider_codes {
    use super::FIRST_CALL_TRANSACTION;
    pub const SET_CALLBACK: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const GET_VENDOR_TAGS: u32 = FIRST_CALL_TRANSACTION + 1; // 2
    pub const GET_CAMERA_ID_LIST: u32 = FIRST_CALL_TRANSACTION + 2; // 3
    pub const GET_CAMERA_DEVICE_INTERFACE: u32 = FIRST_CALL_TRANSACTION + 3; // 4
    pub const NOTIFY_DEVICE_STATE_CHANGE: u32 = FIRST_CALL_TRANSACTION + 4; // 5
    pub const IS_CONCURRENT_STREAM_COMBINATION_SUPPORTED: u32 = FIRST_CALL_TRANSACTION + 5; // 6
}

/// AIDL Interface for discovering and managing camera devices.
pub trait ICameraProvider: Interface + Send + Sync {
    /// Register provider-level status callback.
    fn set_callback(&self, callback: Option<SpIBinder>) -> AidlResult<()>;

    /// Retrieve vendor tag sections.
    fn get_vendor_tags(&self) -> AidlResult<Vec<VendorTagSection>>;

    /// Retrieve list of all available camera device names.
    fn get_camera_id_list(&self) -> AidlResult<Vec<String>>;

    /// Retrieve interface proxy to specified camera device.
    fn get_camera_device_interface(&self, name: &str) -> AidlResult<Strong<dyn ICameraDevice>>;

    /// Notify provider of device physical/posture state change.
    fn notify_device_state_change(&self, device_state: i64) -> AidlResult<()>;

    /// Check if concurrent streaming across camera combinations is supported.
    fn is_concurrent_stream_combination_supported(&self, combinations: &[String]) -> AidlResult<bool>;
}

// -----------------------------------------------------------------------------
// Active Camera Devices Registry (for local in-process client proxies)
// -----------------------------------------------------------------------------

static ACTIVE_CAMERA_DEVICES: RwLock<Option<HashMap<String, Arc<CameraDeviceService>>>> = RwLock::new(None);

pub fn register_active_camera_device(name: &str, device: Arc<CameraDeviceService>) {
    let mut guard = ACTIVE_CAMERA_DEVICES.write().unwrap();
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    guard.as_mut().unwrap().insert(name.to_string(), device);
}

pub fn get_active_camera_device(name: &str) -> Option<Arc<CameraDeviceService>> {
    let guard = ACTIVE_CAMERA_DEVICES.read().unwrap();
    guard.as_ref().and_then(|map| map.get(name).cloned())
}

// -----------------------------------------------------------------------------
// CameraProviderService Implementation
// -----------------------------------------------------------------------------

pub struct CameraProviderService {
    devices: RwLock<HashMap<String, Arc<CameraDeviceService>>>,
    device_state: RwLock<i64>,
}

impl Default for CameraProviderService {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraProviderService {
    /// Create new CameraProviderService with virtual camera `"device@1.0/virtual/0"`.
    pub fn new() -> Self {
        let mut devices = HashMap::new();
        let default_name = "device@1.0/virtual/0";
        let default_dev = Arc::new(CameraDeviceService::new(default_name));
        register_active_camera_device(default_name, Arc::clone(&default_dev));
        devices.insert(
            default_name.to_string(),
            default_dev,
        );

        Self {
            devices: RwLock::new(devices),
            device_state: RwLock::new(0),
        }
    }

    /// Add a custom camera device.
    pub fn add_device(&self, name: &str, device: Arc<CameraDeviceService>) {
        register_active_camera_device(name, Arc::clone(&device));
        self.devices.write().unwrap().insert(name.to_string(), device);
    }

    /// Get direct reference to internal CameraDeviceService by name.
    pub fn get_device(&self, name: &str) -> Option<Arc<CameraDeviceService>> {
        self.devices.read().unwrap().get(name).cloned()
    }
}

impl Interface for CameraProviderService {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::new(CameraProviderBinder {
            inner: Arc::new(Self {
                devices: RwLock::new(self.devices.read().unwrap().clone()),
                device_state: RwLock::new(*self.device_state.read().unwrap()),
            }),
        })
    }
}

impl ICameraProvider for CameraProviderService {
    fn set_callback(&self, _callback: Option<SpIBinder>) -> AidlResult<()> {
        Ok(())
    }

    fn get_vendor_tags(&self) -> AidlResult<Vec<VendorTagSection>> {
        Ok(Vec::new())
    }

    fn get_camera_id_list(&self) -> AidlResult<Vec<String>> {
        let devices = self.devices.read().unwrap();
        let mut list: Vec<String> = devices.keys().cloned().collect();
        list.sort();
        Ok(list)
    }

    fn get_camera_device_interface(&self, name: &str) -> AidlResult<Strong<dyn ICameraDevice>> {
        let devices = self.devices.read().unwrap();
        if let Some(device) = devices.get(name) {
            Ok(Strong::new(Arc::clone(device) as Arc<dyn ICameraDevice>))
        } else {
            Err(CameraHalError::DeviceNotFound(name.to_string()).into())
        }
    }

    fn notify_device_state_change(&self, device_state: i64) -> AidlResult<()> {
        *self.device_state.write().unwrap() = device_state;
        Ok(())
    }

    fn is_concurrent_stream_combination_supported(&self, _combinations: &[String]) -> AidlResult<bool> {
        Ok(true)
    }
}

// -----------------------------------------------------------------------------
// Remotable Binder Stub for CameraProvider
// -----------------------------------------------------------------------------

pub struct CameraProviderBinder {
    pub inner: Arc<CameraProviderService>,
}

impl Interface for CameraProviderBinder {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::new(CameraProviderBinder {
            inner: Arc::clone(&self.inner),
        })
    }
}

impl Remotable for CameraProviderBinder {
    fn get_class_descriptor() -> &'static str {
        ICAMERA_PROVIDER_DESCRIPTOR
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
            icamera_provider_codes::SET_CALLBACK => {
                let callback = if let Ok(flat) = data.read_binder(&mut offset) {
                    let transport = Arc::new(binder_sys::BinderKernelTransport::new());
                    Some(aidl_compat::RemoteBinder::new_with_transport(
                        flat.handle(),
                        flat.cookie,
                        None,
                        transport,
                    ))
                } else {
                    None
                };
                self.inner.set_callback(callback)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            icamera_provider_codes::GET_VENDOR_TAGS => {
                let tags = self.inner.get_vendor_tags()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_i32(tags.len() as i32)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            icamera_provider_codes::GET_CAMERA_ID_LIST => {
                let list = self.inner.get_camera_id_list()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_i32(list.len() as i32)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                for item in list {
                    reply
                        .write_utf8(Some(&item))
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                Ok(())
            }
            icamera_provider_codes::GET_CAMERA_DEVICE_INTERFACE => {
                let name = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                let device = self.inner.get_camera_device_interface(&name)?;
                reply.write_status(&Status::ok()).unwrap();
                let handle = device.as_binder().handle().unwrap_or(0);
                reply
                    .write_binder(handle, 0)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            icamera_provider_codes::NOTIFY_DEVICE_STATE_CHANGE => {
                let state = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                self.inner.notify_device_state_change(state)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            icamera_provider_codes::IS_CONCURRENT_STREAM_COMBINATION_SUPPORTED => {
                let count = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let mut combos = Vec::new();
                for _ in 0..count {
                    if let Ok(Some(s)) = data.read_utf8(&mut offset) {
                        combos.push(s);
                    }
                }
                let supported = self
                    .inner
                    .is_concurrent_stream_combination_supported(&combos)?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_bool(supported)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

impl IBinder for CameraProviderBinder {
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
                    .write_utf16(Some(ICAMERA_PROVIDER_DESCRIPTOR))
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
        Some(ICAMERA_PROVIDER_DESCRIPTOR)
    }
}

// -----------------------------------------------------------------------------
// Client Proxy
// -----------------------------------------------------------------------------

pub struct CameraProviderProxy {
    binder: SpIBinder,
}

impl CameraProviderProxy {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }
}

impl Interface for CameraProviderProxy {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl Proxy for CameraProviderProxy {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl ICameraProvider for CameraProviderProxy {
    fn set_callback(&self, callback: Option<SpIBinder>) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_PROVIDER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        if let Some(cb) = callback {
            let handle = cb.handle().unwrap_or(0);
            data.write_binder(handle, 0)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        } else {
            data.write_i32(0)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_provider_codes::SET_CALLBACK,
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

    fn get_vendor_tags(&self) -> AidlResult<Vec<VendorTagSection>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_PROVIDER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_provider_codes::GET_VENDOR_TAGS,
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
        Ok(Vec::new())
    }

    fn get_camera_id_list(&self) -> AidlResult<Vec<String>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_PROVIDER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_provider_codes::GET_CAMERA_ID_LIST,
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

        let count = reply
            .read_i32(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let mut list = Vec::with_capacity(count.max(0) as usize);
        for _ in 0..count {
            if let Ok(Some(item)) = reply.read_utf8(&mut offset) {
                list.push(item);
            }
        }
        Ok(list)
    }

    fn get_camera_device_interface(&self, name: &str) -> AidlResult<Strong<dyn ICameraDevice>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_PROVIDER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_utf8(Some(name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_provider_codes::GET_CAMERA_DEVICE_INTERFACE,
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

        if let Some(active_dev) = get_active_camera_device(name) {
            return Ok(Strong::new(active_dev as Arc<dyn ICameraDevice>));
        }

        let flat = reply
            .read_binder(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let transport = Arc::new(binder_sys::BinderKernelTransport::new());
        let device_binder = aidl_compat::RemoteBinder::new_with_transport(
            flat.handle(),
            flat.cookie,
            Some(crate::camera_device::ICAMERA_DEVICE_DESCRIPTOR),
            transport,
        );

        let proxy = Arc::new(CameraDeviceProxy::new(device_binder));
        Ok(Strong::new(proxy))
    }

    fn notify_device_state_change(&self, device_state: i64) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_PROVIDER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i64(device_state)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_provider_codes::NOTIFY_DEVICE_STATE_CHANGE,
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

    fn is_concurrent_stream_combination_supported(&self, combinations: &[String]) -> AidlResult<bool> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ICAMERA_PROVIDER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(combinations.len() as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for c in combinations {
            data.write_utf8(Some(c))
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }

        let mut reply = Parcel::new();
        self.binder.transact(
            icamera_provider_codes::IS_CONCURRENT_STREAM_COMBINATION_SUPPORTED,
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
            .read_bool(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))
    }
}

// -----------------------------------------------------------------------------
// Service Registration with VINTF Validation
// -----------------------------------------------------------------------------

/// Register `CameraProviderService` with handle 0 ServiceManager after verifying VINTF manifest.
pub fn register_camera_provider_service(service: Arc<CameraProviderService>) -> AidlResult<()> {
    if !vintf_validator::is_declared(ICAMERA_PROVIDER_VIRTUAL_INSTANCE) {
        log::error!("Cannot register service {}: not declared in VINTF manifest", ICAMERA_PROVIDER_VIRTUAL_INSTANCE);
        return Err(Status::from_status(STATUS_PERMISSION_DENIED));
    }

    binder_sys::add_service(ICAMERA_PROVIDER_VIRTUAL_INSTANCE, service.as_binder())
}
