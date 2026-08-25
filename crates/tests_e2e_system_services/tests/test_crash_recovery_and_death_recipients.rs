//! Process Crash, Death Recipient Recovery, and Adversarial Lifecycle Teardown Integration Tests.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::IBinder;
use aidl_compat::DeathRecipient;
use ams_rs::app_thread::MockApplicationThread;
use ams_rs::types::{ActivityState, START_INTENT_NOT_RESOLVED, START_SUCCESS};
use ams_rs::IActivityManager;
use input_channel::InputChannel;
use inputflinger_rs::{IInputManager, InputManagerProxy};
use pms_rs::types::{
    ActivityInfo, ApplicationInfo, ComponentName, Intent, IntentFilter, PackageInfo,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tests_e2e_system_services::SystemServicesHarness;
use wms_rs::{
    IWindowSession, InsetsState, LayoutParams, SurfaceControl, WindowSessionProxy,
    FLAG_HARDWARE_ACCELERATED,
};
use zygote_client::ProcessState;

/// Mock death recipient to verify death notification dispatch upon process crash.
struct TestDeathRecipient {
    died: Arc<AtomicBool>,
}

impl DeathRecipient for TestDeathRecipient {
    fn binder_died(&self) {
        self.died.store(true, Ordering::SeqCst);
    }
}

#[test]
fn test_process_crash_and_death_recipient_cleanup_during_window_session() {
    let harness = SystemServicesHarness::new();

    // 1. Register test package in PMS
    let pkg_name = "com.crash.testapp";
    let app_info = ApplicationInfo {
        package_name: pkg_name.to_string(),
        name: Some("CrashApp".to_string()),
        label: Some("Crash Test App".to_string()),
        target_sdk_version: 33,
        uid: 10099,
        ..Default::default()
    };
    let act_info = ActivityInfo {
        name: format!("{}.CrashActivity", pkg_name),
        package_name: pkg_name.to_string(),
        label: Some("Crash Activity".to_string()),
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
        package_name: pkg_name.to_string(),
        version_code: 1,
        version_name: Some("1.0.0".to_string()),
        application_info: Some(app_info),
        activities: vec![act_info.clone()],
        requested_permissions: vec![],
        first_install_time: 1000,
        last_update_time: 1000,
        ..Default::default()
    };
    harness.pms.install_package_info(pkg_info, None);

    // 2. Start Activity and Fork Process
    let mut intent = Intent::new(Some("android.intent.action.MAIN"));
    intent.add_category("android.intent.category.LAUNCHER");
    intent.package = Some(pkg_name.to_string());
    intent.component = Some(ComponentName::new(pkg_name, &act_info.name));

    let start_res = harness
        .ams
        .start_activity(None, None, &intent, None, None, None, 0, 0, None, None)
        .unwrap();
    assert_eq!(start_res, START_SUCCESS);

    let spawned_pid = harness
        .ams
        .zygote()
        .tracker()
        .get_process_by_package(pkg_name)
        .unwrap()
        .pid;

    // 3. Attach Application and link DeathRecipient
    let mock_thread = Arc::new(MockApplicationThread::new());
    let thread_binder = SpIBinder::from_arc(Arc::clone(&mock_thread) as Arc<dyn IBinder>);

    let died_flag = Arc::new(AtomicBool::new(false));
    let death_recipient = Arc::new(TestDeathRecipient {
        died: Arc::clone(&died_flag),
    });
    thread_binder
        .link_to_death(death_recipient.clone())
        .expect("Link to death failed");

    harness
        .ams
        .attach_application(thread_binder.clone(), 1)
        .unwrap();

    let top_act_arc = harness.ams.lifecycle().top_activity().unwrap();
    let activity_token = top_act_arc.read().unwrap().token.clone();

    harness.ams.activity_resumed(activity_token.clone()).unwrap();
    assert_eq!(
        top_act_arc.read().unwrap().state,
        ActivityState::RESUMED
    );

    // 4. Open Window Session with SurfaceControl and InputChannel
    let (_session_id, session_arc) = harness.wms.open_session_internal(None).unwrap();
    let session_binder = SpIBinder::from_arc(session_arc.clone() as Arc<dyn IBinder>);
    let session_proxy = WindowSessionProxy::new(session_binder);

    let mut attrs = LayoutParams::default();
    attrs.title = "com.crash.testapp/CrashActivity".to_string();
    attrs.flags = FLAG_HARDWARE_ACCELERATED;

    let mut insets = InsetsState::default();
    let mut client_input_channel = InputChannel::default();

    session_proxy
        .add_to_display(None, &attrs, 0, 0, &mut insets, &mut client_input_channel)
        .unwrap();

    let mut sc = SurfaceControl::default();
    session_proxy
        .relayout(None, &attrs, 1280, 720, 0, 0, &mut sc)
        .unwrap();
    assert_eq!(sc.width, 1280);

    let server_channel = session_arc.get_server_input_channel(1).unwrap();
    let input_proxy = InputManagerProxy::new(SpIBinder::from_arc(
        Arc::clone(&harness.input_service) as Arc<dyn IBinder>,
    ));
    input_proxy.register_input_channel(&server_channel).unwrap();

    // 5. Simulate Process Crash
    // Dispatch Death Notification
    death_recipient.binder_died();
    assert!(died_flag.load(Ordering::SeqCst), "Death recipient must be notified");

    // AMS Handles Process Death / Kill
    harness.ams.zygote().tracker().kill_process(spawned_pid).unwrap();
    assert_eq!(
        harness
            .ams
            .zygote()
            .tracker()
            .get_process(spawned_pid)
            .unwrap()
            .state,
        ProcessState::Killed
    );

    // Finish and clear Activity from AMS stack
    harness.ams.finish_activity(activity_token, 0, None, 0).unwrap();
    assert!(harness.ams.lifecycle().top_activity().is_none());

    // Clean up dead session and surface in WMS
    session_proxy.remove(None).unwrap();
    assert_eq!(session_arc.get_window_count(), 0);
    assert!(harness.surface_bridge.get_surface(sc.layer_id).is_none());

    // Unregister input channel from dispatcher
    harness
        .input_service
        .dispatcher()
        .unregister_window_channel(&server_channel.name());
    assert!(harness
        .input_service
        .dispatcher()
        .get_focused_publisher()
        .is_none());
}

#[test]
fn test_adversarial_lifecycle_and_error_handling() {
    let harness = SystemServicesHarness::new();

    // 1. Unknown intent resolution error handling
    let mut unknown_intent = Intent::new(Some("android.intent.action.VIEW"));
    unknown_intent.package = Some("com.nonexistent.app".to_string());
    let start_unknown = harness
        .ams
        .start_activity(None, None, &unknown_intent, None, None, None, 0, 0, None, None)
        .unwrap();
    assert_eq!(start_unknown, START_INTENT_NOT_RESOLVED);

    // 2. Class not found intent error handling
    let mut bad_class_intent = Intent::new(Some("android.intent.action.MAIN"));
    bad_class_intent.package = Some("com.nonexistent.pkg".to_string());
    bad_class_intent.component = Some(ComponentName::new("com.nonexistent.pkg", "NoClass"));
    let start_bad_class = harness
        .ams
        .start_activity(None, None, &bad_class_intent, None, None, None, 0, 0, None, None)
        .unwrap();
    assert_eq!(start_bad_class, START_INTENT_NOT_RESOLVED);

    // 3. Spurious activity_resumed call with invalid token
    let fake_token = SpIBinder::new(aidl_compat::RemoteBinder::new(999999, 0));
    assert!(harness.ams.activity_resumed(fake_token.clone()).is_err());

    // 4. Spurious activity_paused call with invalid token
    assert!(harness.ams.activity_paused(fake_token.clone()).is_err());

    // 5. Spurious finish_activity call with invalid token returns false
    let finish_res = harness.ams.finish_activity(fake_token, 0, None, 0).unwrap();
    assert!(!finish_res);
}
