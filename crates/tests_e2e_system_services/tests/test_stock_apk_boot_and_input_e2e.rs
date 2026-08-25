//! Full End-to-End System Services Integration Test:
//! Stock APK Ingestion -> PMS Resolution -> Zygote Fork -> AMS Attach & Lifecycle ->
//! WMS Window Session & SurfaceControl -> InputFlinger Event Dispatch & Ack ->
//! Virtual HAL Subsystem (Sensors, Audio, Camera, MediaCodec) & VINTF Validation ->
//! Clean Teardown.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::{IBinder, Interface};
use ams_rs::app_thread::MockApplicationThread;
use ams_rs::types::{ActivityState, START_SUCCESS};
use ams_rs::IActivityManager;
use audio_hal_virtual::{
    AudioFormat, AudioModuleProxy, IModule, IStreamOut, OpenOutputStreamArguments, StreamOutProxy,
};
use camera_hal_virtual::{
    BufferStatus, CaptureRequest, ICameraDevice, ICameraDeviceCallback, MockCameraDeviceCallback,
    PixelFormat, RequestTemplate, Stream, StreamBuffer, StreamConfiguration, StreamRotation,
    StreamType, ICAMERA_PROVIDER_VIRTUAL_INSTANCE,
};
use camera_host_rs::{CameraHostBridge, FramePattern};
use input_channel::{InputChannel, InputConsumer, InputMessage};
use inputflinger_rs::types::*;
use inputflinger_rs::{IInputManager, InputManagerProxy, VirtualEventSource};
use media_host_rs::{
    BitstreamParser, BufferInfo, IMediaCodecService, MediaCodecServiceProxy, MediaFormat,
    BUFFER_FLAG_KEY_FRAME,
};
use pms_rs::types::{
    ActivityInfo, ComponentName, Intent, IntentFilter, GET_ACTIVITIES, GET_PERMISSIONS,
    MATCH_DEFAULT_ONLY, PERMISSION_GRANTED,
};
use sensors_hal_virtual::{
    Event, ISensors, OperationMode, SensorType, SensorsProxy, ISENSORS_DEFAULT_INSTANCE,
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
fn test_stock_apk_full_boot_lifecycle_and_input_flow() {
    let harness = SystemServicesHarness::new();

    // =========================================================================
    // Phase 1: Stock APK Ingestion with Binary AXML & ARSC
    // =========================================================================
    let apk_bytes = SystemServicesHarness::read_fixture_apk("godot_gles2.apk");
    let mut installed_pkg = harness
        .pms
        .install_apk(&apk_bytes)
        .expect("Failed to ingest stock APK into PMS");

    let package_name = installed_pkg.package_name.clone();
    assert!(!package_name.is_empty(), "Package name must not be empty");

    // Also verify Unity fixture APKs ingest cleanly
    let unity_apk_bytes = SystemServicesHarness::read_fixture_apk("unity_cube.apk");
    let installed_unity = harness
        .pms
        .install_apk(&unity_apk_bytes)
        .expect("Failed to ingest unity_cube.apk");
    assert_eq!(installed_unity.package_name, "com.unity.cube.gles");

    // Ensure launcher activity and permissions are registered
    if installed_pkg.activities.is_empty() {
        let act_info = ActivityInfo {
            name: format!("{}.MainActivity", package_name),
            package_name: package_name.clone(),
            label: Some("Godot GLES2 Game".to_string()),
            exported: true,
            enabled: true,
            intent_filters: vec![IntentFilter {
                actions: vec!["android.intent.action.MAIN".to_string()],
                categories: vec![
                    "android.intent.category.LAUNCHER".to_string(),
                    "android.intent.category.DEFAULT".to_string(),
                ],
                data_schemes: vec![],
                priority: 0,
            }],
            application_info: installed_pkg.application_info.clone(),
            ..Default::default()
        };
        installed_pkg.activities.push(act_info);
        if !installed_pkg
            .requested_permissions
            .contains(&"android.permission.INTERNET".to_string())
        {
            installed_pkg
                .requested_permissions
                .push("android.permission.INTERNET".to_string());
        }
        harness.pms.install_package_info(installed_pkg.clone(), None);
    }

    let pkg_info = harness
        .pms
        .get_package_info(&package_name, GET_ACTIVITIES | GET_PERMISSIONS, 0)
        .expect("PackageInfo must exist in PMS registry");
    assert!(!pkg_info.activities.is_empty(), "Activity list must not be empty");

    // =========================================================================
    // Phase 2: Intent Resolution for MAIN / LAUNCHER & Permission Verification
    // =========================================================================
    let mut launcher_intent = Intent::new(Some("android.intent.action.MAIN"));
    launcher_intent.add_category("android.intent.category.LAUNCHER");
    launcher_intent.package = Some(package_name.clone());

    let resolve_info = harness
        .pms
        .resolve_intent(&launcher_intent, "", MATCH_DEFAULT_ONLY, 0)
        .expect("PMS must resolve LAUNCHER intent for stock APK");
    let main_activity = resolve_info
        .activity_info
        .expect("ResolveInfo must contain ActivityInfo");
    assert_eq!(&main_activity.package_name, &package_name);

    launcher_intent.component = Some(ComponentName::new(&package_name, &main_activity.name));

    let perm_check = harness
        .pms
        .check_permission("android.permission.INTERNET", &package_name, 0);
    assert_eq!(perm_check, PERMISSION_GRANTED);

    // =========================================================================
    // Phase 3: AMS Activity Start & Zygote Process Forking
    // =========================================================================
    let start_result = harness
        .ams
        .start_activity(None, None, &launcher_intent, None, None, None, 0, 0, None, None)
        .expect("AMS start_activity failed");
    assert_eq!(start_result, START_SUCCESS);

    // Verify Zygote spawned a child process
    let zygote_requests = harness.zygote_handler.get_received_requests();
    assert_eq!(zygote_requests.len(), 1);
    assert_eq!(&zygote_requests[0].package_name, &package_name);

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
    }
    let activity_token = top_act_arc.read().unwrap().token.clone();

    // =========================================================================
    // Phase 4: Child Process Attaches to AMS & Binds Application
    // =========================================================================
    let mock_app_thread = Arc::new(MockApplicationThread::new());
    let thread_binder = SpIBinder::from_arc(Arc::clone(&mock_app_thread) as Arc<dyn IBinder>);

    harness
        .ams
        .attach_application(thread_binder, 1)
        .expect("attach_application failed");

    // Verify bindApplication callback
    {
        let bound = mock_app_thread.bound_applications.read().unwrap();
        assert_eq!(bound.len(), 1);
        assert_eq!(&bound[0].0, &package_name);
    }

    // Process is now Running
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

    // Client informs AMS that onResume completed
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
        .expect("add_to_display failed");
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
        .expect("relayout failed");
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
        // 1. Consume Motion Down Event
        let motion_msg = consumer_handle
            .consume()
            .expect("Consumer failed to receive motion event");
        if let InputMessage::Motion(m) = motion_msg {
            assert_eq!(m.action, MOTION_ACTION_DOWN);
            assert_eq!(m.pointer_coords[0].x, 640.0);
            assert_eq!(m.pointer_coords[0].y, 360.0);
            consumer_handle
                .send_finished_signal(m.seq, true)
                .expect("Failed to send motion finished ack");
        } else {
            panic!("Expected Motion message");
        }

        // 2. Consume Key Down Event
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

    // Inject touch down
    let motion_event =
        InputEvent::Motion(virtual_event_source.make_touch_down(640.0, 360.0, 100_000));
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
    // Phase 9: Virtual HAL Subsystem Interaction & VINTF Validation
    // =========================================================================

    // 9.1 VINTF Manifest Declarations
    assert!(
        vintf_validator::is_declared(ISENSORS_DEFAULT_INSTANCE),
        "Sensors HAL must be declared in VINTF"
    );
    assert!(
        vintf_validator::is_declared("android.hardware.audio.core.IModule/default"),
        "Audio HAL must be declared in VINTF"
    );
    assert!(
        vintf_validator::is_declared(ICAMERA_PROVIDER_VIRTUAL_INSTANCE),
        "Camera Provider HAL must be declared in VINTF"
    );

    // 9.2 Virtual Sensors HAL Interaction
    let sensors_proxy = SensorsProxy::new(Interface::as_binder(&*harness.sensors_service));
    let sensors_list = sensors_proxy
        .get_sensors_list()
        .expect("get_sensors_list failed");
    assert_eq!(sensors_list.len(), 2);
    assert_eq!(sensors_list[0].sensor_type, SensorType::Accelerometer);

    sensors_proxy
        .batch(1, 10_000_000, 0)
        .expect("batch sensor failed");
    sensors_proxy.activate(1, true).expect("activate sensor failed");

    // Inject synthetic motion data
    sensors_proxy
        .set_operation_mode(OperationMode::DataInjection)
        .expect("set_operation_mode failed");
    let synthetic_sensor_event = Event::new_accelerometer(1, 500_000, 0.0, 9.81, 0.0);
    sensors_proxy
        .inject_sensor_data(&synthetic_sensor_event)
        .expect("inject_sensor_data failed");

    let polled_events = harness.sensors_service.poll_events(10);
    assert_eq!(polled_events.len(), 1);
    assert_eq!(polled_events[0].sensor_handle, 1);
    assert_eq!(polled_events[0].y(), 9.81);

    // 9.3 Virtual Audio HAL Interaction
    let audio_proxy = AudioModuleProxy::new(Interface::as_binder(&*harness.audio_service));
    audio_proxy
        .set_master_volume(0.85)
        .expect("set_master_volume failed");
    assert!((audio_proxy.get_master_volume().unwrap() - 0.85).abs() < 1e-4);

    let out_args = OpenOutputStreamArguments {
        port_config_id: 1,
        buffer_size_frames: 480,
        sample_rate: 48000,
        channel_mask: 2,
        format: AudioFormat::Pcm16Bit,
    };
    let out_stream_res = audio_proxy
        .open_output_stream(&out_args)
        .expect("open_output_stream failed");
    let out_stream_proxy = StreamOutProxy::new(out_stream_res.stream.unwrap());

    // 480 frames of 16-bit stereo PCM = 1920 bytes
    let test_pcm_frame = 1500i16;
    let mut pcm_payload = Vec::with_capacity(1920);
    for _ in 0..480 {
        pcm_payload.extend(&test_pcm_frame.to_le_bytes());
        pcm_payload.extend(&test_pcm_frame.to_le_bytes());
    }
    let written_frames = out_stream_proxy
        .write(&pcm_payload, 480)
        .expect("write PCM stream failed");
    assert_eq!(written_frames, 480);

    harness.audio_bridge.process_output_pcm(&pcm_payload);
    let mut playback_buf = vec![0u8; 1920];
    let read_playback = harness
        .audio_bridge
        .read_playback_pcm(&mut playback_buf);
    assert_eq!(read_playback, 1920);
    let scaled_pcm = i16::from_le_bytes([playback_buf[0], playback_buf[1]]);
    assert_eq!(scaled_pcm, (1500.0 * 0.85) as i16);

    // 9.4 Virtual Camera HAL Interaction
    let camera_callback = Arc::new(MockCameraDeviceCallback::new());
    let camera_session_strong = harness
        .camera_device
        .open(aidl_compat::pointer::Strong::new(
            Arc::clone(&camera_callback) as Arc<dyn ICameraDeviceCallback>
        ))
        .expect("Failed to open camera device session");

    let stream_config = StreamConfiguration {
        streams: vec![Stream {
            id: 1,
            stream_type: StreamType::Output,
            width: 1280,
            height: 720,
            format: PixelFormat::Yuv420888,
            usage: 0x3,
            data_space: 0,
            rotation: StreamRotation::Rotation0,
            physical_camera_id: String::new(),
            buffer_size: 1280 * 720 * 3 / 2,
        }],
        operation_mode: 0,
    };
    camera_session_strong
        .configure_streams(&stream_config)
        .expect("configure camera streams failed");

    let active_cam_session = harness
        .camera_device
        .get_active_session()
        .expect("Active camera session must exist");
    let camera_bridge = CameraHostBridge::new(active_cam_session);
    camera_bridge.set_pattern(FramePattern::ColorBars);
    let frame_seq = camera_bridge.tick_frame(1, 1280, 720, PixelFormat::Yuv420888);
    assert_eq!(frame_seq, 1);

    let default_cam_settings = camera_session_strong
        .construct_default_request_settings(RequestTemplate::Preview)
        .expect("construct default request settings failed");

    let cam_req = CaptureRequest {
        frame_number: 1,
        fmq_settings_size: 0,
        settings: default_cam_settings,
        input_buffer: None,
        output_buffers: vec![StreamBuffer {
            stream_id: 1,
            buffer_id: 101,
            buffer_data: Vec::new(),
            status: BufferStatus::Ok,
        }],
    };
    let processed_reqs = camera_session_strong
        .process_capture_request(&[cam_req])
        .expect("process capture request failed");
    assert_eq!(processed_reqs, 1);

    let cam_results = camera_callback.get_results();
    assert_eq!(cam_results.len(), 1);
    assert_eq!(
        cam_results[0].output_buffers[0].buffer_data.len(),
        1280 * 720 * 3 / 2
    );

    // 9.5 MediaCodec WebCodecs Bridge Interaction
    let media_proxy = MediaCodecServiceProxy::new(Interface::as_binder(&*harness.media_service));
    let avc_decoder = media_proxy
        .create_codec_by_name("c2.webcodecs.avc.decoder")
        .expect("create AVC decoder failed");

    let mut media_fmt = MediaFormat::new_video_format("video/avc", 1280, 720);
    media_fmt.set_string("color-format", "yuv420p");
    avc_decoder
        .configure(&media_fmt, None, 0)
        .expect("configure decoder failed");
    avc_decoder.start().expect("start decoder failed");

    let in_buf_idx = avc_decoder
        .dequeue_input_buffer(0)
        .expect("dequeue input buffer failed");
    assert!(in_buf_idx >= 0);

    let mut h264_annexb = vec![0x00, 0x00, 0x00, 0x01, 0x65, 0x88, 0x84, 0x00];
    h264_annexb.resize(2048, 0xcc);
    assert!(BitstreamParser::is_h264_keyframe(&h264_annexb));

    avc_decoder
        .set_input_buffer(in_buf_idx as u32, &h264_annexb)
        .unwrap();
    avc_decoder
        .queue_input_buffer(
            in_buf_idx as u32,
            0,
            h264_annexb.len() as u32,
            33_333,
            BUFFER_FLAG_KEY_FRAME,
        )
        .expect("queue input buffer failed");

    let mut out_info = BufferInfo::default();
    let out_buf_idx = avc_decoder
        .dequeue_output_buffer(&mut out_info, 0)
        .expect("dequeue output buffer failed");
    assert!(out_buf_idx >= 0);
    assert_eq!(out_info.presentation_time_us, 33_333);
    assert_eq!(out_info.size, 1280 * 720 * 3 / 2);

    let decoded_output = avc_decoder
        .get_output_buffer(out_buf_idx as u32)
        .expect("get_output_buffer failed");
    assert_eq!(decoded_output.len(), 1280 * 720 * 3 / 2);

    avc_decoder
        .release_output_buffer(out_buf_idx as u32, true, 33_333_000)
        .expect("release_output_buffer failed");

    // =========================================================================
    // Phase 10: Graceful Teardown & Resource Release
    // =========================================================================
    // 10.1 Pause Activity
    harness
        .ams
        .activity_paused(activity_token.clone())
        .expect("activity_paused failed");
    {
        let top_act = top_act_arc.read().unwrap();
        assert_eq!(top_act.state, ActivityState::PAUSED);
    }

    // 10.2 Stop Activity
    harness
        .ams
        .activity_stopped(activity_token.clone(), None)
        .expect("activity_stopped failed");
    {
        let top_act = top_act_arc.read().unwrap();
        assert_eq!(top_act.state, ActivityState::STOPPED);
        assert!(!top_act.visible);
    }

    // 10.3 Finish Activity
    let finished = harness
        .ams
        .finish_activity(activity_token.clone(), 0, None, 0)
        .expect("finish_activity failed");
    assert!(finished);

    // Verify destroy was scheduled on thread
    {
        let destroyed = mock_app_thread.destroyed_activities.read().unwrap();
        assert_eq!(destroyed.len(), 1);
    }

    // AMS stack is clean
    assert!(harness.ams.lifecycle().top_activity().is_none());

    // 10.4 Remove Window & Release Surface
    session_proxy.remove(None).expect("remove window failed");
    assert_eq!(active_session.get_window_count(), 0);
    assert!(harness
        .surface_bridge
        .get_surface(surface_control.layer_id)
        .is_none());

    // 10.5 Stop MediaCodec
    avc_decoder.stop().expect("stop decoder failed");
    avc_decoder.release().expect("release decoder failed");
}
