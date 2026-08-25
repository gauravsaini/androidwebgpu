//! Zero-Copy Shared Memory Frame Buffer Pool for Virtual Camera Stream Pipeline.

use camera_hal_virtual::PixelFormat;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Handle to an allocated frame buffer in the shared memory pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedBufferHandle {
    pub buffer_id: u64,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub stride: u32,
    pub data: Vec<u8>,
}

impl SharedBufferHandle {
    /// Calculate required byte capacity for given resolution and pixel format.
    pub fn calculate_byte_size(width: u32, height: u32, format: PixelFormat) -> usize {
        match format {
            PixelFormat::Rgba8888 | PixelFormat::Rgbx8888 => (width * height * 4) as usize,
            PixelFormat::Rgb888 => (width * height * 3) as usize,
            PixelFormat::Rgb565 | PixelFormat::Raw16 => (width * height * 2) as usize,
            PixelFormat::Yuv420888 | PixelFormat::YV12 => (width * height * 3 / 2) as usize,
            PixelFormat::Blob => (width * height) as usize,
        }
    }

    /// Allocate a new initialized shared buffer.
    pub fn new(buffer_id: u64, width: u32, height: u32, format: PixelFormat) -> Self {
        let size = Self::calculate_byte_size(width, height, format);
        Self {
            buffer_id,
            width,
            height,
            format,
            stride: width,
            data: vec![0u8; size],
        }
    }
}

/// Statistics tracking buffer allocations and lifecycle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BufferPoolStats {
    pub total_allocated: u64,
    pub in_flight: u64,
    pub pooled_available: usize,
    pub peak_in_flight: u64,
}

/// Pre-allocated buffer pool eliminating runtime heap thrashing for 30/60fps video capture.
pub struct CameraBufferPool {
    available_buffers: Mutex<VecDeque<SharedBufferHandle>>,
    buffer_id_seq: AtomicU64,
    max_pool_capacity: usize,
    in_flight_count: AtomicU64,
    peak_in_flight: AtomicU64,
}

impl CameraBufferPool {
    pub const DEFAULT_POOL_CAPACITY: usize = 8;

    pub fn new(capacity: usize) -> Self {
        Self {
            available_buffers: Mutex::new(VecDeque::with_capacity(capacity)),
            buffer_id_seq: AtomicU64::new(1),
            max_pool_capacity: capacity,
            in_flight_count: AtomicU64::new(0),
            peak_in_flight: AtomicU64::new(0),
        }
    }

    /// Acquire a buffer for writing frame data. Reuses pooled memory or allocates if empty.
    pub fn acquire(&self, width: u32, height: u32, format: PixelFormat) -> SharedBufferHandle {
        let mut guard = self.available_buffers.lock().unwrap();
        let expected_size = SharedBufferHandle::calculate_byte_size(width, height, format);

        let mut handle = if let Some(mut recycled) = guard.pop_front() {
            if recycled.data.len() == expected_size
                && recycled.width == width
                && recycled.height == height
                && recycled.format == format
            {
                recycled.buffer_id = self.buffer_id_seq.fetch_add(1, Ordering::Relaxed);
                recycled
            } else {
                let id = self.buffer_id_seq.fetch_add(1, Ordering::Relaxed);
                SharedBufferHandle::new(id, width, height, format)
            }
        } else {
            let id = self.buffer_id_seq.fetch_add(1, Ordering::Relaxed);
            SharedBufferHandle::new(id, width, height, format)
        };

        handle.stride = width;
        let in_flight = self.in_flight_count.fetch_add(1, Ordering::Relaxed) + 1;
        self.peak_in_flight.fetch_max(in_flight, Ordering::Relaxed);

        handle
    }

    /// Return a released buffer back to the pool.
    pub fn release(&self, mut handle: SharedBufferHandle) {
        let mut guard = self.available_buffers.lock().unwrap();
        self.in_flight_count.fetch_sub(1, Ordering::Relaxed);

        if guard.len() < self.max_pool_capacity {
            // Keep buffer allocation
            guard.push_back(handle);
        } else {
            // Drop beyond capacity
            handle.data.clear();
        }
    }

    /// Get current buffer pool statistics.
    pub fn get_stats(&self) -> BufferPoolStats {
        let guard = self.available_buffers.lock().unwrap();
        BufferPoolStats {
            total_allocated: self.buffer_id_seq.load(Ordering::Relaxed) - 1,
            in_flight: self.in_flight_count.load(Ordering::Relaxed),
            pooled_available: guard.len(),
            peak_in_flight: self.peak_in_flight.load(Ordering::Relaxed),
        }
    }
}
