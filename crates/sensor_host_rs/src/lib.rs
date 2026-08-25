//! # sensor_host_rs
//!
//! Host-Side Browser Sensor API / `window.ondevicemotion` stream bridge and synthetic
//! physical motion generator for AndroidWebGPU Sensors HAL.

pub mod bridge;
pub mod generator;

pub use bridge::{SensorHostBridge, SensorHostMode};
pub use generator::{SensorMotionGenerator, EARTH_GRAVITY};

#[cfg(test)]
mod tests {
    use super::*;
    use sensors_hal_virtual::{ISensors, SensorsHalService};
    use std::sync::Arc;

    #[test]
    fn test_sensor_host_bridge_synthetic_motion_tick() {
        let hal = Arc::new(SensorsHalService::new());
        let bridge = SensorHostBridge::new(Arc::clone(&hal));

        // Activate sensors with 10ms sampling interval (100Hz = 10_000_000 ns)
        hal.activate(1, true).unwrap();
        hal.batch(1, 10_000_000, 0).unwrap();
        hal.activate(2, true).unwrap();
        hal.batch(2, 10_000_000, 0).unwrap();

        // Tick at t = 0
        let generated = bridge.tick(10_000_000);
        assert_eq!(generated, 2);

        // Tick at t = 5ms (< period) => no new events
        let generated_sub = bridge.tick(15_000_000);
        assert_eq!(generated_sub, 0);

        // Tick at t = 20ms (> period) => 2 new events
        let generated_next = bridge.tick(20_000_000);
        assert_eq!(generated_next, 2);

        let events = hal.poll_events(10);
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].sensor_handle, 1);
        assert_eq!(events[1].sensor_handle, 2);
    }

    #[test]
    fn test_sensor_host_bridge_devicemotion_injection() {
        let hal = Arc::new(SensorsHalService::new());
        let bridge = SensorHostBridge::new(Arc::clone(&hal));
        bridge.set_mode(SensorHostMode::BrowserDeviceMotion);

        hal.activate(1, true).unwrap();
        hal.activate(2, true).unwrap();

        let count = bridge.inject_devicemotion(
            1_000_000_000,
            0.12, 9.80, -0.05,
            0.01, 0.02, -0.01,
        );
        assert_eq!(count, 2);

        let events = hal.poll_events(5);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sensor_handle, 1);
        assert_eq!(events[0].x(), 0.12);
        assert_eq!(events[0].y(), 9.80);
        assert_eq!(events[1].sensor_handle, 2);
        assert_eq!(events[1].x(), 0.01);
    }
}
