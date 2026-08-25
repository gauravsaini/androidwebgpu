//! Android `InputPublisher` sending input events and reading execution acks.

use crate::channel::InputChannel;
use crate::error::{InputChannelError, Result};
use crate::message::{
    FinishedData, InputMessage, KeyEventData, MotionEventData, PointerCoords, PointerProperties,
    MAX_POINTERS,
};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// InputPublisher writes key and motion events into an `InputChannel` and awaits finished signals.
pub struct InputPublisher {
    channel: Arc<InputChannel>,
    seq_generator: AtomicU32,
}

impl InputPublisher {
    /// Create a new `InputPublisher` wrapping an `InputChannel`.
    pub fn new(channel: InputChannel) -> Self {
        Self {
            channel: Arc::new(channel),
            seq_generator: AtomicU32::new(1),
        }
    }

    /// Create a new `InputPublisher` from an `Arc<InputChannel>`.
    pub fn from_arc(channel: Arc<InputChannel>) -> Self {
        Self {
            channel,
            seq_generator: AtomicU32::new(1),
        }
    }

    /// Access reference to underlying input channel.
    pub fn channel(&self) -> &Arc<InputChannel> {
        &self.channel
    }

    /// Generate next sequence number.
    pub fn next_seq(&self) -> u32 {
        self.seq_generator.fetch_add(1, Ordering::SeqCst)
    }

    /// Publish a raw `KeyEventData` over the channel, returning the assigned sequence number.
    pub fn publish_key(&self, mut key_data: KeyEventData) -> Result<u32> {
        if key_data.seq == 0 {
            key_data.seq = self.next_seq();
        }
        let seq = key_data.seq;
        self.channel.send_message(&InputMessage::Key(key_data))?;
        Ok(seq)
    }

    /// Publish key event with individual fields.
    #[allow(clippy::too_many_arguments)]
    pub fn publish_key_event(
        &self,
        device_id: i32,
        source: u32,
        display_id: i32,
        action: i32,
        flags: i32,
        key_code: i32,
        scan_code: i32,
        meta_state: i32,
        repeat_count: i32,
        down_time: i64,
        event_time: i64,
    ) -> Result<u32> {
        let seq = self.next_seq();
        let key_data = KeyEventData {
            seq,
            event_time,
            device_id,
            source,
            display_id,
            action,
            flags,
            key_code,
            scan_code,
            meta_state,
            repeat_count,
            down_time,
        };
        self.channel.send_message(&InputMessage::Key(key_data))?;
        Ok(seq)
    }

    /// Publish a raw `MotionEventData` over the channel, returning the assigned sequence number.
    pub fn publish_motion(&self, mut motion_data: MotionEventData) -> Result<u32> {
        if motion_data.seq == 0 {
            motion_data.seq = self.next_seq();
        }
        let seq = motion_data.seq;
        self.channel.send_message(&InputMessage::Motion(motion_data))?;
        Ok(seq)
    }

    /// Publish motion event with individual pointer arrays.
    #[allow(clippy::too_many_arguments)]
    pub fn publish_motion_event(
        &self,
        device_id: i32,
        source: u32,
        display_id: i32,
        action: i32,
        action_button: i32,
        flags: i32,
        edge_flags: i32,
        meta_state: i32,
        button_state: i32,
        classification: u8,
        x_precision: f32,
        y_precision: f32,
        x_offset: f32,
        y_offset: f32,
        down_time: i64,
        event_time: i64,
        pointer_count: u32,
        pointer_properties: &[PointerProperties],
        pointer_coords: &[PointerCoords],
    ) -> Result<u32> {
        let seq = self.next_seq();
        let mut props = [PointerProperties::default(); MAX_POINTERS];
        let mut coords = [PointerCoords::default(); MAX_POINTERS];

        let count = (pointer_count as usize).min(MAX_POINTERS);
        for i in 0..count {
            if i < pointer_properties.len() {
                props[i] = pointer_properties[i];
            }
            if i < pointer_coords.len() {
                coords[i] = pointer_coords[i];
            }
        }

        let motion_data = MotionEventData {
            seq,
            event_time,
            device_id,
            source,
            display_id,
            action,
            action_button,
            flags,
            edge_flags,
            meta_state,
            button_state,
            classification,
            x_precision,
            y_precision,
            x_offset,
            y_offset,
            down_time,
            pointer_count,
            pointer_properties: props,
            pointer_coords: coords,
        };

        self.channel.send_message(&InputMessage::Motion(motion_data))?;
        Ok(seq)
    }

    /// Await and read finished signal (returns (seq, handled)).
    pub fn receive_finished_signal(&self) -> Result<(u32, bool)> {
        let msg = self.channel.receive_message()?;
        match msg {
            InputMessage::Finished(FinishedData { seq, handled, .. }) => Ok((seq, handled)),
            other => Err(InputChannelError::UnknownMessageType(other.message_type())),
        }
    }

    /// Non-blocking check for finished signal.
    pub fn try_receive_finished_signal(&self) -> Result<Option<(u32, bool)>> {
        if let Some(msg) = self.channel.try_receive_message()? {
            match msg {
                InputMessage::Finished(FinishedData { seq, handled, .. }) => Ok(Some((seq, handled))),
                other => Err(InputChannelError::UnknownMessageType(other.message_type())),
            }
        } else {
            Ok(None)
        }
    }
}
