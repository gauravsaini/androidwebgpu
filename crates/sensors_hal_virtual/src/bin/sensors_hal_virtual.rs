//! Standalone Virtual Sensors HAL Daemon for AndroidWebGPU Guest Environment.

use binder_sys::{IPCThreadState, ProcessState};
use sensors_hal_virtual::{register_sensors_service, SensorsHalService};
use std::sync::Arc;

fn main() {
    let _process = ProcessState::init_with_driver("/dev/binder");
    let hal = Arc::new(SensorsHalService::new());

    // Register with ServiceManager (handle 0)
    register_sensors_service(hal).expect("Failed to register Sensors HAL");
    eprintln!("sensors_hal_virtual: ready");

    IPCThreadState::current(|state| {
        let _ = state.enter_looper();
    });
}
