use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::IBinder;
use input_channel::{InputChannel, InputConsumer, InputMessage};
use inputflinger_rs::*;
use std::sync::Arc;
use std::thread;
use wms_rs::*;

#[test]
fn test_wms_session_and_inputflinger_event_dispatch_e2e() {
    // 1. Initialize Window Manager and Input Manager Services
    let wms = Arc::new(WindowManagerService::new());
    let input_service = Arc::new(InputManagerService::new());

    let wms_binder = SpIBinder::from_arc(Arc::clone(&wms) as Arc<dyn IBinder>);
    let input_binder = SpIBinder::from_arc(Arc::clone(&input_service) as Arc<dyn IBinder>);

    let wms_client = WindowManagerProxy::new(wms_binder);
    let input_client = InputManagerProxy::new(input_binder);

    // 2. Client application opens WMS session
    let session_binder = wms_client
        .open_session(None)
        .expect("Client failed to open WMS session");
    let session = WindowSessionProxy::new(session_binder);

    // 3. Client adds Window to display, receiving its allocated InputChannel
    let mut attrs = LayoutParams::default();
    attrs.title = "com.test.game/MainActivity".to_string();
    let mut insets = InsetsState::default();
    let mut client_input_channel = InputChannel::default();

    let add_result = session
        .add_to_display(
            None,
            &attrs,
            0,
            0,
            &mut insets,
            &mut client_input_channel,
        )
        .expect("Add window to display failed");
    assert_eq!(add_result, 0);

    // 4. Server side: WMS registers server-side input channel with InputFlinger
    let server_channel = wms
        .get_session(1)
        .and_then(|sess| sess.get_server_input_channel(1))
        .expect("Server input channel not found");

    input_client
        .register_input_channel(&server_channel)
        .expect("Failed to register server input channel with InputManager");

    // 5. Client creates InputConsumer from client_input_channel
    let consumer = Arc::new(InputConsumer::new(client_input_channel));
    let con_clone = Arc::clone(&consumer);

    // Background thread consuming events and sending acks
    let consumer_thread = thread::spawn(move || {
        // Expect touch down
        let msg = con_clone.consume().expect("Consumer failed to receive motion event");
        if let InputMessage::Motion(m) = msg {
            assert_eq!(m.action, MOTION_ACTION_DOWN);
            assert_eq!(m.pointer_coords[0].x, 640.0);
            assert_eq!(m.pointer_coords[0].y, 360.0);
            con_clone
                .send_finished_signal(m.seq, true)
                .expect("Failed to send ack");
        } else {
            panic!("Expected MotionEvent");
        }

        // Expect key down (ENTER)
        let key_msg = con_clone.consume().expect("Consumer failed to receive key event");
        if let InputMessage::Key(k) = key_msg {
            assert_eq!(k.key_code, KEYCODE_ENTER);
            assert_eq!(k.action, KEY_ACTION_DOWN);
            con_clone
                .send_finished_signal(k.seq, true)
                .expect("Failed to send key ack");
        } else {
            panic!("Expected KeyEvent");
        }
    });

    // 6. Inject Touch Down event into InputManager (Sync wait mode)
    let mut v_source = VirtualEventSource::new(1);
    let touch_event = InputEvent::Motion(v_source.make_touch_down(640.0, 360.0, 1000));

    let touch_handled = input_client
        .inject_input_event(&touch_event, INJECT_INPUT_EVENT_MODE_WAIT_FOR_FINISH)
        .expect("Touch event injection failed");
    assert!(touch_handled);

    // 7. Inject Key Down (ENTER) event into InputManager (Sync wait mode)
    let key_event = InputEvent::Key(v_source.make_key_event(KEYCODE_ENTER, KEY_ACTION_DOWN, 2000));
    let key_handled = input_client
        .inject_input_event(&key_event, INJECT_INPUT_EVENT_MODE_WAIT_FOR_FINISH)
        .expect("Key event injection failed");
    assert!(key_handled);

    consumer_thread.join().unwrap();

    // 8. Client relayouts window to allocate SurfaceControl
    let mut surface_control = SurfaceControl::default();
    let relayout_res = session
        .relayout(None, &attrs, 1280, 720, 0, 0, &mut surface_control)
        .expect("Relayout failed");
    assert_ne!(relayout_res, 0);
    assert_eq!(surface_control.name, "com.test.game/MainActivity");
    assert_eq!(surface_control.width, 1280);
    assert_eq!(surface_control.height, 720);

    // 9. Client finishes drawing with SurfaceControlTransaction
    let mut tx = SurfaceControlTransaction::new(surface_control.layer_id);
    tx.set_position(0.0, 0.0)
        .set_size(1280, 720)
        .set_alpha(1.0)
        .set_z_order(1);

    session
        .finish_drawing(None, Some(&tx))
        .expect("Finish drawing failed");

    // 10. Client removes window
    session.remove(None).expect("Remove window failed");
}
