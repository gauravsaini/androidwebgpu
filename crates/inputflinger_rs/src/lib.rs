//! # inputflinger_rs
//!
//! Native Android Input Subsystem Service (`android.hardware.input.IInputManager`),
//! Evdev Reader, Virtual Event Source, and Input Event Dispatcher for AndroidWebGPU.

pub mod dispatcher;
pub mod error;
pub mod evdev_reader;
pub mod input_manager;
pub mod types;

pub use dispatcher::InputDispatcher;
pub use error::{InputFlingerError, InputFlingerResult};
pub use evdev_reader::{
    EvdevDecoder, LinuxInputEvent, VirtualEventSource, ABS_MT_POSITION_X, ABS_MT_POSITION_Y,
    ABS_MT_PRESSURE, ABS_MT_SLOT, ABS_MT_TOUCH_MAJOR, ABS_MT_TRACKING_ID, ABS_PRESSURE, ABS_X,
    ABS_Y, BTN_TOUCH, EV_ABS, EV_KEY, EV_REL, EV_SYN, SYN_REPORT,
};
pub use input_manager::{
    iinput_manager_codes, register_input_service, IInputManager, InputManagerProxy,
    InputManagerService, IINPUT_MANAGER_DESCRIPTOR,
};
pub use types::{
    InputDevice, InputEvent, INJECT_INPUT_EVENT_MODE_ASYNC,
    INJECT_INPUT_EVENT_MODE_WAIT_FOR_FINISH, INJECT_INPUT_EVENT_MODE_WAIT_FOR_RESULT,
    KEYCODE_0, KEYCODE_1, KEYCODE_2, KEYCODE_3, KEYCODE_4, KEYCODE_5, KEYCODE_6, KEYCODE_7,
    KEYCODE_8, KEYCODE_9, KEYCODE_A, KEYCODE_B, KEYCODE_BACK, KEYCODE_C, KEYCODE_CALL,
    KEYCODE_CAMERA, KEYCODE_CLEAR, KEYCODE_D, KEYCODE_DPAD_CENTER, KEYCODE_DPAD_DOWN,
    KEYCODE_DPAD_LEFT, KEYCODE_DPAD_RIGHT, KEYCODE_DPAD_UP, KEYCODE_ENDCALL, KEYCODE_ENTER,
    KEYCODE_ESCAPE, KEYCODE_HOME, KEYCODE_POWER, KEYCODE_UNKNOWN, KEYCODE_VOLUME_DOWN,
    KEYCODE_VOLUME_UP, KEY_ACTION_DOWN, KEY_ACTION_MULTIPLE, KEY_ACTION_UP,
    MOTION_ACTION_CANCEL, MOTION_ACTION_DOWN, MOTION_ACTION_HOVER_MOVE, MOTION_ACTION_MOVE,
    MOTION_ACTION_OUTSIDE, MOTION_ACTION_POINTER_DOWN, MOTION_ACTION_POINTER_UP,
    MOTION_ACTION_SCROLL, MOTION_ACTION_UP, SOURCE_ANY, SOURCE_CLASS_BUTTON, SOURCE_CLASS_JOYSTICK,
    SOURCE_CLASS_MASK, SOURCE_CLASS_NONE, SOURCE_CLASS_POINTER, SOURCE_CLASS_POSITION,
    SOURCE_CLASS_TRACKBALL, SOURCE_DPAD, SOURCE_GAMEPAD, SOURCE_KEYBOARD, SOURCE_MOUSE,
    SOURCE_STYLUS, SOURCE_TOUCHPAD, SOURCE_TOUCHSCREEN, SOURCE_TRACKBALL,
};

#[cfg(test)]
mod tests {
    use super::*;
    use aidl_compat::pointer::SpIBinder;
    use input_channel::{InputChannel, InputConsumer, InputMessage};
    use std::sync::Arc;

    #[test]
    fn test_input_manager_device_enumeration() {
        let service = Arc::new(InputManagerService::new());
        let ids = service.get_input_device_ids().expect("Failed to get device ids");
        assert_eq!(ids, vec![1, 2]);

        let dev1 = service.get_input_device(1).expect("Failed to get device 1").unwrap();
        assert_eq!(dev1.name, "Virtual Touchscreen");
        assert_eq!(dev1.sources, SOURCE_TOUCHSCREEN);

        let dev2 = service.get_input_device(2).expect("Failed to get device 2").unwrap();
        assert_eq!(dev2.name, "Virtual Keyboard");
        assert_eq!(dev2.sources, SOURCE_KEYBOARD);
    }

    #[test]
    fn test_input_manager_ipc_roundtrip() {
        let service = Arc::new(InputManagerService::new());
        let binder = SpIBinder::from_arc(service.clone());
        let client = InputManagerProxy::new(binder);

        let (server_chan, client_chan) = InputChannel::open_input_channel_pair("ipc_win").unwrap();
        client
            .register_input_channel(&server_chan)
            .expect("Failed to register channel");

        let consumer = InputConsumer::new(client_chan);

        // Inject touch event
        let mut v_source = VirtualEventSource::new(1);
        let motion_data = v_source.make_touch_down(300.0, 600.0, 10000);
        let event = InputEvent::Motion(motion_data);

        // Async inject
        let injected = client
            .inject_input_event(&event, INJECT_INPUT_EVENT_MODE_ASYNC)
            .expect("Async injection failed");
        assert!(injected);

        // Consume event
        let msg = consumer.consume().expect("Failed to consume event");
        if let InputMessage::Motion(m) = msg {
            assert_eq!(m.action, MOTION_ACTION_DOWN);
            assert_eq!(m.pointer_coords[0].x, 300.0);
            assert_eq!(m.pointer_coords[0].y, 600.0);
            consumer
                .send_finished_signal(m.seq, true)
                .expect("Failed to send ack");
        } else {
            panic!("Expected Motion event");
        }
    }

    #[test]
    fn test_evdev_to_android_motion_stream() {
        let mut decoder = EvdevDecoder::new(1);

        // Simulate touch down at (400, 800)
        let events = vec![
            LinuxInputEvent::new(EV_ABS, ABS_MT_SLOT, 0),
            LinuxInputEvent::new(EV_ABS, ABS_MT_TRACKING_ID, 12),
            LinuxInputEvent::new(EV_ABS, ABS_MT_POSITION_X, 400),
            LinuxInputEvent::new(EV_ABS, ABS_MT_POSITION_Y, 800),
            LinuxInputEvent::new(EV_ABS, ABS_MT_PRESSURE, 200),
            LinuxInputEvent::new(EV_SYN, SYN_REPORT, 0),
        ];

        let android_events = decoder.process_events(&events);
        assert_eq!(android_events.len(), 1);

        if let InputEvent::Motion(ref m) = android_events[0] {
            assert_eq!(m.action, MOTION_ACTION_DOWN);
            assert_eq!(m.pointer_coords[0].x, 400.0);
            assert_eq!(m.pointer_coords[0].y, 800.0);
        } else {
            panic!("Expected Motion event");
        }

        // Simulate touch up
        let up_events = vec![
            LinuxInputEvent::new(EV_ABS, ABS_MT_SLOT, 0),
            LinuxInputEvent::new(EV_ABS, ABS_MT_TRACKING_ID, -1),
            LinuxInputEvent::new(EV_SYN, SYN_REPORT, 0),
        ];

        let android_up = decoder.process_events(&up_events);
        assert_eq!(android_up.len(), 1);

        if let InputEvent::Motion(ref m) = android_up[0] {
            assert_eq!(m.action, MOTION_ACTION_UP);
        } else {
            panic!("Expected Motion UP event");
        }
    }
}
