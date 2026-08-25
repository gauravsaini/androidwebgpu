//! Sensor Host Bridge connecting Browser Sensor API / devicemotion to guest Sensors HAL.

use crate::generator::SensorMotionGenerator;
use sensors_hal_virtual::{Event, SensorsHalService};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorHostMode {
    SyntheticMotion,
    BrowserDeviceMotion,
    StaticGravity,
}

pub struct SensorHostBridge {
    hal_service: Arc<SensorsHalService>,
    generator: SensorMotionGenerator,
    mode: RwLock<SensorHostMode>,
    last_sample_times_ns: Mutex<HashMap<i32, i64>>,
    total_events_dispatched: Mutex<u64>,
}

impl SensorHostBridge {
    /// Create new SensorHostBridge attached to the given `SensorsHalService`.
    pub fn new(hal_service: Arc<SensorsHalService>) -> Self {
        Self {
            hal_service,
            generator: SensorMotionGenerator::new(),
            mode: RwLock::new(SensorHostMode::SyntheticMotion),
            last_sample_times_ns: Mutex::new(HashMap::new()),
            total_events_dispatched: Mutex::new(0),
        }
    }

    /// Set bridge operational mode.
    pub fn set_mode(&self, mode: SensorHostMode) {
        let mut m = self.mode.write().unwrap();
        *m = mode;
    }

    /// Get current operational mode.
    pub fn mode(&self) -> SensorHostMode {
        *self.mode.read().unwrap()
    }

    /// Access reference to underlying SensorsHalService.
    pub fn hal_service(&self) -> &Arc<SensorsHalService> {
        &self.hal_service
    }

    /// Total count of events dispatched to HAL.
    pub fn total_events_dispatched(&self) -> u64 {
        *self.total_events_dispatched.lock().unwrap()
    }

    /// Process devicemotion packet from browser / host frontend.
    #[allow(clippy::too_many_arguments)]
    pub fn inject_devicemotion(
        &self,
        timestamp_ns: i64,
        ax: f32,
        ay: f32,
        az: f32,
        gx: f32,
        gy: f32,
        gz: f32,
    ) -> usize {
        let mut count = 0;

        // Accelerometer (handle 1)
        if self.hal_service.is_sensor_active(1) {
            let ev = Event::new_accelerometer(1, timestamp_ns, ax, ay, az);
            if self.hal_service.push_event(ev).is_ok() {
                count += 1;
            }
        }

        // Gyroscope (handle 2)
        if self.hal_service.is_sensor_active(2) {
            let ev = Event::new_gyroscope(2, timestamp_ns, gx, gy, gz);
            if self.hal_service.push_event(ev).is_ok() {
                count += 1;
            }
        }

        *self.total_events_dispatched.lock().unwrap() += count as u64;
        count
    }

    /// Periodic tick from host timer loop at `now_ns` timestamp.
    /// Evaluates rate batching and generates synthetic samples if sensors are active.
    pub fn tick(&self, now_ns: i64) -> usize {
        let mode = self.mode();
        if mode == SensorHostMode::BrowserDeviceMotion {
            return 0; // Driven externally by inject_devicemotion
        }

        let mut events_generated = 0;
        let mut last_times = self.last_sample_times_ns.lock().unwrap();

        // 1. Check Accelerometer (handle 1)
        if self.hal_service.is_sensor_active(1) {
            let state = self.hal_service.get_sensor_state(1).unwrap_or_default();
            let period_ns = state.sampling_period_ns.max(1_000_000); // minimum 1ms
            let last_t = last_times.get(&1).copied().unwrap_or(0);

            if now_ns - last_t >= period_ns {
                let t_sec = (now_ns as f32) / 1_000_000_000.0;
                let shake = mode == SensorHostMode::SyntheticMotion;
                let (ax, ay, az) = self.generator.generate_accelerometer(t_sec, shake);

                let ev = Event::new_accelerometer(1, now_ns, ax, ay, az);
                if self.hal_service.push_event(ev).is_ok() {
                    events_generated += 1;
                    last_times.insert(1, now_ns);
                }
            }
        }

        // 2. Check Gyroscope (handle 2)
        if self.hal_service.is_sensor_active(2) {
            let state = self.hal_service.get_sensor_state(2).unwrap_or_default();
            let period_ns = state.sampling_period_ns.max(1_000_000);
            let last_t = last_times.get(&2).copied().unwrap_or(0);

            if now_ns - last_t >= period_ns {
                let t_sec = (now_ns as f32) / 1_000_000_000.0;
                let rotate = mode == SensorHostMode::SyntheticMotion;
                let (gx, gy, gz) = self.generator.generate_gyroscope(t_sec, rotate);

                let ev = Event::new_gyroscope(2, now_ns, gx, gy, gz);
                if self.hal_service.push_event(ev).is_ok() {
                    events_generated += 1;
                    last_times.insert(2, now_ns);
                }
            }
        }

        *self.total_events_dispatched.lock().unwrap() += events_generated as u64;
        events_generated
    }
}
