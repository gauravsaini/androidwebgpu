//! Process lifecycle tracking, state transitions, and process registry.

use crate::error::{ZygoteError, ZygoteResult};
use crate::protocol::ZygoteSpawnArgs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

/// Lifecycle state of a spawned application process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessState {
    /// Process has been forked from Zygote and is initializing runtime.
    Starting,
    /// Process has attached to ActivityManager and is actively running.
    Running,
    /// Process exited normally or with a non-zero exit code.
    Exited { exit_code: Option<i32> },
    /// Process was explicitly terminated/killed by system.
    Killed,
}

/// Metadata and runtime state record for an application process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRecord {
    /// Assigned Linux Process ID.
    pub pid: u32,
    /// Android package name.
    pub package_name: String,
    /// Nice process name.
    pub nice_name: String,
    /// User ID assigned to process.
    pub uid: u32,
    /// Primary Group ID assigned to process.
    pub gid: u32,
    /// Target SDK version.
    pub target_sdk_version: u32,
    /// Epoch timestamp when process was spawned.
    pub spawn_time: SystemTime,
    /// Current lifecycle state.
    pub state: ProcessState,
}

impl ProcessRecord {
    /// Construct a new process record from spawn arguments and assigned PID.
    pub fn from_spawn_args(pid: u32, args: &ZygoteSpawnArgs) -> Self {
        Self {
            pid,
            package_name: args.package_name.clone(),
            nice_name: args.nice_name.clone(),
            uid: args.uid,
            gid: args.gid,
            target_sdk_version: args.target_sdk_version,
            spawn_time: SystemTime::now(),
            state: ProcessState::Starting,
        }
    }

    /// Check if process is currently active (Starting or Running).
    pub fn is_alive(&self) -> bool {
        matches!(self.state, ProcessState::Starting | ProcessState::Running)
    }
}

/// Thread-safe in-memory process tracking registry.
#[derive(Debug, Clone, Default)]
pub struct ProcessTracker {
    processes: Arc<RwLock<HashMap<u32, ProcessRecord>>>,
}

impl ProcessTracker {
    /// Create a new empty process tracker.
    pub fn new() -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a newly spawned process.
    pub fn register_process(&self, record: ProcessRecord) -> ZygoteResult<()> {
        let mut map = self.processes.write().unwrap();
        if map.contains_key(&record.pid) {
            return Err(ZygoteError::ProcessAlreadyExists(record.pid));
        }
        map.insert(record.pid, record);
        Ok(())
    }

    /// Retrieve process record by PID.
    pub fn get_process(&self, pid: u32) -> Option<ProcessRecord> {
        let map = self.processes.read().unwrap();
        map.get(&pid).cloned()
    }

    /// Retrieve first active process record by package name.
    pub fn get_process_by_package(&self, package_name: &str) -> Option<ProcessRecord> {
        let map = self.processes.read().unwrap();
        map.values()
            .find(|p| p.package_name == package_name && p.is_alive())
            .cloned()
    }

    /// Retrieve first active process record by nice name / process name.
    pub fn get_process_by_nice_name(&self, nice_name: &str) -> Option<ProcessRecord> {
        let map = self.processes.read().unwrap();
        map.values()
            .find(|p| p.nice_name == nice_name && p.is_alive())
            .cloned()
    }

    /// Update the lifecycle state of a tracked process.
    pub fn update_process_state(&self, pid: u32, new_state: ProcessState) -> ZygoteResult<()> {
        let mut map = self.processes.write().unwrap();
        let record = map
            .get_mut(&pid)
            .ok_or(ZygoteError::ProcessNotFound(pid))?;
        record.state = new_state;
        Ok(())
    }

    /// Terminate/kill a process and update its record state.
    pub fn kill_process(&self, pid: u32) -> ZygoteResult<()> {
        let mut map = self.processes.write().unwrap();
        let record = map
            .get_mut(&pid)
            .ok_or(ZygoteError::ProcessNotFound(pid))?;
        record.state = ProcessState::Killed;
        Ok(())
    }

    /// Remove a process record completely from tracking.
    pub fn remove_process(&self, pid: u32) -> Option<ProcessRecord> {
        let mut map = self.processes.write().unwrap();
        map.remove(&pid)
    }

    /// Return snapshot list of all tracked processes.
    pub fn list_all_processes(&self) -> Vec<ProcessRecord> {
        let map = self.processes.read().unwrap();
        map.values().cloned().collect()
    }

    /// Return list of currently alive (Starting or Running) processes.
    pub fn list_alive_processes(&self) -> Vec<ProcessRecord> {
        let map = self.processes.read().unwrap();
        map.values()
            .filter(|p| p.is_alive())
            .cloned()
            .collect()
    }

    /// Return count of tracked processes.
    pub fn count(&self) -> usize {
        let map = self.processes.read().unwrap();
        map.len()
    }

    /// Clear all tracked processes.
    pub fn clear(&self) {
        let mut map = self.processes.write().unwrap();
        map.clear();
    }
}
