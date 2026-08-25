//! # sensors_hal_virtual
//!
//! Virtual Android 13 AIDL Sensors HAL (`android.hardware.sensors.ISensors`) implementation
//! for AndroidWebGPU. Provides server stubs, client proxies, event queue buffering,
//! and handle 0 ServiceManager registration under `"android.hardware.sensors.ISensors/default"`.

pub mod error;
pub mod event_queue;
pub mod proxy;
pub mod sensors_service;
pub mod types;

// -----------------------------------------------------------------------------
// Top-Level Public Exports
// -----------------------------------------------------------------------------

pub use error::SensorsHalError;
pub use event_queue::{EventQueue, WakeLockQueue, DEFAULT_EVENT_QUEUE_CAPACITY};
pub use proxy::SensorsProxy;
pub use sensors_service::{
    isensors_codes, register_sensors_service, ISensors, SensorsHalService,
    ISENSORS_DEFAULT_INSTANCE, ISENSORS_DESCRIPTOR,
};
pub use types::{Event, OperationMode, SensorInfo, SensorState, SensorType};

#[cfg(test)]
mod tests {
    use super::*;
    use aidl_compat::traits::Interface;
    use std::sync::Arc;

    #[test]
    fn test_sensors_list_and_activation() {
        let service = Arc::new(SensorsHalService::new());
        let list = service.get_sensors_list().expect("Get sensors list failed");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].sensor_handle, 1);
        assert_eq!(list[0].sensor_type, SensorType::Accelerometer);
        assert_eq!(list[1].sensor_handle, 2);
        assert_eq!(list[1].sensor_type, SensorType::Gyroscope);

        // Initially inactive
        assert!(!service.is_sensor_active(1));
        assert!(!service.is_sensor_active(2));

        // Activate accelerometer
        service.activate(1, true).expect("Activation failed");
        assert!(service.is_sensor_active(1));

        // Batch rate setting
        service.batch(1, 10_000_000, 0).expect("Batch failed");
        let state = service.get_sensor_state(1).unwrap();
        assert_eq!(state.sampling_period_ns, 10_000_000);

        // Deactivate
        service.activate(1, false).expect("Deactivation failed");
        assert!(!service.is_sensor_active(1));
    }

    #[test]
    fn test_proxy_transact_roundtrip() {
        let service = Arc::new(SensorsHalService::new());
        let proxy = SensorsProxy::new(service.as_binder());

        let list = proxy.get_sensors_list().expect("Proxy getSensorsList failed");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Virtual Accelerometer");
        assert_eq!(list[1].name, "Virtual Gyroscope");

        proxy.activate(1, true).expect("Proxy activate failed");
        assert!(service.is_sensor_active(1));

        proxy.batch(1, 5_000_000, 100_000).expect("Proxy batch failed");
        assert_eq!(
            service.get_sensor_state(1).unwrap().sampling_period_ns,
            5_000_000
        );

        proxy.set_operation_mode(OperationMode::DataInjection).unwrap();
        let ev = Event::new_accelerometer(1, 1_000_000_000, 0.0, 9.81, 0.0);
        proxy.inject_sensor_data(&ev).expect("Inject failed");

        let polled = service.poll_events(10);
        assert_eq!(polled.len(), 1);
        assert_eq!(polled[0].timestamp, 1_000_000_000);
        assert_eq!(polled[0].y(), 9.81);
    }
}
