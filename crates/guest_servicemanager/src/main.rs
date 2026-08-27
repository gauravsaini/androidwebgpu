//! Standalone ServiceManager Daemon for AndroidWebGPU Guest Environment (Root Handle 0).

use aidl_compat::stub::Binder;
use binder_sys::{IPCThreadState, ProcessState, ServiceManagerServer};

fn main() {
    // 1. Open /dev/binder
    let process = ProcessState::init_with_driver("/dev/binder");

    // 2. Claim context manager (handle 0) via BINDER_SET_CONTEXT_MGR ioctl
    if let Err(e) = process.become_context_manager() {
        eprintln!("[servicemanager] Warning: become_context_manager: {}", e);
    }

    // 3. Create ServiceManager server
    let sm = Binder::new(ServiceManagerServer::new());

    // 4. Register self at handle 0
    process.register_as_binder(sm);

    eprintln!("[servicemanager] context manager ready (handle 0)");

    // 5. Enter binder looper
    IPCThreadState::current(|state| {
        let _ = state.enter_looper();
    });
}
