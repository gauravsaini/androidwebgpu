//! Adversarial mmap buffer exhaustion and fragmentation edge-case stress tests.

use binder_sys::*;
use std::sync::Arc;
use std::thread;

#[test]
fn test_mmap_exhaustion_and_exact_oom_recovery() {
    let region_size = BINDER_MIN_MMAP_SIZE; // 128 KB = 131,072 bytes
    let region = BinderMmapRegion::new_simulated(region_size);

    let block_size = 4096;
    let max_blocks = region_size / block_size;
    let mut allocated_ptrs = Vec::new();

    // Allocate until full
    for _ in 0..max_blocks {
        let ptr = region
            .allocate_buffer(block_size)
            .expect("Allocation should succeed within bounds");
        allocated_ptrs.push(ptr);
    }

    // Next allocation must strictly return OutOfMemory
    assert_eq!(
        region.allocate_buffer(block_size),
        Err(MmapError::OutOfMemory),
        "Allocator must return OutOfMemory when exhausted"
    );

    // Free all blocks
    for ptr in allocated_ptrs {
        region
            .free_buffer(ptr, block_size)
            .expect("Freeing valid allocated block must succeed");
    }

    // After full free, allocating the full region size must succeed (proving complete coalescing)
    let full_ptr = region
        .allocate_buffer(region_size)
        .expect("Full region allocation after total free must succeed");
    assert_eq!(full_ptr, region.base_ptr());
    region.free_buffer(full_ptr, region_size).unwrap();
}

#[test]
fn test_mmap_checkerboard_fragmentation_and_coalescing() {
    let region_size = BINDER_MIN_MMAP_SIZE; // 128 KB
    let region = BinderMmapRegion::new_simulated(region_size);

    let block_size = 1024;
    let count = region_size / block_size;
    let mut ptrs = Vec::new();

    for _ in 0..count {
        ptrs.push(region.allocate_buffer(block_size).unwrap());
    }

    // Free every alternating block (even indices: 0, 2, 4, ...)
    for i in (0..count).step_by(2) {
        region.free_buffer(ptrs[i], block_size).unwrap();
    }

    // Now 50% of memory is free, but each free block is only 1024 bytes.
    // An allocation of 2048 bytes MUST fail due to fragmentation.
    assert_eq!(
        region.allocate_buffer(2048),
        Err(MmapError::OutOfMemory),
        "Allocation of 2048 bytes must fail in checkerboard fragmented state"
    );

    // Free the remaining odd indices (1, 3, 5, ...)
    for i in (1..count).step_by(2) {
        region.free_buffer(ptrs[i], block_size).unwrap();
    }

    // Now all blocks are free; coalescing must restore one contiguous block of full size.
    let big_alloc = region.allocate_buffer(region_size).expect(
        "Full size allocation must succeed after complete checkerboard coalescing"
    );
    assert_eq!(big_alloc, region.base_ptr());
    region.free_buffer(big_alloc, region_size).unwrap();
}

#[test]
fn test_mmap_concurrent_multithreaded_alloc_free_churn() {
    let region = BinderMmapRegion::new_simulated(BINDER_DEFAULT_MMAP_SIZE);
    let num_threads = 8;
    let iterations_per_thread = 200;
    let mut handles = Vec::new();

    for thread_idx in 0..num_threads {
        let reg = Arc::clone(&region);
        let handle = thread::spawn(move || {
            for i in 0..iterations_per_thread {
                let size = match (thread_idx + i) % 4 {
                    0 => 64,
                    1 => 256,
                    2 => 1024,
                    _ => 4096,
                };

                if let Ok(ptr) = reg.allocate_buffer(size) {
                    assert!(reg.contains_ptr(ptr));
                    assert!(reg.is_active_allocation(ptr));

                    let data = vec![(thread_idx as u8) ^ (i as u8); size];
                    reg.write_bytes(ptr, &data).unwrap();
                    let read = reg.read_bytes(ptr, size).unwrap();
                    assert_eq!(read, data);

                    reg.free_buffer(ptr, size).unwrap();
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Worker thread panicked during mmap churn");
    }

    // Confirm the entire region is clean and can allocate the max block size
    let final_alloc = region
        .allocate_buffer(BINDER_DEFAULT_MMAP_SIZE)
        .expect("Mmap region should be fully recovered after concurrent churn");
    region.free_buffer(final_alloc, BINDER_DEFAULT_MMAP_SIZE).unwrap();
}
