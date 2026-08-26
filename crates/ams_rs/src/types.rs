//! Core AMS and Activity Lifecycle data structures and state constants.

use aidl_compat::pointer::SpIBinder;
use pms_rs::types::{ActivityInfo, ApplicationInfo, ComponentName, Intent};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

// -----------------------------------------------------------------------------
// ActivityManager Return Codes (AOSP android.app.ActivityManager)
// -----------------------------------------------------------------------------

/// Activity started successfully.
pub const START_SUCCESS: i32 = 0;
/// Result returned to caller without starting new activity.
pub const START_RETURN_INTENT_TO_CALLER: i32 = 1;
/// Existing task brought to front.
pub const START_TASK_TO_FRONT: i32 = 2;
/// Intent delivered to top activity via `onNewIntent`.
pub const START_DELIVERED_TO_TOP: i32 = 3;
/// Intent could not be resolved to an installed activity.
pub const START_INTENT_NOT_RESOLVED: i32 = -1;
/// Component class could not be found.
pub const START_CLASS_NOT_FOUND: i32 = -2;
/// Caller lacks permission to start requested activity.
pub const START_PERMISSION_DENIED: i32 = -4;

// -----------------------------------------------------------------------------
// Activity Lifecycle State Machine
// -----------------------------------------------------------------------------

/// Android Activity Lifecycle States.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActivityState {
    /// Initial state before creation.
    INITIALIZING,
    /// Created (`onCreate`).
    CREATED,
    /// Started (`onStart`), visible but not in foreground focus.
    STARTED,
    /// Resumed (`onResume`), active and interactive in foreground.
    RESUMED,
    /// Paused (`onPause`), losing focus.
    PAUSED,
    /// Stopped (`onStop`), invisible in background.
    STOPPED,
    /// Destroyed (`onDestroy`), terminal state before release.
    DESTROYED,
}

impl std::fmt::Display for ActivityState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActivityState::INITIALIZING => write!(f, "INITIALIZING"),
            ActivityState::CREATED => write!(f, "CREATED"),
            ActivityState::STARTED => write!(f, "STARTED"),
            ActivityState::RESUMED => write!(f, "RESUMED"),
            ActivityState::PAUSED => write!(f, "PAUSED"),
            ActivityState::STOPPED => write!(f, "STOPPED"),
            ActivityState::DESTROYED => write!(f, "DESTROYED"),
        }
    }
}

// -----------------------------------------------------------------------------
// Activity Record
// -----------------------------------------------------------------------------

/// Record representing a specific instance of an Activity in the system.
#[derive(Clone)]
pub struct ActivityRecord {
    /// Unique Binder token identifying this activity instance across IPC.
    pub token: SpIBinder,
    /// Intent that triggered the launch of this activity.
    pub intent: Intent,
    /// Manifest Activity metadata.
    pub activity_info: ActivityInfo,
    /// Manifest Application metadata.
    pub app_info: ApplicationInfo,
    /// Current activity lifecycle state.
    pub state: ActivityState,
    /// ID of the task stack containing this activity.
    pub task_id: i32,
    /// Process ID hosting this activity if currently bound.
    pub pid: Option<u32>,
    /// Nice process name.
    pub process_name: String,
    /// Package name.
    pub package_name: String,
    /// Component Name.
    pub component_name: ComponentName,
    /// Caller result token if started with `startActivityForResult`.
    pub result_to: Option<SpIBinder>,
    /// Caller result identity string.
    pub result_who: Option<String>,
    /// Request code for activity result.
    pub request_code: i32,
    /// Whether the activity window is currently visible.
    pub visible: bool,
    /// Whether the activity is currently finishing / in destruction.
    pub finishing: bool,
    /// Timestamp when activity record was created.
    pub launch_time: SystemTime,
}

impl ActivityRecord {
    /// Create a new activity record in `INITIALIZING` state.
    pub fn new(
        token: SpIBinder,
        intent: Intent,
        activity_info: ActivityInfo,
        app_info: ApplicationInfo,
        task_id: i32,
    ) -> Self {
        let pkg = activity_info.package_name.clone();
        let cls = activity_info.name.clone();
        let process_name = app_info.name.clone().unwrap_or_else(|| pkg.clone());
        let comp = ComponentName::new(pkg.clone(), cls);

        Self {
            token,
            intent,
            activity_info,
            app_info,
            state: ActivityState::INITIALIZING,
            task_id,
            pid: None,
            process_name,
            package_name: pkg,
            component_name: comp,
            result_to: None,
            result_who: None,
            request_code: -1,
            visible: false,
            finishing: false,
            #[cfg(target_arch = "wasm32")]
            launch_time: SystemTime::UNIX_EPOCH,
            #[cfg(not(target_arch = "wasm32"))]
            launch_time: SystemTime::now(),
        }
    }
}

// -----------------------------------------------------------------------------
// Task Record
// -----------------------------------------------------------------------------

/// Record representing a task (back-stack of activities).
pub struct TaskRecord {
    pub task_id: i32,
    pub affinity: String,
    pub activities: Vec<Arc<RwLock<ActivityRecord>>>,
}

impl TaskRecord {
    pub fn new(task_id: i32, affinity: impl Into<String>) -> Self {
        Self {
            task_id,
            affinity: affinity.into(),
            activities: Vec::new(),
        }
    }

    /// Return reference to top-most activity in this task.
    pub fn top_activity(&self) -> Option<Arc<RwLock<ActivityRecord>>> {
        self.activities.last().cloned()
    }

    /// Add an activity to the top of the stack.
    pub fn push_activity(&mut self, record: Arc<RwLock<ActivityRecord>>) {
        self.activities.push(record);
    }

    /// Remove activity by token from this task stack.
    pub fn remove_activity(&mut self, token: &SpIBinder) -> Option<Arc<RwLock<ActivityRecord>>> {
        if let Some(pos) = self.activities.iter().position(|a| {
            let act = a.read().unwrap();
            Arc::ptr_eq(act.token.as_arc(), token.as_arc()) || (act.token.handle().is_some() && act.token.handle() == token.handle())
        }) {
            Some(self.activities.remove(pos))
        } else {
            None
        }
    }

    /// Check if task stack is empty.
    pub fn is_empty(&self) -> bool {
        self.activities.is_empty()
    }
}

// -----------------------------------------------------------------------------
// Pending App Launch
// -----------------------------------------------------------------------------

/// Represents an activity waiting for `attachApplication` from newly spawned process.
pub struct PendingLaunch {
    pub pid: u32,
    pub package_name: String,
    pub process_name: String,
    pub activity_record: Arc<RwLock<ActivityRecord>>,
}
