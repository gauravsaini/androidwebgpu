//! Utility binary that pings a given Binder handle and exits 0 if alive, 1 if dead.

use binder_sys::{IPCThreadState, ProcessState};

fn main() {
    let handle: u32 = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(0);

    let _process = ProcessState::init_with_driver("/dev/binder");
    let alive = IPCThreadState::current(|s| s.ping(handle)).is_ok();

    if alive {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}
