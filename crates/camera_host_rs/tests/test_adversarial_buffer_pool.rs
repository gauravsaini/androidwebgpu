//! Adversarial Stress and Edge Case Verification for CameraBufferPool.

use camera_hal_virtual::PixelFormat;
use camera_host_rs::CameraBufferPool;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

#[test]
fn test_buffer_pool_zero_capacity_behavior() {
    let pool = CameraBufferPool::new(0);

    // Acquire and release with 0 capacity pool
    let buf1 = pool.acquire(1920, 1080, PixelFormat::Rgba8888);
    assert_eq!(buf1.data.len(), 1920 * 1080 * 4);
    let stats1 = pool.get_stats();
    assert_eq!(stats1.in_flight, 1);
    assert_eq!(stats1.pooled_available, 0);

    pool.release(buf1);
    let stats2 = pool.get_stats();
    assert_eq!(stats2.in_flight, 0);
    assert_eq!(stats2.pooled_available, 0); // Must not retain any buffer
}

#[test]
fn test_buffer_pool_overflow_and_drop_reclamation() {
    let capacity = 4;
    let pool = CameraBufferPool::new(capacity);

    // Acquire 20 buffers
    let mut buffers = Vec::new();
    for _ in 0..20 {
        buffers.push(pool.acquire(640, 480, PixelFormat::Yuv420888));
    }

    let stats_loaded = pool.get_stats();
    assert_eq!(stats_loaded.in_flight, 20);
    assert_eq!(stats_loaded.peak_in_flight, 20);
    assert_eq!(stats_loaded.pooled_available, 0);

    // Release all 20 buffers
    for buf in buffers {
        pool.release(buf);
    }

    let stats_released = pool.get_stats();
    assert_eq!(stats_released.in_flight, 0);
    assert_eq!(stats_released.pooled_available, capacity); // Capped at max_pool_capacity
    assert_eq!(stats_released.peak_in_flight, 20);
}

#[test]
fn test_buffer_pool_high_concurrency_multithreaded_churn() {
    let pool = Arc::new(CameraBufferPool::new(16));
    let num_threads = 16;
    let iterations_per_thread = 500;
    let stop_flag = Arc::new(AtomicBool::new(false));

    let mut handles = Vec::new();
    for t_idx in 0..num_threads {
        let p = Arc::clone(&pool);
        let s = Arc::clone(&stop_flag);
        handles.push(thread::spawn(move || {
            let formats = [
                (640, 480, PixelFormat::Rgba8888),
                (1280, 720, PixelFormat::Yuv420888),
                (320, 240, PixelFormat::Rgb565),
                (800, 600, PixelFormat::Blob),
            ];

            for i in 0..iterations_per_thread {
                if s.load(Ordering::Relaxed) {
                    break;
                }
                let (w, h, fmt) = formats[(t_idx + i) % formats.len()];
                let mut buf = p.acquire(w, h, fmt);
                assert_eq!(buf.width, w);
                assert_eq!(buf.height, h);
                assert_eq!(buf.format, fmt);
                if !buf.data.is_empty() {
                    buf.data[0] = (t_idx & 0xFF) as u8;
                }
                // Short hold
                if i % 7 == 0 {
                    thread::yield_now();
                }
                p.release(buf);
            }
        }));
    }

    for h in handles {
        h.join().expect("Worker thread panicked");
    }

    let final_stats = pool.get_stats();
    assert_eq!(final_stats.in_flight, 0);
    assert!(final_stats.pooled_available <= 16);
    assert!(final_stats.peak_in_flight >= 1);
    assert_eq!(
        final_stats.total_allocated,
        (num_threads * iterations_per_thread) as u64
    );
}

#[test]
fn test_buffer_pool_dynamic_format_mismatch_recycling() {
    let pool = CameraBufferPool::new(8);

    // 1. Acquire 4K RGBA
    let b1 = pool.acquire(3840, 2160, PixelFormat::Rgba8888);
    assert_eq!(b1.data.len(), 3840 * 2160 * 4);
    pool.release(b1);

    // 2. Acquire small YUV420 - should reallocate clean buffer because size/format mismatch
    let b2 = pool.acquire(160, 120, PixelFormat::Yuv420888);
    assert_eq!(b2.data.len(), 160 * 120 * 3 / 2);
    assert_eq!(b2.width, 160);
    assert_eq!(b2.height, 120);
    assert_eq!(b2.format, PixelFormat::Yuv420888);
    pool.release(b2);

    // 3. Pool should now contain the small YUV buffer
    let b3 = pool.acquire(160, 120, PixelFormat::Yuv420888);
    assert_eq!(b3.data.len(), 160 * 120 * 3 / 2);
    pool.release(b3);
}

#[test]
fn test_buffer_pool_large_resolution_calculation() {
    let pool = CameraBufferPool::new(2);
    // 8K UHD (7680 x 4320)
    let b_8k = pool.acquire(7680, 4320, PixelFormat::Rgba8888);
    assert_eq!(b_8k.data.len(), 7680 * 4320 * 4);
    pool.release(b_8k);
}

