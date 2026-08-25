//! Low-latency Input Event Dispatcher routing events to active Window InputChannels.

use crate::error::{InputFlingerError, InputFlingerResult};
use crate::types::*;
use input_channel::{InputChannel, InputPublisher, KeyEventData, MotionEventData};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// InputDispatcher maintains window publishers and directs incoming touch and key events.
pub struct InputDispatcher {
    publishers: Arc<RwLock<HashMap<String, Arc<InputPublisher>>>>,
    focused_window: Arc<RwLock<Option<String>>>,
}

impl Default for InputDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl InputDispatcher {
    /// Create a new InputDispatcher instance.
    pub fn new() -> Self {
        Self {
            publishers: Arc::new(RwLock::new(HashMap::new())),
            focused_window: Arc::new(RwLock::new(None)),
        }
    }

    /// Register a window input channel and establish an `InputPublisher`.
    pub fn register_window_channel(
        &self,
        name: &str,
        channel: Arc<InputChannel>,
    ) -> Arc<InputPublisher> {
        let publisher = Arc::new(InputPublisher::from_arc(channel));
        let mut map = self.publishers.write().unwrap();
        map.insert(name.to_string(), Arc::clone(&publisher));

        let mut focused = self.focused_window.write().unwrap();
        if focused.is_none() {
            *focused = Some(name.to_string());
        }

        publisher
    }

    /// Unregister a window input channel by name.
    pub fn unregister_window_channel(&self, name: &str) {
        let mut map = self.publishers.write().unwrap();
        map.remove(name);

        let mut focused = self.focused_window.write().unwrap();
        if let Some(ref current) = *focused {
            if current == name {
                *focused = map.keys().next().cloned();
            }
        }
    }

    /// Set focused window target for incoming input events.
    pub fn set_focused_window(&self, name: &str) {
        let mut focused = self.focused_window.write().unwrap();
        *focused = Some(name.to_string());
    }

    /// Retrieve the currently focused `InputPublisher`.
    pub fn get_focused_publisher(&self) -> Option<Arc<InputPublisher>> {
        let focused = self.focused_window.read().unwrap();
        let map = self.publishers.read().unwrap();

        if let Some(ref name) = *focused {
            map.get(name).cloned()
        } else {
            map.values().next().cloned()
        }
    }

    /// Dispatch a key event to the focused window.
    pub fn dispatch_key_event(&self, key_event: &KeyEventData) -> InputFlingerResult<u32> {
        let publisher = self
            .get_focused_publisher()
            .ok_or(InputFlingerError::NoFocusedWindow)?;

        publisher
            .publish_key(key_event.clone())
            .map_err(|e| InputFlingerError::Channel(e.to_string()))
    }

    /// Dispatch a motion event to the focused window.
    pub fn dispatch_motion_event(&self, motion_event: &MotionEventData) -> InputFlingerResult<u32> {
        let publisher = self
            .get_focused_publisher()
            .ok_or(InputFlingerError::NoFocusedWindow)?;

        publisher
            .publish_motion(motion_event.clone())
            .map_err(|e| InputFlingerError::Channel(e.to_string()))
    }

    /// Dispatch any `InputEvent` to the focused window.
    pub fn dispatch_event(&self, event: &InputEvent) -> InputFlingerResult<u32> {
        match event {
            InputEvent::Key(k) => self.dispatch_key_event(k),
            InputEvent::Motion(m) => self.dispatch_motion_event(m),
        }
    }

    /// Dispatch an `InputEvent` and await the finished acknowledgement from the consumer.
    pub fn dispatch_and_wait_for_ack(
        &self,
        event: &InputEvent,
        timeout_ms: u64,
    ) -> InputFlingerResult<bool> {
        let publisher = self
            .get_focused_publisher()
            .ok_or(InputFlingerError::NoFocusedWindow)?;

        let seq = match event {
            InputEvent::Key(k) => publisher
                .publish_key(k.clone())
                .map_err(|e| InputFlingerError::Channel(e.to_string()))?,
            InputEvent::Motion(m) => publisher
                .publish_motion(m.clone())
                .map_err(|e| InputFlingerError::Channel(e.to_string()))?,
        };

        let start = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);

        while start.elapsed() < timeout {
            if let Ok(Some((ack_seq, handled))) = publisher.try_receive_finished_signal() {
                if ack_seq == seq {
                    return Ok(handled);
                }
            }
            std::thread::yield_now();
        }

        Err(InputFlingerError::Timeout)
    }
}
