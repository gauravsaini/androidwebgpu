//! Comprehensive integration tests for InputFlinger event dispatch, focus arbitration, and AIDL IPC.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::IBinder;
use input_channel::{InputChannel, InputConsumer, InputMessage};
use inputflinger_rs::*;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_focus_switching_between_windows() {
    let service = Arc::new(InputManagerService::new());
    let binder = SpIBinder::from_arc(Arc::clone(&service) as Arc<dyn IBinder>);
    let client = InputManagerProxy::new(binder);

    // Create 2 Window channels: WinA and WinB
    let (server_a, client_a) = InputChannel::open_input_channel_pair("WinA").unwrap();
    let (server_b, client_b) = InputChannel::open_input_channel_pair("WinB").unwrap();

    client.register_input_channel(&server_a).unwrap();
    client.register_input_channel(&server_b).unwrap();

    let consumer_a = InputConsumer::new(client_a);
    let consumer_b = InputConsumer::new(client_b);

    let v_source = VirtualEventSource::new(1);

    // 1. Focus is WinA by default
    let key_a = v_source.make_key_event(KEYCODE_A, KEY_ACTION_DOWN, 1000);
    client
        .inject_input_event(&InputEvent::Key(key_a), INJECT_INPUT_EVENT_MODE_ASYNC)
        .unwrap();

    let msg_a = consumer_a.consume().unwrap();
    if let InputMessage::Key(k) = msg_a {
        assert_eq!(k.key_code, KEYCODE_A);
    } else {
        panic!("Expected Key event for WinA");
    }

    // Ensure WinB did not receive it
    assert!(consumer_b.try_consume().unwrap().is_none());

    // 2. Switch focus to WinB
    service.dispatcher().set_focused_window("WinB_server");

    let key_b = v_source.make_key_event(KEYCODE_B, KEY_ACTION_DOWN, 2000);
    client
        .inject_input_event(&InputEvent::Key(key_b), INJECT_INPUT_EVENT_MODE_ASYNC)
        .unwrap();

    let msg_b = consumer_b.consume().unwrap();
    if let InputMessage::Key(k) = msg_b {
        assert_eq!(k.key_code, KEYCODE_B);
    } else {
        panic!("Expected Key event for WinB");
    }

    // Ensure WinA did not receive it
    assert!(consumer_a.try_consume().unwrap().is_none());
}

#[test]
fn test_sync_injection_with_ack_handshake() {
    let service = Arc::new(InputManagerService::new());
    let binder = SpIBinder::from_arc(Arc::clone(&service) as Arc<dyn IBinder>);
    let client = InputManagerProxy::new(binder);

    let (server, client_chan) = InputChannel::open_input_channel_pair("SyncWin").unwrap();
    client.register_input_channel(&server).unwrap();

    let consumer = Arc::new(InputConsumer::new(client_chan));
    let con_clone = Arc::clone(&consumer);

    // Background thread answering acks
    let t_handle = thread::spawn(move || {
        let msg = con_clone.consume().expect("Consumer failed to receive sync event");
        let seq = msg.seq();
        // Simulate small processing delay
        thread::sleep(Duration::from_millis(10));
        con_clone
            .send_finished_signal(seq, true)
            .expect("Consumer failed to send ack");
    });

    let mut v_source = VirtualEventSource::new(1);
    let motion = v_source.make_touch_down(500.0, 700.0, 3000);
    let event = InputEvent::Motion(motion);

    let handled = client
        .inject_input_event(&event, INJECT_INPUT_EVENT_MODE_WAIT_FOR_FINISH)
        .expect("Sync inject failed");

    assert!(handled);
    t_handle.join().unwrap();
}

#[test]
fn test_input_service_registration() {
    use aidl_compat::stub::Binder;
    use binder_sys::{
        BinderKernelTransport, IPCThreadState, IServiceManager, MockBinderDriver,
        MockServiceManager, ProcessState, ServiceManagerClient, DUMP_FLAG_PRIORITY_DEFAULT,
        SERVICE_MANAGER_DESCRIPTOR,
    };
    use std::time::Duration;

    let mock_driver = Arc::new(MockBinderDriver::new());

    // 1. Setup ServiceManager process (Handle 0)
    let sm_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let sm_stub = MockServiceManager::new();
    let sm_cookie = 0x534D;
    sm_ps.register_service_object(sm_cookie, Binder::new(sm_stub));
    mock_driver.set_context_manager(sm_ps.pid(), 0, sm_cookie);

    let sm_ps_clone = Arc::clone(&sm_ps);
    let _sm_thread = std::thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(sm_ps_clone);
        let _ = ts.enter_looper();
    });

    std::thread::sleep(Duration::from_millis(30));

    // 2. Server Process: register "input" service
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let sm_client_server = ServiceManagerClient::with_binder(
        aidl_compat::stub::RemoteBinder::new_with_transport(
            0,
            0,
            Some(SERVICE_MANAGER_DESCRIPTOR),
            Arc::new(BinderKernelTransport::with_process(Arc::clone(&server_ps))),
        ),
    );

    let service = Arc::new(InputManagerService::new());
    let service_binder = SpIBinder::from_arc(Arc::clone(&service) as Arc<dyn IBinder>);

    sm_client_server
        .add_service("input", service_binder, false, DUMP_FLAG_PRIORITY_DEFAULT)
        .expect("Failed to register input service with ServiceManager");

    // 3. Lookup service from ServiceManager
    let looked_up = sm_client_server
        .get_service("input")
        .expect("get_service failed")
        .expect("Service not found in ServiceManager");

    assert_eq!(looked_up.handle(), Some(0));
}
