//! AIDL `ISensors` Client Proxy.

use crate::event_queue::{EventQueue, WakeLockQueue};
use crate::sensors_service::{isensors_codes, ISensors, ISENSORS_DESCRIPTOR};
use crate::types::*;
use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{Result as AidlResult, Status, STATUS_BAD_VALUE};
use aidl_compat::traits::{Interface, Parcelable, Proxy};
use binder_rt::Parcel;
use std::sync::Arc;

pub struct SensorsProxy {
    binder: SpIBinder,
}

impl SensorsProxy {
    pub fn new(binder: SpIBinder) -> Self {
        Self { binder }
    }
}

impl Interface for SensorsProxy {
    fn as_binder(&self) -> SpIBinder {
        self.binder.clone()
    }
}

impl Proxy for SensorsProxy {
    fn as_binder(&self) -> &SpIBinder {
        &self.binder
    }
}

impl ISensors for SensorsProxy {
    fn get_sensors_list(&self) -> AidlResult<Vec<SensorInfo>> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISENSORS_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(isensors_codes::GET_SENSORS_LIST, 0, &data, &mut reply)?;

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
            let mut sensor = SensorInfo::default();
            sensor.read_from_parcel_at(&reply, &mut offset)?;
            list.push(sensor);
        }
        Ok(list)
    }

    fn set_operation_mode(&self, mode: OperationMode) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISENSORS_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(mode.into())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(isensors_codes::SET_OPERATION_MODE, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn activate(&self, sensor_handle: i32, enabled: bool) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISENSORS_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(sensor_handle)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_bool(enabled)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(isensors_codes::ACTIVATE, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn batch(
        &self,
        sensor_handle: i32,
        sampling_period_ns: i64,
        max_report_latency_ns: i64,
    ) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISENSORS_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(sensor_handle)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i64(sampling_period_ns)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i64(max_report_latency_ns)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(isensors_codes::BATCH, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn flush(&self, sensor_handle: i32) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISENSORS_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        data.write_i32(sensor_handle)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(isensors_codes::FLUSH, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn inject_sensor_data(&self, event: &Event) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISENSORS_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        event.write_to_parcel(&mut data)?;

        let mut reply = Parcel::new();
        self.binder
            .transact(isensors_codes::INJECT_SENSOR_DATA, 0, &data, &mut reply)?;

        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn initialize(
        &self,
        _event_queue: Option<Arc<EventQueue>>,
        _wake_lock_queue: Option<Arc<WakeLockQueue>>,
    ) -> AidlResult<()> {
        let mut data = Parcel::new();
        data.write_utf16(Some(ISENSORS_DESCRIPTOR))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

        let mut reply = Parcel::new();
        self.binder
            .transact(isensors_codes::INITIALIZE, 0, &data, &mut reply)?;

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
