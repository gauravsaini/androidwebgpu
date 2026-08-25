//! AIDL `IInputManager` Interface, Remotable Server Stub, and Client Proxy.

use crate::dispatcher::InputDispatcher;
use crate::types::*;
use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{
    Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_UNKNOWN_TRANSACTION,
};
use aidl_compat::traits::{IBinder, Interface, Parcelable, Proxy, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use input_channel::InputChannel;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub const IINPUT_MANAGER_DESCRIPTOR: &str = "android.hardware.input.IInputManager";

// -----------------------------------------------------------------------------
// Transaction Opcodes
// -----------------------------------------------------------------------------

pub mod iinput_manager_codes {
    use super::FIRST_CALL_TRANSACTION;

    pub const GET_INPUT_DEVICE_IDS: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const GET_INPUT_DEVICE: u32 = FIRST_CALL_TRANSACTION + 1; // 2
    pub const REGISTER_INPUT_CHANNEL: u32 = FIRST_CALL_TRANSACTION + 2; // 3
    pub const INJECT_INPUT_EVENT: u32 = FIRST_CALL_TRANSACTION + 3; // 4
}

// -----------------------------------------------------------------------------
// IInputManager Trait Definition
// -----------------------------------------------------------------------------

pub trait IInputManager: Interface + Send + Sync {
    fn get_input_device_ids(&self) -> AidlResult<Vec<i32>>;

    fn get_input_device(&self, device_id: i32) -> AidlResult<Option<InputDevice>>;

    fn register_input_channel(&self, channel: &InputChannel) -> AidlResult<()>;

    fn inject_input_event(&self, event: &InputEvent, mode: i32) -> AidlResult<bool>;
}

// -----------------------------------------------------------------------------
// InputManagerService Server Implementation
// -----------------------------------------------------------------------------

pub struct InputManagerService {
    dispatcher: Arc<InputDispatcher>,
    devices: Arc<RwLock<HashMap<i32, InputDevice>>>,
}

impl Default for InputManagerService {
    fn default() -> Self {
        Self::new()
    }
}

impl InputManagerService {
    /// Create a new InputManagerService with default touchscreen and keyboard devices.
    pub fn new() -> Self {
        let mut devices = HashMap::new();
        devices.insert(1, InputDevice::new_touchscreen(1, "Virtual Touchscreen"));
        devices.insert(2, InputDevice::new_keyboard(2, "Virtual Keyboard"));

        Self {
            dispatcher: Arc::new(InputDispatcher::new()),
            devices: Arc::new(RwLock::new(devices)),
        }
    }

    /// Create service with an explicit `InputDispatcher`.
    pub fn with_dispatcher(dispatcher: Arc<InputDispatcher>) -> Self {
        let mut devices = HashMap::new();
        devices.insert(1, InputDevice::new_touchscreen(1, "Virtual Touchscreen"));
        devices.insert(2, InputDevice::new_keyboard(2, "Virtual Keyboard"));

        Self {
            dispatcher,
            devices: Arc::new(RwLock::new(devices)),
        }
    }

    /// Access reference to internal dispatcher.
    pub fn dispatcher(&self) -> &Arc<InputDispatcher> {
        &self.dispatcher
    }

    /// Register or update an input device.
    pub fn add_input_device(&self, device: InputDevice) {
        self.devices.write().unwrap().insert(device.id, device);
    }

    fn clone_internal(&self) -> Self {
        Self {
            dispatcher: Arc::clone(&self.dispatcher),
            devices: Arc::clone(&self.devices),
        }
    }
}

impl Interface for InputManagerService {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::from_arc(Arc::new(self.clone_internal()))
    }
}

impl IInputManager for InputManagerService {
    fn get_input_device_ids(&self) -> AidlResult<Vec<i32>> {
        let devices = self.devices.read().unwrap();
        let mut ids: Vec<i32> = devices.keys().cloned().collect();
        ids.sort_unstable();
        Ok(ids)
    }

    fn get_input_device(&self, device_id: i32) -> AidlResult<Option<InputDevice>> {
        let devices = self.devices.read().unwrap();
        Ok(devices.get(&device_id).cloned())
    }

    fn register_input_channel(&self, channel: &InputChannel) -> AidlResult<()> {
        let name = channel.name();
        self.dispatcher
            .register_window_channel(name, Arc::new(channel.clone()));
        Ok(())
    }

    fn inject_input_event(&self, event: &InputEvent, mode: i32) -> AidlResult<bool> {
        match mode {
            INJECT_INPUT_EVENT_MODE_ASYNC => {
                let _ = self
                    .dispatcher
                    .dispatch_event(event)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(true)
            }
            INJECT_INPUT_EVENT_MODE_WAIT_FOR_RESULT | INJECT_INPUT_EVENT_MODE_WAIT_FOR_FINISH => {
                let handled = self
                    .dispatcher
                    .dispatch_and_wait_for_ack(event, 5000)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(handled)
            }
            _ => Err(Status::from_status(STATUS_BAD_VALUE)),
        }
    }
}

// -----------------------------------------------------------------------------
// Remotable and IBinder Implementations for InputManagerService
// -----------------------------------------------------------------------------

impl Remotable for InputManagerService {
    fn get_class_descriptor() -> &'static str {
        IINPUT_MANAGER_DESCRIPTOR
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        match code {
            PING_TRANSACTION => {
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            INTERFACE_TRANSACTION => {
                reply.write_utf8(Some(IINPUT_MANAGER_DESCRIPTOR)).unwrap();
                Ok(())
            }
            iinput_manager_codes::GET_INPUT_DEVICE_IDS => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != IINPUT_MANAGER_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let ids = self.get_input_device_ids()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_vector(Some(&ids), |p, val| p.write_i32(*val))
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            iinput_manager_codes::GET_INPUT_DEVICE => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != IINPUT_MANAGER_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let device_id = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let dev = self.get_input_device(device_id)?;
                reply.write_status(&Status::ok()).unwrap();
                if let Some(device) = dev {
                    reply
                        .write_bool(true)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    device.write_to_parcel(reply)?;
                } else {
                    reply
                        .write_bool(false)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                Ok(())
            }
            iinput_manager_codes::REGISTER_INPUT_CHANNEL => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != IINPUT_MANAGER_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let mut channel = InputChannel::default();
                channel.read_from_parcel_at(data, &mut offset)?;

                self.register_input_channel(&channel)?;
                reply.write_status(&Status::ok()).unwrap();
                Ok(())
            }
            iinput_manager_codes::INJECT_INPUT_EVENT => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != IINPUT_MANAGER_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let mut event = InputEvent::default();
                event.read_from_parcel_at(data, &mut offset)?;

                let mode = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let success = self.inject_input_event(&event, mode)?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_bool(success)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => {
                reply
                    .write_status(&Status::from_status(STATUS_UNKNOWN_TRANSACTION))
                    .unwrap();
                Ok(())
            }
        }
    }
}

impl IBinder for InputManagerService {
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
                reply.write_utf16(Some(IINPUT_MANAGER_DESCRIPTOR)).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => self.on_transact(code, data, reply),
        }
    }

    fn as_transactable(&self) -> Option<&dyn Remotable> {
        Some(self)
    }

    fn get_class_descriptor(&self) -> Option<&'static str> {
        Some(IINPUT_MANAGER_DESCRIPTOR)
    }
}

// -----------------------------------------------------------------------------
// InputManagerProxy Client Implementation
// -----------------------------------------------------------------------------

pub struct InputManagerProxy {
    binder: SpIBinder,
}

impl InputManagerProxy {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }
}

impl Interface for InputManagerProxy {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl Proxy for InputManagerProxy {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl IInputManager for InputManagerProxy {
    fn get_input_device_ids(&self) -> AidlResult<Vec<i32>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IINPUT_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(iinput_manager_codes::GET_INPUT_DEVICE_IDS, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let ids = reply
            .read_vector(&mut offset, |p, off| p.read_i32(off))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        Ok(ids)
    }

    fn get_input_device(&self, device_id: i32) -> AidlResult<Option<InputDevice>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IINPUT_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(device_id)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(iinput_manager_codes::GET_INPUT_DEVICE, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let has_device = reply
            .read_bool(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if has_device {
            let mut dev = InputDevice::default();
            dev.read_from_parcel_at(&reply, &mut offset)?;
            Ok(Some(dev))
        } else {
            Ok(None)
        }
    }

    fn register_input_channel(&self, channel: &InputChannel) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IINPUT_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        channel.write_to_parcel(&mut data)?;

        let mut reply = Parcel::new();
        self.binder
            .transact(iinput_manager_codes::REGISTER_INPUT_CHANNEL, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn inject_input_event(&self, event: &InputEvent, mode: i32) -> AidlResult<bool> {
        let mut data = Parcel::new();
        data.write_utf16(Some(IINPUT_MANAGER_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        event.write_to_parcel(&mut data)?;
        data.write_i32(mode)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(iinput_manager_codes::INJECT_INPUT_EVENT, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }

        let success = reply
            .read_bool(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(success)
    }
}

// -----------------------------------------------------------------------------
// Service Registration Helper
// -----------------------------------------------------------------------------

/// Register `InputManagerService` with handle 0 ServiceManager as `"input"`.
pub fn register_input_service(service: Arc<InputManagerService>) -> AidlResult<()> {
    let binder = service as Arc<dyn IBinder>;
    binder_sys::add_service("input", SpIBinder::from_arc(binder))
}
