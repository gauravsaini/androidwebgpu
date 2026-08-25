//! # input_channel
//!
//! Android 13 `InputChannel` Unix socketpair transport, binary `InputMessage` wire format codec,
//! `InputPublisher`, and `InputConsumer` for AndroidWebGPU.

pub mod channel;
pub mod consumer;
pub mod error;
pub mod message;
pub mod publisher;

pub use channel::InputChannel;
pub use consumer::InputConsumer;
pub use error::{InputChannelError, Result};
pub use message::{
    FinishedData, InputMessage, KeyEventData, MotionEventData, PointerCoords, PointerProperties,
    INPUT_MESSAGE_HEADER_SIZE, INPUT_MESSAGE_TYPE_CAPTURE, INPUT_MESSAGE_TYPE_DRAG,
    INPUT_MESSAGE_TYPE_FINISHED, INPUT_MESSAGE_TYPE_FOCUS, INPUT_MESSAGE_TYPE_KEY,
    INPUT_MESSAGE_TYPE_MOTION, INPUT_MESSAGE_WIRE_SIZE, MAX_POINTERS,
};
pub use publisher::InputPublisher;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_end_to_end_publisher_consumer_handshake() {
        let (server_chan, client_chan) = InputChannel::open_input_channel_pair("e2e_test").unwrap();
        let publisher = InputPublisher::new(server_chan);
        let consumer = InputConsumer::new(client_chan);

        // 1. Publish Key Event
        let seq = publisher
            .publish_key_event(
                1,          // device_id
                0x00000101, // SOURCE_KEYBOARD
                0,          // display_id
                0,          // ACTION_DOWN
                0,          // flags
                29,         // KEYCODE_A
                30,         // scan_code
                0,          // meta_state
                0,          // repeat_count
                1000,       // down_time
                1000,       // event_time
            )
            .expect("Failed to publish key event");

        assert_eq!(seq, 1);

        // 2. Consume Key Event
        let msg = consumer.consume().expect("Failed to consume message");
        if let InputMessage::Key(k) = msg {
            assert_eq!(k.seq, 1);
            assert_eq!(k.key_code, 29);
            assert_eq!(k.action, 0);
            consumer
                .send_finished_signal(k.seq, true)
                .expect("Failed to send finished signal");
        } else {
            panic!("Expected Key event");
        }

        // 3. Publisher receives finished ack
        let (ack_seq, handled) = publisher
            .receive_finished_signal()
            .expect("Failed to receive ack");
        assert_eq!(ack_seq, 1);
        assert!(handled);

        // 4. Publish Motion Event
        let props = [PointerProperties::new(0, 1)];
        let coords = [PointerCoords::new(540.0, 960.0, 1.0, 1.0)];
        let m_seq = publisher
            .publish_motion_event(
                2,          // device_id
                0x00001002, // SOURCE_TOUCHSCREEN
                0,          // display_id
                0,          // ACTION_DOWN
                0,          // action_button
                0,          // flags
                0,          // edge_flags
                0,          // meta_state
                0,          // button_state
                0,          // classification
                1.0,        // x_precision
                1.0,        // y_precision
                0.0,        // x_offset
                0.0,        // y_offset
                2000,       // down_time
                2000,       // event_time
                1,          // pointer_count
                &props,
                &coords,
            )
            .expect("Failed to publish motion event");

        assert_eq!(m_seq, 2);

        // 5. Consume Motion Event
        let m_msg = consumer.consume().expect("Failed to consume motion message");
        if let InputMessage::Motion(m) = m_msg {
            assert_eq!(m.seq, 2);
            assert_eq!(m.pointer_count, 1);
            assert_eq!(m.pointer_coords[0].x, 540.0);
            assert_eq!(m.pointer_coords[0].y, 960.0);
            consumer
                .send_finished_signal(m.seq, true)
                .expect("Failed to send finished signal");
        } else {
            panic!("Expected Motion event");
        }

        // 6. Publisher receives motion ack
        let (m_ack_seq, m_handled) = publisher
            .receive_finished_signal()
            .expect("Failed to receive motion ack");
        assert_eq!(m_ack_seq, 2);
        assert!(m_handled);
    }
}
