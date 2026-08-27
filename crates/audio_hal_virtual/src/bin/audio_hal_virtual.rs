//! Standalone Virtual Audio HAL Daemon for AndroidWebGPU Guest Environment.

use audio_hal_virtual::{register_audio_service, AudioModuleService};
use binder_sys::{IPCThreadState, ProcessState};
use std::sync::Arc;

fn main() {
    let _process = ProcessState::init_with_driver("/dev/binder");
    let hal = Arc::new(AudioModuleService::new());

    // Register with ServiceManager (handle 0)
    register_audio_service(hal).expect("Failed to register Audio HAL");
    eprintln!("audio_hal_virtual: ready");

    IPCThreadState::current(|state| {
        let _ = state.enter_looper();
    });
}
