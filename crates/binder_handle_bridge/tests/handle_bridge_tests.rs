//! Comprehensive integration and stress tests for `binder_handle_bridge`.

use aidl_compat::{DeathRecipient, IBinder, Result as AidlResult};
use binder_handle_bridge::{BridgeError, ClientId, DeathNotification, HandleBridge};
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::Parcel;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// Mock AIDL service tracking method invocations and exact destruction via drop hook.
struct MockAidlService {
    descriptor: &'static str,
    call_count: AtomicUsize,
    drop_flag: Arc<AtomicBool>,
}

impl MockAidlService {
    fn new(descriptor: &'static str, drop_flag: Arc<AtomicBool>) -> Self {
        Self {
            descriptor,
            call_count: AtomicUsize::new(0),
            drop_flag,
        }
    }
}

impl Drop for MockAidlService {
    fn drop(&mut self) {
        self.drop_flag.store(true, Ordering::SeqCst);
    }
}

impl IBinder for MockAidlService {
    fn transact(
        &self,
        _code: TransactionCode,
        _flags: TransactionFlags,
        _data: &Parcel,
        _reply: &mut Parcel,
    ) -> AidlResult<()> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn link_to_death(&self, _recipient: Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn unlink_to_death(&self, _recipient: &Arc<dyn DeathRecipient>) -> AidlResult<()> {
        Ok(())
    }

    fn get_class_descriptor(&self) -> Option<&'static str> {
        Some(self.descriptor)
    }
}

// -----------------------------------------------------------------------------
// Lifecycle & Reference Counting Tests
// -----------------------------------------------------------------------------

#[test]
fn test_handle_acquisition_and_release_lifecycle() {
    let bridge = HandleBridge::new();
    let client: ClientId = 100;
    let dropped = Arc::new(AtomicBool::new(false));

    let service: Arc<dyn IBinder> =
        Arc::new(MockAidlService::new("android.os.ITestService", Arc::clone(&dropped)));

    // Initial registration: allocates handle, strong count = 1
    let handle = bridge.register_service(client, "android.os.ITestService", Arc::clone(&service));
    assert_eq!(bridge.get_strong_count(client, handle), Some(1));
    assert_eq!(bridge.handle_count(client), 1);
    assert_eq!(bridge.total_handles(), 1);
    assert!(!dropped.load(Ordering::SeqCst));

    // Retrieve and verify service descriptor
    assert_eq!(
        bridge.get_descriptor(client, handle),
        Some("android.os.ITestService".to_string())
    );
    let retrieved = bridge.get_service(client, handle).expect("Service should exist");
    assert_eq!(retrieved.get_class_descriptor(), Some("android.os.ITestService"));

    // Acquire additional references
    bridge.acquire_ref(client, handle, 3).unwrap();
    assert_eq!(bridge.get_strong_count(client, handle), Some(4));
    assert!(!dropped.load(Ordering::SeqCst));

    // Release 2 references -> count = 2 (not dropped)
    let is_dropped = bridge.release_ref(client, handle, 2).unwrap();
    assert!(!is_dropped);
    assert_eq!(bridge.get_strong_count(client, handle), Some(2));
    assert!(!dropped.load(Ordering::SeqCst));

    // Release 1 reference -> count = 1 (not dropped)
    let is_dropped = bridge.release_ref(client, handle, 1).unwrap();
    assert!(!is_dropped);
    assert_eq!(bridge.get_strong_count(client, handle), Some(1));
    assert!(!dropped.load(Ordering::SeqCst));

    // Drop caller's local Arc reference to ensure only the bridge holds it
    drop(service);
    drop(retrieved);
    assert!(!dropped.load(Ordering::SeqCst));

    // Final release: count reaches 0 -> handle removed, host service dropped immediately
    let is_dropped = bridge.release_ref(client, handle, 1).unwrap();
    assert!(is_dropped);
    assert!(dropped.load(Ordering::SeqCst));
    assert!(bridge.get_service(client, handle).is_none());
    assert_eq!(bridge.handle_count(client), 0);
    assert_eq!(bridge.total_handles(), 0);

    // Further operations on the dropped handle fail
    assert_eq!(
        bridge.acquire_ref(client, handle, 1),
        Err(BridgeError::HandleNotFound(handle, client))
    );
    assert_eq!(
        bridge.release_ref(client, handle, 1),
        Err(BridgeError::HandleNotFound(handle, client))
    );
}

#[test]
fn test_weak_refcount_lifecycle() {
    let bridge = HandleBridge::new();
    let client: ClientId = 101;
    let dropped = Arc::new(AtomicBool::new(false));
    let service: Arc<dyn IBinder> =
        Arc::new(MockAidlService::new("android.os.IWeakTest", Arc::clone(&dropped)));

    let handle = bridge.register_service(client, "android.os.IWeakTest", service);
    assert_eq!(bridge.get_weak_count(client, handle), Some(0));

    bridge.acquire_weak_ref(client, handle, 5).unwrap();
    assert_eq!(bridge.get_weak_count(client, handle), Some(5));

    let weak_zero = bridge.release_weak_ref(client, handle, 3).unwrap();
    assert!(!weak_zero);
    assert_eq!(bridge.get_weak_count(client, handle), Some(2));

    let weak_zero = bridge.release_weak_ref(client, handle, 2).unwrap();
    assert!(weak_zero);
    assert_eq!(bridge.get_weak_count(client, handle), Some(0));
}

#[test]
fn test_refcount_underflow_error() {
    let bridge = HandleBridge::new();
    let client: ClientId = 102;
    let dropped = Arc::new(AtomicBool::new(false));
    let service: Arc<dyn IBinder> =
        Arc::new(MockAidlService::new("android.os.IUnderflow", Arc::clone(&dropped)));

    let handle = bridge.register_service(client, "android.os.IUnderflow", service);
    assert_eq!(bridge.get_strong_count(client, handle), Some(1));

    // Attempt to release 5 refs when count is 1 -> returns InvalidRefCount error
    let err = bridge.release_ref(client, handle, 5);
    assert_eq!(err, Err(BridgeError::InvalidRefCount(handle, client)));

    // Verify strong count was NOT modified or corrupted
    assert_eq!(bridge.get_strong_count(client, handle), Some(1));
    assert!(!dropped.load(Ordering::SeqCst));
}

// -----------------------------------------------------------------------------
// Multi-Hop Handle Transfer Tests
// -----------------------------------------------------------------------------

#[test]
fn test_multi_hop_handle_transfer_preserves_service_alive() {
    let bridge = HandleBridge::new();
    let client_a: ClientId = 1;
    let client_b: ClientId = 2;
    let client_c: ClientId = 3;
    let dropped = Arc::new(AtomicBool::new(false));

    let service: Arc<dyn IBinder> =
        Arc::new(MockAidlService::new("android.gui.ISurfaceComposer", Arc::clone(&dropped)));

    // Client A registers service
    let handle_a =
        bridge.register_service(client_a, "android.gui.ISurfaceComposer", Arc::clone(&service));
    drop(service); // Release local arc
    assert!(!dropped.load(Ordering::SeqCst));

    // Client A transfers handle to Client B
    let handle_b = bridge.transfer_handle(client_a, client_b, handle_a).unwrap();
    assert_eq!(bridge.handle_count(client_a), 1);
    assert_eq!(bridge.handle_count(client_b), 1);
    assert_eq!(bridge.total_handles(), 2);
    assert_eq!(bridge.get_strong_count(client_b, handle_b), Some(1));

    // Client B transfers handle to Client C
    let handle_c = bridge.transfer_handle(client_b, client_c, handle_b).unwrap();
    assert_eq!(bridge.handle_count(client_c), 1);
    assert_eq!(bridge.total_handles(), 3);

    // Verify all clients can transact with the same host service
    let p_in = Parcel::new();
    let mut p_out = Parcel::new();
    bridge.get_service(client_a, handle_a).unwrap().transact(1, 0, &p_in, &mut p_out).unwrap();
    bridge.get_service(client_b, handle_b).unwrap().transact(1, 0, &p_in, &mut p_out).unwrap();
    bridge.get_service(client_c, handle_c).unwrap().transact(1, 0, &p_in, &mut p_out).unwrap();

    // Client A releases its handle -> drops handle A, service STILL ALIVE (held by B and C)
    let a_dropped = bridge.release_ref(client_a, handle_a, 1).unwrap();
    assert!(a_dropped);
    assert!(bridge.get_service(client_a, handle_a).is_none());
    assert!(!dropped.load(Ordering::SeqCst));

    // Client B releases its handle -> drops handle B, service STILL ALIVE (held by C)
    let b_dropped = bridge.release_ref(client_b, handle_b, 1).unwrap();
    assert!(b_dropped);
    assert!(bridge.get_service(client_b, handle_b).is_none());
    assert!(!dropped.load(Ordering::SeqCst));

    // Client C still transacts successfully
    bridge.get_service(client_c, handle_c).unwrap().transact(1, 0, &p_in, &mut p_out).unwrap();

    // Client C releases its handle -> drops handle C, service FINALLY DROPS!
    let c_dropped = bridge.release_ref(client_c, handle_c, 1).unwrap();
    assert!(c_dropped);
    assert!(dropped.load(Ordering::SeqCst));
    assert!(bridge.get_service(client_c, handle_c).is_none());
    assert_eq!(bridge.total_handles(), 0);
}

#[test]
fn test_transfer_to_same_client_increments_refcount() {
    let bridge = HandleBridge::new();
    let client: ClientId = 10;
    let dropped = Arc::new(AtomicBool::new(false));
    let service: Arc<dyn IBinder> =
        Arc::new(MockAidlService::new("android.os.ISelfTransfer", Arc::clone(&dropped)));

    let handle = bridge.register_service(client, "android.os.ISelfTransfer", service);
    assert_eq!(bridge.get_strong_count(client, handle), Some(1));

    // Transferring to same client returns existing handle and increments refcount
    let transferred = bridge.transfer_handle(client, client, handle).unwrap();
    assert_eq!(transferred, handle);
    assert_eq!(bridge.get_strong_count(client, handle), Some(2));
}

#[test]
fn test_transfer_existing_service_to_other_client_increments_existing_handle() {
    let bridge = HandleBridge::new();
    let client_a: ClientId = 20;
    let client_b: ClientId = 21;
    let dropped = Arc::new(AtomicBool::new(false));

    let service: Arc<dyn IBinder> =
        Arc::new(MockAidlService::new("android.os.ISharedService", Arc::clone(&dropped)));

    // Register service for both clients
    let handle_a =
        bridge.register_service(client_a, "android.os.ISharedService", Arc::clone(&service));
    let handle_b =
        bridge.register_service(client_b, "android.os.ISharedService", Arc::clone(&service));
    assert_eq!(bridge.get_strong_count(client_b, handle_b), Some(1));

    // Transfer A's handle to B -> B already has a handle for this service, so it reuses handle_b and increments refcount
    let transferred = bridge.transfer_handle(client_a, client_b, handle_a).unwrap();
    assert_eq!(transferred, handle_b);
    assert_eq!(bridge.get_strong_count(client_b, handle_b), Some(2));
}

// -----------------------------------------------------------------------------
// Client Crash & Death Notification Tests
// -----------------------------------------------------------------------------

#[test]
fn test_client_crash_cleanup_and_death_notifications() {
    let bridge = HandleBridge::new();
    let client_victim: ClientId = 50;
    let client_survivor: ClientId = 51;

    let dropped_victim_exclusive = Arc::new(AtomicBool::new(false));
    let dropped_shared = Arc::new(AtomicBool::new(false));

    let service_exclusive: Arc<dyn IBinder> = Arc::new(MockAidlService::new(
        "android.os.IExclusiveService",
        Arc::clone(&dropped_victim_exclusive),
    ));
    let service_shared: Arc<dyn IBinder> = Arc::new(MockAidlService::new(
        "android.os.ISharedService",
        Arc::clone(&dropped_shared),
    ));

    // Victim registers exclusive service and shared service
    let h_victim_exclusive = bridge.register_service(
        client_victim,
        "android.os.IExclusiveService",
        service_exclusive,
    );
    let h_victim_shared = bridge.register_service(
        client_victim,
        "android.os.ISharedService",
        Arc::clone(&service_shared),
    );

    // Survivor registers shared service
    let h_survivor_shared = bridge.register_service(
        client_survivor,
        "android.os.ISharedService",
        service_shared,
    );

    // Register death recipients on victim handles
    bridge.register_death_recipient(client_victim, h_victim_exclusive, 0xAAAA_1111).unwrap();
    bridge.register_death_recipient(client_victim, h_victim_exclusive, 0xAAAA_2222).unwrap();
    bridge.register_death_recipient(client_victim, h_victim_shared, 0xBBBB_3333).unwrap();

    // Register death recipient on survivor handle
    bridge.register_death_recipient(client_survivor, h_survivor_shared, 0xCCCC_4444).unwrap();

    // Record notifications via death registry listener
    let received_events = Arc::new(Mutex::new(Vec::new()));
    let rec_clone = Arc::clone(&received_events);
    bridge.death_registry().add_listener(move |notif: DeathNotification| {
        rec_clone.lock().unwrap().push(notif);
    });

    // Simulate Client Victim Crash!
    let death_events = bridge.on_client_died(client_victim);

    // Verify emitted death events
    assert_eq!(death_events.len(), 3);
    assert!(death_events.contains(&(h_victim_exclusive, 0xAAAA_1111)));
    assert!(death_events.contains(&(h_victim_exclusive, 0xAAAA_2222)));
    assert!(death_events.contains(&(h_victim_shared, 0xBBBB_3333)));

    // Verify victim's exclusive service dropped immediately
    assert!(dropped_victim_exclusive.load(Ordering::SeqCst));
    assert!(bridge.get_service(client_victim, h_victim_exclusive).is_none());
    assert_eq!(bridge.handle_count(client_victim), 0);

    // Verify shared service is STILL ALIVE because survivor still holds reference
    assert!(!dropped_shared.load(Ordering::SeqCst));
    assert!(bridge.get_service(client_survivor, h_survivor_shared).is_some());
    assert_eq!(bridge.handle_count(client_survivor), 1);

    // Verify received events via listener
    let listener_events = received_events.lock().unwrap();
    assert_eq!(listener_events.len(), 3);
    assert!(listener_events.iter().all(|n| n.client_id == client_victim));

    // Survivor releases shared service -> now shared service drops
    drop(listener_events);
    bridge.release_ref(client_survivor, h_survivor_shared, 1).unwrap();
    assert!(dropped_shared.load(Ordering::SeqCst));
    assert_eq!(bridge.total_handles(), 0);
}

#[test]
fn test_death_recipient_register_unregister_and_duplicate_checks() {
    let bridge = HandleBridge::new();
    let client: ClientId = 60;
    let dropped = Arc::new(AtomicBool::new(false));
    let service: Arc<dyn IBinder> =
        Arc::new(MockAidlService::new("android.os.IDeathTest", Arc::clone(&dropped)));

    let handle = bridge.register_service(client, "android.os.IDeathTest", service);

    // Register cookie
    bridge.register_death_recipient(client, handle, 0x1234).unwrap();
    assert_eq!(
        bridge.get_death_recipients(client, handle),
        Some(vec![0x1234])
    );

    // Duplicate registration should fail
    assert_eq!(
        bridge.register_death_recipient(client, handle, 0x1234),
        Err(BridgeError::DeathRecipientAlreadyRegistered(0x1234, handle, client))
    );

    // Unregister non-existent cookie should fail
    assert_eq!(
        bridge.unregister_death_recipient(client, handle, 0x9999),
        Err(BridgeError::DeathRecipientNotFound(0x9999, handle, client))
    );

    // Unregister valid cookie
    bridge.unregister_death_recipient(client, handle, 0x1234).unwrap();
    assert_eq!(bridge.get_death_recipients(client, handle), Some(vec![]));
}

#[test]
fn test_explicit_handle_registration_handle_0() {
    let bridge = HandleBridge::new();
    let client: ClientId = 70;
    let dropped = Arc::new(AtomicBool::new(false));
    let service: Arc<dyn IBinder> =
        Arc::new(MockAidlService::new("android.os.IServiceManager", Arc::clone(&dropped)));

    // Register well-known handle 0
    let handle = bridge
        .register_service_with_handle(client, 0, "android.os.IServiceManager", service)
        .unwrap();
    assert_eq!(handle, 0);
    assert!(bridge.get_service(client, 0).is_some());

    // Registering duplicate handle 0 fails
    let dropped2 = Arc::new(AtomicBool::new(false));
    let service2: Arc<dyn IBinder> =
        Arc::new(MockAidlService::new("android.os.IDuplicate", Arc::clone(&dropped2)));
    assert_eq!(
        bridge.register_service_with_handle(client, 0, "android.os.IDuplicate", service2),
        Err(BridgeError::HandleAlreadyExists(0, client))
    );
}

// -----------------------------------------------------------------------------
// Multi-Threaded Concurrency & Stress Tests
// -----------------------------------------------------------------------------

#[test]
fn test_multithreaded_concurrent_acquire_release_transfer_stress() {
    let bridge = Arc::new(HandleBridge::new());
    let dropped_flags: Arc<Mutex<Vec<Arc<AtomicBool>>>> = Arc::new(Mutex::new(Vec::new()));

    // Pre-populate 8 services
    let num_services = 8;
    let mut initial_handles = Vec::new();
    for _ in 0..num_services {
        let flag = Arc::new(AtomicBool::new(false));
        dropped_flags.lock().unwrap().push(Arc::clone(&flag));
        let svc: Arc<dyn IBinder> = Arc::new(MockAidlService::new("android.os.IStressService", flag));
        let handle = bridge.register_service(0, "android.os.IStressService", svc);
        initial_handles.push(handle);
    }

    let initial_handles = Arc::new(initial_handles);
    let mut handles = Vec::new();
    let num_threads = 16;
    let ops_per_thread = 200;

    for t in 0..num_threads {
        let b = Arc::clone(&bridge);
        let init_h = Arc::clone(&initial_handles);
        let flags = Arc::clone(&dropped_flags);

        let handle = thread::spawn(move || {
            let client_id: ClientId = (t + 1) as u32;

            for op in 0..ops_per_thread {
                let target_svc_idx = op % init_h.len();
                let src_handle = init_h[target_svc_idx];

                // 1. Transfer handle from client 0 or other client to this client
                let my_handle = match b.transfer_handle(0, client_id, src_handle) {
                    Ok(h) => h,
                    Err(_) => {
                        // Create a fresh private service if source is dropped
                        let flag = Arc::new(AtomicBool::new(false));
                        flags.lock().unwrap().push(Arc::clone(&flag));
                        let svc: Arc<dyn IBinder> =
                            Arc::new(MockAidlService::new("android.os.IPrivateStress", flag));
                        b.register_service(client_id, "android.os.IPrivateStress", svc)
                    }
                };

                // 2. Perform concurrent acquire/release operations
                b.acquire_ref(client_id, my_handle, 2).unwrap_or(());
                b.acquire_weak_ref(client_id, my_handle, 1).unwrap_or(());

                // 3. Register death recipient and unregister
                let cookie = (client_id as u64) << 32 | (op as u64);
                let _ = b.register_death_recipient(client_id, my_handle, cookie);
                let _ = b.unregister_death_recipient(client_id, my_handle, cookie);

                // 4. Query service and transact
                if let Some(service) = b.get_service(client_id, my_handle) {
                    let in_p = Parcel::new();
                    let mut out_p = Parcel::new();
                    let _ = service.transact(1, 0, &in_p, &mut out_p);
                }

                // 5. Release refs
                let _ = b.release_weak_ref(client_id, my_handle, 1);
                let _ = b.release_ref(client_id, my_handle, 2);
                let _ = b.release_ref(client_id, my_handle, 1);
            }

            // Teardown this client on finish
            b.on_client_died(client_id);
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Thread joined successfully without panics");
    }

    // Teardown root client 0
    bridge.on_client_died(0);

    // Total handles across bridge must drop to 0
    assert_eq!(bridge.total_handles(), 0);

    // All services created during the test must be completely dropped
    let flags = dropped_flags.lock().unwrap();
    for (idx, flag) in flags.iter().enumerate() {
        assert!(
            flag.load(Ordering::SeqCst),
            "Service {} was leaked and not dropped!",
            idx
        );
    }
}

#[test]
fn test_large_ring_transfer_and_sequential_teardown() {
    let bridge = HandleBridge::new();
    let num_nodes = 50;
    let dropped = Arc::new(AtomicBool::new(false));
    let service: Arc<dyn IBinder> =
        Arc::new(MockAidlService::new("android.os.IRingService", Arc::clone(&dropped)));

    // Client 0 registers
    let mut current_handle = bridge.register_service(0, "android.os.IRingService", service);

    // Transfer in a ring: 0 -> 1 -> 2 -> ... -> 49
    let mut handles_per_client = vec![current_handle];
    for client in 1..num_nodes {
        let prev_client = client - 1;
        current_handle = bridge
            .transfer_handle(prev_client, client, current_handle)
            .expect("Transfer around ring must succeed");
        handles_per_client.push(current_handle);
    }

    assert_eq!(bridge.total_handles(), num_nodes as usize);
    assert!(!dropped.load(Ordering::SeqCst));

    // Release handles 0 to 48: service must stay alive
    for client in 0..(num_nodes - 1) {
        let h = handles_per_client[client as usize];
        let is_dropped = bridge.release_ref(client, h, 1).unwrap();
        assert!(is_dropped);
        assert!(!dropped.load(Ordering::SeqCst));
    }

    // Release last handle (node 49) -> drops service
    let last_handle = handles_per_client[(num_nodes - 1) as usize];
    let is_dropped = bridge.release_ref(num_nodes - 1, last_handle, 1).unwrap();
    assert!(is_dropped);
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(bridge.total_handles(), 0);
}
