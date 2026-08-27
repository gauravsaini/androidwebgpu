//! Standalone WindowManagerService Daemon for AndroidWebGPU Guest Environment.

use binder_sys::{IPCThreadState, ProcessState};
use std::sync::Arc;
use wms_rs::{register_window_service, WindowManagerService};

fn main() {
    let _process = ProcessState::init_with_driver("/dev/binder");
    let wms = Arc::new(WindowManagerService::new());

    // Register with ServiceManager (handle 0)
    register_window_service(wms).expect("Failed to register WMS");
    eprintln!("wms_rs: ready");

    IPCThreadState::current(|state| {
        let _ = state.enter_looper();
    });
}
