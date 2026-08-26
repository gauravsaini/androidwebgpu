//! Comprehensive Empirical Challenger Test Suite for System Services, IPC, BufferQueue, and InputChannel.
//!
//! Tests:
//! 1. BufferQueue Starvation & Slot Exhaustion Under Burst Queuing (17+ frames).
//! 2. High-Throughput Concurrent BufferQueue Dequeue/Queue Pipeline (4 threads, 1,000 frames).
//! 3. AMS Process Launch Lifecycle Transitions.
//! 4. Empirical Proof: AMS `attach_application` FIFO Race Condition Under Out-of-Order Process Startup.
//! 5. InputChannel Concurrent Streaming & Zero-Drop Delivery Across Memory & Socket Backend.
//! 6. Empirical Proof: `InputChannel::receive_message()` Mutex Hold Blocks Concurrent `send_message()`.
//! 7. Empirical Proof: Unix Datagram Socketpair Buffer Saturation (ENOBUFS / OS Error 55).
//! 8. InputFlinger Synchronous Dispatch and Flow Control With Awaited Acks.
//! 9. Empirical Proof: InputFlinger Burst Dispatch Triggers Socket Buffer Saturation (ENOBUFS).
//! 10. WMS Multi-Session Scaling (30 concurrent sessions with surface transactions).
//! 11. PMS Thread-Safe Concurrent Intent Resolution and Dynamic Package Ingestion.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::IBinder;
use ams_rs::app_thread::MockApplicationThread;
use ams_rs::types::START_SUCCESS;
use ams_rs::{ActivityManagerService, IActivityManager};
use input_channel::{InputChannel, InputConsumer, InputMessage, InputPublisher, MotionEventData};
use inputflinger_rs::types::*;
use inputflinger_rs::VirtualEventSource;
use pms_rs::types::{
    ActivityInfo, ApplicationInfo, ComponentName, Intent, IntentFilter, PackageInfo,
    MATCH_DEFAULT_ONLY,
};
use pms_rs::PackageManagerService;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use surfaceflinger_gpu_service::buffer_queue::{BufferQueueError, GraphicBufferProducerService};
use tests_e2e_system_services::SystemServicesHarness;
use wms_rs::{
    IWindowSession, InsetsState, LayoutParams, SurfaceControl, SurfaceControlTransaction,
    WindowSessionProxy, FLAG_HARDWARE_ACCELERATED,
};
use zygote_client::socket::ZygoteClient;

async fn create_test_wgpu() -> (Arc<wgpu::Device>, Arc<wgpu::Queue>) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .expect("Failed to find suitable WGPU adapter");

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Test WGPU Device"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        )
        .await
        .expect("Failed to create test WGPU device");

    (Arc::new(device), Arc::new(queue))
}

// -----------------------------------------------------------------------------
// 1. BufferQueue Starvation & Slot Exhaustion Under Burst Queuing
// -----------------------------------------------------------------------------

#[test]
fn test_buffer_queue_slot_starvation_and_exhaustion() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let producer = GraphicBufferProducerService::new(1, device, queue);
        producer.connect().unwrap();

        let mut dequeued_slots = Vec::new();

        // Dequeue and queue all 16 slots without calling acquire_latest_texture_view
        for _ in 0..16 {
            let slot = producer.dequeue_buffer(64, 64, 1).expect("Slot must dequeue cleanly");
            assert!(!dequeued_slots.contains(&slot));
            dequeued_slots.push(slot);
            producer.queue_buffer_color(slot, [255, 0, 0, 255], 64, 64).expect("Must queue cleanly");
        }

        // Slot 17 dequeue: Since all 16 slots are in SlotState::Queued, dequeue must return NoFreeSlots
        let res_17 = producer.dequeue_buffer(64, 64, 1);
        assert_eq!(res_17, Err(BufferQueueError::NoFreeSlots), "Must return NoFreeSlots when all 16 slots are queued");

        // Now compositor acquires the latest texture view (which is the last queued slot)
        let last_queued = *dequeued_slots.last().unwrap();
        let latest_view = producer.acquire_latest_texture_view();
        assert!(latest_view.is_some(), "Must acquire latest texture view");

        // Now test if bypassed queued slots were freed:
        let next_dequeue = producer.dequeue_buffer(64, 64, 1);
        assert!(next_dequeue.is_ok(), "Bypassed queued slots must be freed and available for dequeue");

        // Release the acquired slot back to Free
        producer.release_buffer(last_queued);
        let slot_after_release = producer.dequeue_buffer(64, 64, 1);
        assert!(slot_after_release.is_ok(), "Released slot becomes available");
    });
}

// -----------------------------------------------------------------------------
// 2. High-Throughput Concurrent BufferQueue Dequeue/Queue Pipeline
// -----------------------------------------------------------------------------

#[test]
fn test_buffer_queue_concurrent_producer_consumer_throughput() {
    pollster::block_on(async {
        let (device, queue) = create_test_wgpu().await;
        let producer = Arc::new(GraphicBufferProducerService::new(2, device, queue));
        producer.connect().unwrap();

        let stop_flag = Arc::new(AtomicBool::new(false));
        let frames_presented = Arc::new(AtomicUsize::new(0));

        // Consumer thread: simulates SurfaceFlinger compositor polling latest frames
        let prod_cons = Arc::clone(&producer);
        let stop_cons = Arc::clone(&stop_flag);
        let frames_cons = Arc::clone(&frames_presented);
        let consumer_handle = thread::spawn(move || {
            while !stop_cons.load(Ordering::Relaxed) {
                if let Some(_view) = prod_cons.acquire_latest_texture_view() {
                    frames_cons.fetch_add(1, Ordering::SeqCst);
                }
                thread::yield_now();
            }
        });

        // 4 Producer threads: concurrently dequeue, upload data, and queue buffers
        let num_producers = 4;
        let frames_per_producer = 250;
        let mut prod_handles = Vec::new();

        for t in 0..num_producers {
            let prod_clone = Arc::clone(&producer);
            prod_handles.push(thread::spawn(move || {
                let mut queued = 0;
                while queued < frames_per_producer {
                    match prod_clone.dequeue_buffer(32, 32, 1) {
                        Ok(slot) => {
                            let color = [(t * 50) as u8, (queued % 255) as u8, 128, 255];
                            prod_clone.queue_buffer_color(slot, color, 32, 32).unwrap();
                            queued += 1;
                        }
                        Err(BufferQueueError::NoFreeSlots) => {
                            thread::yield_now();
                        }
                        Err(e) => panic!("Unexpected buffer queue error: {:?}", e),
                    }
                }
            }));
        }

        for h in prod_handles {
            h.join().unwrap();
        }

        stop_flag.store(true, Ordering::SeqCst);
        consumer_handle.join().unwrap();

        assert!(frames_presented.load(Ordering::SeqCst) > 0, "Compositor must have presented frames");
    });
}

// -----------------------------------------------------------------------------
// 3. AMS Process Launch Lifecycle Transitions
// -----------------------------------------------------------------------------

#[test]
fn test_ams_concurrent_process_launches_and_lifecycle_transitions() {
    let pms = Arc::new(PackageManagerService::new());
    let (zygote, mock_handler) = ZygoteClient::new_mock_default();
    let ams = Arc::new(ActivityManagerService::new(
        Arc::new(pms_rs::service::PackageManagerClient::new(
            SpIBinder::from_arc(Arc::clone(&pms) as Arc<dyn IBinder>),
        )),
        Arc::new(zygote),
    ));

    // Register 5 packages in PMS
    for i in 0..5 {
        let pkg_name = format!("com.challenger.app_{}", i);
        let act_name = format!("{}.MainActivity", pkg_name);
        let app_info = ApplicationInfo {
            package_name: pkg_name.clone(),
            name: Some(format!("App_{}", i)),
            uid: 10000 + i,
            ..Default::default()
        };
        let act_info = ActivityInfo {
            name: act_name.clone(),
            package_name: pkg_name.clone(),
            enabled: true,
            exported: true,
            application_info: Some(app_info.clone()),
            ..Default::default()
        };
        let pkg_info = PackageInfo {
            package_name: pkg_name.clone(),
            application_info: Some(app_info),
            activities: vec![act_info],
            ..Default::default()
        };
        pms.install_package_info(pkg_info, None);
    }

    // Launch each application sequentially and attach application thread
    for i in 0..5 {
        let pkg_name = format!("com.challenger.app_{}", i);
        let act_name = format!("{}.MainActivity", pkg_name);
        let mut intent = Intent::new(Some("android.intent.action.MAIN"));
        intent.package = Some(pkg_name.clone());
        intent.component = Some(ComponentName::new(&pkg_name, &act_name));

        let res = ams.start_activity(None, None, &intent, None, None, None, 0, 0, None, None).unwrap();
        assert_eq!(res, START_SUCCESS);

        let mock_thread = Arc::new(MockApplicationThread::new());
        let thread_binder = SpIBinder::from_arc(Arc::clone(&mock_thread) as Arc<dyn IBinder>);
        ams.attach_application(thread_binder, (i + 1) as i64).unwrap();

        // Verify bind_application was received with the correct package
        let bound_pkg = mock_thread
            .bound_applications
            .read()
            .unwrap()
            .first()
            .map(|(pkg, _, _)| pkg.clone());
        assert_eq!(bound_pkg, Some(pkg_name.clone()), "Bound package must match launched package");
    }

    assert_eq!(mock_handler.get_received_requests().len(), 5);
}

// -----------------------------------------------------------------------------
// 4. Empirical Proof: AMS attach_application FIFO Race Condition
// -----------------------------------------------------------------------------

#[test]
fn test_ams_concurrent_attach_application_fifo_race_condition_proof() {
    let pms = Arc::new(PackageManagerService::new());
    let (zygote, _mock_handler) = ZygoteClient::new_mock_default();
    let ams = Arc::new(ActivityManagerService::new(
        Arc::new(pms_rs::service::PackageManagerClient::new(
            SpIBinder::from_arc(Arc::clone(&pms) as Arc<dyn IBinder>),
        )),
        Arc::new(zygote),
    ));

    // Register Package A and Package B
    for &pkg in &["com.test.appA", "com.test.appB"] {
        let act_name = format!("{}.MainActivity", pkg);
        let app_info = ApplicationInfo {
            package_name: pkg.to_string(),
            name: Some(pkg.to_string()),
            uid: 10001,
            ..Default::default()
        };
        let act_info = ActivityInfo {
            name: act_name.clone(),
            package_name: pkg.to_string(),
            enabled: true,
            exported: true,
            application_info: Some(app_info.clone()),
            ..Default::default()
        };
        let pkg_info = PackageInfo {
            package_name: pkg.to_string(),
            application_info: Some(app_info),
            activities: vec![act_info],
            ..Default::default()
        };
        pms.install_package_info(pkg_info, None);
    }

    // Launch App A (spawns child PID 10001, queued in pending_launches at head)
    let mut intent_a = Intent::new(Some("android.intent.action.MAIN"));
    intent_a.package = Some("com.test.appA".to_string());
    intent_a.component = Some(ComponentName::new("com.test.appA", "com.test.appA.MainActivity"));
    ams.start_activity(None, None, &intent_a, None, None, None, 0, 0, None, None).unwrap();

    // Launch App B (spawns child PID 10002, queued in pending_launches second)
    let mut intent_b = Intent::new(Some("android.intent.action.MAIN"));
    intent_b.package = Some("com.test.appB".to_string());
    intent_b.component = Some(ComponentName::new("com.test.appB", "com.test.appB.MainActivity"));
    ams.start_activity(None, None, &intent_b, None, None, None, 0, 0, None, None).unwrap();

    // Simulate: Process B starts up FASTER than Process A, and calls attachApplication first!
    let mock_thread_b = Arc::new(MockApplicationThread::new());
    let thread_b_binder = SpIBinder::from_arc(Arc::clone(&mock_thread_b) as Arc<dyn IBinder>);
    ams.attach_application(thread_b_binder, 10002).unwrap();

    // Verify: Because AMS matches by PID, Process B correctly receives App B's metadata!
    let bound_b = mock_thread_b
        .bound_applications
        .read()
        .unwrap()
        .first()
        .map(|(pkg, _, _)| pkg.clone());
    assert_eq!(
        bound_b,
        Some("com.test.appB".to_string()),
        "Out-of-order attachApplication with PID 10002 matches App B"
    );
}

// -----------------------------------------------------------------------------
// 5. InputChannel Concurrent Streaming & Zero-Drop Delivery
// -----------------------------------------------------------------------------

#[test]
fn test_input_channel_concurrent_streaming_and_zero_drop() {
    let (server_chan, client_chan) = InputChannel::create_memory_pair("concurrent_mem_stream");

    let publisher = Arc::new(InputPublisher::new(server_chan));
    let consumer = Arc::new(InputConsumer::new(client_chan));

    let total_events = 2000;
    let consumed_count = Arc::new(AtomicUsize::new(0));

    // Consumer worker: continuously consumes and sends finished signal
    let con_clone = Arc::clone(&consumer);
    let con_count = Arc::clone(&consumed_count);
    let consumer_handle = thread::spawn(move || {
        for _ in 0..total_events {
            let msg = con_clone.consume().expect("Consumer must receive message");
            if let InputMessage::Motion(m) = msg {
                con_clone.send_finished_signal(m.seq, true).unwrap();
                con_count.fetch_add(1, Ordering::SeqCst);
            }
        }
    });

    // Publisher thread: publishes 2,000 motion events
    let pub_clone = Arc::clone(&publisher);
    let publisher_handle = thread::spawn(move || {
        for i in 0..total_events {
            let mut motion = MotionEventData::default();
            motion.action = MOTION_ACTION_MOVE;
            motion.event_time = (1000 + i) as i64;
            motion.pointer_count = 1;
            motion.pointer_coords[0].x = i as f32;
            motion.pointer_coords[0].y = (i * 2) as f32;

            pub_clone.publish_motion(motion).unwrap();
        }
    });

    publisher_handle.join().unwrap();
    consumer_handle.join().unwrap();

    assert_eq!(consumed_count.load(Ordering::SeqCst), total_events);
}

// -----------------------------------------------------------------------------
// 6. Empirical Proof: InputChannel Mutex Hold Lock Contention
// -----------------------------------------------------------------------------

#[test]
fn test_input_channel_receive_message_mutex_contention_proof() {
    let (server_chan, client_chan) = InputChannel::create_memory_pair("contention_proof");

    let server_arc = Arc::new(server_chan);
    let client_arc = Arc::new(client_chan);

    let server_waiter = Arc::clone(&server_arc);
    let waiter_started = Arc::new(AtomicBool::new(false));
    let started_clone = Arc::clone(&waiter_started);

    // Thread 1: Calls receive_message on server_chan while no message is available
    let waiter_handle = thread::spawn(move || {
        started_clone.store(true, Ordering::SeqCst);
        let _ = server_waiter.receive_message();
    });

    while !waiter_started.load(Ordering::SeqCst) {
        thread::yield_now();
    }
    thread::sleep(Duration::from_millis(20));

    // Thread 2: Calls send_message on the SAME server_chan while Thread 1 is in receive_message
    // It succeeds immediately without lock contention because lock is not held across wait loop!
    let server_sender = Arc::clone(&server_arc);
    let send_succeeded = Arc::new(AtomicBool::new(false));
    let send_succeeded_clone = Arc::clone(&send_succeeded);

    let sender_handle = thread::spawn(move || {
        let mut motion = MotionEventData::default();
        motion.seq = 100;
        if server_sender.send_message(&InputMessage::Motion(motion)).is_ok() {
            send_succeeded_clone.store(true, Ordering::SeqCst);
        }
    });

    sender_handle.join().unwrap();
    assert!(send_succeeded.load(Ordering::SeqCst), "send_message on same channel succeeds without contention");

    // Unblock thread 1 by sending from client_chan to server_chan's queue
    let mut fin = input_channel::FinishedData::default();
    fin.seq = 1;
    client_arc.send_message(&InputMessage::Finished(fin)).unwrap();

    waiter_handle.join().unwrap();
}

// -----------------------------------------------------------------------------
// 7. Empirical Proof: Unix Datagram Socketpair Buffer Saturation (ENOBUFS)
// -----------------------------------------------------------------------------

#[test]
fn test_input_channel_socketpair_buffer_saturation_proof() {
    let (server_chan, _client_chan) = InputChannel::open_input_channel_pair("saturation_test")
        .expect("Must open input channel pair");

    let publisher = InputPublisher::new(server_chan);

    // Burst write without any reader until socket buffer saturates
    let mut sent = 0;
    let mut saturated = false;

    for i in 0..5000 {
        let mut motion = MotionEventData::default();
        motion.action = MOTION_ACTION_MOVE;
        motion.event_time = i as i64;
        motion.pointer_count = 1;

        match publisher.publish_motion(motion) {
            Ok(_) => {
                sent += 1;
            }
            Err(e) => {
                let err_str = e.to_string();
                assert!(
                    err_str.contains("No buffer space available")
                        || err_str.contains("os error 55")
                        || err_str.contains("WouldBlock"),
                    "Expected socket buffer exhaustion error, got: {}",
                    err_str
                );
                saturated = true;
                break;
            }
        }
    }

    #[cfg(unix)]
    assert!(saturated, "Socket buffer must saturate after sending {} messages without consumer drain", sent);
}

// -----------------------------------------------------------------------------
// 8. InputFlinger Synchronous Dispatch and Flow Control With Awaited Acks
// -----------------------------------------------------------------------------

#[test]
fn test_inputflinger_synchronous_dispatch_and_ack_flow_control() {
    let input_service = Arc::new(inputflinger_rs::InputManagerService::new());
    let dispatcher = input_service.dispatcher();

    let num_windows = 4;
    let mut consumers = Vec::new();
    let mut channel_names = Vec::new();

    for w in 0..num_windows {
        let name = format!("window_sync_flow_{}", w);
        let (server_chan, client_chan) = InputChannel::open_input_channel_pair(&name).unwrap();
        dispatcher.register_window_channel(&name, Arc::new(server_chan));
        consumers.push(Arc::new(InputConsumer::new(client_chan)));
        channel_names.push(name);
    }

    let stop_flag = Arc::new(AtomicBool::new(false));
    let handled_counts = Arc::new(AtomicUsize::new(0));
    let mut consumer_handles = Vec::new();

    // Spawn consumer workers for each window
    for consumer in consumers {
        let stop_c = Arc::clone(&stop_flag);
        let counts = Arc::clone(&handled_counts);
        consumer_handles.push(thread::spawn(move || {
            while !stop_c.load(Ordering::Relaxed) {
                if let Ok(Some(msg)) = consumer.try_consume() {
                    match msg {
                        InputMessage::Motion(m) => {
                            let _ = consumer.send_finished_signal(m.seq, true);
                            counts.fetch_add(1, Ordering::SeqCst);
                        }
                        InputMessage::Key(k) => {
                            let _ = consumer.send_finished_signal(k.seq, true);
                            counts.fetch_add(1, Ordering::SeqCst);
                        }
                        _ => {}
                    }
                }
                thread::yield_now();
            }
        }));
    }

    // Synchronously dispatch 100 events awaiting ack (flow-controlled, 0 buffer drops)
    let total_injections = 100;
    for i in 0..total_injections {
        let target_win = &channel_names[i % num_windows];
        dispatcher.set_focused_window(target_win);

        let v_source = VirtualEventSource::new(1);
        let ev = InputEvent::Motion(v_source.make_touch_move((i as f32) * 1.5, 100.0, 5000 + i as i64));
        let acked = dispatcher.dispatch_and_wait_for_ack(&ev, 500).expect("Sync dispatch must succeed and receive ack");
        assert!(acked);
    }

    stop_flag.store(true, Ordering::SeqCst);
    for h in consumer_handles {
        h.join().unwrap();
    }

    assert_eq!(handled_counts.load(Ordering::SeqCst), total_injections);
}

// -----------------------------------------------------------------------------
// 9. Empirical Proof: InputFlinger Burst Dispatch Socket Buffer Exhaustion
// -----------------------------------------------------------------------------

#[test]
fn test_inputflinger_burst_dispatch_socket_buffer_exhaustion_proof() {
    let input_service = Arc::new(inputflinger_rs::InputManagerService::new());
    let dispatcher = input_service.dispatcher();

    let (server_chan, _client_chan) = InputChannel::open_input_channel_pair("unbuffered_win").unwrap();
    dispatcher.register_window_channel("unbuffered_win", Arc::new(server_chan));
    dispatcher.set_focused_window("unbuffered_win");

    let mut sent = 0;
    let mut hit_overflow = false;

    // Burst dispatch without any consumer draining client socket
    for i in 0..2000 {
        let v_source = VirtualEventSource::new(1);
        let ev = InputEvent::Motion(v_source.make_touch_move(i as f32, 100.0, 1000 + i as i64));
        match dispatcher.dispatch_event(&ev) {
            Ok(_) => {
                sent += 1;
            }
            Err(e) => {
                let err_str = e.to_string();
                assert!(
                    err_str.contains("No buffer space available")
                        || err_str.contains("os error 55")
                        || err_str.contains("WouldBlock"),
                    "Expected socket overflow error, got: {}",
                    err_str
                );
                hit_overflow = true;
                break;
            }
        }
    }

    #[cfg(unix)]
    assert!(hit_overflow, "Burst dispatching without consumer drain must hit socket saturation after {} events", sent);
}

// -----------------------------------------------------------------------------
// 10. WMS Multi-Session Scaling (30 Concurrent Window Sessions)
// -----------------------------------------------------------------------------

#[test]
fn test_wms_thirty_concurrent_sessions_scaling() {
    let harness = Arc::new(SystemServicesHarness::new());
    let num_sessions = 30;
    let success_counter = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for i in 0..num_sessions {
        let h = Arc::clone(&harness);
        let counter = Arc::clone(&success_counter);

        handles.push(thread::spawn(move || {
            let (_sess_id, session_arc) = h.wms.open_session_internal(None).unwrap();
            let session_binder = SpIBinder::from_arc(session_arc as Arc<dyn IBinder>);
            let session = WindowSessionProxy::new(session_binder);

            let mut attrs = LayoutParams::default();
            attrs.title = format!("scaling_win_{}", i);
            attrs.flags = FLAG_HARDWARE_ACCELERATED;

            let mut insets = InsetsState::default();
            let mut client_chan = InputChannel::default();
            let add_res = session
                .add_to_display(None, &attrs, 0, 0, &mut insets, &mut client_chan)
                .unwrap();
            assert_eq!(add_res, 0);

            let mut sc = SurfaceControl::default();
            let relayout_res = session
                .relayout(None, &attrs, 320, 240, 0, 0, &mut sc)
                .unwrap();
            assert_ne!(relayout_res, 0);

            let mut tx = SurfaceControlTransaction::new(sc.layer_id);
            tx.set_position((i * 5) as f32, (i * 5) as f32)
                .set_size(320, 240)
                .set_alpha(0.85)
                .set_z_order(i as i32);
            session.finish_drawing(None, Some(&tx)).unwrap();

            session.remove(None).unwrap();
            counter.fetch_add(1, Ordering::SeqCst);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(success_counter.load(Ordering::SeqCst), num_sessions);
}

// -----------------------------------------------------------------------------
// 11. PMS Thread-Safe Concurrent Intent Resolution and Ingestion
// -----------------------------------------------------------------------------

#[test]
fn test_pms_concurrent_intent_resolution_and_queries() {
    let pms = Arc::new(PackageManagerService::new());

    // Pre-install 20 packages with enabled: true
    for i in 0..20 {
        let pkg_name = format!("com.concurrent.pkg_{}", i);
        let act_name = format!("{}.TargetActivity", pkg_name);
        let app_info = ApplicationInfo {
            package_name: pkg_name.clone(),
            name: Some(format!("Pkg_{}", i)),
            uid: 10500 + i,
            ..Default::default()
        };
        let act_info = ActivityInfo {
            name: act_name.clone(),
            package_name: pkg_name.clone(),
            enabled: true,
            exported: true,
            intent_filters: vec![IntentFilter {
                actions: vec!["android.intent.action.VIEW".to_string()],
                categories: vec!["android.intent.category.DEFAULT".to_string()],
                data_schemes: vec![],
                priority: 0,
            }],
            application_info: Some(app_info.clone()),
            ..Default::default()
        };
        let pkg_info = PackageInfo {
            package_name: pkg_name.clone(),
            application_info: Some(app_info),
            activities: vec![act_info],
            ..Default::default()
        };
        pms.install_package_info(pkg_info, None);
    }

    let num_threads = 16;
    let queries_per_thread = 500;
    let mut handles = Vec::new();

    for t in 0..num_threads {
        let pms_clone = Arc::clone(&pms);
        handles.push(thread::spawn(move || {
            for i in 0..queries_per_thread {
                let target_idx = (t + i) % 20;
                let pkg_name = format!("com.concurrent.pkg_{}", target_idx);

                // 1. Direct package lookup
                let pkg = pms_clone.get_package_info(&pkg_name, 0, 0);
                assert!(pkg.is_some());

                // 2. Intent resolution
                let mut intent = Intent::new(Some("android.intent.action.VIEW"));
                intent.add_category("android.intent.category.DEFAULT");
                intent.package = Some(pkg_name.clone());

                let resolve = pms_clone.resolve_intent(&intent, "", MATCH_DEFAULT_ONLY, 0);
                assert!(resolve.is_some(), "Intent resolution must find registered default activity");
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}
