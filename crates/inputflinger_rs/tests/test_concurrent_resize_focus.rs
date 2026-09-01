//! Empirical Challenger 2: Concurrent Window Focus and Rapid Event Dispatch Stress Test

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::IBinder;
use input_channel::{InputChannel, InputConsumer, InputMessage};
use inputflinger_rs::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

#[test]
fn test_concurrent_window_focus_and_event_storm() {
    let service = Arc::new(InputManagerService::new());
    let binder = SpIBinder::from_arc(Arc::clone(&service) as Arc<dyn IBinder>);
    let client = Arc::new(InputManagerProxy::new(binder));

    // Register 4 window channels: Window0, Window1, Window2, Window3
    let num_windows = 4;
    let mut consumers = Vec::new();
    let mut server_names = Vec::new();

    for i in 0..num_windows {
        let name = format!("Window{}", i);
        let (server_chan, client_chan) = InputChannel::create_memory_pair(&name);
        client.register_input_channel(&server_chan).unwrap();
        server_names.push(format!("{}_server", name));
        consumers.push(Arc::new(InputConsumer::new(client_chan)));
    }

    let running = Arc::new(AtomicBool::new(true));
    let total_injected = Arc::new(AtomicUsize::new(0));

    // 1. Thread: Focus Switcher (rapidly toggles focus across the 4 windows)
    let s_svc = Arc::clone(&service);
    let s_names = server_names.clone();
    let s_running = Arc::clone(&running);
    let focus_thread = thread::spawn(move || {
        let mut idx = 0;
        while s_running.load(Ordering::Relaxed) {
            s_svc.dispatcher().set_focused_window(&s_names[idx % s_names.len()]);
            idx += 1;
            thread::yield_now();
        }
    });

    // 2. Thread: Event Injector (injects rapid touch & key events)
    let i_client = Arc::clone(&client);
    let i_running = Arc::clone(&running);
    let i_injected = Arc::clone(&total_injected);
    let inject_thread = thread::spawn(move || {
        let mut v_source = VirtualEventSource::new(1);
        let coords = [
            (0.0, 0.0),
            (719.0, 1439.0),
            (360.0, 720.0),
            (-50.0, -50.0),
            (99999.0, 99999.0),
        ];

        for iter in 0..1000 {
            let (x, y) = coords[iter % coords.len()];
            let motion = v_source.make_touch_down(x, y, (iter * 10) as i64);
            if i_client
                .inject_input_event(&InputEvent::Motion(motion), INJECT_INPUT_EVENT_MODE_ASYNC)
                .is_ok()
            {
                i_injected.fetch_add(1, Ordering::Relaxed);
            }

            let key = v_source.make_key_event(KEYCODE_A, KEY_ACTION_DOWN, (iter * 10 + 1) as i64);
            if i_client
                .inject_input_event(&InputEvent::Key(key), INJECT_INPUT_EVENT_MODE_ASYNC)
                .is_ok()
            {
                i_injected.fetch_add(1, Ordering::Relaxed);
            }
        }
        i_running.store(false, Ordering::Relaxed);
    });

    // 3. Drain consumers in parallel
    let mut consumer_threads = Vec::new();
    let total_consumed = Arc::new(AtomicUsize::new(0));

    for (_c_idx, consumer) in consumers.into_iter().enumerate() {
        let c_clone = Arc::clone(&consumer);
        let c_running = Arc::clone(&running);
        let c_tot = Arc::clone(&total_consumed);

        let t = thread::spawn(move || {
            let mut count = 0;
            while c_running.load(Ordering::Relaxed) {
                while let Ok(Some(msg)) = c_clone.try_consume() {
                    count += 1;
                    c_tot.fetch_add(1, Ordering::Relaxed);
                    let _ = c_clone.send_finished_signal(msg.seq(), true);
                }
                thread::yield_now();
            }
            // Final drain
            while let Ok(Some(msg)) = c_clone.try_consume() {
                count += 1;
                c_tot.fetch_add(1, Ordering::Relaxed);
                let _ = c_clone.send_finished_signal(msg.seq(), true);
            }
            count
        });
        consumer_threads.push(t);
    }

    inject_thread.join().unwrap();
    focus_thread.join().unwrap();

    let mut sum_consumed = 0;
    for t in consumer_threads {
        sum_consumed += t.join().unwrap();
    }

    let injected = total_injected.load(Ordering::Relaxed);
    assert_eq!(injected, 2000, "All 2,000 events must be injected");
    assert_eq!(
        sum_consumed, injected,
        "Total consumed events ({}) must match total injected events ({})",
        sum_consumed, injected
    );
}

#[test]
fn test_unregister_focused_window_fallback() {
    let service = Arc::new(InputManagerService::new());
    let binder = SpIBinder::from_arc(Arc::clone(&service) as Arc<dyn IBinder>);
    let client = InputManagerProxy::new(binder);

    let (server_a, client_a) = InputChannel::create_memory_pair("WinA");
    let (server_b, client_b) = InputChannel::create_memory_pair("WinB");

    client.register_input_channel(&server_a).unwrap();
    client.register_input_channel(&server_b).unwrap();

    let consumer_a = InputConsumer::new(client_a);
    let consumer_b = InputConsumer::new(client_b);

    // Focus is WinA
    service.dispatcher().set_focused_window("WinA_server");

    // Unregister WinA -> focus should cleanly fall back to WinB_server without crashing
    service.dispatcher().unregister_window_channel("WinA_server");

    let mut v_source = VirtualEventSource::new(1);
    let motion = v_source.make_touch_down(100.0, 200.0, 5000);
    client
        .inject_input_event(&InputEvent::Motion(motion), INJECT_INPUT_EVENT_MODE_ASYNC)
        .expect("Injection must succeed after focused window unregistration");

    let msg = consumer_b.consume().expect("WinB must receive event after WinA unregistration");
    if let InputMessage::Motion(m) = msg {
        assert_eq!(m.pointer_coords[0].x, 100.0);
        assert_eq!(m.pointer_coords[0].y, 200.0);
    } else {
        panic!("Expected Motion event on WinB");
    }

    // WinA must receive nothing
    assert!(consumer_a.try_consume().unwrap().is_none());
}
