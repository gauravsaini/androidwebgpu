//! Adversarial Stress & Boundary Verification Test Suite for WASM Virtio-Binder Bridge.
//!
//! Tests:
//! 1. Short packets (<48 bytes) and truncated headers
//! 2. Payload size mismatches and integer overflow in data_size / offsets_size
//! 3. Unaligned offsets and invalid offsets sizes
//! 4. Fuzzing with random chaotic byte buffers
//! 5. Multi-threaded concurrent transaction storm & handle translation
//! 6. Refcount upper bounds, underflow resistance, and double release
//! 7. Death recipient registration storm, unlinking, and trigger verification
//! 8. SurfaceComposer offloaded transactions via WASM binder entrypoint

use aidl_compat::{DeathRecipient, IBinder, Result as AidlResult};
use binder_rt::status::{STATUS_BAD_VALUE, STATUS_DEAD_OBJECT, STATUS_INVALID_OPERATION};
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::wire::{BR_DEAD_REPLY, BR_FAILED_REPLY};
use binder_rt::Parcel;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use virtio_binder::protocol::*;
use virtio_binder::VirtioBinderDevice;
use virtio_gpu_bridge::VirtioGpuBridge;

/// Mock test service for stress tests.
struct TestMockService {
    descriptor: &'static str,
    call_count: AtomicUsize,
    is_alive: AtomicBool,
}

impl TestMockService {
    fn new(descriptor: &'static str) -> Self {
        Self {
            descriptor,
            call_count: AtomicUsize::new(0),
            is_alive: AtomicBool::new(true),
        }
    }
}

impl IBinder for TestMockService {
    fn transact(
        &self,
        code: TransactionCode,
        _flags: TransactionFlags,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        match code {
            1 => {
                // Echo int
                let mut offset = 0;
                let val = data.read_i32(&mut offset).unwrap_or(-1);
                reply.write_i32(val * 2).unwrap();
                Ok(())
            }
            2 => {
                // Echo string
                let mut offset = 0;
                let s = data.read_utf8(&mut offset).unwrap_or_default().unwrap_or_default();
                reply.write_utf8(Some(&format!("ECHO:{}", s))).unwrap();
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn link_to_death(&self, _recipient: Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn unlink_to_death(&self, _recipient: &Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn ping_binder(&self) -> AidlResult<()> {
        if self.is_alive.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(aidl_compat::Status::new_service_specific_error(-1, Some("Dead")))
        }
    }

    fn is_binder_alive(&self) -> bool {
        self.is_alive.load(Ordering::SeqCst)
    }

    fn get_class_descriptor(&self) -> Option<&'static str> {
        Some(self.descriptor)
    }
}

// -----------------------------------------------------------------------------
// 1. Boundary: Short & Truncated Packets (<48 bytes)
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_short_and_truncated_packets() {
    pollster::block_on(async {
        let bridge = match VirtioGpuBridge::new(64, 64).await {
            Ok(b) => b,
            Err(_) => return,
        };

        // Test packet lengths from 0 to 47 bytes
        for len in 0..48 {
            let garbage = vec![0xAA; len];
            let resp_bytes = bridge.process_binder_packet(&garbage);
            let resp = VirtioBinderResponse::deserialize(&resp_bytes)
                .expect("Deserializing error response must succeed");

            assert_eq!(resp.hdr.msg_id, 0);
            assert_eq!(resp.hdr.status, STATUS_BAD_VALUE);
            assert_eq!(resp.hdr.result_code, BR_FAILED_REPLY as i32);
            assert!(!resp.hdr.is_success());
        }
    });
}

// -----------------------------------------------------------------------------
// 2. Boundary: Payload Size Mismatches and Integer Overflows
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_payload_size_mismatches() {
    pollster::block_on(async {
        let bridge = match VirtioGpuBridge::new(64, 64).await {
            Ok(b) => b,
            Err(_) => return,
        };

        // Case A: Header says data_size = 100, but buffer is only 48 bytes
        let mut hdr = VirtioBinderReqHdr::new_transact(5001, 1, 1, 0, 0, 100, 0);
        let mut pkt = hdr.as_bytes().to_vec();
        let resp_bytes = bridge.process_binder_packet(&pkt);
        let resp = VirtioBinderResponse::deserialize(&resp_bytes).unwrap();
        assert_eq!(resp.hdr.status, STATUS_BAD_VALUE);

        // Case B: Header says offsets_size = 64, but buffer has no offsets
        hdr.data_size = 0;
        hdr.offsets_size = 64;
        pkt = hdr.as_bytes().to_vec();
        let resp_bytes = bridge.process_binder_packet(&pkt);
        let resp = VirtioBinderResponse::deserialize(&resp_bytes).unwrap();
        assert_eq!(resp.hdr.status, STATUS_BAD_VALUE);

        // Case C: offsets_size not multiple of 8 (e.g. 7 bytes)
        hdr.data_size = 0;
        hdr.offsets_size = 7;
        pkt = hdr.as_bytes().to_vec();
        pkt.extend_from_slice(&[0u8; 7]);
        let resp_bytes = bridge.process_binder_packet(&pkt);
        let resp = VirtioBinderResponse::deserialize(&resp_bytes).unwrap();
        assert_eq!(resp.hdr.status, STATUS_BAD_VALUE);

        // Case D: Arithmetic overflow in data_size + offsets_size
        hdr.data_size = 0xFFFF_FFF0;
        hdr.offsets_size = 0x0000_0020;
        pkt = hdr.as_bytes().to_vec();
        let resp_bytes = bridge.process_binder_packet(&pkt);
        let resp = VirtioBinderResponse::deserialize(&resp_bytes).unwrap();
        assert_eq!(resp.hdr.status, STATUS_BAD_VALUE);
    });
}

// -----------------------------------------------------------------------------
// 3. Boundary: Invalid Commands & Non-existent / Dead Handles
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_invalid_commands_and_dead_handles() {
    pollster::block_on(async {
        let bridge = match VirtioGpuBridge::new(64, 64).await {
            Ok(b) => b,
            Err(_) => return,
        };

        // Unknown command opcode 0x9999
        let mut hdr = VirtioBinderReqHdr::new_ping(6001, 0);
        hdr.cmd = 0x9999;
        let resp_bytes = bridge.process_binder_packet(hdr.as_bytes());
        let resp = VirtioBinderResponse::deserialize(&resp_bytes).unwrap();
        assert_eq!(resp.hdr.msg_id, 6001);
        assert_eq!(resp.hdr.status, STATUS_INVALID_OPERATION);
        assert_eq!(resp.hdr.result_code, BR_FAILED_REPLY as i32);

        // Transact against non-existent handle 9999
        let req = VirtioBinderRequest::new_transact(6002, 9999, 1, 0, 0, Vec::new(), Vec::new());
        let resp_bytes = bridge.process_binder_packet(&req.serialize());
        let resp = VirtioBinderResponse::deserialize(&resp_bytes).unwrap();
        assert_eq!(resp.hdr.msg_id, 6002);
        assert_eq!(resp.hdr.status, STATUS_DEAD_OBJECT);
        assert_eq!(resp.hdr.result_code, BR_DEAD_REPLY as i32);

        // Ping against non-existent handle 9999
        let req_ping = VirtioBinderRequest::new_ping(6003, 9999);
        let resp_bytes = bridge.process_binder_packet(&req_ping.serialize());
        let resp = VirtioBinderResponse::deserialize(&resp_bytes).unwrap();
        assert_eq!(resp.hdr.msg_id, 6003);
        assert_eq!(resp.hdr.status, STATUS_DEAD_OBJECT);

        // Register a mock service, mark dead, test transact & ping
        let mock = Arc::new(TestMockService::new("test.IDeadService"));
        bridge.binder_device.register_service(200, Arc::clone(&mock) as Arc<dyn IBinder>);

        mock.is_alive.store(false, Ordering::SeqCst);
        let req_transact = VirtioBinderRequest::new_transact(6004, 200, 1, 0, 0, Vec::new(), Vec::new());
        let resp_bytes = bridge.process_binder_packet(&req_transact.serialize());
        let resp = VirtioBinderResponse::deserialize(&resp_bytes).unwrap();
        assert_eq!(resp.hdr.status, STATUS_DEAD_OBJECT);

        let req_ping_dead = VirtioBinderRequest::new_ping(6005, 200);
        let resp_bytes = bridge.process_binder_packet(&req_ping_dead.serialize());
        let resp = VirtioBinderResponse::deserialize(&resp_bytes).unwrap();
        assert_eq!(resp.hdr.status, STATUS_DEAD_OBJECT);
    });
}

// -----------------------------------------------------------------------------
// 4. Fuzzing: Chaotic Random Buffer Ingestion
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_fuzzing_random_buffers() {
    pollster::block_on(async {
        let bridge = match VirtioGpuBridge::new(64, 64).await {
            Ok(b) => b,
            Err(_) => return,
        };

        // Pseudo-random state generator
        let mut seed: u64 = 0xCAFE_BABE_1234_5678;
        let mut next_rand = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };

        // 2,000 randomized malformed buffers of varying lengths
        for _ in 0..2000 {
            let len = (next_rand() % 512) as usize;
            let mut buf = vec![0u8; len];
            for byte in buf.iter_mut() {
                *byte = (next_rand() & 0xFF) as u8;
            }

            // Must never panic
            let resp_bytes = bridge.process_binder_packet(&buf);
            let _ = VirtioBinderResponse::deserialize(&resp_bytes);
        }
    });
}

// -----------------------------------------------------------------------------
// 5. Stress: Refcount Bounds, Double Release & Underflow Resistance
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_refcount_bounds_and_lifecycle() {
    let device = VirtioBinderDevice::new();
    let mock = Arc::new(TestMockService::new("test.IRefCountService"));
    device.register_service(300, Arc::clone(&mock) as Arc<dyn IBinder>);

    // Initial count is 1
    assert_eq!(device.get_ref_count(300), Some(1));

    // Acquire 1000 times
    for _ in 0..1000 {
        assert!(device.acquire_handle(300).is_ok());
    }
    assert_eq!(device.get_ref_count(300), Some(1001));

    // Release 1000 times
    for _ in 0..1000 {
        let removed = device.release_handle(300).expect("Release must succeed");
        assert!(!removed);
    }
    assert_eq!(device.get_ref_count(300), Some(1));

    // Final release -> removes service
    let removed = device.release_handle(300).expect("Final release succeeds");
    assert!(removed);
    assert_eq!(device.get_ref_count(300), None);

    // Extra releases on non-existent handle -> returns Error, does not underflow
    for _ in 0..10 {
        assert!(device.release_handle(300).is_err());
    }
}

// -----------------------------------------------------------------------------
// 6. Stress: Death Recipient Registration Storm, Unlinking & Triggers
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_death_recipient_storm_and_triggers() {
    let device = VirtioBinderDevice::new();
    let mock = Arc::new(TestMockService::new("test.IDeathStormService"));
    device.register_service(400, Arc::clone(&mock) as Arc<dyn IBinder>);

    // Link 100 distinct cookies
    for i in 1..=100 {
        let cookie = 0xD000_0000 + i as u64;
        assert!(device.link_death(400, cookie).is_ok());
    }

    // Duplicate links should be deduplicated
    for i in 1..=50 {
        let cookie = 0xD000_0000 + i as u64;
        assert!(device.link_death(400, cookie).is_ok());
    }

    // Unlink the first 40 cookies
    for i in 1..=40 {
        let cookie = 0xD000_0000 + i as u64;
        let unlinked = device.unlink_death(400, cookie).expect("Unlink succeeds");
        assert!(unlinked);
    }

    // Unlinking already unlinked cookie returns false
    for i in 1..=40 {
        let cookie = 0xD000_0000 + i as u64;
        let unlinked = device.unlink_death(400, cookie).expect("Unlink returns false");
        assert!(!unlinked);
    }

    // Trigger death -> should deliver 60 events (cookies 41..=100) to Queue 1
    device.trigger_death(400);

    let event_q = device.event_queue();
    let mut eq = event_q.lock().unwrap();

    let mut received_cookies = Vec::new();
    while let Some(evt) = eq.pop_event() {
        assert_eq!(evt.event_type, EVENT_TYPE_DEATH);
        assert_eq!(evt.target_handle, 400);
        received_cookies.push(evt.cookie);
    }

    assert_eq!(received_cookies.len(), 60);
    for i in 41..=100 {
        let expected_cookie = 0xD000_0000 + i as u64;
        assert!(received_cookies.contains(&expected_cookie));
    }
}

// -----------------------------------------------------------------------------
// 7. Concurrency: Multi-threaded Transaction Storm (16 threads, 500 ops each)
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_multithreaded_transaction_storm() {
    let device = Arc::new(VirtioBinderDevice::new());
    let mock = Arc::new(TestMockService::new("test.IConcurrentService"));
    device.register_service(500, Arc::clone(&mock) as Arc<dyn IBinder>);

    let num_threads = 16;
    let ops_per_thread = 500;
    let mut handles = Vec::with_capacity(num_threads);

    for thread_id in 0..num_threads {
        let dev = Arc::clone(&device);
        let h = thread::spawn(move || {
            for op in 0..ops_per_thread {
                let msg_id = ((thread_id as u64) << 32) | (op as u64);
                let mode = op % 5;
                match mode {
                    0 => {
                        // Ping
                        let req = VirtioBinderRequest::new_ping(msg_id, 500);
                        let resp = dev.process_request(&req);
                        assert!(resp.hdr.is_success());
                    }
                    1 => {
                        // Transact Code 1 (Echo Int)
                        let mut p = Parcel::new();
                        p.write_i32(op as i32).unwrap();
                        let req = VirtioBinderRequest::from_parcel(msg_id, 500, 1, 0, 0, &p);
                        let resp = dev.process_request(&req);
                        assert!(resp.hdr.is_success());
                        let reply = resp.to_parcel();
                        let mut off = 0;
                        let val = reply.read_i32(&mut off).unwrap();
                        assert_eq!(val, (op as i32) * 2);
                    }
                    2 => {
                        // Transact Code 2 (Echo String)
                        let mut p = Parcel::new();
                        p.write_utf8(Some(&format!("msg_{}", op))).unwrap();
                        let req = VirtioBinderRequest::from_parcel(msg_id, 500, 2, 0, 0, &p);
                        let resp = dev.process_request(&req);
                        assert!(resp.hdr.is_success());
                    }
                    3 => {
                        // Acquire / Release
                        dev.acquire_handle(500).unwrap();
                        dev.release_handle(500).unwrap();
                    }
                    4 => {
                        // Link / Unlink death
                        let cookie = msg_id;
                        dev.link_death(500, cookie).unwrap();
                        dev.unlink_death(500, cookie).unwrap();
                    }
                    _ => unreachable!(),
                }
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().expect("Thread must not panic");
    }

    assert_eq!(device.get_ref_count(500), Some(1));
}

// -----------------------------------------------------------------------------
// 8. Integration: WASM Virtio-Binder Bridge SurfaceComposer Offloading
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_surface_composer_offload_and_large_parcel() {
    pollster::block_on(async {
        let bridge = match VirtioGpuBridge::new(128, 128).await {
            Ok(b) => b,
            Err(_) => return,
        };

        if bridge.surface_composer.is_none() {
            return;
        }

        // Test BOOT_FINISHED (opcode 1025)
        let req_boot = VirtioBinderRequest::new_transact(
            7001,
            1,
            surfaceflinger_gpu_service::isurfacecomposer_codes::BOOT_FINISHED,
            0,
            0,
            Vec::new(),
            Vec::new(),
        );
        let resp_bytes = bridge.process_binder_packet(&req_boot.serialize());
        let resp = VirtioBinderResponse::deserialize(&resp_bytes).unwrap();
        assert!(resp.hdr.is_success());

        // Test large 256KB parcel transaction
        let big_data = vec![0x55u8; 256 * 1024];
        let req_big = VirtioBinderRequest::new_transact(7002, 1, 999, 0, 0, big_data, Vec::new());
        let resp_bytes = bridge.process_binder_packet(&req_big.serialize());
        let resp = VirtioBinderResponse::deserialize(&resp_bytes).unwrap();
        assert_eq!(resp.hdr.msg_id, 7002);
    });
}
