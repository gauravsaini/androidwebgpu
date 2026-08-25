//! Core AIDL Sensor Types, Constants, and Parcelable Definitions.

use aidl_compat::status::{Result as AidlResult, Status, STATUS_BAD_VALUE};
use aidl_compat::traits::Parcelable;
use binder_rt::Parcel;
use serde::{Deserialize, Serialize};

// -----------------------------------------------------------------------------
// Sensor Types (Android 13+ AIDL SensorType enum)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum SensorType {
    Accelerometer = 1,
    GeomagneticField = 2,
    Orientation = 3,
    Gyroscope = 4,
    Light = 5,
    Pressure = 6,
    Temperature = 7,
    Proximity = 8,
    Gravity = 9,
    LinearAcceleration = 10,
    RotationVector = 11,
    RelativeHumidity = 12,
    AmbientTemperature = 13,
    MagneticFieldUncalibrated = 14,
    GameRotationVector = 15,
    GyroscopeUncalibrated = 16,
    SignificantMotion = 17,
    StepDetector = 18,
    StepCounter = 19,
    GeomagneticRotationVector = 20,
    HeartRate = 21,
    AccelerometerUncalibrated = 35,
    Unknown = -1,
}

impl From<i32> for SensorType {
    fn from(val: i32) -> Self {
        match val {
            1 => SensorType::Accelerometer,
            2 => SensorType::GeomagneticField,
            3 => SensorType::Orientation,
            4 => SensorType::Gyroscope,
            5 => SensorType::Light,
            6 => SensorType::Pressure,
            7 => SensorType::Temperature,
            8 => SensorType::Proximity,
            9 => SensorType::Gravity,
            10 => SensorType::LinearAcceleration,
            11 => SensorType::RotationVector,
            12 => SensorType::RelativeHumidity,
            13 => SensorType::AmbientTemperature,
            14 => SensorType::MagneticFieldUncalibrated,
            15 => SensorType::GameRotationVector,
            16 => SensorType::GyroscopeUncalibrated,
            17 => SensorType::SignificantMotion,
            18 => SensorType::StepDetector,
            19 => SensorType::StepCounter,
            20 => SensorType::GeomagneticRotationVector,
            21 => SensorType::HeartRate,
            35 => SensorType::AccelerometerUncalibrated,
            _ => SensorType::Unknown,
        }
    }
}

impl From<SensorType> for i32 {
    fn from(st: SensorType) -> Self {
        st as i32
    }
}

// -----------------------------------------------------------------------------
// Operation Mode (Android 13+ AIDL OperationMode enum)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum OperationMode {
    Normal = 0,
    DataInjection = 1,
}

impl From<i32> for OperationMode {
    fn from(val: i32) -> Self {
        match val {
            1 => OperationMode::DataInjection,
            _ => OperationMode::Normal,
        }
    }
}

impl From<OperationMode> for i32 {
    fn from(mode: OperationMode) -> Self {
        mode as i32
    }
}

// -----------------------------------------------------------------------------
// SensorInfo (android.hardware.sensors.SensorInfo)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SensorInfo {
    pub sensor_handle: i32,
    pub name: String,
    pub vendor: String,
    pub version: i32,
    pub sensor_type: SensorType,
    pub type_as_string: String,
    pub required_permission: String,
    pub max_range: f32,
    pub resolution: f32,
    pub power: f32,
    pub min_delay_us: i32,
    pub fifo_reserved_event_count: i32,
    pub fifo_max_event_count: i32,
    pub max_delay_us: i32,
    pub flags: i32,
}

impl Default for SensorInfo {
    fn default() -> Self {
        Self {
            sensor_handle: 0,
            name: String::new(),
            vendor: "AndroidWebGPU".to_string(),
            version: 1,
            sensor_type: SensorType::Accelerometer,
            type_as_string: "android.sensor.accelerometer".to_string(),
            required_permission: String::new(),
            max_range: 78.48, // 8g in m/s^2
            resolution: 0.001,
            power: 0.1, // mA
            min_delay_us: 10000, // 100 Hz
            fifo_reserved_event_count: 0,
            fifo_max_event_count: 300,
            max_delay_us: 200000, // 5 Hz
            flags: 0,
        }
    }
}

impl SensorInfo {
    /// Create standard Accelerometer (handle 1).
    pub fn new_accelerometer(handle: i32) -> Self {
        Self {
            sensor_handle: handle,
            name: "Virtual Accelerometer".to_string(),
            vendor: "AndroidWebGPU".to_string(),
            version: 1,
            sensor_type: SensorType::Accelerometer,
            type_as_string: "android.sensor.accelerometer".to_string(),
            required_permission: String::new(),
            max_range: 78.48,
            resolution: 0.001,
            power: 0.1,
            min_delay_us: 10000,
            fifo_reserved_event_count: 0,
            fifo_max_event_count: 300,
            max_delay_us: 200000,
            flags: 0,
        }
    }

    /// Create standard Gyroscope (handle 2).
    pub fn new_gyroscope(handle: i32) -> Self {
        Self {
            sensor_handle: handle,
            name: "Virtual Gyroscope".to_string(),
            vendor: "AndroidWebGPU".to_string(),
            version: 1,
            sensor_type: SensorType::Gyroscope,
            type_as_string: "android.sensor.gyroscope".to_string(),
            required_permission: String::new(),
            max_range: 34.90, // ~2000 deg/s in rad/s
            resolution: 0.001,
            power: 0.2,
            min_delay_us: 10000,
            fifo_reserved_event_count: 0,
            fifo_max_event_count: 300,
            max_delay_us: 200000,
            flags: 0,
        }
    }
}

impl Parcelable for SensorInfo {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel
            .write_i32(self.sensor_handle)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf16(Some(&self.name))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf16(Some(&self.vendor))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.version)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.sensor_type.into())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf16(Some(&self.type_as_string))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_utf16(Some(&self.required_permission))
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_f32(self.max_range)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_f32(self.resolution)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_f32(self.power)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.min_delay_us)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.fifo_reserved_event_count)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.fifo_max_event_count)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.max_delay_us)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.flags)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> AidlResult<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.sensor_handle = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.name = parcel
            .read_utf16(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.vendor = parcel
            .read_utf16(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.version = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let st_raw = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.sensor_type = SensorType::from(st_raw);
        self.type_as_string = parcel
            .read_utf16(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.required_permission = parcel
            .read_utf16(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
            .unwrap_or_default();
        self.max_range = parcel
            .read_f32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.resolution = parcel
            .read_f32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.power = parcel
            .read_f32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.min_delay_us = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.fifo_reserved_event_count = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.fifo_max_event_count = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.max_delay_us = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.flags = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Sensor Event (android.hardware.sensors.Event)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub timestamp: i64,
    pub sensor_handle: i32,
    pub sensor_type: SensorType,
    pub accuracy: i8,
    pub values: [f32; 6],
}

impl Default for Event {
    fn default() -> Self {
        Self {
            timestamp: 0,
            sensor_handle: 0,
            sensor_type: SensorType::Accelerometer,
            accuracy: 3, // SENSOR_STATUS_ACCURACY_HIGH
            values: [0.0; 6],
        }
    }
}

impl Event {
    pub fn new_accelerometer(handle: i32, timestamp_ns: i64, x: f32, y: f32, z: f32) -> Self {
        Self {
            timestamp: timestamp_ns,
            sensor_handle: handle,
            sensor_type: SensorType::Accelerometer,
            accuracy: 3,
            values: [x, y, z, 0.0, 0.0, 0.0],
        }
    }

    pub fn new_gyroscope(handle: i32, timestamp_ns: i64, x: f32, y: f32, z: f32) -> Self {
        Self {
            timestamp: timestamp_ns,
            sensor_handle: handle,
            sensor_type: SensorType::Gyroscope,
            accuracy: 3,
            values: [x, y, z, 0.0, 0.0, 0.0],
        }
    }

    pub fn x(&self) -> f32 {
        self.values[0]
    }

    pub fn y(&self) -> f32 {
        self.values[1]
    }

    pub fn z(&self) -> f32 {
        self.values[2]
    }
}

impl Parcelable for Event {
    fn write_to_parcel(&self, parcel: &mut Parcel) -> AidlResult<()> {
        parcel
            .write_i64(self.timestamp)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.sensor_handle)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.sensor_type.into())
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.accuracy as i32)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        for &val in &self.values {
            parcel
                .write_f32(val)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }
        Ok(())
    }

    fn read_from_parcel(&mut self, parcel: &Parcel) -> AidlResult<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(&mut self, parcel: &Parcel, offset: &mut usize) -> AidlResult<()> {
        self.timestamp = parcel
            .read_i64(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.sensor_handle = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        let st_raw = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        self.sensor_type = SensorType::from(st_raw);
        self.accuracy = parcel
            .read_i32(offset)
            .map_err(|_| Status::from_status(STATUS_BAD_VALUE))? as i8;
        for i in 0..6 {
            self.values[i] = parcel
                .read_f32(offset)
                .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
        }
        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Sensor State for Rate Batching & Active Monitoring
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensorState {
    pub enabled: bool,
    pub sampling_period_ns: i64,
    pub max_report_latency_ns: i64,
    pub last_sample_time_ns: i64,
}

impl Default for SensorState {
    fn default() -> Self {
        Self {
            enabled: false,
            sampling_period_ns: 20_000_000, // 50 Hz (20ms)
            max_report_latency_ns: 0,
            last_sample_time_ns: 0,
        }
    }
}
