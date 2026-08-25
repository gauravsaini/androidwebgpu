//! End-to-End Integration Test Suite for Real-World F-Droid Client (`F-Droid.apk`).
//!
//! Verifies:
//! 1. APK Ingestion & Full AXML/ARSC Manifest Parsing (25 Activities, 4 Providers, 16 Services, 16 Receivers, 29 Permissions)
//! 2. PMS Intent & ContentProvider Resolution (`org.fdroid.fdroid.views.main.MainActivity`, `ApkFileProvider`)
//! 3. AMS Process Fork via Zygote abstract socket
//! 4. ApplicationThread Attachment (`bindApplication`)
//! 5. Full Activity Lifecycle Transitions (`INITIALIZING` -> `STARTED` -> `RESUMED`)
//! 6. WMS Window Layout & SurfaceControl Allocation for WebGPU Compositor
//! 7. Bi-directional Touch Input Event Injection & InputConsumer Acknowledgement
//! 8. Graceful Teardown and Resource Cleanup

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::IBinder;
use ams_rs::app_thread::MockApplicationThread;
use ams_rs::types::{ActivityState, START_SUCCESS};
use ams_rs::IActivityManager;
use input_channel::{InputChannel, InputConsumer, InputMessage};
use inputflinger_rs::types::*;
use inputflinger_rs::{IInputManager, InputManagerProxy, VirtualEventSource};
use pms_rs::types::{
    ComponentName, Intent, GET_ACTIVITIES, GET_PERMISSIONS, GET_PROVIDERS, GET_RECEIVERS,
    GET_SERVICES, MATCH_DEFAULT_ONLY, PERMISSION_GRANTED,
};
use std::sync::Arc;
use std::thread;
use tests_e2e_system_services::SystemServicesHarness;
use wms_rs::{
    IWindowManager, IWindowSession, InsetsState, LayoutParams, SurfaceControl,
    SurfaceControlTransaction, WindowManagerProxy, WindowSessionProxy,
    FLAG_HARDWARE_ACCELERATED,
};
use zygote_client::ProcessState;

#[test]
fn test_fdroid_full_stack_boot_and_input_flow() {
    let harness = SystemServicesHarness::new();

    // =========================================================================
    // Phase 1: Real F-Droid APK Ingestion & Full Manifest Parsing
    // =========================================================================
    let apk_bytes = SystemServicesHarness::read_fixture_apk("F-Droid.apk");
    assert!(!apk_bytes.is_empty(), "F-Droid.apk must not be empty");

    let installed_pkg = harness
        .pms
        .install_apk(&apk_bytes)
        .expect("Failed to ingest real-world F-Droid.apk into PMS");

    let package_name = installed_pkg.package_name.clone();
    assert_eq!(package_name, "org.fdroid.fdroid");
    assert_eq!(installed_pkg.version_code, 1023051);
    assert_eq!(installed_pkg.version_name.as_deref(), Some("1.23.1"));

    // Verify all 25 Activities, 4 Providers, 16 Services, 16 Receivers, 29 Permissions
    assert_eq!(installed_pkg.activities.len(), 25);
    assert_eq!(installed_pkg.providers.len(), 4);
    assert_eq!(installed_pkg.services.len(), 16);
    assert_eq!(installed_pkg.receivers.len(), 16);
    assert_eq!(installed_pkg.requested_permissions.len(), 29);

    let pkg_info = harness
        .pms
        .get_package_info(
            &package_name,
            GET_ACTIVITIES | GET_PROVIDERS | GET_SERVICES | GET_RECEIVERS | GET_PERMISSIONS,
            0,
        )
        .expect("PackageInfo must exist in PMS registry");
    assert_eq!(pkg_info.activities.len(), 25);
    assert_eq!(pkg_info.providers.len(), 4);

    // =========================================================================
    // Phase 2: Intent Resolution for F-Droid MainActivity & Permissions
    // =========================================================================
    let mut launcher_intent = Intent::new(Some("android.intent.action.MAIN"));
    launcher_intent.add_category("android.intent.category.LAUNCHER");
    launcher_intent.package = Some(package_name.clone());

    let resolve_info = harness
        .pms
        .resolve_intent(&launcher_intent, "", MATCH_DEFAULT_ONLY, 0)
        .expect("PMS must resolve LAUNCHER intent for F-Droid");
    let main_activity = resolve_info
        .activity_info
        .expect("ResolveInfo must contain ActivityInfo");
    assert_eq!(&main_activity.name, "org.fdroid.fdroid.views.main.MainActivity");
    assert_eq!(&main_activity.package_name, &package_name);

    launcher_intent.component = Some(ComponentName::new(&package_name, &main_activity.name));

    let perm_check = harness
        .pms
        .check_permission("android.permission.INTERNET", &package_name, 0);
    assert_eq!(perm_check, PERMISSION_GRANTED);

    // Verify ContentProvider Resolution
    let apk_prov = harness
        .pms
        .resolve_content_provider("org.fdroid.fdroid.installer.ApkFileProvider", 0, 0)
        .expect("ApkFileProvider must resolve");
    assert_eq!(apk_prov.package_name, "org.fdroid.fdroid");
    assert!(apk_prov.grant_uri_permissions);

    // =========================================================================
    // Phase 3: AMS Activity Start & Zygote Process Forking
    // =========================================================================
    let start_result = harness
        .ams
        .start_activity(None, None, &launcher_intent, None, None, None, 0, 0, None, None)
        .expect("AMS start_activity failed for F-Droid");
    assert_eq!(start_result, START_SUCCESS);

    // Verify Zygote spawned a child process for org.fdroid.fdroid
    let zygote_requests = harness.zygote_handler.get_received_requests();
    assert_eq!(zygote_requests.len(), 1);
    assert_eq!(&zygote_requests[0].package_name, "org.fdroid.fdroid");
    assert_eq!(&zygote_requests[0].nice_name, "org.fdroid.fdroid.FDroidApp");

    let spawned_pid = harness
        .ams
        .zygote()
        .tracker()
        .get_process_by_package(&package_name)
        .expect("Spawned process must be in Zygote ProcessTracker")
        .pid;

    let top_act_arc = harness
        .ams
        .lifecycle()
        .top_activity()
        .expect("Top activity must exist in AMS stack");
    {
        let top_act = top_act_arc.read().unwrap();
        assert_eq!(top_act.state, ActivityState::INITIALIZING);
        assert_eq!(top_act.pid, Some(spawned_pid));
        if let Some(ref comp) = top_act.intent.component {
            assert_eq!(comp.class_name, "org.fdroid.fdroid.views.main.MainActivity");
        }
    }
    let activity_token = top_act_arc.read().unwrap().token.clone();

    // =========================================================================
    // Phase 4: Child Process Attaches to AMS & Binds F-Droid Application
    // =========================================================================
    let mock_app_thread = Arc::new(MockApplicationThread::new());
    let thread_binder = SpIBinder::from_arc(Arc::clone(&mock_app_thread) as Arc<dyn IBinder>);

    harness
        .ams
        .attach_application(thread_binder, 1)
        .expect("attach_application failed for F-Droid");

    // Verify bindApplication callback for F-Droid
    {
        let bound = mock_app_thread.bound_applications.read().unwrap();
        assert_eq!(bound.len(), 1);
        assert_eq!(&bound[0].0, "org.fdroid.fdroid");
    }

    let proc_state = harness
        .ams
        .zygote()
        .tracker()
        .get_process(spawned_pid)
        .expect("Process must exist")
        .state;
    assert_eq!(proc_state, ProcessState::Running);

    // =========================================================================
    // Phase 5: Activity Lifecycle Transitions (INITIALIZING -> STARTED -> RESUMED)
    // =========================================================================
    {
        let top_act = top_act_arc.read().unwrap();
        assert_eq!(top_act.state, ActivityState::STARTED);
    }

    // F-Droid notifies AMS that MainActivity onResume has completed
    harness
        .ams
        .activity_resumed(activity_token.clone())
        .expect("activity_resumed failed");

    {
        let top_act = top_act_arc.read().unwrap();
        assert_eq!(top_act.state, ActivityState::RESUMED);
        assert!(top_act.visible);
    }

    // =========================================================================
    // Phase 6: WMS Window Session & InputChannel Socketpair Allocation
    // =========================================================================
    let wms_binder = SpIBinder::from_arc(Arc::clone(&harness.wms) as Arc<dyn IBinder>);
    let wms_proxy = WindowManagerProxy::new(wms_binder);

    let session_binder = wms_proxy
        .open_session(None)
        .expect("Failed to open WMS session");
    let session_proxy = WindowSessionProxy::new(session_binder);

    let mut layout_attrs = LayoutParams::default();
    layout_attrs.title = format!("{}/{}", package_name, main_activity.name);
    layout_attrs.flags = FLAG_HARDWARE_ACCELERATED;

    let mut insets_state = InsetsState::default();
    let mut client_input_channel = InputChannel::default();

    let add_result = session_proxy
        .add_to_display(
            None,
            &layout_attrs,
            0,
            0,
            &mut insets_state,
            &mut client_input_channel,
        )
        .expect("add_to_display failed for F-Droid");
    assert_eq!(add_result, 0);
    assert_eq!(insets_state.display_frame.right, 1280);
    assert_eq!(insets_state.display_frame.bottom, 720);
    assert!(!client_input_channel.name().is_empty());

    // Register server-side channel with InputManager
    let active_session = harness
        .wms
        .get_session(1)
        .expect("Active session 1 must exist");
    let server_channel = active_session
        .get_server_input_channel(1)
        .expect("Server input channel must exist");

    let input_binder = SpIBinder::from_arc(Arc::clone(&harness.input_service) as Arc<dyn IBinder>);
    let input_proxy = InputManagerProxy::new(input_binder);

    input_proxy
        .register_input_channel(&server_channel)
        .expect("Failed to register server input channel with InputManager");

    // =========================================================================
    // Phase 7: Relayout Window with SurfaceControl & WebGPU Compositor Layer
    // =========================================================================
    let mut surface_control = SurfaceControl::default();
    let relayout_res = session_proxy
        .relayout(
            None,
            &layout_attrs,
            1280,
            720,
            0,
            0,
            &mut surface_control,
        )
        .expect("relayout failed for F-Droid");
    assert_ne!(relayout_res, 0);
    assert_eq!(surface_control.width, 1280);
    assert_eq!(surface_control.height, 720);
    assert_eq!(surface_control.name, layout_attrs.title);

    let mut draw_tx = SurfaceControlTransaction::new(surface_control.layer_id);
    draw_tx
        .set_position(0.0, 0.0)
        .set_size(1280, 720)
        .set_alpha(1.0)
        .set_z_order(1);

    session_proxy
        .finish_drawing(None, Some(&draw_tx))
        .expect("finish_drawing failed");

    // =========================================================================
    // Phase 8: Input Injection & InputConsumer Bi-directional Acknowledgement
    // =========================================================================
    let consumer = Arc::new(InputConsumer::new(client_input_channel));
    let consumer_handle = Arc::clone(&consumer);

    let consumer_worker = thread::spawn(move || {
        // 1. Consume Motion Down Event (App Item Tap)
        let motion_msg = consumer_handle
            .consume()
            .expect("Consumer failed to receive motion event");
        if let InputMessage::Motion(m) = motion_msg {
            assert_eq!(m.action, MOTION_ACTION_DOWN);
            assert_eq!(m.pointer_coords[0].x, 300.0);
            assert_eq!(m.pointer_coords[0].y, 250.0);
            consumer_handle
                .send_finished_signal(m.seq, true)
                .expect("Failed to send motion finished ack");
        } else {
            panic!("Expected Motion message");
        }

        // 2. Consume Key Down Event (Search Key)
        let key_msg = consumer_handle
            .consume()
            .expect("Consumer failed to receive key event");
        if let InputMessage::Key(k) = key_msg {
            assert_eq!(k.key_code, KEYCODE_ENTER);
            assert_eq!(k.action, KEY_ACTION_DOWN);
            consumer_handle
                .send_finished_signal(k.seq, true)
                .expect("Failed to send key finished ack");
        } else {
            panic!("Expected Key message");
        }
    });

    let mut virtual_event_source = VirtualEventSource::new(1);

    // Inject touch tap on F-Droid catalog item
    let motion_event =
        InputEvent::Motion(virtual_event_source.make_touch_down(300.0, 250.0, 100_000));
    let motion_handled = input_proxy
        .inject_input_event(&motion_event, INJECT_INPUT_EVENT_MODE_WAIT_FOR_FINISH)
        .expect("Motion event injection failed");
    assert!(motion_handled, "Motion event must be handled synchronously");

    // Inject key event
    let key_event = InputEvent::Key(virtual_event_source.make_key_event(
        KEYCODE_ENTER,
        KEY_ACTION_DOWN,
        200_000,
    ));
    let key_handled = input_proxy
        .inject_input_event(&key_event, INJECT_INPUT_EVENT_MODE_WAIT_FOR_FINISH)
        .expect("Key event injection failed");
    assert!(key_handled, "Key event must be handled synchronously");

    consumer_worker.join().unwrap();

    // =========================================================================
    // Phase 9: Graceful Teardown & Resource Release
    // =========================================================================
    harness
        .ams
        .activity_paused(activity_token.clone())
        .expect("activity_paused failed");
    {
        let top_act = top_act_arc.read().unwrap();
        assert_eq!(top_act.state, ActivityState::PAUSED);
    }

    harness
        .ams
        .activity_stopped(activity_token.clone(), None)
        .expect("activity_stopped failed");
    {
        let top_act = top_act_arc.read().unwrap();
        assert_eq!(top_act.state, ActivityState::STOPPED);
    }

    let finished = harness
        .ams
        .finish_activity(activity_token.clone(), 0, None, 0)
        .expect("finish_activity failed");
    assert!(finished);

    session_proxy.remove(None).expect("remove window failed");
    assert_eq!(active_session.get_window_count(), 0);
    assert!(harness
        .surface_bridge
        .get_surface(surface_control.layer_id)
        .is_none());
}
