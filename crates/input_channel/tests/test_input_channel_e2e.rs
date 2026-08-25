//! Comprehensive integration tests for Android 13 InputChannel and InputMessage wire codec.

use input_channel::{
    InputChannel, InputConsumer, InputMessage, InputPublisher, PointerCoords, PointerProperties,
    MAX_POINTERS,
};
use std::sync::Arc;
use std::thread;

#[test]
fn test_multithreaded_high_throughput_stream() {
    let (server_chan, client_chan) =
        InputChannel::open_input_channel_pair("throughput_test").unwrap();
    let publisher = Arc::new(InputPublisher::new(server_chan));
    let consumer = Arc::new(InputConsumer::new(client_chan));

    let pub_clone = Arc::clone(&publisher);
    let con_clone = Arc::clone(&consumer);

    let event_count = 500;

    // Consumer thread: reads events and replies with acks
    let consumer_handle = thread::spawn(move || {
        for _ in 0..event_count {
            let msg = con_clone.consume().expect("Failed to consume event");
            let seq = msg.seq();
            con_clone
                .send_finished_signal(seq, true)
                .expect("Failed to send finished ack");
        }
    });

    // Publisher thread: sends events and awaits acks
    let publisher_handle = thread::spawn(move || {
        for i in 0..event_count {
            let props = [PointerProperties::new(0, 1)];
            let coords = [PointerCoords::new(i as f32, (i * 2) as f32, 1.0, 1.0)];

            let seq = pub_clone
                .publish_motion_event(
                    1,
                    0x00001002, // SOURCE_TOUCHSCREEN
                    0,
                    2, // ACTION_MOVE
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    1.0,
                    1.0,
                    0.0,
                    0.0,
                    1000 + i as i64,
                    1000 + i as i64,
                    1,
                    &props,
                    &coords,
                )
                .expect("Failed to publish motion event");

            let (ack_seq, handled) = pub_clone
                .receive_finished_signal()
                .expect("Failed to receive ack");
            assert_eq!(ack_seq, seq);
            assert!(handled);
        }
    });

    consumer_handle.join().unwrap();
    publisher_handle.join().unwrap();
}

#[test]
fn test_all_16_multitouch_pointers_precision() {
    let (server_chan, client_chan) =
        InputChannel::open_input_channel_pair("multitouch_16").unwrap();
    let publisher = InputPublisher::new(server_chan);
    let consumer = InputConsumer::new(client_chan);

    let mut props = [PointerProperties::default(); MAX_POINTERS];
    let mut coords = [PointerCoords::default(); MAX_POINTERS];

    for i in 0..MAX_POINTERS {
        props[i] = PointerProperties::new(i as i32, 1);
        coords[i] = PointerCoords {
            x: (i * 50) as f32,
            y: (i * 100) as f32,
            pressure: 0.1 * (i as f32 + 1.0),
            size: 1.5,
            touch_major: 10.0,
            touch_minor: 5.0,
            tool_major: 12.0,
            tool_minor: 6.0,
            orientation: 0.5,
            distance: 2.0,
            is_resampled: i % 2 == 0,
        };
    }

    let seq = publisher
        .publish_motion_event(
            1,
            0x00001002,
            0,
            2, // ACTION_MOVE
            0,
            0,
            0,
            0,
            0,
            0,
            1.0,
            1.0,
            0.0,
            0.0,
            5000,
            5000,
            MAX_POINTERS as u32,
            &props,
            &coords,
        )
        .unwrap();

    let msg = consumer.consume().unwrap();
    if let InputMessage::Motion(m) = msg {
        assert_eq!(m.seq, seq);
        assert_eq!(m.pointer_count, 16);
        for i in 0..MAX_POINTERS {
            assert_eq!(m.pointer_properties[i].id, i as i32);
            assert_eq!(m.pointer_coords[i].x, (i * 50) as f32);
            assert_eq!(m.pointer_coords[i].y, (i * 100) as f32);
            assert!((m.pointer_coords[i].pressure - (0.1 * (i as f32 + 1.0))).abs() < 1e-4);
            assert_eq!(m.pointer_coords[i].is_resampled, i % 2 == 0);
        }
    } else {
        panic!("Expected Motion message");
    }
}

#[test]
fn test_corrupted_wire_bytes_rejection() {
    let truncated_bytes = [1u8, 0, 0, 0]; // Only 4 bytes (needs at least 8)
    assert!(InputMessage::decode(&truncated_bytes).is_err());

    let invalid_type_bytes = [99u8, 0, 0, 0, 1, 0, 0, 0]; // Unknown message type 99
    assert!(InputMessage::decode(&invalid_type_bytes).is_err());
}
