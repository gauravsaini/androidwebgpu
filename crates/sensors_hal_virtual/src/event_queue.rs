//! Fast In-Memory Ring Buffer Event Queue and WakeLock Queue for Sensors HAL.

use crate::error::SensorsHalError;
use crate::types::Event;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::RwLock;

pub const DEFAULT_EVENT_QUEUE_CAPACITY: usize = 2048;

/// Fast thread-safe FIFO EventQueue for Sensors HAL event dispatching.
pub struct EventQueue {
    buffer: RwLock<VecDeque<Event>>,
    capacity: usize,
    dropped_events: AtomicU32,
    total_written: AtomicU32,
    total_read: AtomicU32,
}

impl EventQueue {
    /// Create new event queue with default capacity.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_EVENT_QUEUE_CAPACITY)
    }

    /// Create new event queue with custom capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: RwLock::new(VecDeque::with_capacity(capacity)),
            capacity,
            dropped_events: AtomicU32::new(0),
            total_written: AtomicU32::new(0),
            total_read: AtomicU32::new(0),
        }
    }

    /// Push single sensor event into the queue.
    pub fn push(&self, event: Event) -> Result<(), SensorsHalError> {
        let mut buf = self.buffer.write().unwrap();
        if buf.len() >= self.capacity {
            // Drop oldest event when full to maintain real-time freshness
            buf.pop_front();
            self.dropped_events.fetch_add(1, Ordering::Relaxed);
        }
        buf.push_back(event);
        self.total_written.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Push multiple sensor events into the queue.
    pub fn push_batch(&self, events: &[Event]) -> Result<usize, SensorsHalError> {
        let mut buf = self.buffer.write().unwrap();
        let mut pushed = 0;
        for ev in events {
            if buf.len() >= self.capacity {
                buf.pop_front();
                self.dropped_events.fetch_add(1, Ordering::Relaxed);
            }
            buf.push_back(ev.clone());
            pushed += 1;
        }
        self.total_written.fetch_add(pushed as u32, Ordering::Relaxed);
        Ok(pushed)
    }

    /// Pop a single event from the queue.
    pub fn pop(&self) -> Option<Event> {
        let mut buf = self.buffer.write().unwrap();
        let event = buf.pop_front();
        if event.is_some() {
            self.total_read.fetch_add(1, Ordering::Relaxed);
        }
        event
    }

    /// Drain up to `max_count` events from the queue into a vector.
    pub fn drain(&self, max_count: usize) -> Vec<Event> {
        let mut buf = self.buffer.write().unwrap();
        let count = buf.len().min(max_count);
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            if let Some(ev) = buf.pop_front() {
                result.push(ev);
            }
        }
        self.total_read.fetch_add(result.len() as u32, Ordering::Relaxed);
        result
    }

    /// Clear all events in the queue.
    pub fn clear(&self) {
        let mut buf = self.buffer.write().unwrap();
        buf.clear();
    }

    /// Current number of events in the queue.
    pub fn len(&self) -> usize {
        self.buffer.read().unwrap().len()
    }

    /// Check if queue is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.read().unwrap().is_empty()
    }

    /// Number of dropped events due to buffer overflow.
    pub fn dropped_events(&self) -> u32 {
        self.dropped_events.load(Ordering::Relaxed)
    }

    /// Total events written to the queue over its lifetime.
    pub fn total_written(&self) -> u32 {
        self.total_written.load(Ordering::Relaxed)
    }

    /// Total events read from the queue over its lifetime.
    pub fn total_read(&self) -> u32 {
        self.total_read.load(Ordering::Relaxed)
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// WakeLock Queue for tracking wakelock tokens and acknowledgments.
pub struct WakeLockQueue {
    tokens: RwLock<VecDeque<u32>>,
}

impl WakeLockQueue {
    pub fn new() -> Self {
        Self {
            tokens: RwLock::new(VecDeque::new()),
        }
    }

    pub fn push_token(&self, token: u32) {
        self.tokens.write().unwrap().push_back(token);
    }

    pub fn pop_token(&self) -> Option<u32> {
        self.tokens.write().unwrap().pop_front()
    }

    pub fn len(&self) -> usize {
        self.tokens.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.read().unwrap().is_empty()
    }
}

impl Default for WakeLockQueue {
    fn default() -> Self {
        Self::new()
    }
}
