//! Adversarial Stress Suite for Process Crash Recovery, Death Recipients, and Intent Resolution.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::IBinder;
use aidl_compat::DeathRecipient;
use ams_rs::app_thread::MockApplicationThread;
use ams_rs::types::{START_INTENT_NOT_RESOLVED, START_SUCCESS};
use ams_rs::IActivityManager;
use input_channel::InputChannel;
use inputflinger_rs::{IInputManager, InputManagerProxy};
use pms_rs::types::{
    ActivityInfo, ApplicationInfo, ComponentName, Intent, IntentFilter, PackageInfo,
};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use tests_e2e_system_services::SystemServicesHarness;
use wms_rs::{
    IWindowSession, InsetsState, LayoutParams, SurfaceControl, WindowSessionProxy,
    FLAG_HARDWARE_ACCELERATED,
};
use zygote_client::ProcessState;

struct MultiDeathRecipient {
    counter: Arc<AtomicU32>,
}

impl DeathRecipient for MultiDeathRecipient {
    fn binder_died(&self) {
        self.counter.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn test_adversarial_cascading_process_crashes_and_cleanup() {
    let harness = SystemServicesHarness::new();
    let num_crashes = 10;

    for i in 0..num_crashes {
        let pkg_name = format!("com.crash.cascade_{}", i);
        let act_name = format!("{}.CrashActivity", pkg_name);

        let app_info = ApplicationInfo {
            package_name: pkg_name.clone(),
            name: Some("CascadeApp".to_string()),
            label: Some("Cascade App".to_string()),
            target_sdk_version: 33,
            uid: 10200 + i,
            ..Default::default()
        };

        let act_info = ActivityInfo {
            name: act_name.clone(),
            package_name: pkg_name.clone(),
            label: Some("Cascade Activity".to_string()),
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
            package_name: pkg_name.clone(),
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

        // 1. Launch Activity
        let mut intent = Intent::new(Some("android.intent.action.MAIN"));
        intent.add_category("android.intent.category.LAUNCHER");
        intent.package = Some(pkg_name.clone());
        intent.component = Some(ComponentName::new(&pkg_name, &act_name));

        let start_res = harness
            .ams
            .start_activity(None, None, &intent, None, None, None, 0, 0, None, None)
            .unwrap();
        assert_eq!(start_res, START_SUCCESS);

        let pid = harness
            .ams
            .zygote()
            .tracker()
            .get_process_by_package(&pkg_name)
            .unwrap()
            .pid;

        // 2. Attach App & link death recipient
        let death_count = Arc::new(AtomicU32::new(0));
        let death_rcpt = Arc::new(MultiDeathRecipient {
            counter: Arc::clone(&death_count),
        });

        let mock_thread = Arc::new(MockApplicationThread::new());
        let thread_binder = SpIBinder::from_arc(Arc::clone(&mock_thread) as Arc<dyn IBinder>);
        thread_binder.link_to_death(death_rcpt.clone()).unwrap();

        harness
            .ams
            .attach_application(thread_binder, i as i64 + 1)
            .unwrap();

        let top_act_arc = harness.ams.lifecycle().top_activity().unwrap();
        let activity_token = top_act_arc.read().unwrap().token.clone();
        harness.ams.activity_resumed(activity_token.clone()).unwrap();

        // 3. Open Window Session
        let (_session_id, session_arc) = harness.wms.open_session_internal(None).unwrap();
        let session_binder = SpIBinder::from_arc(session_arc.clone() as Arc<dyn IBinder>);
        let session_proxy = WindowSessionProxy::new(session_binder);

        let mut attrs = LayoutParams::default();
        attrs.title = format!("{}/MainActivity", pkg_name);
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

        let server_channel = session_arc.get_server_input_channel(1).unwrap();
        let input_proxy = InputManagerProxy::new(SpIBinder::from_arc(
            Arc::clone(&harness.input_service) as Arc<dyn IBinder>,
        ));
        input_proxy.register_input_channel(&server_channel).unwrap();

        // 4. Sudden Crash & Teardown
        death_rcpt.binder_died();
        assert_eq!(death_count.load(Ordering::SeqCst), 1);

        harness.ams.zygote().tracker().kill_process(pid).unwrap();
        assert_eq!(
            harness.ams.zygote().tracker().get_process(pid).unwrap().state,
            ProcessState::Killed
        );

        harness.ams.finish_activity(activity_token, 0, None, 0).unwrap();
        session_proxy.remove(None).unwrap();
        harness
            .input_service
            .dispatcher()
            .unregister_window_channel(&server_channel.name());

        // Stack must be clear
        assert!(harness.ams.lifecycle().top_activity().is_none());
        assert_eq!(session_arc.get_window_count(), 0);
    }
}

#[test]
fn test_adversarial_non_existent_intent_storm() {
    let harness = Arc::new(SystemServicesHarness::new());
    let num_threads = 8;
    let iterations = 100;

    let mut handles = Vec::new();
    for t in 0..num_threads {
        let h = Arc::clone(&harness);
        handles.push(thread::spawn(move || {
            for i in 0..iterations {
                let bad_pkg = format!("com.nonexistent.bogus_{}_{}", t, i);
                let mut intent = Intent::new(Some("android.intent.action.VIEW"));
                intent.package = Some(bad_pkg.clone());
                intent.component = Some(ComponentName::new(&bad_pkg, "MissingActivity"));

                let res = h.ams.start_activity(
                    None, None, &intent, None, None, None, 0, 0, None, None,
                ).unwrap();

                assert_eq!(res, START_INTENT_NOT_RESOLVED);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Intent storm thread panicked");
    }

    // AMS stack and zygote tracker must remain completely untouched
    assert!(harness.ams.lifecycle().top_activity().is_none());
    assert_eq!(harness.ams.zygote().tracker().list_alive_processes().len(), 0);
}

