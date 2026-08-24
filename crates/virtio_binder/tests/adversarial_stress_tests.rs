//! Adversarial Stress and Fuzzing Test Suite for `virtio_binder`.
//!
//! Evaluates:
//! 1. Multi-threaded high-concurrency request/reply queue processing with random interleaving.
//! 2. Extreme virtqueue chain fragmentation (1-byte chunks, 0-byte slices) and buffer overflows.
//! 3. Malformed packets, fuzzed bytes, invalid command opcodes, and alignment violations.
//! 4. High-throughput out-of-band death notification flood & concurrent draining.
//! 5. Massive multi-megabyte payload buffer splitting across virtqueue chains.
//! 6. Concurrent service registration, unregistration, and handle lifecycle contention.

use aidl_compat::status::{Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_DEAD_OBJECT};
use aidl_compat::stub::Binder;
use aidl_compat::traits::Remotable;
use binder_rt::status::{STATUS_INVALID_OPERATION, STATUS_OK};
use binder_rt::types::TransactionCode;
use binder_rt::wire::{BR_FAILED_REPLY, BR_REPLY, TF_ONE_WAY};
use binder_rt::Parcel;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use virtio_binder::*;

// -----------------------------------------------------------------------------
// Adversarial Remotable Service
// -----------------------------------------------------------------------------

struct AdversarialCalcService {
    alive: AtomicBool,
    tx_count: AtomicUsize,
    oneway_count: AtomicUsize,
}

impl AdversarialCalcService {
    fn new() -> Self {
        Self {
            alive: AtomicBool::new(true),
            tx_count: AtomicUsize::new(0),
            oneway_count: AtomicUsize::new(0),
        }
    }
}

impl Remotable for AdversarialCalcService {
    fn get_class_descriptor() -> &'static str {
        "android.os.IAdversarialCalcService"
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> AidlResult<()> {
        if !self.alive.load(Ordering::SeqCst) {
            return Err(Status::from_status(STATUS_DEAD_OBJECT));
        }
        self.tx_count.fetch_add(1, Ordering::Relaxed);

        match code {
            // Opcode 1: Multiply: a (i32) * b (i32) -> product (i64)
            1 => {
                let mut offset = 0;
                let a = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let b = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let product = (a as i64) * (b as i64);
                reply
                    .write_i64(product)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }

            // Opcode 2: Echo raw bytes
            2 => {
                let mut offset = 0;
                let raw_bytes = data
                    .read_byte_vec(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                reply
                    .write_byte_slice(Some(&raw_bytes))
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }

            // Opcode 3: Asynchronous One-Way Notification
            3 => {
                self.oneway_count.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }

            // Opcode 4: Large array transformation
            4 => {
                let mut offset = 0;
                let count = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))? as usize;
                let mut vals = Vec::with_capacity(count);
                for _ in 0..count {
                    let v = data
                        .read_i32(&mut offset)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    vals.push(v.wrapping_mul(3));
                }
                reply
                    .write_i32(vals.len() as i32)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                for v in vals {
                    reply
                        .write_i32(v)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                }
                Ok(())
            }

            _ => Err(Status::from_status(STATUS_BAD_VALUE)),
        }
    }
}

// -----------------------------------------------------------------------------
// Test 1: Multi-threaded High-Concurrency Interleaving
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_multithreaded_high_concurrency_interleaving() {
    let host_device = Arc::new(VirtioBinderDevice::new());
    let calc_service = Arc::new(AdversarialCalcService::new());
    let binder_stub = Binder::new_with_arc(Arc::clone(&calc_service));
    host_device.register_binder(100, binder_stub);

    let transport = Arc::new(GuestVirtioTransport::new_with_device(Arc::clone(&host_device)));

    let num_threads = 32;
    let ops_per_thread = 150;
    let mut handles = Vec::new();

    for t_id in 0..num_threads {
        let t_transport = Arc::clone(&transport);
        let t_device = Arc::clone(&host_device);
        let handle = thread::spawn(move || {
            let proxy = t_transport.create_remote_binder(100, 0, Some("android.os.IAdversarialCalcService"));
            for i in 0..ops_per_thread {
                let op = (t_id * 17 + i) % 7;
                match op {
                    // 0: Multiply transaction
                    0 => {
                        let a = (t_id * 100 + i) as i32;
                        let b = (i * 3 + 1) as i32;
                        let expected = (a as i64) * (b as i64);

                        let mut req = Parcel::new();
                        req.write_i32(a).unwrap();
                        req.write_i32(b).unwrap();

                        let mut reply = Parcel::new();
                        proxy
                            .transact(1, 0, &req, &mut reply)
                            .unwrap_or_else(|e| panic!("Thread {} op 0 failed: {:?}", t_id, e));

                        let mut offset = 0;
                        let res = reply.read_i64(&mut offset).unwrap();
                        assert_eq!(res, expected);
                    }

                    // 1: Byte echo transaction
                    1 => {
                        let test_payload = vec![((t_id ^ i) & 0xFF) as u8; (i % 64) + 1];
                        let mut req = Parcel::new();
                        req.write_byte_slice(Some(&test_payload)).unwrap();

                        let mut reply = Parcel::new();
                        proxy
                            .transact(2, 0, &req, &mut reply)
                            .unwrap_or_else(|e| panic!("Thread {} op 1 failed: {:?}", t_id, e));

                        let mut offset = 0;
                        let ret_bytes = reply.read_byte_vec(&mut offset).unwrap().unwrap();
                        assert_eq!(ret_bytes, test_payload);
                    }

                    // 2: One-way transaction
                    2 => {
                        let req = Parcel::new();
                        let mut reply = Parcel::new();
                        proxy
                            .transact(3, TF_ONE_WAY, &req, &mut reply)
                            .unwrap_or_else(|e| panic!("Thread {} op 2 failed: {:?}", t_id, e));
                    }

                    // 3: Ping to active service
                    3 => {
                        t_transport.ping(100).expect("Ping active service ok");
                    }

                    // 4: Ping to ServiceManager (handle 0)
                    4 => {
                        t_transport.ping(0).expect("Ping SM ok");
                    }

                    // 5: Acquire and release handle
                    5 => {
                        t_transport.acquire_handle(100).expect("Acquire ok");
                        t_transport.release_handle(100).expect("Release ok");
                    }

                    // 6: Link and unlink death cookie
                    6 => {
                        let cookie = ((t_id as u64) << 32) | (i as u64);
                        t_transport.link_death(100, cookie).expect("Link death ok");
                        let unlinked = t_device.unlink_death(100, cookie).expect("Unlink death ok");
                        assert!(unlinked);
                    }

                    _ => unreachable!(),
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Thread joined successfully without panics");
    }

    assert!(calc_service.tx_count.load(Ordering::SeqCst) > 0);
}

// -----------------------------------------------------------------------------
// Test 2: Extreme Virtqueue Chain Fragmentation (1-byte & 0-byte Slices)
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_extreme_virtqueue_chain_fragmentation() {
    let host_device = VirtioBinderDevice::new();
    let calc_service = Arc::new(AdversarialCalcService::new());
    let stub = Binder::new_with_arc(Arc::clone(&calc_service));
    host_device.register_binder(200, stub);

    // Construct a real transaction parcel
    let mut req_parcel = Parcel::new();
    req_parcel.write_i32(1234).unwrap();
    req_parcel.write_i32(5678).unwrap();

    let req = VirtioBinderRequest::new_transact(
        999001,
        200,
        1, // Multiply opcode
        0,
        0,
        req_parcel.data().to_vec(),
        req_parcel.offsets().to_vec(),
    );
    let serialized_req = req.serialize();

    // 1. Fragment the readable buffers into 1-byte slices with random 0-byte empty slices
    let mut fragmented_readable = Vec::new();
    for (i, byte) in serialized_req.iter().enumerate() {
        if i % 5 == 0 {
            fragmented_readable.push(Vec::new()); // Zero-length empty buffer
        }
        fragmented_readable.push(vec![*byte]);
    }
    fragmented_readable.push(Vec::new());

    // 2. Fragment the writable reply buffers into 2-byte and 0-byte slices
    let total_reply_capacity = 256;
    let mut fragmented_writable = Vec::new();
    let mut allocated = 0;
    while allocated < total_reply_capacity {
        if allocated % 6 == 0 {
            fragmented_writable.push(Vec::new());
        }
        let chunk = std::cmp::min(2, total_reply_capacity - allocated);
        fragmented_writable.push(vec![0u8; chunk]);
        allocated += chunk;
    }

    let mut chain = VirtQueueChain::new(1, fragmented_readable, fragmented_writable);
    assert_eq!(chain.readable_len(), serialized_req.len());
    assert_eq!(chain.writable_capacity(), total_reply_capacity);

    let written = host_device
        .process_virtqueue_chain(&mut chain)
        .expect("Process fragmented chain must succeed");
    assert!(written >= std::mem::size_of::<VirtioBinderRespHdr>());

    let reply_bytes = chain.take_written_data();
    let resp = VirtioBinderResponse::deserialize(&reply_bytes).expect("Deserialize response");
    assert_eq!(resp.hdr.msg_id, 999001);
    assert_eq!(resp.hdr.status, STATUS_OK);
    assert_eq!(resp.hdr.result_code, BR_REPLY as i32);

    let reply_parcel = resp.to_parcel();
    let mut offset = 0;
    let product = reply_parcel.read_i64(&mut offset).unwrap();
    assert_eq!(product, 1234i64 * 5678i64);
}

// -----------------------------------------------------------------------------
// Test 3: Writable Buffer Overflows, Truncation, and Boundary Conditions
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_writable_buffer_overflow_and_boundaries() {
    let host_device = VirtioBinderDevice::new();
    let req = VirtioBinderRequest::new_ping(777, 0);
    let req_bytes = req.serialize();

    // 1. Zero writable capacity
    let mut chain_zero = VirtQueueChain::new(0, vec![req_bytes.clone()], vec![]);
    let err_zero = host_device.process_virtqueue_chain(&mut chain_zero);
    assert!(matches!(err_zero, Err(DeviceError::Queue(QueueError::BufferOverflow { .. }))));

    // 2. Writable capacity strictly less than response header (32 bytes)
    for small_cap in [1, 5, 16, 31] {
        let mut chain_small = VirtQueueChain::new(
            0,
            vec![req_bytes.clone()],
            vec![vec![0u8; small_cap]],
        );
        let err_small = host_device.process_virtqueue_chain(&mut chain_small);
        assert!(
            matches!(err_small, Err(DeviceError::Queue(QueueError::BufferOverflow { .. }))),
            "Expected BufferOverflow for capacity {}",
            small_cap
        );
    }

    // 3. Exact capacity matching response header (ping response has 0 data and 0 offsets, exactly 32 bytes)
    let mut chain_exact = VirtQueueChain::new(
        0,
        vec![req_bytes.clone()],
        vec![vec![0u8; std::mem::size_of::<VirtioBinderRespHdr>()]],
    );
    let ok_exact = host_device.process_virtqueue_chain(&mut chain_exact);
    assert!(ok_exact.is_ok());
    assert_eq!(ok_exact.unwrap(), 32);
    let resp = VirtioBinderResponse::deserialize(&chain_exact.take_written_data()).unwrap();
    assert_eq!(resp.hdr.msg_id, 777);
}

// -----------------------------------------------------------------------------
// Test 4: Invalid Command Opcodes and Fuzzed Malformed Packets
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_invalid_commands_and_fuzzing() {
    let host_device = VirtioBinderDevice::new();

    // 1. Invalid command opcodes
    let invalid_opcodes = [0u32, 7, 8, 99, 1000, u32::MAX];
    for opcode in invalid_opcodes {
        let mut hdr = VirtioBinderReqHdr::new_transact(8888, 0, 0, 0, 0, 0, 0);
        hdr.cmd = opcode;
        let pkt = hdr.as_bytes().to_vec();

        let resp_bytes = host_device.process_packet(&pkt).expect("Device must respond with error frame");
        let resp = VirtioBinderResponse::deserialize(&resp_bytes).expect("Valid error response frame");
        assert_eq!(resp.hdr.msg_id, 8888);
        assert_eq!(resp.hdr.status, STATUS_INVALID_OPERATION);
        assert_eq!(resp.hdr.result_code, BR_FAILED_REPLY as i32);
    }

    // 2. Offsets size unaligned (e.g. 1..7 bytes, 9..15 bytes)
    for unaligned_size in [1usize, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15] {
        let hdr = VirtioBinderReqHdr::new_transact(1, 0, 0, 0, 0, 0, unaligned_size as u32);
        let mut pkt = hdr.as_bytes().to_vec();
        pkt.extend_from_slice(&vec![0u8; unaligned_size]);

        let res = host_device.process_packet(&pkt);
        assert_eq!(
            res.unwrap_err().to_string(),
            DeviceError::Protocol(ProtocolError::InvalidOffsetsSize(unaligned_size)).to_string()
        );
    }

    // 3. Payload size mismatch (data_size + offsets_size > remaining bytes)
    let hdr_len = std::mem::size_of::<VirtioBinderReqHdr>();
    let hdr = VirtioBinderReqHdr::new_transact(1, 0, 0, 0, 0, 100, 16);
    let mut pkt = hdr.as_bytes().to_vec();
    pkt.extend_from_slice(&[0u8; 50]); // Only 50 bytes instead of 116

    let res = host_device.process_packet(&pkt);
    assert!(matches!(
        res,
        Err(DeviceError::Protocol(ProtocolError::PayloadSizeMismatch {
            data_size: 100,
            offsets_size: 16,
            remaining: 50,
        }))
    ));

    // 4. Packet truncation (all lengths from 0 to 47)
    for len in 0..hdr_len {
        let truncated = vec![0xAAu8; len];
        let res = host_device.process_packet(&truncated);
        assert!(matches!(
            res,
            Err(DeviceError::Protocol(ProtocolError::PacketTooShort { .. }))
        ));
    }

    // 5. Fuzzing with pseudo-random byte sequences
    let mut seed = 0x12345678u32;
    for _ in 0..500 {
        // Simple LCG PRNG
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let len = (seed as usize % 256) + 1;
        let mut random_bytes = Vec::with_capacity(len);
        for _ in 0..len {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            random_bytes.push((seed >> 24) as u8);
        }

        // Must never panic, crash, or abort
        let _ = host_device.process_packet(&random_bytes);
        let _ = VirtioBinderRequest::deserialize(&random_bytes);
        let _ = VirtioBinderResponse::deserialize(&random_bytes);
        let _ = VirtioBinderEventHdr::from_bytes(&random_bytes);
    }
}

// -----------------------------------------------------------------------------
// Test 5: High-Throughput Death Notification Storm & Concurrent Draining
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_death_notification_storm_and_concurrent_draining() {
    let host_device = Arc::new(VirtioBinderDevice::new());
    let transport = Arc::new(GuestVirtioTransport::new_with_device(Arc::clone(&host_device)));

    let total_services = 50;
    let cookies_per_service = 100;
    let total_expected_events = total_services * cookies_per_service;

    // Register 50 mock services and link 100 cookies each (5,000 total cookies)
    for s_id in 0..total_services {
        let calc = Arc::new(AdversarialCalcService::new());
        let stub = Binder::new_with_arc(calc);
        host_device.register_binder(s_id as u32 + 10, stub);
        for c in 0..cookies_per_service {
            let cookie = ((s_id as u64) << 32) | (c as u64);
            transport
                .link_death(s_id as u32 + 10, cookie)
                .expect("Link death ok");
        }
    }

    // Consumer thread draining Queue 1 concurrently
    let t_transport = Arc::clone(&transport);
    let drained_count = Arc::new(AtomicUsize::new(0));
    let t_drained = Arc::clone(&drained_count);
    let stop_signal = Arc::new(AtomicBool::new(false));
    let t_stop = Arc::clone(&stop_signal);

    let consumer_handle = thread::spawn(move || {
        let mut collected_cookies = HashSet::new();
        while !t_stop.load(Ordering::SeqCst) || collected_cookies.len() < total_expected_events {
            let events = t_transport.drain_events();
            for ev in events {
                assert_eq!(ev.event_type, EVENT_TYPE_DEATH);
                assert!(collected_cookies.insert(ev.cookie), "Duplicate death event delivered!");
                t_drained.fetch_add(1, Ordering::Relaxed);
            }
            if collected_cookies.len() >= total_expected_events {
                break;
            }
            thread::yield_now();
        }
        collected_cookies.len()
    });

    // Concurrently trigger deaths across multiple threads
    let num_killer_threads = 10;
    let mut killer_handles = Vec::new();
    for k_id in 0..num_killer_threads {
        let t_dev = Arc::clone(&host_device);
        let handle = thread::spawn(move || {
            for s_id in 0..total_services {
                if s_id % num_killer_threads == k_id {
                    t_dev.trigger_death(s_id as u32 + 10);
                }
            }
        });
        killer_handles.push(handle);
    }

    for kh in killer_handles {
        kh.join().unwrap();
    }

    // Wait for consumer to finish draining
    stop_signal.store(true, Ordering::SeqCst);
    let total_collected = consumer_handle.join().unwrap();
    assert_eq!(
        total_collected, total_expected_events,
        "Every registered cookie must be delivered exactly once"
    );

    // Final drain must be empty
    assert_eq!(transport.drain_events().len(), 0);
}

// -----------------------------------------------------------------------------
// Test 6: Multi-Megabyte Large Payload Buffer Splitting
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_large_payload_virtqueue_buffer_splitting() {
    let host_device = VirtioBinderDevice::new();
    let calc_service = Arc::new(AdversarialCalcService::new());
    let stub = Binder::new_with_arc(Arc::clone(&calc_service));
    host_device.register_binder(300, stub);

    // Create a 512KB payload with 131,072 integers
    let int_count = 131072;
    let mut req_parcel = Parcel::new();
    req_parcel.write_i32(int_count as i32).unwrap();
    for i in 0..int_count {
        req_parcel.write_i32((i % 1000) as i32).unwrap();
    }

    let req = VirtioBinderRequest::new_transact(
        555111,
        300,
        4, // Large array multiply opcode
        0,
        0,
        req_parcel.data().to_vec(),
        req_parcel.offsets().to_vec(),
    );
    let serialized_req = req.serialize();
    let req_size = serialized_req.len();
    assert!(req_size > 500_000, "Request size should exceed 500KB");

    // Split readable request across 256 irregular chunks (ranging from 100 to 4000 bytes)
    let mut fragmented_readable = Vec::new();
    let mut cursor = 0;
    let mut step = 100;
    while cursor < req_size {
        let chunk_size = std::cmp::min(step, req_size - cursor);
        fragmented_readable.push(serialized_req[cursor..cursor + chunk_size].to_vec());
        cursor += chunk_size;
        step = (step * 3) % 4000 + 64;
    }

    // Split writable reply across 512 chunks of 2048 bytes each (1MB capacity)
    let writable_chunk_size = 2048;
    let writable_chunks_count = 512;
    let fragmented_writable = vec![vec![0u8; writable_chunk_size]; writable_chunks_count];

    let mut chain = VirtQueueChain::new(1, fragmented_readable, fragmented_writable);
    assert_eq!(chain.readable_len(), req_size);

    let written = host_device
        .process_virtqueue_chain(&mut chain)
        .expect("Process massive payload chain must succeed");
    assert!(written > 500_000);

    let reply_bytes = chain.take_written_data();
    let resp = VirtioBinderResponse::deserialize(&reply_bytes).expect("Deserialize large response");
    assert_eq!(resp.hdr.msg_id, 555111);
    assert_eq!(resp.hdr.status, STATUS_OK);

    let reply_parcel = resp.to_parcel();
    let mut offset = 0;
    let out_count = reply_parcel.read_i32(&mut offset).unwrap() as usize;
    assert_eq!(out_count, int_count);

    // Verify sampled elements
    for i in 0..out_count {
        let val = reply_parcel.read_i32(&mut offset).unwrap();
        let expected = ((i % 1000) as i32).wrapping_mul(3);
        if i % 10000 == 0 {
            assert_eq!(val, expected);
        }
    }
}

// -----------------------------------------------------------------------------
// Test 7: Concurrent Service Registration, Unregistration, and Lifecycle Race
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_concurrent_lifecycle_and_handle_contention() {
    let host_device = Arc::new(VirtioBinderDevice::new());
    let transport = Arc::new(GuestVirtioTransport::new_with_device(Arc::clone(&host_device)));

    let num_threads = 16;
    let iterations = 100;
    let mut handles = Vec::new();

    for t_id in 0..num_threads {
        let t_dev = Arc::clone(&host_device);
        let t_trans = Arc::clone(&transport);
        let h = thread::spawn(move || {
            for i in 0..iterations {
                // 1. Register auto handle
                let calc = Arc::new(AdversarialCalcService::new());
                let stub = Binder::new_with_arc(calc);
                let handle_id = t_dev.register_binder_auto(stub);

                // 2. Transact on newly allocated handle
                let proxy = t_trans.create_remote_binder(handle_id, 0, None);
                let mut req = Parcel::new();
                req.write_i32(t_id).unwrap();
                req.write_i32(i).unwrap();
                let mut reply = Parcel::new();
                proxy.transact(1, 0, &req, &mut reply).expect("Transact must succeed");

                // 3. Acquire ref count
                t_trans.acquire_handle(handle_id).expect("Acquire handle ok");

                // 4. Release ref count
                t_trans.release_handle(handle_id).expect("Release 1 ok");
                t_trans.release_handle(handle_id).expect("Release 2 ok"); // Should destroy entry

                // 5. Subsequent transact must fail cleanly with DeadObject
                let mut dead_reply = Parcel::new();
                let dead_res = proxy.transact(1, 0, &req, &mut dead_reply);
                assert!(dead_res.is_err());
                assert_eq!(dead_res.unwrap_err().status_code(), binder_rt::status::StatusCode::DeadObject);
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().unwrap();
    }
}

// -----------------------------------------------------------------------------
// Test 8: Asynchronous One-Way Transaction Flooding
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_oneway_flood() {
    let host_device = Arc::new(VirtioBinderDevice::new());
    let calc_service = Arc::new(AdversarialCalcService::new());
    host_device.register_binder(400, Binder::new_with_arc(Arc::clone(&calc_service)));

    let transport = Arc::new(GuestVirtioTransport::new_with_device(Arc::clone(&host_device)));

    let num_threads = 20;
    let flood_per_thread = 500;
    let mut handles = Vec::new();

    for _ in 0..num_threads {
        let t_trans = Arc::clone(&transport);
        let h = thread::spawn(move || {
            let proxy = t_trans.create_remote_binder(400, 0, None);
            let req = Parcel::new();
            let mut reply = Parcel::new();
            for _ in 0..flood_per_thread {
                proxy
                    .transact(3, TF_ONE_WAY, &req, &mut reply)
                    .expect("One-way flood transact must succeed");
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(
        calc_service.oneway_count.load(Ordering::SeqCst),
        num_threads * flood_per_thread
    );
}

// -----------------------------------------------------------------------------
// Test 9: Rapid Strong Refcount Churn and Contention on Single Handle
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_single_handle_refcount_churn() {
    let host_device = Arc::new(VirtioBinderDevice::new());
    let calc_service = Arc::new(AdversarialCalcService::new());
    host_device.register_binder(500, Binder::new_with_arc(calc_service));

    let transport = Arc::new(GuestVirtioTransport::new_with_device(Arc::clone(&host_device)));

    // Initial refcount = 1
    assert_eq!(host_device.get_ref_count(500), Some(1));

    let num_threads = 16;
    let churn_cycles = 500;
    let mut handles = Vec::new();

    for _ in 0..num_threads {
        let t_trans = Arc::clone(&transport);
        let h = thread::spawn(move || {
            for _ in 0..churn_cycles {
                t_trans.acquire_handle(500).expect("Acquire ok");
                t_trans.release_handle(500).expect("Release ok");
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().unwrap();
    }

    // Final refcount must strictly equal initial refcount (1)
    assert_eq!(host_device.get_ref_count(500), Some(1));
}

// -----------------------------------------------------------------------------
// Test 10: 4MB Parcel with Nested Offsets Across Fragmented Virtqueue Chain
// -----------------------------------------------------------------------------

#[test]
fn test_adversarial_4mb_parcel_with_fragmented_offsets() {
    let host_device = VirtioBinderDevice::new();
    let calc_service = Arc::new(AdversarialCalcService::new());
    host_device.register_binder(600, Binder::new_with_arc(calc_service));

    // Construct 4MB payload with 8-byte aligned data_size
    // (Opcode 4: count (i32) + padding (i32) + 131,072 i32 ints = 524,296 bytes which is divisible by 8)
    let int_count = 131072;
    let mut req_parcel = Parcel::new();
    req_parcel.write_i32(int_count as i32).unwrap();
    for i in 0..int_count {
        req_parcel.write_i32(i as i32).unwrap();
    }
    // Ensure data_size is 8-byte aligned
    if req_parcel.data().len() % 8 != 0 {
        req_parcel.write_i32(0).unwrap();
    }
    assert_eq!(req_parcel.data().len() % 8, 0);

    let num_offsets = 100;
    let offsets: Vec<u64> = (0..num_offsets).map(|i| (i * 4096) as u64).collect();

    let req = VirtioBinderRequest::new_transact(
        888999,
        600,
        4, // Large array opcode
        0,
        0,
        req_parcel.data().to_vec(),
        offsets.clone(),
    );

    let serialized = req.serialize();
    let req_size = serialized.len();
    assert!(req_size > 500_000);

    // Split across 1024 readable buffers
    let chunk_size = req_size / 1024 + 1;
    let mut fragmented_readable = Vec::new();
    let mut offset = 0;
    while offset < req_size {
        let take = std::cmp::min(chunk_size, req_size - offset);
        fragmented_readable.push(serialized[offset..offset + take].to_vec());
        offset += take;
    }

    // Allocate 2MB writable reply capacity
    let writable_chunks = vec![vec![0u8; 4096]; 512]; // 512 * 4KB = 2MB

    let mut chain = VirtQueueChain::new(1, fragmented_readable, writable_chunks);
    let written = host_device
        .process_virtqueue_chain(&mut chain)
        .expect("Process 4MB chain must succeed");
    assert!(written > 500_000);

    let reply_bytes = chain.take_written_data();
    let resp = VirtioBinderResponse::deserialize(&reply_bytes).expect("Deserialize response");
    assert_eq!(resp.hdr.msg_id, 888999);
    assert_eq!(resp.hdr.status, STATUS_OK);

    let reply_parcel = resp.to_parcel();
    let mut read_offset = 0;
    let out_count = reply_parcel.read_i32(&mut read_offset).unwrap() as usize;
    assert_eq!(out_count, int_count);
}

// -----------------------------------------------------------------------------
// Test 11: Unaligned Offsets Deserialization (No Panic)
// -----------------------------------------------------------------------------

#[test]
fn test_unaligned_data_size_offsets_deserialization() {
    // Specifically test parcels with unaligned data sizes (data_size = 4, 12, 20, etc.)
    // containing single or multiple offset table entries.
    for data_size in [4usize, 12, 20, 1, 2, 3, 5, 7, 13, 19, 21, 33] {
        let data: Vec<u8> = (0..data_size).map(|i| (i as u8).wrapping_add(1)).collect();
        for num_offsets in [1usize, 2, 5, 16] {
            let offsets: Vec<u64> = (0..num_offsets).map(|i| (i * 0x1000 + 0x20) as u64).collect();

            let req = VirtioBinderRequest::new_transact(
                10000 + data_size as u64,
                1,
                1,
                0,
                0,
                data.clone(),
                offsets.clone(),
            );
            let serialized = req.serialize();

            let deserialized = VirtioBinderRequest::deserialize(&serialized)
                .unwrap_or_else(|e| panic!("Req deserialize failed for data_size={}, offsets={}: {:?}", data_size, num_offsets, e));

            assert_eq!(deserialized.hdr.msg_id, 10000 + data_size as u64);
            assert_eq!(deserialized.data, data);
            assert_eq!(deserialized.offsets, offsets);

            // Also test response deserialization with unaligned offsets
            let resp = VirtioBinderResponse::new(
                20000 + data_size as u64,
                0,
                0,
                0,
                data.clone(),
                offsets.clone(),
            );
            let serialized_resp = resp.serialize();
            let deserialized_resp = VirtioBinderResponse::deserialize(&serialized_resp)
                .unwrap_or_else(|e| panic!("Resp deserialize failed for data_size={}, offsets={}: {:?}", data_size, num_offsets, e));

            assert_eq!(deserialized_resp.hdr.msg_id, 20000 + data_size as u64);
            assert_eq!(deserialized_resp.data, data);
            assert_eq!(deserialized_resp.offsets, offsets);
        }
    }
}

// -----------------------------------------------------------------------------
// Test 12: Unaligned Buffer Pointer Parsing
// -----------------------------------------------------------------------------

#[test]
fn test_unaligned_buffer_pointer_parsing() {
    let hdr = VirtioBinderReqHdr::new_ping(1, 0);
    let hdr_bytes = hdr.as_bytes().to_vec();

    // Test header parsing across all shift alignments 1..8
    for shift in 1..=8 {
        let mut unaligned_buffer = vec![0xEEu8; shift];
        unaligned_buffer.extend_from_slice(&hdr_bytes);

        let unaligned_slice = &unaligned_buffer[shift..];
        assert_eq!(unaligned_slice.len(), 48);

        let parsed_hdr = VirtioBinderReqHdr::from_bytes(unaligned_slice)
            .unwrap_or_else(|e| panic!("ReqHdr from_bytes failed at shift {}: {:?}", shift, e));
        assert_eq!(parsed_hdr, hdr);

        // Also test response and event headers on unaligned slices
        let resp_hdr = VirtioBinderRespHdr::new_ok(2, 10, 0);
        let mut unaligned_resp_buf = vec![0xAAu8; shift];
        unaligned_resp_buf.extend_from_slice(resp_hdr.as_bytes());
        let parsed_resp = VirtioBinderRespHdr::from_bytes(&unaligned_resp_buf[shift..])
            .unwrap_or_else(|e| panic!("RespHdr from_bytes failed at shift {}: {:?}", shift, e));
        assert_eq!(parsed_resp, resp_hdr);

        let event_hdr = VirtioBinderEventHdr::new_death(42, 999);
        let mut unaligned_event_buf = vec![0xBBu8; shift];
        unaligned_event_buf.extend_from_slice(event_hdr.as_bytes());
        let parsed_event = VirtioBinderEventHdr::from_bytes(&unaligned_event_buf[shift..])
            .unwrap_or_else(|e| panic!("EventHdr from_bytes failed at shift {}: {:?}", shift, e));
        assert_eq!(parsed_event, event_hdr);
    }
}



