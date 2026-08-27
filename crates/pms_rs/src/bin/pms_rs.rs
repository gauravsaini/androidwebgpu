//! Standalone PackageManagerService Daemon for AndroidWebGPU Guest Environment.

use binder_sys::{IPCThreadState, ProcessState};
use pms_rs::{register_package_service, PackageManagerService};
use std::sync::Arc;

fn main() {
    let _process = ProcessState::init_with_driver("/dev/binder");
    let pms = Arc::new(PackageManagerService::new());

    // Register with ServiceManager (handle 0)
    register_package_service(pms).expect("Failed to register PMS");
    eprintln!("pms_rs: ready");

    IPCThreadState::current(|state| {
        let _ = state.enter_looper();
    });
}
