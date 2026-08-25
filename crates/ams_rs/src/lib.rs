//! # ams_rs
//!
//! Native Rust Android Activity Manager Service (`android.app.IActivityManager`)
//! and Activity Lifecycle State Machine for AndroidWebGPU.
//!
//! Features:
//! - Full Activity Lifecycle state machine (`INITIALIZING` -> `CREATED` -> `STARTED` -> `RESUMED` -> `PAUSED` -> `STOPPED` -> `DESTROYED`).
//! - AIDL `IActivityManager` implementation answering `startActivity`, `attachApplication`,
//!   `activityResumed`, `activityPaused`, `activityStopped`, `finishActivity`.
//! - AIDL `IApplicationThread` proxy & stub for process binding and lifecycle callbacks.
//! - Direct integration with `pms_rs::IPackageManager` for component and intent resolution.
//! - Direct integration with `zygote_client::ZygoteClient` for process spawning.
//! - Direct AIDL IPC over `binder_sys::BinderKernelTransport` registered as `"activity"`.

pub mod activity_manager;
pub mod app_thread;
pub mod error;
pub mod lifecycle;
pub mod types;

// -----------------------------------------------------------------------------
// Top-Level Public Exports
// -----------------------------------------------------------------------------

pub use activity_manager::{
    iactivity_manager_codes, register_activity_service, ActivityManagerClient,
    ActivityManagerService, IActivityManager, IACTIVITY_MANAGER_DESCRIPTOR,
};

pub use app_thread::{
    application_thread_codes, ActivityTokenBinder, ApplicationThreadProxy, ClientTransaction,
    ClientTransactionItem, IApplicationThread, MockApplicationThread,
    IAPPLICATION_THREAD_DESCRIPTOR,
};

pub use error::{AmsError, AmsResult};

pub use lifecycle::{is_same_binder, is_valid_transition, ActivityStack, LifecycleManager};

pub use types::{
    ActivityRecord, ActivityState, PendingLaunch, TaskRecord, START_CLASS_NOT_FOUND,
    START_DELIVERED_TO_TOP, START_INTENT_NOT_RESOLVED, START_PERMISSION_DENIED,
    START_RETURN_INTENT_TO_CALLER, START_SUCCESS, START_TASK_TO_FRONT,
};
