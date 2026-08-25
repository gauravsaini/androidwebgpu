//! Adversarial Concurrency and Edge Case Verification for AudioRingBuffer.

use audio_host_rs::AudioRingBuffer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

#[test]
fn test_audio_ring_buffer_zero_capacity() {
    let buf = AudioRingBuffer::with_capacity(0);
    assert_eq!(buf.len(), 0);
    assert!(buf.is_empty());

    let data = [1u8, 2, 3, 4];
    let written = buf.write(&data);
    assert_eq!(written, 4);
    assert_eq!(buf.len(), 0);
    assert_eq!(buf.stats().overflow_drops, 4);

    let mut out = [0u8; 4];
    let read = buf.read(&mut out);
    assert_eq!(read, 0);
    assert_eq!(out, [0, 0, 0, 0]);
    assert_eq!(buf.stats().underflow_silence_bytes, 4);
}

#[test]
fn test_audio_ring_buffer_pcm_peak_amplitude_extremes() {
    let buf = AudioRingBuffer::new();

    // 1. Write i16::MIN (-32768) which in naive abs() overflows i16
    let min_sample = i16::MIN.to_le_bytes();
    buf.write(&min_sample);
    let stats = buf.stats();
    assert_eq!(stats.peak_amplitude, i16::MAX);

    // 2. Write i16::MAX (32767)
    let max_sample = i16::MAX.to_le_bytes();
    buf.write(&max_sample);
    let stats2 = buf.stats();
    assert_eq!(stats2.peak_amplitude, i16::MAX);

    // 3. Write odd number of bytes (3 bytes)
    let odd_data = [0x12, 0x34, 0x56];
    buf.write(&odd_data);
    assert_eq!(buf.stats().total_written_bytes, 2 + 2 + 3);
}

#[test]
fn test_audio_ring_buffer_concurrent_producers_and_consumers() {
    let capacity = 4096;
    let ring = Arc::new(AudioRingBuffer::with_capacity(capacity));
    let stop_flag = Arc::new(AtomicBool::new(false));

    let num_producers = 8;
    let num_consumers = 8;
    let items_per_thread = 2000;

    let mut handles = Vec::new();

    // Spawn writers
    for p_id in 0..num_producers {
        let r = Arc::clone(&ring);
        let s = Arc::clone(&stop_flag);
        handles.push(thread::spawn(move || {
            let chunk = vec![(p_id + 1) as u8; 64];
            for _ in 0..items_per_thread {
                if s.load(Ordering::Relaxed) {
                    break;
                }
                r.write(&chunk);
                if p_id % 2 == 0 {
                    thread::yield_now();
                }
            }
        }));
    }

    // Spawn readers
    for c_id in 0..num_consumers {
        let r = Arc::clone(&ring);
        let s = Arc::clone(&stop_flag);
        handles.push(thread::spawn(move || {
            let mut out = [0u8; 64];
            for _ in 0..items_per_thread {
                if s.load(Ordering::Relaxed) {
                    break;
                }
                r.read(&mut out);
                if c_id % 2 == 1 {
                    thread::yield_now();
                }
            }
        }));
    }

    for h in handles {
        h.join().expect("Producer/consumer thread panicked");
    }

    let final_stats = ring.stats();
    assert_eq!(
        final_stats.total_written_bytes,
        (num_producers * items_per_thread * 64) as u64
    );
    assert_eq!(
        final_stats.total_read_bytes + final_stats.overflow_drops + (ring.len() as u64),
        final_stats.total_written_bytes
    );
}

#[test]
fn test_audio_ring_buffer_clear_underflow_isolation() {
    let ring = AudioRingBuffer::with_capacity(128);
    ring.write(&[0xAA; 64]);
    assert_eq!(ring.len(), 64);

    ring.clear();
    assert_eq!(ring.len(), 0);
    assert!(ring.is_empty());

    let mut out = [0xFF; 32];
    let read = ring.read(&mut out);
    assert_eq!(read, 0);
    assert_eq!(out, [0x00; 32]); // Filled with silence
}

#[test]
fn test_audio_ring_buffer_zero_capacity_concurrent_hammer() {
    let ring = Arc::new(AudioRingBuffer::with_capacity(0));
    let num_threads = 16;
    let iterations = 1000;
    let mut handles = Vec::new();

    for t in 0..num_threads {
        let r = Arc::clone(&ring);
        handles.push(thread::spawn(move || {
            let chunk = vec![t as u8; 32];
            let mut read_buf = [0u8; 32];
            for _ in 0..iterations {
                let w = r.write(&chunk);
                assert_eq!(w, 32);
                let rd = r.read(&mut read_buf);
                assert_eq!(rd, 0);
                assert_eq!(read_buf, [0u8; 32]);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let stats = ring.stats();
    assert_eq!(stats.total_written_bytes, (num_threads * iterations * 32) as u64);
    assert_eq!(stats.overflow_drops, (num_threads * iterations * 32) as u64);
    assert_eq!(stats.underflow_silence_bytes, (num_threads * iterations * 32) as u64);
    assert_eq!(ring.len(), 0);
}

#[test]
fn test_audio_ring_buffer_concurrent_clear_and_variable_chunks() {
    let ring = Arc::new(AudioRingBuffer::with_capacity(512));
    let stop_flag = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();

    // 4 writers with varying slice sizes (0, 1, 17, 1024 bytes)
    for i in 0..4 {
        let r = Arc::clone(&ring);
        let s = Arc::clone(&stop_flag);
        handles.push(thread::spawn(move || {
            let sizes = [0, 1, 17, 1024];
            let size = sizes[i % sizes.len()];
            let chunk = vec![0x55; size];
            for _ in 0..500 {
                if s.load(Ordering::Relaxed) {
                    break;
                }
                r.write(&chunk);
            }
        }));
    }

    // 2 readers
    for _ in 0..2 {
        let r = Arc::clone(&ring);
        let s = Arc::clone(&stop_flag);
        handles.push(thread::spawn(move || {
            let mut out = [0u8; 64];
            for _ in 0..500 {
                if s.load(Ordering::Relaxed) {
                    break;
                }
                r.read(&mut out);
            }
        }));
    }

    // 1 thread constantly clearing
    {
        let r = Arc::clone(&ring);
        let s = Arc::clone(&stop_flag);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                if s.load(Ordering::Relaxed) {
                    break;
                }
                r.clear();
                thread::yield_now();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

