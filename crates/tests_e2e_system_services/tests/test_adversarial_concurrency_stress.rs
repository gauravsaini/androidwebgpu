//! High-Throughput Concurrency and Adversarial Stress Test Suite for Android System Services.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::{IBinder, Interface};
use ams_rs::app_thread::MockApplicationThread;
use ams_rs::types::START_SUCCESS;
use ams_rs::IActivityManager;
use audio_hal_virtual::{
    AudioFormat, AudioModuleProxy, IModule, IStreamOut, OpenOutputStreamArguments, StreamOutProxy,
};
use input_channel::{InputChannel, InputConsumer, InputMessage};
use inputflinger_rs::types::*;
use inputflinger_rs::{IInputManager, InputManagerProxy, VirtualEventSource};
use pms_rs::types::{
    ActivityInfo, ApplicationInfo, ComponentName, Intent, IntentFilter, PackageInfo,
    MATCH_DEFAULT_ONLY,
};
use sensors_hal_virtual::{Event, ISensors, OperationMode, SensorsProxy};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use tests_e2e_system_services::SystemServicesHarness;
use wms_rs::{
    IWindowSession, InsetsState, LayoutParams, SurfaceControl,
    SurfaceControlTransaction, WindowSessionProxy,
    FLAG_HARDWARE_ACCELERATED,
};

#[test]
fn test_high_throughput_multithreaded_system_services_stress() {
    let harness = Arc::new(SystemServicesHarness::new());

    // Register 10 distinct packages in PMS
    for i in 0..10 {
        let pkg_name = format!("com.stress.app_{}", i);
        let act_name = format!("{}.MainActivity", pkg_name);
        let app_info = ApplicationInfo {
            package_name: pkg_name.clone(),
            name: Some(format!("App_{}", i)),
            label: Some(format!("Stress App {}", i)),
            target_sdk_version: 33,
            uid: 10100 + i,
            ..Default::default()
        };
        let act_info = ActivityInfo {
            name: act_name.clone(),
            package_name: pkg_name.clone(),
            label: Some(format!("Main Activity {}", i)),
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
            activities: vec![act_info],
            requested_permissions: vec!["android.permission.INTERNET".to_string()],
            first_install_time: 1000,
            last_update_time: 1000,
            ..Default::default()
        };
        harness.pms.install_package_info(pkg_info, None);
    }

    let success_counter = Arc::new(AtomicU32::new(0));
    let mut handles = Vec::new();

    // Spawn 10 concurrent threads each exercising full service lifecycle
    for i in 0..10 {
        let h = Arc::clone(&harness);
        let counter = Arc::clone(&success_counter);

        handles.push(thread::spawn(move || {
            let pkg_name = format!("com.stress.app_{}", i);
            let act_name = format!("{}.MainActivity", pkg_name);

            // 1. Resolve Intent
            let mut intent = Intent::new(Some("android.intent.action.MAIN"));
            intent.add_category("android.intent.category.LAUNCHER");
            intent.package = Some(pkg_name.clone());
            intent.component = Some(ComponentName::new(&pkg_name, &act_name));

            let resolve = h.pms.resolve_intent(&intent, "", MATCH_DEFAULT_ONLY, 0);
            assert!(resolve.is_some());

            // 2. Start Activity
            let start_res = h.ams.start_activity(
                None, None, &intent, None, None, None, 0, 0, None, None,
            ).unwrap();
            assert_eq!(start_res, START_SUCCESS);

            // 3. Attach Application
            let mock_thread = Arc::new(MockApplicationThread::new());
            let thread_binder = SpIBinder::from_arc(Arc::clone(&mock_thread) as Arc<dyn IBinder>);
            h.ams.attach_application(thread_binder, i as i64 + 1).unwrap();

            // 4. Open WMS Window Session
            let (session_id, session_arc) = h.wms.open_session_internal(None).unwrap();
            let session_binder = SpIBinder::from_arc(session_arc as Arc<dyn IBinder>);
            let session = WindowSessionProxy::new(session_binder);

            let mut attrs = LayoutParams::default();
            attrs.title = format!("win_stress_{}", i);
            attrs.flags = FLAG_HARDWARE_ACCELERATED;

            let mut insets = InsetsState::default();
            let mut client_channel = InputChannel::default();
            let add_res = session.add_to_display(
                None, &attrs, 0, 0, &mut insets, &mut client_channel,
            ).unwrap();
            assert_eq!(add_res, 0);

            // 5. Relayout Surface
            let mut sc = SurfaceControl::default();
            let relayout_res = session.relayout(
                None, &attrs, 640, 360, 0, 0, &mut sc,
            ).unwrap();
            assert_ne!(relayout_res, 0);
            assert_eq!(sc.width, 640);
            assert_eq!(sc.height, 360);

            let mut tx = SurfaceControlTransaction::new(sc.layer_id);
            tx.set_position((i * 10) as f32, (i * 10) as f32)
                .set_size(640, 360)
                .set_alpha(0.9)
                .set_z_order(i as i32);
            session.finish_drawing(None, Some(&tx)).unwrap();

            // 6. Sensor, Audio, and Input Operations
            let sensors_proxy = SensorsProxy::new(Interface::as_binder(&*h.sensors_service));
            sensors_proxy.batch(1, 10_000_000, 0).unwrap();
            sensors_proxy.activate(1, true).unwrap();
            sensors_proxy.set_operation_mode(OperationMode::DataInjection).unwrap();

            let sensor_ev = Event::new_accelerometer(1, (i + 1) as i64 * 1000, 1.0, 2.0, 3.0);
            sensors_proxy.inject_sensor_data(&sensor_ev).unwrap();

            let audio_proxy = AudioModuleProxy::new(Interface::as_binder(&*h.audio_service));
            let out_args = OpenOutputStreamArguments {
                port_config_id: i + 1,
                buffer_size_frames: 240,
                sample_rate: 48000,
                channel_mask: 2,
                format: AudioFormat::Pcm16Bit,
            };
            let stream_res = audio_proxy.open_output_stream(&out_args).unwrap();
            let stream_proxy = StreamOutProxy::new(stream_res.stream.unwrap());
            let pcm_buf = vec![0x11u8; 960];
            let written = stream_proxy.write(&pcm_buf, 240).unwrap();
            assert_eq!(written, 240);

            // 7. Cleanup
            session.remove(None).unwrap();
            let _ = session_id;
            counter.fetch_add(1, Ordering::SeqCst);
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(success_counter.load(Ordering::SeqCst), 10);
}

#[test]
fn test_concurrent_window_sessions_and_input_channels_isolation() {
    let harness = Arc::new(SystemServicesHarness::new());

    let num_sessions = 5;
    let mut session_proxies = Vec::new();
    let mut client_channels = Vec::new();
    let mut server_channels = Vec::new();

    for i in 0..num_sessions {
        let (_sess_id, session_arc) = harness.wms.open_session_internal(None).unwrap();
        let session_binder = SpIBinder::from_arc(session_arc.clone() as Arc<dyn IBinder>);
        let session = WindowSessionProxy::new(session_binder);

        let mut attrs = LayoutParams::default();
        attrs.title = format!("com.multi.win_{}", i);
        let mut insets = InsetsState::default();
        let mut client_chan = InputChannel::default();

        let add_res = session
            .add_to_display(None, &attrs, 0, 0, &mut insets, &mut client_chan)
            .unwrap();
        assert_eq!(add_res, 0);

        let server_chan = session_arc.get_server_input_channel(1).unwrap();

        session_proxies.push(session);
        client_channels.push(client_chan);
        server_channels.push(server_chan);
    }

    // Verify all 5 client channels have distinct, non-empty names
    for i in 0..num_sessions {
        for j in (i + 1)..num_sessions {
            assert_ne!(client_channels[i].name(), client_channels[j].name());
        }
    }

    // Register all server channels with InputManager
    let input_proxy = InputManagerProxy::new(SpIBinder::from_arc(
        Arc::clone(&harness.input_service) as Arc<dyn IBinder>,
    ));

    for chan in &server_channels {
        input_proxy.register_input_channel(chan).unwrap();
    }

    // Concurrently consume from all channels and verify input delivery
    let mut consumer_handles = Vec::new();
    for (idx, chan) in client_channels.into_iter().enumerate() {
        let consumer = Arc::new(InputConsumer::new(chan));
        let con_clone = Arc::clone(&consumer);

        consumer_handles.push(thread::spawn(move || {
            let msg = con_clone.consume().expect("Failed to consume in worker");
            if let InputMessage::Motion(m) = msg {
                assert_eq!(m.action, MOTION_ACTION_DOWN);
                assert_eq!(m.pointer_coords[0].x, (idx * 100 + 50) as f32);
                con_clone.send_finished_signal(m.seq, true).unwrap();
            } else {
                panic!("Expected MotionEvent");
            }
        }));
    }

    // Inject motion events targeting each focused channel
    for idx in 0..num_sessions {
        let chan_name = server_channels[idx].name();
        harness.input_service.dispatcher().set_focused_window(&chan_name);

        let mut v_source = VirtualEventSource::new(1);
        let x = (idx * 100 + 50) as f32;
        let event = InputEvent::Motion(v_source.make_touch_down(x, 200.0, 1000 + idx as i64));
        let injected = input_proxy
            .inject_input_event(&event, INJECT_INPUT_EVENT_MODE_WAIT_FOR_FINISH)
            .unwrap();
        assert!(injected);
    }

    for h in consumer_handles {
        h.join().unwrap();
    }

    // Cleanup all sessions
    for session in session_proxies {
        session.remove(None).unwrap();
    }
}
