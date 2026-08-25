//! Android `InputConsumer` receiving input events and replying with finished acks.

use crate::channel::InputChannel;
use crate::error::Result;
use crate::message::{FinishedData, InputMessage};
use std::sync::Arc;

/// InputConsumer reads key/motion input events from an `InputChannel` and transmits finished signals.
pub struct InputConsumer {
    channel: Arc<InputChannel>,
}

impl InputConsumer {
    /// Create a new `InputConsumer` wrapping an `InputChannel`.
    pub fn new(channel: InputChannel) -> Self {
        Self {
            channel: Arc::new(channel),
        }
    }

    /// Create a new `InputConsumer` from an `Arc<InputChannel>`.
    pub fn from_arc(channel: Arc<InputChannel>) -> Self {
        Self { channel }
    }

    /// Access reference to underlying input channel.
    pub fn channel(&self) -> &Arc<InputChannel> {
        &self.channel
    }

    /// Consume the next available input message (blocking).
    pub fn consume(&self) -> Result<InputMessage> {
        self.channel.receive_message()
    }

    /// Try to consume an input message without blocking.
    pub fn try_consume(&self) -> Result<Option<InputMessage>> {
        self.channel.try_receive_message()
    }

    /// Send a finished execution signal acknowledging an event sequence number.
    pub fn send_finished_signal(&self, seq: u32, handled: bool) -> Result<()> {
        let fin = FinishedData::new(seq, handled);
        self.channel.send_message(&InputMessage::Finished(fin))
    }
}
