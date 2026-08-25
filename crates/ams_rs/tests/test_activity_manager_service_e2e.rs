//! End-to-End Activity Lifecycle Integration Test driving:
//! startActivity -> zygote_fork -> attachApplication -> bindApplication -> activityResumed -> RESUMED.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::IBinder;
use ams_rs::app_thread::MockApplicationThread;
use ams_rs::types::{ActivityState, START_INTENT_NOT_RESOLVED, START_SUCCESS};
use ams_rs::{ActivityManagerService, IActivityManager};
use pms_rs::service::PackageManagerClient;
use pms_rs::types::{ActivityInfo, ApplicationInfo, ComponentName, Intent, IntentFilter, PackageInfo};
use pms_rs::PackageManagerService;
use std::sync::Arc;
use zygote_client::socket::ZygoteClient;
use zygote_client::ProcessState;

fn setup_test_environment() -> (Arc<ActivityManagerService>, Arc<PackageManagerService>, Arc<zygote_client::MockZygoteHandler>) {
    let pms = Arc::new(PackageManagerService::new());
    let (zygote_client, zygote_handler) = ZygoteClient::new_mock_default();
    let zygote = Arc::new(zygote_client);

    // Register a sample APK in PMS
    let app_info = ApplicationInfo {
        package_name: "com.androidwebgpu.arcade".to_string(),
        name: Some("ArcadeApplication".to_string()),
        label: Some("Arcade 3D Flight".to_string()),
        target_sdk_version: 33,
        uid: 10042,
        ..Default::default()
    };

    let act_info = ActivityInfo {
        name: "com.androidwebgpu.arcade.MainActivity".to_string(),
        package_name: "com.androidwebgpu.arcade".to_string(),
        label: Some("Arcade Main".to_string()),
        exported: true,
        enabled: true,
        intent_filters: vec![IntentFilter {
            actions: vec!["android.intent.action.MAIN".to_string()],
            categories: vec!["android.intent.category.LAUNCHER".to_string()],
            data_schemes: vec![],
            priority: 0,
        }],
        application_info: Some(app_info.clone()),
        ..Default::default()
    };

    let pkg_info = PackageInfo {
        package_name: "com.androidwebgpu.arcade".to_string(),
        version_code: 1,
        version_name: Some("1.0.0".to_string()),
        application_info: Some(app_info),
        activities: vec![act_info],
        requested_permissions: vec!["android.permission.INTERNET".to_string()],
        first_install_time: 1000,
        last_update_time: 1000,
        ..Default::default()
    };

    pms.install_package_info(pkg_info, None);

    let pms_client = Arc::new(PackageManagerClient::new(SpIBinder::from_arc(
        pms.clone() as Arc<dyn IBinder>,
    )));

    let ams = Arc::new(ActivityManagerService::new(pms_client, zygote));
    (ams, pms, zygote_handler)
}

#[test]
fn test_end_to_end_cold_start_lifecycle() {
    let (ams, _pms, zygote_handler) = setup_test_environment();

    // 1. Launch Intent
    let mut launch_intent = Intent::new(Some("android.intent.action.MAIN"));
    launch_intent.add_category("android.intent.category.LAUNCHER");
    launch_intent.package = Some("com.androidwebgpu.arcade".to_string());
    launch_intent.component = Some(ComponentName::new(
        "com.androidwebgpu.arcade",
        "com.androidwebgpu.arcade.MainActivity",
    ));

    let start_result = ams
        .start_activity(None, None, &launch_intent, None, None, None, 0, 0, None, None)
        .expect("start_activity failed");
    assert_eq!(start_result, START_SUCCESS);

    // 2. Verify Zygote spawned a process
    let zygote_reqs = zygote_handler.get_received_requests();
    assert_eq!(zygote_reqs.len(), 1);
    assert_eq!(zygote_reqs[0].package_name, "com.androidwebgpu.arcade");
    assert_eq!(zygote_reqs[0].uid, 10042);
    assert_eq!(zygote_reqs[0].target_sdk_version, 33);

    let spawned_pid = ams.zygote().tracker().get_process_by_package("com.androidwebgpu.arcade").unwrap().pid;
    assert_eq!(spawned_pid, 10001);

    // Activity record is in INITIALIZING state
    let top_act = ams.lifecycle().top_activity().expect("Top activity must exist");
    assert_eq!(top_act.read().unwrap().state, ActivityState::INITIALIZING);
    assert_eq!(top_act.read().unwrap().pid, Some(spawned_pid));

    let activity_token = top_act.read().unwrap().token.clone();

    // 3. Child process attaches to AMS
    let mock_thread = Arc::new(MockApplicationThread::new());
    let thread_binder = SpIBinder::from_arc(mock_thread.clone() as Arc<dyn IBinder>);

    ams.attach_application(thread_binder, 1).expect("attach_application failed");

    // 4. Verify bindApplication and schedule_resume were called on the client thread
    {
        let bound = mock_thread.bound_applications.read().unwrap();
        assert_eq!(bound.len(), 1);
        assert_eq!(bound[0].0, "com.androidwebgpu.arcade");
    }
    {
        let resumed = mock_thread.resumed_activities.read().unwrap();
        assert_eq!(resumed.len(), 1);
    }

    // Process state in tracker is Running
    let proc = ams.zygote().tracker().get_process(spawned_pid).unwrap();
    assert_eq!(proc.state, ProcessState::Running);

    // Activity state is now STARTED (waiting for client activityResumed confirmation)
    assert_eq!(top_act.read().unwrap().state, ActivityState::STARTED);

    // 5. Client notifies AMS that activity onResume completed
    ams.activity_resumed(activity_token.clone()).expect("activity_resumed failed");

    // 6. Verify top activity is now RESUMED & visible
    assert_eq!(top_act.read().unwrap().state, ActivityState::RESUMED);
    assert!(top_act.read().unwrap().visible);

    // 7. Pause and Stop
    ams.activity_paused(activity_token.clone()).expect("activity_paused failed");
    assert_eq!(top_act.read().unwrap().state, ActivityState::PAUSED);

    ams.activity_stopped(activity_token.clone(), None).expect("activity_stopped failed");
    assert_eq!(top_act.read().unwrap().state, ActivityState::STOPPED);
    assert!(!top_act.read().unwrap().visible);

    // 8. Finish Activity
    let finish_result = ams.finish_activity(activity_token, 0, None, 0).expect("finish_activity failed");
    assert!(finish_result);

    // Verify destroy was scheduled on thread
    {
        let destroyed = mock_thread.destroyed_activities.read().unwrap();
        assert_eq!(destroyed.len(), 1);
    }

    // Activity stack is clean
    assert!(ams.lifecycle().top_activity().is_none());
}

#[test]
fn test_intent_resolution_error_handling() {
    let (ams, _pms, _zygote_handler) = setup_test_environment();

    // Unknown package intent
    let mut unknown_intent = Intent::new(Some("android.intent.action.VIEW"));
    unknown_intent.package = Some("com.nonexistent.app".to_string());

    let res = ams
        .start_activity(None, None, &unknown_intent, None, None, None, 0, 0, None, None)
        .unwrap();
    assert_eq!(res, START_INTENT_NOT_RESOLVED);
}
