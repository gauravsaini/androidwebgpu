//! Standalone Virtual Camera HAL Daemon for AndroidWebGPU Guest Environment.

use binder_sys::{IPCThreadState, ProcessState};
use camera_hal_virtual::{register_camera_provider_service, CameraProviderService};
use std::sync::Arc;

fn main() {
    let _process = ProcessState::init_with_driver("/dev/binder");
    let provider = Arc::new(CameraProviderService::new());

    // Register with ServiceManager (handle 0)
    register_camera_provider_service(provider).expect("Failed to register Camera Provider HAL");
    eprintln!("camera_hal_virtual: ready");

    IPCThreadState::current(|state| {
        let _ = state.enter_looper();
    });
}
