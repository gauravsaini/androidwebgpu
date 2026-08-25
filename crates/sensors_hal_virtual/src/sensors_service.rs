//! AIDL `ISensors` Interface, Remotable Server Stub, and ServiceManager Registration.

use crate::error::SensorsHalError;
use crate::event_queue::{EventQueue, WakeLockQueue};
use crate::types::*;
use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{
    Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_UNKNOWN_TRANSACTION,
};
use aidl_compat::traits::{IBinder, Interface, Parcelable, Remotable};
use aidl_compat::DeathRecipient;
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, PING_TRANSACTION};
use binder_rt::Parcel;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub const ISENSORS_DESCRIPTOR: &str = "android.hardware.sensors.ISensors";
pub const ISENSORS_DEFAULT_INSTANCE: &str = "android.hardware.sensors.ISensors/default";

// -----------------------------------------------------------------------------
// Transaction Opcodes
// -----------------------------------------------------------------------------

pub mod isensors_codes {
    use super::FIRST_CALL_TRANSACTION;

    pub const GET_SENSORS_LIST: u32 = FIRST_CALL_TRANSACTION; // 1
    pub const SET_OPERATION_MODE: u32 = FIRST_CALL_TRANSACTION + 1; // 2
    pub const ACTIVATE: u32 = FIRST_CALL_TRANSACTION + 2; // 3
    pub const BATCH: u32 = FIRST_CALL_TRANSACTION + 3; // 4
    pub const FLUSH: u32 = FIRST_CALL_TRANSACTION + 4; // 5
    pub const INJECT_SENSOR_DATA: u32 = FIRST_CALL_TRANSACTION + 5; // 6
    pub const INITIALIZE: u32 = FIRST_CALL_TRANSACTION + 6; // 7
}

// -----------------------------------------------------------------------------
// ISensors Trait Definition
// -----------------------------------------------------------------------------

pub trait ISensors: Interface + Send + Sync {
    /// Retrieve list of all available hardware/virtual sensors.
    fn get_sensors_list(&self) -> AidlResult<Vec<SensorInfo>>;

    /// Set HAL operation mode (Normal or DataInjection).
    fn set_operation_mode(&self, mode: OperationMode) -> AidlResult<()>;

    /// Enable or disable sensor event streaming.
    fn activate(&self, sensor_handle: i32, enabled: bool) -> AidlResult<()>;

    /// Configure sampling period and max report latency.
    fn batch(
        &self,
        sensor_handle: i32,
        sampling_period_ns: i64,
        max_report_latency_ns: i64,
    ) -> AidlResult<()>;

    /// Flush FIFO for specified sensor handle.
    fn flush(&self, sensor_handle: i32) -> AidlResult<()>;

    /// Inject synthetic sensor event in DataInjection mode.
    fn inject_sensor_data(&self, event: &Event) -> AidlResult<()>;

    /// Initialize HAL with shared EventQueue and WakeLockQueue.
    fn initialize(
        &self,
        event_queue: Option<Arc<EventQueue>>,
        wake_lock_queue: Option<Arc<WakeLockQueue>>,
    ) -> AidlResult<()>;
}

// -----------------------------------------------------------------------------
// SensorsHalService Implementation
// -----------------------------------------------------------------------------

pub struct SensorsHalService {
    sensors: Arc<RwLock<HashMap<i32, SensorInfo>>>,
    sensor_states: Arc<RwLock<HashMap<i32, SensorState>>>,
    operation_mode: Arc<RwLock<OperationMode>>,
    event_queue: Arc<RwLock<Arc<EventQueue>>>,
    wake_lock_queue: Arc<RwLock<Arc<WakeLockQueue>>>,
}

impl Default for SensorsHalService {
    fn default() -> Self {
        Self::new()
    }
}

impl SensorsHalService {
    /// Create new SensorsHalService with default Accelerometer (handle 1) and Gyroscope (handle 2).
    pub fn new() -> Self {
        let mut sensors = HashMap::new();
        sensors.insert(1, SensorInfo::new_accelerometer(1));
        sensors.insert(2, SensorInfo::new_gyroscope(2));

        let mut states = HashMap::new();
        states.insert(1, SensorState::default());
        states.insert(2, SensorState::default());

        Self {
            sensors: Arc::new(RwLock::new(sensors)),
            sensor_states: Arc::new(RwLock::new(states)),
            operation_mode: Arc::new(RwLock::new(OperationMode::Normal)),
            event_queue: Arc::new(RwLock::new(Arc::new(EventQueue::new()))),
            wake_lock_queue: Arc::new(RwLock::new(Arc::new(WakeLockQueue::new()))),
        }
    }

    /// Register a new custom sensor declaration.
    pub fn register_sensor(&self, sensor: SensorInfo) {
        let handle = sensor.sensor_handle;
        self.sensors.write().unwrap().insert(handle, sensor);
        self.sensor_states
            .write()
            .unwrap()
            .entry(handle)
            .or_default();
    }

    /// Access reference to the active event queue.
    pub fn event_queue(&self) -> Arc<EventQueue> {
        Arc::clone(&self.event_queue.read().unwrap())
    }

    /// Check if a given sensor handle is active.
    pub fn is_sensor_active(&self, sensor_handle: i32) -> bool {
        self.sensor_states
            .read()
            .unwrap()
            .get(&sensor_handle)
            .map(|s| s.enabled)
            .unwrap_or(false)
    }

    /// Get current state of a sensor handle.
    pub fn get_sensor_state(&self, sensor_handle: i32) -> Option<SensorState> {
        self.sensor_states
            .read()
            .unwrap()
            .get(&sensor_handle)
            .cloned()
    }

    /// Push an event directly into the active event queue.
    pub fn push_event(&self, event: Event) -> Result<(), SensorsHalError> {
        self.event_queue.read().unwrap().push(event)
    }

    /// Drain events from the active event queue.
    pub fn poll_events(&self, max_count: usize) -> Vec<Event> {
        self.event_queue.read().unwrap().drain(max_count)
    }

    fn clone_internal(&self) -> Self {
        Self {
            sensors: Arc::clone(&self.sensors),
            sensor_states: Arc::clone(&self.sensor_states),
            operation_mode: Arc::clone(&self.operation_mode),
            event_queue: Arc::clone(&self.event_queue),
            wake_lock_queue: Arc::clone(&self.wake_lock_queue),
        }
    }
}

impl Interface for SensorsHalService {
    fn as_binder(&self) -> SpIBinder {
        SpIBinder::from_arc(Arc::new(self.clone_internal()))
    }
}

impl ISensors for SensorsHalService {
    fn get_sensors_list(&self) -> AidlResult<Vec<SensorInfo>> {
        let sensors = self.sensors.read().unwrap();
        let mut list: Vec<SensorInfo> = sensors.values().cloned().collect();
        list.sort_by_key(|s| s.sensor_handle);
        Ok(list)
    }

    fn set_operation_mode(&self, mode: OperationMode) -> AidlResult<()> {
        let mut op_mode = self.operation_mode.write().unwrap();
        *op_mode = mode;
        Ok(())
    }

    fn activate(&self, sensor_handle: i32, enabled: bool) -> AidlResult<()> {
        let mut states = self.sensor_states.write().unwrap();
        if let Some(state) = states.get_mut(&sensor_handle) {
            state.enabled = enabled;
            Ok(())
        } else {
            Err(Status::from(SensorsHalError::SensorNotFound(sensor_handle)))
        }
    }

    fn batch(
        &self,
        sensor_handle: i32,
        sampling_period_ns: i64,
        max_report_latency_ns: i64,
    ) -> AidlResult<()> {
        if sampling_period_ns <= 0 {
            return Err(Status::from(SensorsHalError::InvalidSamplingRate {
                period_ns: sampling_period_ns,
                latency_ns: max_report_latency_ns,
            }));
        }

        let mut states = self.sensor_states.write().unwrap();
        if let Some(state) = states.get_mut(&sensor_handle) {
            state.sampling_period_ns = sampling_period_ns;
            state.max_report_latency_ns = max_report_latency_ns;
            Ok(())
        } else {
            Err(Status::from(SensorsHalError::SensorNotFound(sensor_handle)))
        }
    }

    fn flush(&self, sensor_handle: i32) -> AidlResult<()> {
        let sensors = self.sensors.read().unwrap();
        if !sensors.contains_key(&sensor_handle) {
            return Err(Status::from(SensorsHalError::SensorNotFound(sensor_handle)));
        }

        // Android Sensors HAL specification: generate a META_DATA_FLUSH_COMPLETE event
        let flush_event = Event {
            timestamp: 0,
            sensor_handle,
            sensor_type: SensorType::Unknown,
            accuracy: 0,
            values: [0.0; 6],
        };
        let _ = self.push_event(flush_event);
        Ok(())
    }

    fn inject_sensor_data(&self, event: &Event) -> AidlResult<()> {
        let op_mode = *self.operation_mode.read().unwrap();
        if op_mode != OperationMode::DataInjection {
            return Err(Status::from(SensorsHalError::OperationNotPermitted {
                expected: "DataInjection",
                actual: "Normal",
            }));
        }

        let sensors = self.sensors.read().unwrap();
        if !sensors.contains_key(&event.sensor_handle) {
            return Err(Status::from(SensorsHalError::SensorNotFound(
                event.sensor_handle,
            )));
        }

        self.push_event(event.clone())
            .map_err(Status::from)?;
        Ok(())
    }

    fn initialize(
        &self,
        event_queue: Option<Arc<EventQueue>>,
        wake_lock_queue: Option<Arc<WakeLockQueue>>,
    ) -> AidlResult<()> {
        if let Some(eq) = event_queue {
            let mut q_guard = self.event_queue.write().unwrap();
            *q_guard = eq;
        }
        if let Some(wl) = wake_lock_queue {
            let mut wl_guard = self.wake_lock_queue.write().unwrap();
            *wl_guard = wl;
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Remotable and IBinder Implementations
// -----------------------------------------------------------------------------

impl Remotable for SensorsHalService {
    fn get_class_descriptor() -> &'static str {
        ISENSORS_DESCRIPTOR
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
                reply.write_utf8(Some(ISENSORS_DESCRIPTOR)).unwrap();
                Ok(())
            }
            isensors_codes::GET_SENSORS_LIST => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != ISENSORS_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let list = self.get_sensors_list()?;
                reply.write_status(&Status::ok()).unwrap();
                reply
                    .write_i32(list.len() as i32)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                for sensor in &list {
                    sensor.write_to_parcel(reply)?;
                }
                Ok(())
            }
            isensors_codes::SET_OPERATION_MODE => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != ISENSORS_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let mode_val = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let mode = OperationMode::from(mode_val);

                let res = self.set_operation_mode(mode);
                match res {
                    Ok(()) => {
                        reply.write_status(&Status::ok()).unwrap();
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            isensors_codes::ACTIVATE => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != ISENSORS_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let handle = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let enabled = data
                    .read_bool(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let res = self.activate(handle, enabled);
                match res {
                    Ok(()) => {
                        reply.write_status(&Status::ok()).unwrap();
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            isensors_codes::BATCH => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != ISENSORS_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let handle = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let period_ns = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let latency_ns = data
                    .read_i64(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let res = self.batch(handle, period_ns, latency_ns);
                match res {
                    Ok(()) => {
                        reply.write_status(&Status::ok()).unwrap();
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            isensors_codes::FLUSH => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != ISENSORS_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let handle = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                let res = self.flush(handle);
                match res {
                    Ok(()) => {
                        reply.write_status(&Status::ok()).unwrap();
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            isensors_codes::INJECT_SENSOR_DATA => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != ISENSORS_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                let mut event = Event::default();
                event.read_from_parcel_at(data, &mut offset)?;

                let res = self.inject_sensor_data(&event);
                match res {
                    Ok(()) => {
                        reply.write_status(&Status::ok()).unwrap();
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
            }
            isensors_codes::INITIALIZE => {
                let mut offset = 0;
                let interface_desc = data.read_utf16(&mut offset).unwrap_or_default();
                if let Some(desc) = interface_desc {
                    if desc != ISENSORS_DESCRIPTOR {
                        reply
                            .write_status(&Status::from_status(STATUS_BAD_VALUE))
                            .unwrap();
                        return Ok(());
                    }
                }

                // In native virtual HAL, initialize attaches shared event queue
                let res = self.initialize(None, None);
                match res {
                    Ok(()) => {
                        reply.write_status(&Status::ok()).unwrap();
                        Ok(())
                    }
                    Err(st) => {
                        reply.write_status(&st).unwrap();
                        Ok(())
                    }
                }
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

impl IBinder for SensorsHalService {
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
                    .write_utf16(Some(ISENSORS_DESCRIPTOR))
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
        Some(ISENSORS_DESCRIPTOR)
    }
}

// -----------------------------------------------------------------------------
// Service Registration Helper
// -----------------------------------------------------------------------------

/// Register `SensorsHalService` with handle 0 ServiceManager as `"android.hardware.sensors.ISensors/default"`.
pub fn register_sensors_service(service: Arc<SensorsHalService>) -> AidlResult<()> {
    let binder = service as Arc<dyn IBinder>;
    binder_sys::add_service(ISENSORS_DEFAULT_INSTANCE, SpIBinder::from_arc(binder))
}
