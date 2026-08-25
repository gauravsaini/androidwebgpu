//! # zygote_client
//!
//! Native Android Zygote abstract unix domain socket client and process lifecycle
//! tracking for AndroidWebGPU.
//!
//! Features:
//! - Zygote protocol argument encoder formatting commands (`--setuid=10000`,
//!   `--setgid=10000`, `--target-sdk-version=33`, `--package-name=<pkg>`,
//!   `--nice-name=<pkg>`, `android.app.ActivityThread`).
//! - 4-byte little-endian PID response parsing and error propagation.
//! - In-memory mock transport for host unit testing on macOS/Linux.
//! - Thread-safe `ProcessTracker` maintaining process records and lifecycle states.

pub mod error;
pub mod process;
pub mod protocol;
pub mod socket;

pub use error::{ZygoteError, ZygoteResult};
pub use process::{ProcessRecord, ProcessState, ProcessTracker};
pub use protocol::{
    format_pid_response, parse_pid_response, ZygoteSpawnArgs, DEFAULT_APP_GID, DEFAULT_APP_UID,
    DEFAULT_ENTRY_POINT, DEFAULT_TARGET_SDK_VERSION,
};
pub use socket::{
    MockZygoteHandler, ZygoteClient, ZygoteEndpoint, ZygoteMockHandler,
    DEFAULT_ZYGOTE_SOCKET_ABSTRACT, DEFAULT_ZYGOTE_SOCKET_PATH,
};
