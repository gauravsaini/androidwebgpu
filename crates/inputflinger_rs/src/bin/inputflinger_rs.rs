//! Standalone InputFlinger Daemon for AndroidWebGPU Guest Environment.

use binder_sys::{IPCThreadState, ProcessState};
use inputflinger_rs::{register_input_service, InputManagerService};
use std::sync::Arc;

fn main() {
    let _process = ProcessState::init_with_driver("/dev/binder");
    let input_service = Arc::new(InputManagerService::new());

    // Register with ServiceManager (handle 0)
    register_input_service(input_service).expect("Failed to register InputFlinger");
    eprintln!("inputflinger_rs: ready");

    IPCThreadState::current(|state| {
        let _ = state.enter_looper();
    });
}
