//! Memory mapping lifecycle and buffer allocation tests.

use binder_sys::*;

#[test]
fn test_mmap_region_creation_and_bounds() {
    // Clamping to min/max
    let small_region = BinderMmapRegion::new_simulated(1024);
    assert_eq!(small_region.size(), BINDER_MIN_MMAP_SIZE);

    let large_region = BinderMmapRegion::new_simulated(10 * 1024 * 1024);
    assert_eq!(large_region.size(), BINDER_MAX_MMAP_SIZE);

    let default_region = BinderMmapRegion::new_simulated(BINDER_DEFAULT_MMAP_SIZE);
    assert_eq!(default_region.size(), BINDER_DEFAULT_MMAP_SIZE);

    let base = default_region.base_ptr();
    assert!(default_region.contains_ptr(base));
    assert!(default_region.contains_ptr(base + 100));
    assert!(default_region.contains_ptr(base + BINDER_DEFAULT_MMAP_SIZE as u64 - 1));
    assert!(!default_region.contains_ptr(base - 1));
    assert!(!default_region.contains_ptr(base + BINDER_DEFAULT_MMAP_SIZE as u64));
}

#[test]
fn test_mmap_buffer_alloc_read_write_free() {
    let region = BinderMmapRegion::new_simulated(BINDER_MIN_MMAP_SIZE);

    let payload = b"Binder Transaction Payload Data with UTF-8 String";
    let ptr = region.allocate_buffer(payload.len()).expect("Alloc failed");
    assert!(region.is_active_allocation(ptr));

    region.write_bytes(ptr, payload).expect("Write failed");

    let read_back = region
        .read_bytes(ptr, payload.len())
        .expect("Read failed");
    assert_eq!(read_back, payload);

    // Free buffer
    region.free_buffer(ptr, payload.len()).expect("Free failed");
    assert!(!region.is_active_allocation(ptr));

    // Double free should fail with InvalidBuffer
    assert_eq!(
        region.free_buffer(ptr, payload.len()),
        Err(MmapError::InvalidBuffer(ptr))
    );
}

#[test]
fn test_mmap_out_of_bounds_errors() {
    let region = BinderMmapRegion::new_simulated(BINDER_MIN_MMAP_SIZE);
    let invalid_ptr = 0xdeadbeef;

    assert!(region.read_bytes(invalid_ptr, 10).is_err());
    assert!(region.write_bytes(invalid_ptr, &[1, 2, 3]).is_err());
    assert_eq!(
        region.free_buffer(invalid_ptr, 10),
        Err(MmapError::OutOfBounds(
            invalid_ptr,
            region.base_ptr(),
            region.base_ptr() + region.size() as u64
        ))
    );
}

#[test]
fn test_mmap_fragmentation_coalescing() {
    let region = BinderMmapRegion::new_simulated(BINDER_MIN_MMAP_SIZE);

    // Allocate 3 blocks
    let ptr1 = region.allocate_buffer(1024).unwrap();
    let ptr2 = region.allocate_buffer(1024).unwrap();
    let ptr3 = region.allocate_buffer(1024).unwrap();

    // Free block 2 then block 1 then block 3
    region.free_buffer(ptr2, 1024).unwrap();
    region.free_buffer(ptr1, 1024).unwrap();
    region.free_buffer(ptr3, 1024).unwrap();

    // Reallocate large block to confirm coalescing
    let big_ptr = region.allocate_buffer(3072).unwrap();
    assert_eq!(big_ptr, ptr1);
    region.free_buffer(big_ptr, 3072).unwrap();
}
