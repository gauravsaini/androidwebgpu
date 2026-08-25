//! Test Activity Lifecycle State Machine, Transitions, and Task Stacks.

use ams_rs::app_thread::ActivityTokenBinder;
use ams_rs::lifecycle::{is_same_binder, is_valid_transition, LifecycleManager};
use ams_rs::types::ActivityState;
use pms_rs::types::{ActivityInfo, ApplicationInfo, Intent};

fn make_dummy_metadata(pkg: &str, cls: &str) -> (Intent, ActivityInfo, ApplicationInfo) {
    let intent = Intent::new(Some("android.intent.action.MAIN"));
    let act_info = ActivityInfo {
        name: cls.to_string(),
        package_name: pkg.to_string(),
        exported: true,
        enabled: true,
        ..Default::default()
    };
    let app_info = ApplicationInfo {
        package_name: pkg.to_string(),
        name: Some("TestApplication".to_string()),
        target_sdk_version: 33,
        uid: 10042,
        ..Default::default()
    };
    (intent, act_info, app_info)
}

#[test]
fn test_lifecycle_valid_transitions() {
    assert!(is_valid_transition(
        ActivityState::INITIALIZING,
        ActivityState::CREATED
    ));
    assert!(is_valid_transition(
        ActivityState::CREATED,
        ActivityState::STARTED
    ));
    assert!(is_valid_transition(
        ActivityState::STARTED,
        ActivityState::RESUMED
    ));
    assert!(is_valid_transition(
        ActivityState::RESUMED,
        ActivityState::PAUSED
    ));
    assert!(is_valid_transition(
        ActivityState::PAUSED,
        ActivityState::STOPPED
    ));
    assert!(is_valid_transition(
        ActivityState::STOPPED,
        ActivityState::DESTROYED
    ));

    // Return to foreground
    assert!(is_valid_transition(
        ActivityState::PAUSED,
        ActivityState::RESUMED
    ));
    assert!(is_valid_transition(
        ActivityState::STOPPED,
        ActivityState::STARTED
    ));

    // Finish while paused or created
    assert!(is_valid_transition(
        ActivityState::PAUSED,
        ActivityState::DESTROYED
    ));
    assert!(is_valid_transition(
        ActivityState::CREATED,
        ActivityState::DESTROYED
    ));

    // Idempotent
    assert!(is_valid_transition(
        ActivityState::RESUMED,
        ActivityState::RESUMED
    ));
}

#[test]
fn test_lifecycle_invalid_transitions() {
    assert!(!is_valid_transition(
        ActivityState::INITIALIZING,
        ActivityState::RESUMED
    ));
    assert!(!is_valid_transition(
        ActivityState::STOPPED,
        ActivityState::RESUMED
    ));
    assert!(!is_valid_transition(
        ActivityState::DESTROYED,
        ActivityState::RESUMED
    ));
}

#[test]
fn test_lifecycle_manager_flow() {
    let lifecycle = LifecycleManager::new();
    let token = ActivityTokenBinder::new(101);
    let (intent, act_info, app_info) = make_dummy_metadata("com.example.game", ".GameActivity");

    let record_arc = lifecycle.create_activity(token.clone(), intent, act_info, app_info);
    assert_eq!(record_arc.read().unwrap().state, ActivityState::INITIALIZING);

    // Drive cold startup
    lifecycle
        .transition_activity(&token, ActivityState::CREATED)
        .unwrap();
    assert_eq!(record_arc.read().unwrap().state, ActivityState::CREATED);

    lifecycle
        .transition_activity(&token, ActivityState::STARTED)
        .unwrap();
    assert_eq!(record_arc.read().unwrap().state, ActivityState::STARTED);

    lifecycle.record_activity_resumed(&token).unwrap();
    assert_eq!(record_arc.read().unwrap().state, ActivityState::RESUMED);
    assert!(record_arc.read().unwrap().visible);

    // Drive pause -> stop
    lifecycle.record_activity_paused(&token).unwrap();
    assert_eq!(record_arc.read().unwrap().state, ActivityState::PAUSED);

    lifecycle.record_activity_stopped(&token).unwrap();
    assert_eq!(record_arc.read().unwrap().state, ActivityState::STOPPED);
    assert!(!record_arc.read().unwrap().visible);

    // Finish activity
    let finished = lifecycle.finish_activity(&token).unwrap();
    assert!(finished);
    assert_eq!(record_arc.read().unwrap().state, ActivityState::DESTROYED);

    // Stack should be empty now
    assert!(lifecycle.top_activity().is_none());
}

#[test]
fn test_multi_activity_task_stack() {
    let lifecycle = LifecycleManager::new();
    let token1 = ActivityTokenBinder::new(201);
    let (intent1, act_info1, app_info1) = make_dummy_metadata("com.example.app", ".FirstActivity");
    let rec1 = lifecycle.create_activity(token1.clone(), intent1, act_info1, app_info1);

    let token2 = ActivityTokenBinder::new(202);
    let (intent2, act_info2, app_info2) = make_dummy_metadata("com.example.app", ".SecondActivity");
    let rec2 = lifecycle.create_activity(token2.clone(), intent2, act_info2, app_info2);

    let top = lifecycle.top_activity().expect("Top activity not found");
    assert!(is_same_binder(&top.read().unwrap().token, &token2));

    // Remove top activity
    lifecycle.finish_activity(&token2).unwrap();

    let new_top = lifecycle.top_activity().expect("New top activity not found");
    assert!(is_same_binder(&new_top.read().unwrap().token, &token1));
    assert_eq!(rec1.read().unwrap().activity_info.name, ".FirstActivity");
    assert_eq!(rec2.read().unwrap().state, ActivityState::DESTROYED);
}
