//! High-performance Thread-Safe PCM Audio Ring Buffer.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

pub const DEFAULT_RING_BUFFER_CAPACITY_BYTES: usize = 192000; // 1 second of 48kHz 16-bit stereo PCM

#[derive(Debug, Default, Clone, Copy)]
pub struct AudioBufferStats {
    pub total_written_bytes: u64,
    pub total_read_bytes: u64,
    pub overflow_drops: u64,
    pub underflow_silence_bytes: u64,
    pub peak_amplitude: i16,
}

pub struct AudioRingBuffer {
    buffer: RwLock<VecDeque<u8>>,
    capacity_bytes: usize,
    total_written_bytes: AtomicU64,
    total_read_bytes: AtomicU64,
    overflow_drops: AtomicU64,
    underflow_silence_bytes: AtomicU64,
    peak_amplitude: AtomicU64, // encoded as positive i16
}

impl AudioRingBuffer {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_RING_BUFFER_CAPACITY_BYTES)
    }

    pub fn with_capacity(capacity_bytes: usize) -> Self {
        Self {
            buffer: RwLock::new(VecDeque::with_capacity(capacity_bytes)),
            capacity_bytes,
            total_written_bytes: AtomicU64::new(0),
            total_read_bytes: AtomicU64::new(0),
            overflow_drops: AtomicU64::new(0),
            underflow_silence_bytes: AtomicU64::new(0),
            peak_amplitude: AtomicU64::new(0),
        }
    }

    /// Write PCM bytes into the ring buffer. If buffer exceeds capacity, drops oldest bytes.
    pub fn write(&self, data: &[u8]) -> usize {
        if self.capacity_bytes == 0 {
            self.total_written_bytes
                .fetch_add(data.len() as u64, Ordering::Relaxed);
            self.overflow_drops
                .fetch_add(data.len() as u64, Ordering::Relaxed);
            return data.len();
        }

        let mut buf = self.buffer.write().unwrap();
        let mut dropped = 0;

        for &byte in data {
            if buf.len() >= self.capacity_bytes {
                buf.pop_front();
                dropped += 1;
            }
            buf.push_back(byte);
        }

        self.total_written_bytes
            .fetch_add(data.len() as u64, Ordering::Relaxed);
        if dropped > 0 {
            self.overflow_drops
                .fetch_add(dropped as u64, Ordering::Relaxed);
        }

        // Track peak amplitude for 16-bit PCM
        if data.len() >= 2 {
            let mut peak = 0i16;
            for chunk in data.chunks_exact(2) {
                let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                let abs_val = if sample == i16::MIN {
                    i16::MAX
                } else {
                    sample.abs()
                };
                if abs_val > peak {
                    peak = abs_val;
                }
            }
            let current_peak = self.peak_amplitude.load(Ordering::Relaxed) as i16;
            if peak > current_peak {
                self.peak_amplitude.store(peak as u64, Ordering::Relaxed);
            }
        }

        data.len()
    }

    /// Read PCM bytes from the ring buffer. Fills missing bytes with silence (0) on underflow.
    pub fn read(&self, out: &mut [u8]) -> usize {
        let mut buf = self.buffer.write().unwrap();
        let available = buf.len();
        let to_read = available.min(out.len());

        for slot in out[..to_read].iter_mut() {
            *slot = buf.pop_front().unwrap_or(0);
        }

        if to_read < out.len() {
            let silence = out.len() - to_read;
            for slot in out[to_read..].iter_mut() {
                *slot = 0;
            }
            self.underflow_silence_bytes
                .fetch_add(silence as u64, Ordering::Relaxed);
        }

        self.total_read_bytes
            .fetch_add(to_read as u64, Ordering::Relaxed);
        to_read
    }

    /// Read statistics.
    pub fn stats(&self) -> AudioBufferStats {
        AudioBufferStats {
            total_written_bytes: self.total_written_bytes.load(Ordering::Relaxed),
            total_read_bytes: self.total_read_bytes.load(Ordering::Relaxed),
            overflow_drops: self.overflow_drops.load(Ordering::Relaxed),
            underflow_silence_bytes: self.underflow_silence_bytes.load(Ordering::Relaxed),
            peak_amplitude: self.peak_amplitude.load(Ordering::Relaxed) as i16,
        }
    }

    /// Clear all audio in buffer.
    pub fn clear(&self) {
        let mut buf = self.buffer.write().unwrap();
        buf.clear();
    }

    /// Available bytes in buffer.
    pub fn len(&self) -> usize {
        self.buffer.read().unwrap().len()
    }

    /// Check if buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.read().unwrap().is_empty()
    }
}

impl Default for AudioRingBuffer {
    fn default() -> Self {
        Self::new()
    }
}
