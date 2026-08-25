//! Activity lifecycle state machine, task stacks, and activity record tracking.

use crate::error::{AmsError, AmsResult};
use crate::types::{ActivityRecord, ActivityState, TaskRecord};
use aidl_compat::pointer::SpIBinder;
use pms_rs::types::{ActivityInfo, ApplicationInfo, Intent};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, RwLock};

/// Compare two `SpIBinder` references for identity parity.
pub fn is_same_binder(a: &SpIBinder, b: &SpIBinder) -> bool {
    Arc::ptr_eq(a.as_arc(), b.as_arc())
        || (a.handle().is_some() && a.handle() == b.handle())
}

/// Validate whether a lifecycle state transition is legal according to Android lifecycle rules.
pub fn is_valid_transition(from: ActivityState, to: ActivityState) -> bool {
    match (from, to) {
        // Normal cold startup sequence
        (ActivityState::INITIALIZING, ActivityState::CREATED) => true,
        (ActivityState::CREATED, ActivityState::STARTED) => true,
        (ActivityState::STARTED, ActivityState::RESUMED) => true,

        // Focus loss & backgrounding
        (ActivityState::RESUMED, ActivityState::PAUSED) => true,
        (ActivityState::PAUSED, ActivityState::STOPPED) => true,

        // Returning to foreground
        (ActivityState::PAUSED, ActivityState::RESUMED) => true,
        (ActivityState::STOPPED, ActivityState::STARTED) => true,

        // Destruction sequence
        (ActivityState::PAUSED, ActivityState::DESTROYED) => true,
        (ActivityState::STOPPED, ActivityState::DESTROYED) => true,
        (ActivityState::CREATED, ActivityState::DESTROYED) => true,
        (ActivityState::STARTED, ActivityState::STOPPED) => true,
        (ActivityState::INITIALIZING, ActivityState::DESTROYED) => true,

        // Same state is idempotent
        (s1, s2) if s1 == s2 => true,

        _ => false,
    }
}

/// Activity Stack holding tasks and tracking top-most foreground activity.
pub struct ActivityStack {
    tasks: Vec<Arc<RwLock<TaskRecord>>>,
    next_task_id: AtomicI32,
}

impl ActivityStack {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            next_task_id: AtomicI32::new(1),
        }
    }

    /// Retrieve or create a task with the given affinity.
    pub fn get_or_create_task(&mut self, affinity: &str) -> Arc<RwLock<TaskRecord>> {
        for t in &self.tasks {
            let task = t.read().unwrap();
            if task.affinity == affinity {
                return t.clone();
            }
        }

        let task_id = self.next_task_id.fetch_add(1, Ordering::SeqCst);
        let task = Arc::new(RwLock::new(TaskRecord::new(task_id, affinity)));
        self.tasks.push(task.clone());
        task
    }

    /// Return reference to top task.
    pub fn top_task(&self) -> Option<Arc<RwLock<TaskRecord>>> {
        self.tasks.last().cloned()
    }

    /// Return reference to the top-most activity across all tasks.
    pub fn top_activity(&self) -> Option<Arc<RwLock<ActivityRecord>>> {
        for task_arc in self.tasks.iter().rev() {
            let task = task_arc.read().unwrap();
            if let Some(top) = task.top_activity() {
                return Some(top);
            }
        }
        None
    }

    /// Find activity record by token.
    pub fn find_activity_by_token(&self, token: &SpIBinder) -> Option<Arc<RwLock<ActivityRecord>>> {
        for task_arc in &self.tasks {
            let task = task_arc.read().unwrap();
            for act_arc in &task.activities {
                let act = act_arc.read().unwrap();
                if is_same_binder(&act.token, token) {
                    return Some(act_arc.clone());
                }
            }
        }
        None
    }

    /// Add an activity record to the appropriate task stack.
    pub fn add_activity(&mut self, record: Arc<RwLock<ActivityRecord>>) {
        let affinity = {
            let act = record.read().unwrap();
            act.package_name.clone()
        };
        let task = self.get_or_create_task(&affinity);
        let mut t = task.write().unwrap();
        t.push_activity(record);
    }

    /// Remove an activity by token. Clean up empty tasks.
    pub fn remove_activity(&mut self, token: &SpIBinder) -> Option<Arc<RwLock<ActivityRecord>>> {
        let mut removed = None;
        for task_arc in &self.tasks {
            let mut task = task_arc.write().unwrap();
            if let Some(act) = task.remove_activity(token) {
                removed = Some(act);
                break;
            }
        }

        // Clean up empty tasks
        self.tasks.retain(|t| !t.read().unwrap().is_empty());
        removed
    }
}

/// Lifecycle Manager managing state transitions and stack updates.
pub struct LifecycleManager {
    stack: Arc<RwLock<ActivityStack>>,
}

impl LifecycleManager {
    pub fn new() -> Self {
        Self {
            stack: Arc::new(RwLock::new(ActivityStack::new())),
        }
    }

    /// Create and register a new activity in `INITIALIZING` state.
    pub fn create_activity(
        &self,
        token: SpIBinder,
        intent: Intent,
        activity_info: ActivityInfo,
        app_info: ApplicationInfo,
    ) -> Arc<RwLock<ActivityRecord>> {
        let mut stack = self.stack.write().unwrap();
        let task = stack.get_or_create_task(&activity_info.package_name);
        let task_id = task.read().unwrap().task_id;

        let record = Arc::new(RwLock::new(ActivityRecord::new(
            token,
            intent,
            activity_info,
            app_info,
            task_id,
        )));

        stack.add_activity(record.clone());
        record
    }

    /// Return the current top-most activity.
    pub fn top_activity(&self) -> Option<Arc<RwLock<ActivityRecord>>> {
        let stack = self.stack.read().unwrap();
        stack.top_activity()
    }

    /// Find activity record by token.
    pub fn find_activity(&self, token: &SpIBinder) -> Option<Arc<RwLock<ActivityRecord>>> {
        let stack = self.stack.read().unwrap();
        stack.find_activity_by_token(token)
    }

    /// Drive a state transition for the given activity token.
    pub fn transition_activity(
        &self,
        token: &SpIBinder,
        target_state: ActivityState,
    ) -> AmsResult<ActivityState> {
        let act_arc = self
            .find_activity(token)
            .ok_or_else(|| AmsError::ActivityNotFound("Unknown token".to_string()))?;

        let mut act = act_arc.write().unwrap();
        let current_state = act.state;

        if !is_valid_transition(current_state, target_state) {
            return Err(AmsError::InvalidStateTransition {
                from: current_state,
                to: target_state,
            });
        }

        act.state = target_state;
        match target_state {
            ActivityState::RESUMED => {
                act.visible = true;
            }
            ActivityState::STOPPED => {
                act.visible = false;
            }
            ActivityState::DESTROYED => {
                act.visible = false;
                act.finishing = true;
            }
            _ => {}
        }

        Ok(target_state)
    }

    /// Notification that client activity has reached `RESUMED` state.
    pub fn record_activity_resumed(&self, token: &SpIBinder) -> AmsResult<()> {
        let _ = self.transition_activity(token, ActivityState::RESUMED)?;
        Ok(())
    }

    /// Notification that client activity has reached `PAUSED` state.
    pub fn record_activity_paused(&self, token: &SpIBinder) -> AmsResult<()> {
        let _ = self.transition_activity(token, ActivityState::PAUSED)?;
        Ok(())
    }

    /// Notification that client activity has reached `STOPPED` state.
    pub fn record_activity_stopped(&self, token: &SpIBinder) -> AmsResult<()> {
        let _ = self.transition_activity(token, ActivityState::STOPPED)?;
        Ok(())
    }

    /// Finish an activity and transition to `DESTROYED`.
    pub fn finish_activity(&self, token: &SpIBinder) -> AmsResult<bool> {
        let act_arc = match self.find_activity(token) {
            Some(a) => a,
            None => return Ok(false),
        };

        {
            let mut act = act_arc.write().unwrap();
            act.finishing = true;
            act.state = ActivityState::DESTROYED;
            act.visible = false;
        }

        let mut stack = self.stack.write().unwrap();
        stack.remove_activity(token);

        Ok(true)
    }
}
