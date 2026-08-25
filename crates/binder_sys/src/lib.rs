//! # binder_sys
//!
//! Direct Linux Kernel Binder Userspace Transport, Shared Memory Management,
//! Looper Threadpool, and Handle 0 ServiceManager Client for AndroidWebGPU.

pub mod driver;
pub mod ioctl;
pub mod ipc_thread_state;
pub mod looper;
pub mod mmap;
pub mod mock_driver;
pub mod process_state;
pub mod service_manager;
pub mod sys;
pub mod transport;

// -----------------------------------------------------------------------------
// Top-Level Public Exports
// -----------------------------------------------------------------------------

pub use driver::{BinderDriverBackend, LinuxBinderDriver, MockDriverBackend};
pub use ioctl::*;
pub use ipc_thread_state::IPCThreadState;
pub use mmap::{
    BinderMmapRegion, MmapError, BINDER_DEFAULT_MMAP_SIZE, BINDER_MAX_MMAP_SIZE,
    BINDER_MIN_MMAP_SIZE,
};
pub use mock_driver::{DriverError, MockBinderDriver, MockClientProcess};
pub use process_state::ProcessState;
pub use service_manager::{
    add_service, check_service, default_service_manager, get_service, IServiceManager,
    MockServiceManager, ServiceManagerClient, ADD_SERVICE_TRANSACTION, CHECK_SERVICE_TRANSACTION,
    DUMP_FLAG_PRIORITY_ALL, DUMP_FLAG_PRIORITY_CRITICAL, DUMP_FLAG_PRIORITY_DEFAULT,
    DUMP_FLAG_PRIORITY_HIGH, DUMP_FLAG_PRIORITY_NORMAL, GET_SERVICE_TRANSACTION,
    IS_DECLARED_TRANSACTION, LIST_SERVICES_TRANSACTION, REGISTER_FOR_NOTIFICATIONS_TRANSACTION,
    SERVICE_MANAGER_DESCRIPTOR, UNREGISTER_FOR_NOTIFICATIONS_TRANSACTION,
};
pub use transport::BinderKernelTransport;
