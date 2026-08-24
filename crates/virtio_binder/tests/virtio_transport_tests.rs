//! Integration test suite for `virtio_binder`.
//!
//! Covers:
//! 1. Synthetic guest transaction round-trip across virtqueue to host device and back.
//! 2. Concurrent transactions matched via `msg_id`.
//! 3. Ping transaction handling.
//! 4. Death event delivery on Queue 1.
//! 5. Malformed/truncated packet rejection.
//! 6. Reference counting and lifecycle management.

use aidl_compat::status::{
    ExceptionCode, Result as AidlResult, Status, STATUS_BAD_VALUE, STATUS_DEAD_OBJECT, STATUS_OK,
};
use aidl_compat::stub::Binder;
use aidl_compat::traits::{IBinder, Remotable};
use binder_rt::types::TransactionCode;
use binder_rt::wire::TF_ONE_WAY;
use binder_rt::Parcel;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use virtio_binder::*;

// -----------------------------------------------------------------------------
// Test AIDL Remotable Service
// -----------------------------------------------------------------------------

struct TestCalcService {
    alive: AtomicBool,
    one_way_counter: AtomicUsize,
}

impl TestCalcService {
    fn new() -> Self {
        Self {
            alive: AtomicBool::new(true),
            one_way_counter: AtomicUsize::new(0),
        }
    }
}

impl Remotable for TestCalcService {
    fn get_class_descriptor() -> &'static str {
        "android.os.ITestCalcService"
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

        match code {
            // Opcode 1: Add two integers: a (i32) + b (i32) -> sum (i32)
            1 => {
                let mut offset = 0;
                let a = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let b = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply
                    .write_i32(a + b)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }

            // Opcode 2: Echo UTF-8 string: str -> "Echo: {str}"
            2 => {
                let mut offset = 0;
                let msg = data
                    .read_utf8(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?
                    .unwrap_or_default();
                let res = format!("Echo: {}", msg);
                reply
                    .write_utf8(Some(&res))
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }

            // Opcode 3: One-way notify
            3 => {
                self.one_way_counter.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }

            // Opcode 4: Error simulation
            4 => Err(Status::new_exception(
                ExceptionCode::IllegalArgument,
                Some("Invalid argument test"),
            )),

            _ => Err(Status::from_status(STATUS_BAD_VALUE)),
        }
    }
}

// -----------------------------------------------------------------------------
// Test 1: Synthetic Guest Transaction Round-Trip Across Virtqueue
// -----------------------------------------------------------------------------

#[test]
fn test_virtio_transaction_roundtrip_direct_backend() {
    let host_device = Arc::new(VirtioBinderDevice::new());
    let calc_service = Arc::new(TestCalcService::new());
    let binder_stub = Binder::new_with_arc(Arc::clone(&calc_service));
    host_device.register_binder(1, binder_stub);

    let transport = Arc::new(GuestVirtioTransport::new_with_device(Arc::clone(&host_device)));
    let proxy = transport.create_remote_binder(1, 0, Some("android.os.ITestCalcService"));

    // Test addition (10 + 25 = 35)
    let mut req_parcel = Parcel::new();
    req_parcel.write_i32(10).unwrap();
    req_parcel.write_i32(25).unwrap();

    let mut reply_parcel = Parcel::new();
    proxy
        .transact(1, 0, &req_parcel, &mut reply_parcel)
        .expect("Transact should succeed");

    let mut offset = 0;
    let sum = reply_parcel.read_i32(&mut offset).unwrap();
    assert_eq!(sum, 35);

    // Test string echo
    let mut str_req = Parcel::new();
    str_req.write_utf8(Some("AndroidWebGPU")).unwrap();

    let mut str_reply = Parcel::new();
    proxy
        .transact(2, 0, &str_req, &mut str_reply)
        .expect("Echo transact should succeed");

    let mut str_offset = 0;
    let echo_str = str_reply.read_utf8(&mut str_offset).unwrap();
    assert_eq!(echo_str, Some("Echo: AndroidWebGPU".to_string()));

    // Test one-way transaction
    assert_eq!(calc_service.one_way_counter.load(Ordering::SeqCst), 0);
    let oneway_req = Parcel::new();
    let mut oneway_reply = Parcel::new();
    proxy
        .transact(3, TF_ONE_WAY, &oneway_req, &mut oneway_reply)
        .expect("One-way transact should succeed");
    assert_eq!(calc_service.one_way_counter.load(Ordering::SeqCst), 1);
}

#[test]
fn test_virtio_transaction_roundtrip_virtqueue_chain_backend() {
    let host_device = Arc::new(VirtioBinderDevice::new());
    let calc_service = Arc::new(TestCalcService::new());
    let binder_stub = Binder::new_with_arc(Arc::clone(&calc_service));
    host_device.register_binder(2, binder_stub);

    // Use descriptor chain backend with 4KB reply buffer capacity
    let transport = Arc::new(GuestVirtioTransport::new_with_virtqueue(
        Arc::clone(&host_device),
        4096,
    ));
    let proxy = transport.create_remote_binder(2, 0, Some("android.os.ITestCalcService"));

    let mut req_parcel = Parcel::new();
    req_parcel.write_i32(100).unwrap();
    req_parcel.write_i32(250).unwrap();

    let mut reply_parcel = Parcel::new();
    proxy
        .transact(1, 0, &req_parcel, &mut reply_parcel)
        .expect("Transact across virtqueue chain should succeed");

    let mut offset = 0;
    let sum = reply_parcel.read_i32(&mut offset).unwrap();
    assert_eq!(sum, 350);
}

// -----------------------------------------------------------------------------
// Test 2: Concurrent Transactions Matched via msg_id
// -----------------------------------------------------------------------------

#[test]
fn test_concurrent_transactions_msg_id_matching() {
    let host_device = Arc::new(VirtioBinderDevice::new());
    let calc_service = Arc::new(TestCalcService::new());
    let binder_stub = Binder::new_with_arc(Arc::clone(&calc_service));
    host_device.register_binder(10, binder_stub);

    let transport = Arc::new(GuestVirtioTransport::new_with_device(Arc::clone(&host_device)));

    let mut handles = Vec::new();
    let num_threads = 16;
    let iterations_per_thread = 50;

    for thread_idx in 0..num_threads {
        let t_transport = Arc::clone(&transport);
        let handle = thread::spawn(move || {
            let proxy = t_transport.create_remote_binder(10, 0, Some("android.os.ITestCalcService"));
            for i in 0..iterations_per_thread {
                let a = thread_idx * 1000 + i;
                let b = i * 7;
                let expected = a + b;

                let mut req = Parcel::new();
                req.write_i32(a).unwrap();
                req.write_i32(b).unwrap();

                let mut reply = Parcel::new();
                proxy
                    .transact(1, 0, &req, &mut reply)
                    .unwrap_or_else(|e| panic!("Thread {} iteration {} failed: {:?}", thread_idx, i, e));

                let mut offset = 0;
                let actual = reply.read_i32(&mut offset).unwrap();
                assert_eq!(
                    actual, expected,
                    "Mismatch in thread {} on iteration {}",
                    thread_idx, i
                );
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Thread joined successfully");
    }
}

// -----------------------------------------------------------------------------
// Test 3: Ping Transaction Handling
// -----------------------------------------------------------------------------

#[test]
fn test_ping_transaction_handling() {
    let host_device = Arc::new(VirtioBinderDevice::new());
    let raw_binder = Arc::new(Binder::new_raw(TestCalcService::new()));
    host_device.register_service(42, Arc::clone(&raw_binder) as Arc<dyn IBinder>);

    let transport = Arc::new(GuestVirtioTransport::new_with_device(Arc::clone(&host_device)));

    // Ping registered alive service
    let ping_res = transport.ping(42);
    assert!(ping_res.is_ok(), "Ping on registered service must succeed");

    // Ping general host / service manager
    let ping_sm = transport.ping(0);
    assert!(ping_sm.is_ok(), "Ping on ServiceManager (0) must succeed");

    // Ping dead service
    raw_binder.trigger_death();
    let ping_dead = transport.ping(42);
    assert!(
        ping_dead.is_err(),
        "Ping on dead service must return error status"
    );
    assert_eq!(
        ping_dead.unwrap_err().status_code(),
        binder_rt::status::StatusCode::DeadObject
    );

    // Ping nonexistent handle
    let ping_nonexistent = transport.ping(999);
    assert!(
        ping_nonexistent.is_err(),
        "Ping on nonexistent handle must return error"
    );
    assert_eq!(
        ping_nonexistent.unwrap_err().status_code(),
        binder_rt::status::StatusCode::DeadObject
    );
}

// -----------------------------------------------------------------------------
// Test 4: Death Event Delivery on Queue 1
// -----------------------------------------------------------------------------

#[test]
fn test_death_event_delivery_queue_1() {
    let host_device = Arc::new(VirtioBinderDevice::new());
    let calc_service = Arc::new(TestCalcService::new());
    let binder_stub = Binder::new_with_arc(Arc::clone(&calc_service));
    host_device.register_binder(55, binder_stub);

    let transport = Arc::new(GuestVirtioTransport::new_with_device(Arc::clone(&host_device)));

    let cookie_1 = 0x1122334455667788u64;
    let cookie_2 = 0xAABBCCDDEEFF0011u64;

    // Link two death cookies
    transport.link_death(55, cookie_1).expect("Link death 1 ok");
    transport.link_death(55, cookie_2).expect("Link death 2 ok");

    // Initial event queue should be empty
    assert_eq!(transport.drain_events().len(), 0);

    // Trigger death on host device
    host_device.trigger_death(55);

    // Drain events from Queue 1
    let events = transport.drain_events();
    assert_eq!(events.len(), 2, "Expected exactly 2 death events");

    assert_eq!(events[0].event_type, EVENT_TYPE_DEATH);
    assert_eq!(events[0].target_handle, 55);
    assert_eq!(events[0].cookie, cookie_1);

    assert_eq!(events[1].event_type, EVENT_TYPE_DEATH);
    assert_eq!(events[1].target_handle, 55);
    assert_eq!(events[1].cookie, cookie_2);

    // Queue 1 is now drained
    assert_eq!(transport.drain_events().len(), 0);

    // Test unlink death
    host_device.register_binder(56, Binder::new_with_arc(Arc::new(TestCalcService::new())));
    transport.link_death(56, 0x999).unwrap();
    transport.unlink_death(56, 0x999).unwrap();
    host_device.trigger_death(56);
    assert_eq!(transport.drain_events().len(), 0);
}

// -----------------------------------------------------------------------------
// Test 5: Malformed & Truncated Packet Rejection
// -----------------------------------------------------------------------------

#[test]
fn test_malformed_truncated_packet_rejection() {
    let host_device = VirtioBinderDevice::new();

    // 1. Packet too short for request header
    let short_bytes = vec![0u8; 16]; // ReqHdr is 48 bytes
    let err_short = host_device.process_packet(&short_bytes);
    assert!(err_short.is_err());
    match err_short.unwrap_err() {
        DeviceError::Protocol(ProtocolError::PacketTooShort { expected, actual }) => {
            assert_eq!(expected, std::mem::size_of::<VirtioBinderReqHdr>());
            assert_eq!(actual, 16);
        }
        other => panic!("Unexpected error type: {:?}", other),
    }

    // 2. Payload size exceeds available buffer
    let mut hdr = VirtioBinderReqHdr::new_transact(1, 1, 1, 0, 0, 500, 0);
    let mut pkt = hdr.as_bytes().to_vec();
    pkt.extend_from_slice(&[0u8; 50]); // Only 50 bytes provided instead of 500
    let err_payload = host_device.process_packet(&pkt);
    assert!(err_payload.is_err());
    match err_payload.unwrap_err() {
        DeviceError::Protocol(ProtocolError::PayloadSizeMismatch {
            data_size,
            offsets_size,
            remaining,
        }) => {
            assert_eq!(data_size, 500);
            assert_eq!(offsets_size, 0);
            assert_eq!(remaining, 50);
        }
        other => panic!("Unexpected error type: {:?}", other),
    }

    // 3. Offsets size not aligned to 8 bytes
    hdr.data_size = 0;
    hdr.offsets_size = 7; // Invalid: not multiple of 8
    let mut pkt_offsets = hdr.as_bytes().to_vec();
    pkt_offsets.extend_from_slice(&[0u8; 7]);
    let err_offsets = host_device.process_packet(&pkt_offsets);
    assert!(err_offsets.is_err());
    match err_offsets.unwrap_err() {
        DeviceError::Protocol(ProtocolError::InvalidOffsetsSize(7)) => {}
        other => panic!("Unexpected error type: {:?}", other),
    }

    // 4. Response header deserialization on truncated bytes
    let resp_short = vec![0u8; 10]; // RespHdr is 32 bytes
    assert!(VirtioBinderResponse::deserialize(&resp_short).is_err());

    // 5. Event header deserialization on truncated bytes
    let event_short = vec![0u8; 8]; // EventHdr is 16 bytes
    assert!(VirtioBinderEventHdr::from_bytes(&event_short).is_err());
}

// -----------------------------------------------------------------------------
// Test 6: Reference Counting & Lifecycle Management
// -----------------------------------------------------------------------------

#[test]
fn test_handle_refcounting_and_lifecycle() {
    let host_device = Arc::new(VirtioBinderDevice::new());
    let calc_service = Arc::new(TestCalcService::new());
    let binder_stub = Binder::new_with_arc(Arc::clone(&calc_service));
    host_device.register_binder(77, binder_stub);

    let transport = Arc::new(GuestVirtioTransport::new_with_device(Arc::clone(&host_device)));

    assert_eq!(host_device.get_ref_count(77), Some(1));

    // Acquire ref
    transport.acquire_handle(77).expect("Acquire handle ok");
    assert_eq!(host_device.get_ref_count(77), Some(2));

    transport.acquire_handle(77).expect("Acquire handle 2 ok");
    assert_eq!(host_device.get_ref_count(77), Some(3));

    // Release ref
    transport.release_handle(77).expect("Release handle 1 ok");
    assert_eq!(host_device.get_ref_count(77), Some(2));

    transport.release_handle(77).expect("Release handle 2 ok");
    assert_eq!(host_device.get_ref_count(77), Some(1));

    // Final release -> handle removed
    transport.release_handle(77).expect("Final release handle ok");
    assert_eq!(host_device.get_ref_count(77), None);
    assert!(host_device.get_service(77).is_none());

    // Subsequent transaction on released handle returns DEAD_OBJECT
    let proxy = transport.create_remote_binder(77, 0, None);
    let req = Parcel::new();
    let mut reply = Parcel::new();
    let res = proxy.transact(1, 0, &req, &mut reply);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().status_code(), binder_rt::status::StatusCode::DeadObject);
}

// -----------------------------------------------------------------------------
// Test 7: VirtQueueChain Multi-buffer Split Read & Write
// -----------------------------------------------------------------------------

#[test]
fn test_virtqueue_chain_split_buffers() {
    let req = VirtioBinderRequest::new_ping(12345, 0);
    let req_bytes = req.serialize();

    // Split request across two readable descriptors
    let split_pos = req_bytes.len() / 2;
    let part1 = req_bytes[..split_pos].to_vec();
    let part2 = req_bytes[split_pos..].to_vec();

    // Split reply space across three writable descriptors (32 bytes each)
    let writable = vec![vec![0u8; 32], vec![0u8; 32], vec![0u8; 32]];

    let mut chain = VirtQueueChain::new(1, vec![part1, part2], writable);
    assert_eq!(chain.readable_len(), req_bytes.len());
    assert_eq!(chain.writable_capacity(), 96);

    let host_device = VirtioBinderDevice::new();
    let written = host_device
        .process_virtqueue_chain(&mut chain)
        .expect("Process chain should succeed");
    assert!(written > 0);

    let reply_bytes = chain.take_written_data();
    let resp = VirtioBinderResponse::deserialize(&reply_bytes).expect("Deserialize response");
    assert_eq!(resp.hdr.msg_id, 12345);
    assert_eq!(resp.hdr.status, STATUS_OK);
}
