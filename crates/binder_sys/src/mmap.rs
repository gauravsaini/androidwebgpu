//! Memory mapping management for `/dev/binder` shared buffers.
//!
//! Manages memory mapped regions (typically 128KB - 1MB) for kernel-to-userspace transaction buffers
//! and tracks buffer lifecycles to emit `BC_FREE_BUFFER`.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Default Binder mmap size in AOSP: 1MB minus two 4KB guard pages = 1040384 bytes.
pub const BINDER_DEFAULT_MMAP_SIZE: usize = (1024 * 1024) - (2 * 4096);

/// Minimum allowable mmap size: 128 KB.
pub const BINDER_MIN_MMAP_SIZE: usize = 128 * 1024;

/// Maximum allowable mmap size: 1 MB.
pub const BINDER_MAX_MMAP_SIZE: usize = 1024 * 1024;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MmapError {
    #[error("Out of memory in Binder mmap region")]
    OutOfMemory,
    #[error("Pointer 0x{0:x} is out of bounds for mmap region [0x{1:x}..0x{2:x}]")]
    OutOfBounds(u64, u64, u64),
    #[error("Invalid buffer pointer 0x{0:x} (not active or double-free)")]
    InvalidBuffer(u64),
    #[error("System mmap allocation failed: {0}")]
    SysAllocFailed(String),
}

/// Internal allocator block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AllocBlock {
    offset: usize,
    size: usize,
}

struct AllocatorState {
    active_allocations: HashSet<u64>,
    free_blocks: Vec<AllocBlock>,
}

/// Thread-safe shared memory mapping region for Binder IPC.
pub struct BinderMmapRegion {
    base_ptr: u64,
    size: usize,
    #[allow(dead_code)]
    buffer_memory: Option<Vec<u8>>,
    state: Mutex<AllocatorState>,
}

// Send and Sync are safe because access to memory is synchronized and base_ptr is valid for lifetime
unsafe impl Send for BinderMmapRegion {}
unsafe impl Sync for BinderMmapRegion {}

impl BinderMmapRegion {
    /// Create an in-memory mapped region with specified capacity (clamped between min and max).
    pub fn new_simulated(size: usize) -> Arc<Self> {
        let actual_size = size.clamp(BINDER_MIN_MMAP_SIZE, BINDER_MAX_MMAP_SIZE);
        let mut buffer = vec![0u8; actual_size];
        let base_ptr = buffer.as_mut_ptr() as u64;

        Arc::new(Self {
            base_ptr,
            size: actual_size,
            buffer_memory: Some(buffer),
            state: Mutex::new(AllocatorState {
                active_allocations: HashSet::new(),
                free_blocks: vec![AllocBlock {
                    offset: 0,
                    size: actual_size,
                }],
            }),
        })
    }

    /// Create from raw pointer mapped via system `mmap` (on Linux /dev/binder).
    ///
    /// # Safety
    /// `raw_ptr` must point to valid mapped memory of at least `size` bytes.
    pub unsafe fn from_raw_mmap(raw_ptr: *mut u8, size: usize) -> Arc<Self> {
        Arc::new(Self {
            base_ptr: raw_ptr as u64,
            size,
            buffer_memory: None,
            state: Mutex::new(AllocatorState {
                active_allocations: HashSet::new(),
                free_blocks: vec![AllocBlock { offset: 0, size }],
            }),
        })
    }

    /// Return base memory address.
    pub fn base_ptr(&self) -> u64 {
        self.base_ptr
    }

    /// Return region size in bytes.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Check if pointer falls within the mapped region.
    pub fn contains_ptr(&self, ptr: u64) -> bool {
        ptr >= self.base_ptr && ptr < (self.base_ptr + self.size as u64)
    }

    /// Allocate a buffer slot inside the mapped region (used by driver to copy transaction data).
    pub fn allocate_buffer(&self, size: usize) -> Result<u64, MmapError> {
        let align = 8;
        let aligned_size = (size + (align - 1)) & !(align - 1);
        let mut state = self.state.lock().unwrap();

        for i in 0..state.free_blocks.len() {
            let block = state.free_blocks[i];
            if block.size >= aligned_size {
                let allocated_offset = block.offset;
                if block.size == aligned_size {
                    state.free_blocks.remove(i);
                } else {
                    state.free_blocks[i] = AllocBlock {
                        offset: block.offset + aligned_size,
                        size: block.size - aligned_size,
                    };
                }

                let ptr = self.base_ptr + allocated_offset as u64;
                state.active_allocations.insert(ptr);
                return Ok(ptr);
            }
        }

        Err(MmapError::OutOfMemory)
    }

    /// Mark an allocated buffer slot as freed (corresponding to `BC_FREE_BUFFER`).
    pub fn free_buffer(&self, ptr: u64, size: usize) -> Result<(), MmapError> {
        if !self.contains_ptr(ptr) {
            return Err(MmapError::OutOfBounds(
                ptr,
                self.base_ptr,
                self.base_ptr + self.size as u64,
            ));
        }

        let mut state = self.state.lock().unwrap();
        if !state.active_allocations.remove(&ptr) {
            return Err(MmapError::InvalidBuffer(ptr));
        }

        let align = 8;
        let aligned_size = (size + (align - 1)) & !(align - 1);
        let offset = (ptr - self.base_ptr) as usize;

        state.free_blocks.push(AllocBlock {
            offset,
            size: aligned_size,
        });
        // Coalesce blocks
        state.free_blocks.sort_by_key(|b| b.offset);
        let mut i = 0;
        while i + 1 < state.free_blocks.len() {
            if state.free_blocks[i].offset + state.free_blocks[i].size == state.free_blocks[i + 1].offset {
                state.free_blocks[i].size += state.free_blocks[i + 1].size;
                state.free_blocks.remove(i + 1);
            } else {
                i += 1;
            }
        }

        Ok(())
    }

    /// Read a slice of bytes from the mapped memory region safely.
    pub fn read_bytes(&self, ptr: u64, len: usize) -> Result<Vec<u8>, MmapError> {
        if len == 0 {
            return Ok(Vec::new());
        }

        let end_ptr = ptr.checked_add(len as u64).ok_or(MmapError::OutOfBounds(
            ptr,
            self.base_ptr,
            self.base_ptr + self.size as u64,
        ))?;

        if ptr < self.base_ptr || end_ptr > (self.base_ptr + self.size as u64) {
            return Err(MmapError::OutOfBounds(
                ptr,
                self.base_ptr,
                self.base_ptr + self.size as u64,
            ));
        }

        let mut data = vec![0u8; len];
        unsafe {
            let src = ptr as *const u8;
            std::ptr::copy_nonoverlapping(src, data.as_mut_ptr(), len);
        }
        Ok(data)
    }

    /// Write bytes into the mapped memory region safely.
    pub fn write_bytes(&self, ptr: u64, data: &[u8]) -> Result<(), MmapError> {
        if data.is_empty() {
            return Ok(());
        }

        let len = data.len();
        let end_ptr = ptr.checked_add(len as u64).ok_or(MmapError::OutOfBounds(
            ptr,
            self.base_ptr,
            self.base_ptr + self.size as u64,
        ))?;

        if ptr < self.base_ptr || end_ptr > (self.base_ptr + self.size as u64) {
            return Err(MmapError::OutOfBounds(
                ptr,
                self.base_ptr,
                self.base_ptr + self.size as u64,
            ));
        }

        unsafe {
            let dst = ptr as *mut u8;
            std::ptr::copy_nonoverlapping(data.as_ptr(), dst, len);
        }
        Ok(())
    }

    /// Check if pointer is currently recorded as an active allocation.
    pub fn is_active_allocation(&self, ptr: u64) -> bool {
        self.state.lock().unwrap().active_allocations.contains(&ptr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mmap_allocation_and_free() {
        let region = BinderMmapRegion::new_simulated(BINDER_DEFAULT_MMAP_SIZE);
        assert_eq!(region.size(), BINDER_DEFAULT_MMAP_SIZE);

        let ptr1 = region.allocate_buffer(128).expect("Allocation failed");
        assert!(region.contains_ptr(ptr1));
        assert!(region.is_active_allocation(ptr1));

        let test_data = b"Hello Binder Shared Memory!";
        region
            .write_bytes(ptr1, test_data)
            .expect("Write bytes failed");
        let read_back = region
            .read_bytes(ptr1, test_data.len())
            .expect("Read bytes failed");
        assert_eq!(read_back, test_data);

        let ptr2 = region.allocate_buffer(256).expect("Allocation 2 failed");
        assert_ne!(ptr1, ptr2);
        assert!(region.is_active_allocation(ptr2));

        // Free buffer 1
        region.free_buffer(ptr1, 128).expect("Free buffer 1 failed");
        assert!(!region.is_active_allocation(ptr1));

        // Double free should fail
        assert_eq!(
            region.free_buffer(ptr1, 128),
            Err(MmapError::InvalidBuffer(ptr1))
        );

        // Free buffer 2
        region.free_buffer(ptr2, 256).expect("Free buffer 2 failed");
        assert!(!region.is_active_allocation(ptr2));
    }
}
