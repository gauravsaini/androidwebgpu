//! Virtqueue memory buffer descriptor model and packet reader/writer.
//!
//! Provides data structures and utilities for managing VirtIO ring descriptor chains,
//! packet serialization to/from descriptor buffers, and synchronized queue processing.

use crate::protocol::{
    ProtocolError, VirtioBinderEventHdr, VirtioBinderRequest, VirtioBinderResponse,
};
use bytemuck::{Pod, Zeroable};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use thiserror::Error;

// -----------------------------------------------------------------------------
// Virtqueue Descriptor Flags (OASIS VirtIO 1.2)
// -----------------------------------------------------------------------------

/// Buffer continues via the `next` field.
pub const VRING_DESC_F_NEXT: u16 = 1;
/// Buffer is write-only for the device (device writes, driver reads).
pub const VRING_DESC_F_WRITE: u16 = 2;
/// Buffer contains a list of buffer descriptors (indirect descriptor table).
pub const VRING_DESC_F_INDIRECT: u16 = 4;

// -----------------------------------------------------------------------------
// Queue Errors
// -----------------------------------------------------------------------------

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum QueueError {
    #[error("Descriptor buffer overflow: required {required} bytes, available {available} bytes")]
    BufferOverflow {
        required: usize,
        available: usize,
    },

    #[error("Malformed descriptor chain: {0}")]
    MalformedChain(String),

    #[error("Virtqueue is empty")]
    EmptyQueue,

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),
}

// -----------------------------------------------------------------------------
// Standard VirtIO 16-byte Descriptor Struct
// -----------------------------------------------------------------------------

/// Raw 16-byte C-ABI VirtIO Ring Descriptor (`vring_desc`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable, Default)]
pub struct VirtqDesc {
    /// Guest physical address or virtual buffer pointer.
    pub addr: u64,
    /// Length of the buffer in bytes.
    pub len: u32,
    /// Descriptor flags (`VRING_DESC_F_*`).
    pub flags: u16,
    /// Index of next descriptor in chain if `VRING_DESC_F_NEXT` is set.
    pub next: u16,
}

impl VirtqDesc {
    pub const fn new(addr: u64, len: u32, flags: u16, next: u16) -> Self {
        Self {
            addr,
            len,
            flags,
            next,
        }
    }

    pub const fn is_write(&self) -> bool {
        (self.flags & VRING_DESC_F_WRITE) != 0
    }

    pub const fn has_next(&self) -> bool {
        (self.flags & VRING_DESC_F_NEXT) != 0
    }
}

// -----------------------------------------------------------------------------
// VirtQueueChain: In-Memory Descriptor Chain
// -----------------------------------------------------------------------------

/// Represents a resolved VirtIO descriptor chain consisting of readable (input)
/// and writable (output) buffers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtQueueChain {
    /// Head descriptor index in the virtqueue table.
    pub head_index: u16,
    /// Readable buffers provided by guest (device input).
    pub readable: Vec<Vec<u8>>,
    /// Writable buffers provided by guest for device reply (device output).
    pub writable: Vec<Vec<u8>>,
    /// Number of bytes written into writable buffers.
    pub written_len: usize,
}

impl VirtQueueChain {
    /// Construct a new descriptor chain with given readable and writable buffers.
    pub fn new(head_index: u16, readable: Vec<Vec<u8>>, writable: Vec<Vec<u8>>) -> Self {
        Self {
            head_index,
            readable,
            writable,
            written_len: 0,
        }
    }

    /// Construct a descriptor chain from a contiguous request byte slice and a target reply capacity.
    pub fn from_request_bytes(head_index: u16, req_bytes: &[u8], reply_capacity: usize) -> Self {
        Self {
            head_index,
            readable: vec![req_bytes.to_vec()],
            writable: vec![vec![0u8; reply_capacity]],
            written_len: 0,
        }
    }

    /// Total readable bytes across all input descriptors.
    pub fn readable_len(&self) -> usize {
        self.readable.iter().map(|b| b.len()).sum()
    }

    /// Total writable capacity across all output descriptors.
    pub fn writable_capacity(&self) -> usize {
        self.writable.iter().map(|b| b.len()).sum()
    }

    /// Read all readable buffers concatenated into a single contiguous `Vec<u8>`.
    pub fn read_all(&self) -> Vec<u8> {
        let total = self.readable_len();
        let mut out = Vec::with_capacity(total);
        for buf in &self.readable {
            out.extend_from_slice(buf);
        }
        out
    }

    /// Write contiguous data across the chain's writable buffers.
    pub fn write_all(&mut self, data: &[u8]) -> Result<usize, QueueError> {
        let capacity = self.writable_capacity();
        if data.len() > capacity {
            return Err(QueueError::BufferOverflow {
                required: data.len(),
                available: capacity,
            });
        }

        let mut offset = 0;
        let mut remaining = data;

        for buf in &mut self.writable {
            if remaining.is_empty() {
                break;
            }
            let to_copy = std::cmp::min(buf.len(), remaining.len());
            buf[..to_copy].copy_from_slice(&remaining[..to_copy]);
            remaining = &remaining[to_copy..];
            offset += to_copy;
        }

        self.written_len = offset;
        Ok(offset)
    }

    /// Retrieve the exact bytes written so far into the writable buffers.
    pub fn take_written_data(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.written_len);
        let mut remaining = self.written_len;
        for buf in &self.writable {
            if remaining == 0 {
                break;
            }
            let to_take = std::cmp::min(buf.len(), remaining);
            out.extend_from_slice(&buf[..to_take]);
            remaining -= to_take;
        }
        out
    }

    /// Parse a `VirtioBinderRequest` directly from the readable descriptors.
    pub fn parse_request(&self) -> Result<VirtioBinderRequest, QueueError> {
        let raw = self.read_all();
        VirtioBinderRequest::deserialize(&raw).map_err(QueueError::Protocol)
    }

    /// Write a `VirtioBinderResponse` directly into the writable descriptors.
    pub fn write_response(&mut self, resp: &VirtioBinderResponse) -> Result<usize, QueueError> {
        let bytes = resp.serialize();
        self.write_all(&bytes)
    }
}

// -----------------------------------------------------------------------------
// VirtQueue: Synchronized Queue Implementation
// -----------------------------------------------------------------------------

/// Thread-safe Virtqueue instance managing descriptor chains or event packets.
#[derive(Debug, Default)]
pub struct VirtQueue {
    queue_id: u16,
    chains: VecDeque<VirtQueueChain>,
    events: VecDeque<VirtioBinderEventHdr>,
}

impl VirtQueue {
    /// Construct a new empty VirtQueue with queue ID.
    pub fn new(queue_id: u16) -> Self {
        Self {
            queue_id,
            chains: VecDeque::new(),
            events: VecDeque::new(),
        }
    }

    /// Return queue ID (0 for TX/RX, 1 for Event).
    pub fn queue_id(&self) -> u16 {
        self.queue_id
    }

    /// Push a descriptor chain to the queue.
    pub fn push_chain(&mut self, chain: VirtQueueChain) {
        self.chains.push_back(chain);
    }

    /// Pop the next descriptor chain from the queue.
    pub fn pop_chain(&mut self) -> Option<VirtQueueChain> {
        self.chains.pop_front()
    }

    /// Push an event header to the event queue.
    pub fn push_event(&mut self, event: VirtioBinderEventHdr) {
        self.events.push_back(event);
    }

    /// Pop the next event header from the event queue.
    pub fn pop_event(&mut self) -> Option<VirtioBinderEventHdr> {
        self.events.pop_front()
    }

    /// Drain all pending events.
    pub fn drain_events(&mut self) -> Vec<VirtioBinderEventHdr> {
        self.events.drain(..).collect()
    }

    /// Return number of pending descriptor chains.
    pub fn chain_count(&self) -> usize {
        self.chains.len()
    }

    /// Return number of pending events.
    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    /// Check if queue is empty.
    pub fn is_empty(&self) -> bool {
        self.chains.is_empty() && self.events.is_empty()
    }
}

/// Shared reference to a synchronized `VirtQueue`.
pub type SharedVirtQueue = Arc<Mutex<VirtQueue>>;

/// Helper to create a new shared `VirtQueue`.
pub fn new_shared_virtqueue(queue_id: u16) -> SharedVirtQueue {
    Arc::new(Mutex::new(VirtQueue::new(queue_id)))
}
