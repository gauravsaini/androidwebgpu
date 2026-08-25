//! Adversarial stress test suite for `binder_sys` error handling, edge cases, and lifecycles.

use aidl_compat::death::{DeathCallback, DeathRecipient};
use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{Result, Status, StatusCode, STATUS_BAD_VALUE, STATUS_UNKNOWN_TRANSACTION};
use aidl_compat::stub::{Binder, RemoteBinder};
use aidl_compat::traits::{Interface, Remotable};
use binder_rt::types::TransactionCode;
use binder_rt::Parcel;
use binder_sys::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// -----------------------------------------------------------------------------
// Test Stub Helpers
// -----------------------------------------------------------------------------

struct EchoStub {
    call_count: AtomicUsize,
}

impl Interface for EchoStub {
    fn as_binder(&self) -> SpIBinder {
        Binder::new(Self {
            call_count: AtomicUsize::new(0),
        })
    }
}

impl Remotable for EchoStub {
    fn get_class_descriptor() -> &'static str {
        "android.test.IEchoStub"
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        match code {
            1 => {
                let mut offset = 0;
                let _ = data.read_utf16(&mut offset);
                let text = data.read_utf8(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply.write_utf8(text.as_deref()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

// -----------------------------------------------------------------------------
// 1. Invalid Handles and Unknown Transaction Codes
// -----------------------------------------------------------------------------

#[test]
fn test_handle_refcount_ops_on_nonexistent_handles() {
    let mock_driver = Arc::new(MockBinderDriver::new());
    let client_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let client = mock_driver.get_client(client_ps.pid()).unwrap();

    let mut bwr = binder_write_read::new();
    let mut cmds = Vec::new();
    cmds.extend_from_slice(&BC_ACQUIRE.to_ne_bytes());
    cmds.extend_from_slice(&8888u32.to_ne_bytes());
    cmds.extend_from_slice(&BC_INCREFS.to_ne_bytes());
    cmds.extend_from_slice(&8888u32.to_ne_bytes());
    cmds.extend_from_slice(&BC_DECREFS.to_ne_bytes());
    cmds.extend_from_slice(&8888u32.to_ne_bytes());
    cmds.extend_from_slice(&BC_RELEASE.to_ne_bytes());
    cmds.extend_from_slice(&8888u32.to_ne_bytes());

    bwr.write_buffer = cmds.as_ptr() as u64;
    bwr.write_size = cmds.len() as u64;
    let write_res = mock_driver.write_read(&client, &mut bwr);
    assert!(write_res.is_ok(), "Driver must ignore refcount ops on non-existent handles without error");
}

/// Reproduction for Bug 1: Deadlock/Hang on Invalid Handle References
/// `resolve_target` erroneously routes unknown handle to sender process, causing self-deadlock.
#[test]
fn test_reproduce_bug1_invalid_handle_hang() {
    let mock_driver = Arc::new(MockBinderDriver::new());
    let client_ps = ProcessState::init_mock(Arc::clone(&mock_driver));

    let invalid_remote = RemoteBinder::new_with_transport(
        9999,
        0,
        Some("android.test.INonExistent"),
        Arc::new(BinderKernelTransport::with_process(Arc::clone(&client_ps))),
    );

    let data = Parcel::new();
    let mut reply = Parcel::new();

    // Must return Err immediately, but currently hangs forever
    let res = invalid_remote.transact(1, 0, &data, &mut reply);
    assert!(res.is_err(), "Transaction to invalid handle must return Error");
}

/// Reproduction for Bug 2: Silent Error Suppression for Unknown Transaction Codes
/// `handle_incoming_transaction` ignores `transact_res` error and sends empty successful `BC_REPLY`.
#[test]
fn test_reproduce_bug2_unknown_transaction_codes_error_suppression() {
    let mock_driver = Arc::new(MockBinderDriver::new());

    // Server
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let echo_stub = Arc::new(EchoStub { call_count: AtomicUsize::new(0) });
    let cookie = 0xAAAA;
    server_ps.register_service_object(cookie, Binder::new_with_arc(Arc::clone(&echo_stub)));

    let server_ps_clone = Arc::clone(&server_ps);
    let _server_thread = thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(server_ps_clone);
        let _ = ts.enter_looper();
    });

    thread::sleep(Duration::from_millis(40));

    // Client
    let client_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let handle = mock_driver.add_handle_for_client(
        &mock_driver.get_client(client_ps.pid()).unwrap(),
        server_ps.pid(),
        0,
        cookie,
    );

    let remote = RemoteBinder::new_with_transport(
        handle,
        0,
        Some("android.test.IEchoStub"),
        Arc::new(BinderKernelTransport::with_process(Arc::clone(&client_ps))),
    );

    let mut data = Parcel::new();
    data.write_utf16(Some("android.test.IEchoStub")).unwrap();
    data.write_utf8(Some("ping")).unwrap();

    let mut unk_reply = Parcel::new();
    let unk_res = remote.transact(0xDEADBEEF, 0, &data, &mut unk_reply);
    assert!(unk_res.is_err(), "Unknown transaction code must error");
    if let Err(status) = unk_res {
        assert_eq!(
            status.status_code(),
            StatusCode::UnknownTransaction,
            "Must return StatusCode::UnknownTransaction"
        );
    }
}

// -----------------------------------------------------------------------------
// 2. Corrupted Parcel Buffers & Driver Resilience
// -----------------------------------------------------------------------------

#[test]
fn test_corrupted_parcel_buffers_and_malformed_driver_writes() {
    let mock_driver = Arc::new(MockBinderDriver::new());
    let client_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let client = mock_driver.get_client(client_ps.pid()).unwrap();

    // 1. Partial write buffer (< 4 bytes)
    let short_bytes = [0x01, 0x02, 0x03];
    let mut bwr = binder_write_read::new();
    bwr.write_buffer = short_bytes.as_ptr() as u64;
    bwr.write_size = short_bytes.len() as u64;
    let res = mock_driver.write_read(&client, &mut bwr);
    assert!(res.is_ok(), "Driver should safely stop parsing short write buffers");
    assert_eq!(bwr.write_consumed, 0, "No full command was consumed");

    // 2. BC_TRANSACTION with truncated BinderTransactionData (< 40 bytes)
    let mut bad_tx_bytes = Vec::new();
    bad_tx_bytes.extend_from_slice(&BC_TRANSACTION.to_ne_bytes());
    bad_tx_bytes.extend_from_slice(&[0u8; 10]); // incomplete struct
    let mut bwr2 = binder_write_read::new();
    bwr2.write_buffer = bad_tx_bytes.as_ptr() as u64;
    bwr2.write_size = bad_tx_bytes.len() as u64;
    let res2 = mock_driver.write_read(&client, &mut bwr2);
    assert!(res2.is_err(), "Driver must reject truncated BC_TRANSACTION payload");

    // 3. BC_REPLY with truncated BinderTransactionData
    let mut bad_reply_bytes = Vec::new();
    bad_reply_bytes.extend_from_slice(&BC_REPLY.to_ne_bytes());
    bad_reply_bytes.extend_from_slice(&[0u8; 8]);
    let mut bwr3 = binder_write_read::new();
    bwr3.write_buffer = bad_reply_bytes.as_ptr() as u64;
    bwr3.write_size = bad_reply_bytes.len() as u64;
    let res3 = mock_driver.write_read(&client, &mut bwr3);
    assert!(res3.is_err(), "Driver must reject truncated BC_REPLY payload");

    // 4. Mmap boundary violations
    let mmap = client_ps.mmap_region();
    let out_of_bounds_ptr = mmap.base_ptr() + (mmap.size() as u64) + 1000;
    assert_eq!(mmap.read_bytes(out_of_bounds_ptr, 64), Err(MmapError::OutOfBounds(out_of_bounds_ptr, mmap.base_ptr(), mmap.base_ptr() + mmap.size() as u64)));
    assert_eq!(mmap.write_bytes(out_of_bounds_ptr, &[1, 2, 3]), Err(MmapError::OutOfBounds(out_of_bounds_ptr, mmap.base_ptr(), mmap.base_ptr() + mmap.size() as u64)));

    // Double free or freeing unallocated pointer
    let unallocated_ptr = mmap.base_ptr() + 64;
    assert_eq!(mmap.free_buffer(unallocated_ptr, 64), Err(MmapError::InvalidBuffer(unallocated_ptr)));
}

// -----------------------------------------------------------------------------
// 3. Dead Recipient Callbacks and Lifecycle Cleanup
// -----------------------------------------------------------------------------

#[test]
fn test_dead_recipient_lifecycle_and_unregistration() {
    let mock_driver = Arc::new(MockBinderDriver::new());

    // 1. Server process
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let server_pid = server_ps.pid();

    // 2. Client process
    let client_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let handle = mock_driver.add_handle_for_client(
        &mock_driver.get_client(client_ps.pid()).unwrap(),
        server_pid,
        0,
        0x1111,
    );

    let death_counter_1 = Arc::new(AtomicUsize::new(0));
    let dc1 = Arc::clone(&death_counter_1);
    let recipient_1: Arc<dyn DeathRecipient> = Arc::new(DeathCallback(move || {
        dc1.fetch_add(1, Ordering::SeqCst);
    }));

    let death_counter_2 = Arc::new(AtomicUsize::new(0));
    let dc2 = Arc::clone(&death_counter_2);
    let recipient_2: Arc<dyn DeathRecipient> = Arc::new(DeathCallback(move || {
        dc2.fetch_add(1, Ordering::SeqCst);
    }));

    let cookie_1 = 0xAAAA1111;
    let cookie_2 = 0xBBBB2222;

    client_ps.register_death_recipient(cookie_1, Arc::clone(&recipient_1));
    client_ps.register_death_recipient(cookie_2, Arc::clone(&recipient_2));

    // Register both with mock driver
    let client = mock_driver.get_client(client_ps.pid()).unwrap();
    let mut req = Vec::new();
    req.extend_from_slice(&BC_REQUEST_DEATH_NOTIFICATION.to_ne_bytes());
    req.extend_from_slice(bytemuck::bytes_of(&BinderHandleCookie { handle, padding: 0, cookie: cookie_1 }));
    req.extend_from_slice(&BC_REQUEST_DEATH_NOTIFICATION.to_ne_bytes());
    req.extend_from_slice(bytemuck::bytes_of(&BinderHandleCookie { handle, padding: 0, cookie: cookie_2 }));

    let mut bwr = binder_write_read::new();
    bwr.write_buffer = req.as_ptr() as u64;
    bwr.write_size = req.len() as u64;
    mock_driver.write_read(&client, &mut bwr).unwrap();

    // Unregister cookie_1 before death
    client_ps.unregister_death_recipient(cookie_1);
    let mut clear_req = Vec::new();
    clear_req.extend_from_slice(&BC_CLEAR_DEATH_NOTIFICATION.to_ne_bytes());
    clear_req.extend_from_slice(bytemuck::bytes_of(&BinderHandleCookie { handle, padding: 0, cookie: cookie_1 }));

    let mut clear_bwr = binder_write_read::new();
    clear_bwr.write_buffer = clear_req.as_ptr() as u64;
    clear_bwr.write_size = clear_req.len() as u64;
    mock_driver.write_read(&client, &mut clear_bwr).unwrap();

    // Drain BR_CLEAR_DEATH_NOTIFICATION_DONE synchronously
    {
        let mut ts_drain = IPCThreadState::with_process(Arc::clone(&client_ps));
        let _ = ts_drain.process_pending_commands();
    }

    // Start client looper thread to wait for BR_DEAD_BINDER
    let client_ps_clone = Arc::clone(&client_ps);
    let looper_thread = thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(client_ps_clone);
        let _ = ts.process_pending_commands();
    });

    thread::sleep(Duration::from_millis(50));

    // Trigger process death for server
    mock_driver.trigger_process_death(server_pid);

    let _ = looper_thread.join();

    // Recipient 1 was unregistered -> count must be 0
    assert_eq!(death_counter_1.load(Ordering::SeqCst), 0, "Unregistered death recipient must not receive callback");
    // Recipient 2 remained registered -> count must be 1
    assert_eq!(death_counter_2.load(Ordering::SeqCst), 1, "Registered death recipient must receive callback");
}

#[test]
fn test_local_service_object_lifecycle_cleanup() {
    let mock_driver = Arc::new(MockBinderDriver::new());
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));

    let echo_stub = Arc::new(EchoStub { call_count: AtomicUsize::new(0) });
    let cookie = 0x7777;

    // 1. Register local service object
    server_ps.register_service_object(cookie, Binder::new_with_arc(Arc::clone(&echo_stub)));
    assert!(server_ps.get_service_object(cookie).is_some());

    // 2. Unregister local service object
    server_ps.unregister_service_object(cookie);
    assert!(server_ps.get_service_object(cookie).is_none(), "Unregistered object must return None");
}

// -----------------------------------------------------------------------------
// 4. ServiceManager Collisions, Empty Queries, and Edge Cases
// -----------------------------------------------------------------------------

#[test]
fn test_service_manager_collision_resolution_and_empty_queries() {
    let mock_driver = Arc::new(MockBinderDriver::new());

    // 1. Setup ServiceManager on handle 0
    let sm_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let sm_cookie = 0x534D;
    sm_ps.register_service_object(sm_cookie, Binder::new(MockServiceManager::new()));
    mock_driver.set_context_manager(sm_ps.pid(), 0, sm_cookie);

    let sm_ps_clone = Arc::clone(&sm_ps);
    let _sm_thread = thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(sm_ps_clone);
        let _ = ts.enter_looper();
    });

    thread::sleep(Duration::from_millis(40));

    // ServiceManager client
    let client_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let sm_client = ServiceManagerClient::with_binder(RemoteBinder::new_with_transport(
        0,
        0,
        Some(SERVICE_MANAGER_DESCRIPTOR),
        Arc::new(BinderKernelTransport::with_process(Arc::clone(&client_ps))),
    ));

    // Test A: Empty query on uninitialized SM
    let non_existent = sm_client.get_service("android.non.existent.Service").expect("Query should succeed");
    assert!(non_existent.is_none(), "Non-existent service must return None");

    let check_non_existent = sm_client.check_service("android.non.existent.Service").expect("Check should succeed");
    assert!(check_non_existent.is_none(), "Check non-existent service must return None");

    // Empty string query
    let empty_query = sm_client.get_service("").expect("Empty service name query should succeed");
    assert!(empty_query.is_none(), "Empty service name must return None");

    let empty_list = sm_client.list_services(DUMP_FLAG_PRIORITY_ALL).expect("list_services should succeed");
    assert_eq!(empty_list.len(), 0, "Initial service list must be empty");

    // Test B: Collision resolution (Overwriting registration)
    let stub_v1 = Binder::new(EchoStub { call_count: AtomicUsize::new(0) });
    let stub_v2 = Binder::new(EchoStub { call_count: AtomicUsize::new(0) });

    let svc_name = "android.test.OverwrittenService";

    // Add version 1
    sm_client.add_service(svc_name, stub_v1, false, DUMP_FLAG_PRIORITY_DEFAULT).expect("add v1 failed");
    let res_v1 = sm_client.get_service(svc_name).expect("get v1 failed");
    assert!(res_v1.is_some(), "Service v1 should exist");

    // Add version 2 (overwrite collision)
    sm_client.add_service(svc_name, stub_v2, false, DUMP_FLAG_PRIORITY_HIGH).expect("add v2 collision failed");
    let res_v2 = sm_client.get_service(svc_name).expect("get v2 failed");
    assert!(res_v2.is_some(), "Service v2 should exist after overwrite collision");

    // Check list services contains the service exactly once
    let list_after = sm_client.list_services(DUMP_FLAG_PRIORITY_ALL).expect("list_services after collision failed");
    assert_eq!(list_after.len(), 1, "List services must have exactly 1 entry after overwrite");
    assert_eq!(list_after[0], svc_name);

    // Test C: is_declared on various inputs
    assert!(sm_client.is_declared("android.hardware.sensors.ISensors/default").unwrap());
    assert!(sm_client.is_declared("").unwrap());
    assert!(sm_client.is_declared("!@#$%^&*()").unwrap());
}

// -----------------------------------------------------------------------------
// 5. Mmap Stress, Fragmentation & OOM Resilience
// -----------------------------------------------------------------------------

#[test]
fn test_mmap_stress_exhaustion_and_coalescing() {
    let region = BinderMmapRegion::new_simulated(BINDER_MIN_MMAP_SIZE);
    let block_size = 4096;
    let num_blocks = region.size() / block_size;
    let mut ptrs = Vec::new();

    // 1. Allocate until full
    for _ in 0..num_blocks {
        let ptr = region.allocate_buffer(block_size).expect("Block allocation should succeed");
        ptrs.push(ptr);
    }

    // 2. Next allocation must fail with OutOfMemory
    assert_eq!(region.allocate_buffer(block_size), Err(MmapError::OutOfMemory));

    // 3. Free even blocks (create fragmentation holes)
    for i in (0..ptrs.len()).step_by(2) {
        region.free_buffer(ptrs[i], block_size).expect("Free even block failed");
    }

    // 4. Free odd blocks (coalescing test)
    for i in (1..ptrs.len()).step_by(2) {
        region.free_buffer(ptrs[i], block_size).expect("Free odd block failed");
    }

    // 5. After full coalescing, single allocation of entire size minus alignment should succeed
    let huge_alloc = region.allocate_buffer(region.size() - 64);
    assert!(huge_alloc.is_ok(), "Fully coalesced region must allow large contiguous allocation");
}

// -----------------------------------------------------------------------------
// 6. Concurrent ServiceManager Stress
// -----------------------------------------------------------------------------

/// Reproduction for Bug 3: Concurrent ServiceManager Reply Misrouting
/// `MockClientProcess::transaction_stack` is a shared process LIFO stack, misrouting replies under concurrent calls.
#[test]
fn test_reproduce_bug3_service_manager_concurrent_reply_misrouting() {
    let mock_driver = Arc::new(MockBinderDriver::new());

    let sm_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let sm_cookie = 0x534D;
    sm_ps.register_service_object(sm_cookie, Binder::new(MockServiceManager::new()));
    mock_driver.set_context_manager(sm_ps.pid(), 0, sm_cookie);

    let sm_ps_clone = Arc::clone(&sm_ps);
    let _sm_thread = thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(sm_ps_clone);
        let _ = ts.enter_looper();
    });

    thread::sleep(Duration::from_millis(40));

    // Spawn 8 client threads querying and registering services concurrently
    let mut handles = Vec::new();
    for thread_idx in 0..8 {
        let driver_clone = Arc::clone(&mock_driver);
        let h = thread::spawn(move || {
            let client_ps = ProcessState::init_mock(driver_clone);
            let sm_client = ServiceManagerClient::with_binder(RemoteBinder::new_with_transport(
                0,
                0,
                Some(SERVICE_MANAGER_DESCRIPTOR),
                Arc::new(BinderKernelTransport::with_process(Arc::clone(&client_ps))),
            ));

            let svc_name = format!("android.test.ConcurrentService_{}", thread_idx);
            let stub = Binder::new(EchoStub { call_count: AtomicUsize::new(0) });

            sm_client.add_service(&svc_name, stub, false, DUMP_FLAG_PRIORITY_DEFAULT).expect("add_service failed");
            let found = sm_client.get_service(&svc_name).expect("get_service failed");
            assert!(found.is_some(), "Concurrent service must be found");
        });
        handles.push(h);
    }

    for h in handles {
        h.join().expect("Worker thread panicked");
    }
}
