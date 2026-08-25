use sensor_host_rs::SensorHostBridge;
use sensors_hal_virtual::{ISensors, SensorsHalService};
use std::sync::Arc;

#[test]
fn test_sensor_host_streaming_end_to_end() {
    let hal = Arc::new(SensorsHalService::new());
    let bridge = SensorHostBridge::new(Arc::clone(&hal));

    // Turn on both accelerometer and gyroscope
    hal.activate(1, true).unwrap();
    hal.batch(1, 10_000_000, 0).unwrap(); // 100 Hz
    hal.activate(2, true).unwrap();
    hal.batch(2, 20_000_000, 0).unwrap(); // 50 Hz

    // Simulate 100ms run (10ms steps)
    let mut total_accel = 0;
    let mut total_gyro = 0;

    for i in 1..=10 {
        let t_ns = i * 10_000_000;
        bridge.tick(t_ns);
        let events = hal.poll_events(10);
        for ev in events {
            if ev.sensor_handle == 1 {
                total_accel += 1;
            } else if ev.sensor_handle == 2 {
                total_gyro += 1;
            }
        }
    }

    assert_eq!(total_accel, 10, "Should generate 10 accelerometer samples for 100ms at 100Hz");
    assert_eq!(total_gyro, 5, "Should generate 5 gyroscope samples for 100ms at 50Hz");
    assert_eq!(bridge.total_events_dispatched(), 15);
}
