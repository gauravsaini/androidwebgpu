//! Test process tracker registry, state transitions, and querying.

use zygote_client::process::{ProcessRecord, ProcessState, ProcessTracker};
use zygote_client::protocol::ZygoteSpawnArgs;

#[test]
fn test_process_tracker_lifecycle() {
    let tracker = ProcessTracker::new();
    assert_eq!(tracker.count(), 0);

    let args = ZygoteSpawnArgs::new("com.androidwebgpu.demo", "com.androidwebgpu.demo")
        .with_uid(10042)
        .with_gid(10042);

    let record = ProcessRecord::from_spawn_args(20001, &args);
    tracker.register_process(record).unwrap();

    assert_eq!(tracker.count(), 1);

    // Query by PID
    let proc = tracker.get_process(20001).expect("Process not found");
    assert_eq!(proc.pid, 20001);
    assert_eq!(proc.package_name, "com.androidwebgpu.demo");
    assert_eq!(proc.state, ProcessState::Starting);
    assert!(proc.is_alive());

    // Query by Package
    let by_pkg = tracker
        .get_process_by_package("com.androidwebgpu.demo")
        .expect("Not found by pkg");
    assert_eq!(by_pkg.pid, 20001);

    // Transition to Running
    tracker
        .update_process_state(20001, ProcessState::Running)
        .unwrap();
    let proc_running = tracker.get_process(20001).unwrap();
    assert_eq!(proc_running.state, ProcessState::Running);
    assert!(proc_running.is_alive());

    // Kill process
    tracker.kill_process(20001).unwrap();
    let proc_killed = tracker.get_process(20001).unwrap();
    assert_eq!(proc_killed.state, ProcessState::Killed);
    assert!(!proc_killed.is_alive());

    // Alive processes should be 0 now
    assert_eq!(tracker.list_alive_processes().len(), 0);

    // Remove process
    let removed = tracker.remove_process(20001);
    assert!(removed.is_some());
    assert_eq!(tracker.count(), 0);
}

#[test]
fn test_duplicate_registration_prevented() {
    let tracker = ProcessTracker::new();
    let args = ZygoteSpawnArgs::new("com.test", "com.test");
    let r1 = ProcessRecord::from_spawn_args(5000, &args);
    let r2 = ProcessRecord::from_spawn_args(5000, &args);

    assert!(tracker.register_process(r1).is_ok());
    assert!(tracker.register_process(r2).is_err());
}
