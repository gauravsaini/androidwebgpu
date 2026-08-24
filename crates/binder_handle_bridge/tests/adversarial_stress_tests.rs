//! Adversarial and empirical stress test suite for `binder_handle_bridge`.
//!
//! Tests multi-threaded concurrency, sudden client disconnects, death recipient churn,
//! refcount underflow resistance, double-release handling, and handle wrapping.

use aidl_compat::{DeathRecipient, IBinder, Result as AidlResult};
use binder_handle_bridge::{BridgeError, ClientId, DeathNotification, HandleBridge};
use binder_rt::types::{TransactionCode, TransactionFlags};
use binder_rt::Parcel;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Simple fast PRNG (Xorshift64) for reproducible multi-threaded chaos testing without external crates.
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xDEAD_BEEF_CAFE_BABE } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_usize(&mut self, max: usize) -> usize {
        if max == 0 {
            0
        } else {
            (self.next_u64() % (max as u64)) as usize
        }
    }

    fn next_range(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            min
        } else {
            min + self.next_usize(max - min + 1)
        }
    }
}

/// Mock AIDL Service instrumented for drop tracking and transaction invocation counts.
struct AdversarialMockService {
    descriptor: &'static str,
    call_count: AtomicUsize,
    drop_flag: Arc<AtomicBool>,
}

impl AdversarialMockService {
    fn new(descriptor: &'static str, drop_flag: Arc<AtomicBool>) -> Self {
        Self {
            descriptor,
            call_count: AtomicUsize::new(0),
            drop_flag,
        }
    }
}

impl Drop for AdversarialMockService {
    fn drop(&mut self) {
        self.drop_flag.store(true, Ordering::SeqCst);
    }
}

impl IBinder for AdversarialMockService {
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

// =============================================================================
// 1. Multi-threaded concurrent handle passing across 32 clients in random rings
// =============================================================================

#[test]
fn test_multithreaded_32_client_random_rings() {
    let bridge = Arc::new(HandleBridge::new());
    let num_clients: usize = 32;
    let num_services: usize = 16;
    let rounds_per_client: usize = 100;

    let mut service_flags = Vec::new();
    let mut initial_handles = Vec::new();

    // Register 16 services under client 1
    for _i in 0..num_services {
        let flag = Arc::new(AtomicBool::new(false));
        service_flags.push(Arc::clone(&flag));
        let svc: Arc<dyn IBinder> =
            Arc::new(AdversarialMockService::new("android.os.IRingService", flag));
        let handle = bridge.register_service(1, "android.os.IRingService", svc);
        initial_handles.push(handle);
    }

    let initial_handles = Arc::new(initial_handles);
    let service_flags = Arc::new(service_flags);
    let mut threads = Vec::with_capacity(num_clients);

    for client_idx in 1..=num_clients {
        let b = Arc::clone(&bridge);
        let init_h = Arc::clone(&initial_handles);
        let _flags = Arc::clone(&service_flags);

        let handle = thread::spawn(move || {
            let client_id = client_idx as ClientId;
            let mut rng = XorShift64::new(0x1234_5678_0000 + client_idx as u64);

            for round in 0..rounds_per_client {
                let target_svc = round % init_h.len();
                let seed_handle = init_h[target_svc];

                // Random hop: transfer from random previous client or client 1 to this client
                let from_client = if round == 0 {
                    1
                } else {
                    (rng.next_range(1, num_clients)) as ClientId
                };

                let handle_opt = match b.transfer_handle(from_client, client_id, seed_handle) {
                    Ok(h) => Some(h),
                    Err(_) => {
                        // If from_client handle was not found or released, register new or transfer from client 1
                        b.transfer_handle(1, client_id, seed_handle).ok().or_else(|| {
                            let flag = Arc::new(AtomicBool::new(false));
                            let svc: Arc<dyn IBinder> =
                                Arc::new(AdversarialMockService::new("android.os.IRingFallback", flag));
                            Some(b.register_service(client_id, "android.os.IRingFallback", svc))
                        })
                    }
                };

                if let Some(h) = handle_opt {
                    // Acquire extra strong & weak references
                    assert!(b.acquire_ref(client_id, h, 2).is_ok());
                    assert!(b.acquire_weak_ref(client_id, h, 1).is_ok());

                    // Execute transaction
                    if let Some(service) = b.get_service(client_id, h) {
                        let p_in = Parcel::new();
                        let mut p_out = Parcel::new();
                        assert!(service.transact(1, 0, &p_in, &mut p_out).is_ok());
                    }

                    // Release some references
                    let _ = b.release_weak_ref(client_id, h, 1);
                    let _ = b.release_ref(client_id, h, 2);

                    // Next hop: pass handle forward to a random neighbor client
                    let next_client = (rng.next_range(1, num_clients)) as ClientId;
                    let _ = b.transfer_handle(client_id, next_client, h);

                    // Release remaining local reference
                    let _ = b.release_ref(client_id, h, 1);
                }
            }

            // Client finishes all rounds and cleans up
            b.on_client_died(client_id);
        });

        threads.push(handle);
    }

    for t in threads {
        t.join().expect("All 32 client threads must finish cleanly without panics");
    }

    // Clean up all clients 1..=num_clients after threads finish
    for c in 1..=num_clients {
        bridge.on_client_died(c as ClientId);
    }

    // Total handles across bridge must drop to 0
    assert_eq!(bridge.total_handles(), 0);

    // Verify all services were cleanly dropped (no memory leaks)
    for (i, flag) in service_flags.iter().enumerate() {
        assert!(
            flag.load(Ordering::SeqCst),
            "Service #{} leaked and was not dropped!",
            i
        );
    }
}

// =============================================================================
// 2. Sudden client disconnect simulation during in-flight transfers
// =============================================================================

#[test]
fn test_sudden_client_disconnect_during_inflight_transfers() {
    let bridge = Arc::new(HandleBridge::new());
    let num_workers = 16;
    let num_clients = 32;
    let ops_per_worker = 150;

    let stop_chaos = Arc::new(AtomicBool::new(false));
    let service_flags = Arc::new(Mutex::new(Vec::new()));

    // Seed services
    let mut initial_handles = Vec::new();
    for _i in 0..8 {
        let flag = Arc::new(AtomicBool::new(false));
        service_flags.lock().unwrap().push(Arc::clone(&flag));
        let svc: Arc<dyn IBinder> =
            Arc::new(AdversarialMockService::new("android.os.IChaosService", flag));
        let handle = bridge.register_service(0, "android.os.IChaosService", svc);
        initial_handles.push(handle);
    }
    let initial_handles = Arc::new(initial_handles);

    // Spawn Chaos Reaper Thread: randomly kills clients while workers transfer handles
    let reaper_bridge = Arc::clone(&bridge);
    let reaper_stop = Arc::clone(&stop_chaos);
    let reaper_thread = thread::spawn(move || {
        let mut rng = XorShift64::new(0xCAFE_BABE_9999);
        while !reaper_stop.load(Ordering::SeqCst) {
            let victim = (rng.next_range(1, num_clients)) as ClientId;
            reaper_bridge.on_client_died(victim);
            thread::sleep(Duration::from_millis(1));
        }
    });

    // Spawn Worker Threads: actively transfer, query, register, release
    let mut workers = Vec::new();
    for w in 0..num_workers {
        let b = Arc::clone(&bridge);
        let inits = Arc::clone(&initial_handles);
        let s_flags = Arc::clone(&service_flags);

        let handle = thread::spawn(move || {
            let mut rng = XorShift64::new(0xABCD_0000 + w as u64);
            let my_client = (w + 1) as ClientId;

            for _ in 0..ops_per_worker {
                let target_client = (rng.next_range(1, num_clients)) as ClientId;
                let seed_h = inits[rng.next_usize(inits.len())];

                // Attempt transfer from root or random client
                match b.transfer_handle(0, my_client, seed_h) {
                    Ok(h) => {
                        // Register death cookie
                        let cookie = rng.next_u64();
                        let _ = b.register_death_recipient(my_client, h, cookie);

                        // Concurrent transfer to another random client
                        let _ = b.transfer_handle(my_client, target_client, h);

                        // Query service and transact
                        if let Some(svc) = b.get_service(my_client, h) {
                            let p_in = Parcel::new();
                            let mut p_out = Parcel::new();
                            let _ = svc.transact(1, 0, &p_in, &mut p_out);
                        }

                        // Release handle
                        let _ = b.release_ref(my_client, h, 1);
                    }
                    Err(_) => {
                        // Root handle might have been transferred or modified, create private service
                        let flag = Arc::new(AtomicBool::new(false));
                        s_flags.lock().unwrap().push(Arc::clone(&flag));
                        let svc: Arc<dyn IBinder> =
                            Arc::new(AdversarialMockService::new("android.os.IPrivateChaos", flag));
                        let h = b.register_service(my_client, "android.os.IPrivateChaos", svc);
                        let _ = b.transfer_handle(my_client, target_client, h);
                        let _ = b.release_ref(my_client, h, 1);
                    }
                }
            }
        });
        workers.push(handle);
    }

    for w in workers {
        w.join().expect("Worker thread must not panic during concurrent chaos disconnects");
    }

    // Stop reaper
    stop_chaos.store(true, Ordering::SeqCst);
    reaper_thread.join().expect("Reaper thread must finish cleanly");

    // Final teardown of all remaining clients 0..=32
    for c in 0..=num_clients {
        bridge.on_client_died(c as ClientId);
    }

    // Assert zero remaining handles
    assert_eq!(bridge.total_handles(), 0);

    // Assert all services are completely dropped
    let flags = service_flags.lock().unwrap();
    for (i, flag) in flags.iter().enumerate() {
        assert!(
            flag.load(Ordering::SeqCst),
            "Chaos service #{} was leaked!",
            i
        );
    }
}

// =============================================================================
// 3. Death recipient link/unlink churn while releasing handles
// =============================================================================

#[test]
fn test_death_recipient_link_unlink_churn_while_releasing() {
    let bridge = Arc::new(HandleBridge::new());
    let num_threads = 12;
    let ops_per_thread = 200;
    let client_id: ClientId = 77;

    let drop_flag = Arc::new(AtomicBool::new(false));
    let service: Arc<dyn IBinder> = Arc::new(AdversarialMockService::new(
        "android.os.IDeathChurn",
        Arc::clone(&drop_flag),
    ));

    // Register initial handles for client 77
    let mut handles = Vec::new();
    for _ in 0..8 {
        let h = bridge.register_service(client_id, "android.os.IDeathChurn", Arc::clone(&service));
        // Bump refcount so handles stay alive during churn
        bridge.acquire_ref(client_id, h, 1000).unwrap();
        handles.push(h);
    }
    let handles = Arc::new(handles);

    let death_event_counter = Arc::new(AtomicU64::new(0));
    let counter_clone = Arc::clone(&death_event_counter);
    bridge.death_registry().add_listener(move |_notif: DeathNotification| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    });

    let mut threads = Vec::new();

    // Spawn concurrent churners: Link, Unlink, Acquire, Release
    for t in 0..num_threads {
        let b = Arc::clone(&bridge);
        let h_list = Arc::clone(&handles);

        let handle = thread::spawn(move || {
            let mut rng = XorShift64::new(0xDEAF_0000 + t as u64);

            for op in 0..ops_per_thread {
                let h = h_list[rng.next_usize(h_list.len())];
                let cookie = ((t as u64) << 32) | (op as u64);

                match op % 4 {
                    0 => {
                        // Register death recipient
                        let _ = b.register_death_recipient(client_id, h, cookie);
                    }
                    1 => {
                        // Unregister death recipient
                        let _ = b.unregister_death_recipient(client_id, h, cookie);
                    }
                    2 => {
                        // Acquire ref
                        let _ = b.acquire_ref(client_id, h, 1);
                    }
                    _ => {
                        // Release ref
                        let _ = b.release_ref(client_id, h, 1);
                    }
                }
            }
        });
        threads.push(handle);
    }

    for t in threads {
        t.join().expect("Death recipient churn thread completed without panics");
    }

    // Now trigger client death and verify notifications
    let death_events = bridge.on_client_died(client_id);
    let fired_count = death_event_counter.load(Ordering::SeqCst);

    assert_eq!(
        death_events.len() as u64,
        fired_count,
        "Dispatched death event count must match listener receipt count"
    );

    // Verify death registry history count matches
    assert_eq!(bridge.death_registry().history_count(), death_events.len());
    let drained = bridge.death_registry().drain_history();
    assert_eq!(drained.len(), death_events.len());
    assert_eq!(bridge.death_registry().history_count(), 0);

    // Bridge total handles must be 0
    assert_eq!(bridge.total_handles(), 0);

    // Drop our local Arc copy of service
    drop(service);
    assert!(drop_flag.load(Ordering::SeqCst), "Service must drop after client teardown");
}

// =============================================================================
// 4. Refcount underflow and double-release assertions
// =============================================================================

#[test]
fn test_refcount_underflow_and_double_release_assertions() {
    let bridge = HandleBridge::new();
    let client: ClientId = 88;
    let drop_flag = Arc::new(AtomicBool::new(false));
    let service: Arc<dyn IBinder> = Arc::new(AdversarialMockService::new(
        "android.os.IUnderflowGuard",
        Arc::clone(&drop_flag),
    ));

    let handle = bridge.register_service(client, "android.os.IUnderflowGuard", service);
    assert_eq!(bridge.get_strong_count(client, handle), Some(1));

    // Underflow assertion on strong count
    let err = bridge.release_ref(client, handle, 2);
    assert_eq!(err, Err(BridgeError::InvalidRefCount(handle, client)));
    assert_eq!(bridge.get_strong_count(client, handle), Some(1));

    // Massive underflow assertion
    let err = bridge.release_ref(client, handle, usize::MAX);
    assert_eq!(err, Err(BridgeError::InvalidRefCount(handle, client)));
    assert_eq!(bridge.get_strong_count(client, handle), Some(1));

    // Underflow assertion on weak count
    let err = bridge.release_weak_ref(client, handle, 1);
    assert_eq!(err, Err(BridgeError::InvalidRefCount(handle, client)));
    assert_eq!(bridge.get_weak_count(client, handle), Some(0));

    // Normal release -> drops handle
    let dropped = bridge.release_ref(client, handle, 1).unwrap();
    assert!(dropped);
    assert!(drop_flag.load(Ordering::SeqCst));
    assert_eq!(bridge.get_strong_count(client, handle), None);

    // Double-release on already dropped handle -> returns HandleNotFound
    let err = bridge.release_ref(client, handle, 1);
    assert_eq!(err, Err(BridgeError::HandleNotFound(handle, client)));

    // Double-release with weak ref -> returns HandleNotFound
    let err = bridge.release_weak_ref(client, handle, 1);
    assert_eq!(err, Err(BridgeError::HandleNotFound(handle, client)));

    // Acquire on already dropped handle -> returns HandleNotFound
    let err = bridge.acquire_ref(client, handle, 1);
    assert_eq!(err, Err(BridgeError::HandleNotFound(handle, client)));

    // Register death recipient on already dropped handle -> returns HandleNotFound
    let err = bridge.register_death_recipient(client, handle, 0x1234);
    assert_eq!(err, Err(BridgeError::HandleNotFound(handle, client)));

    // Unregister death recipient on already dropped handle -> returns HandleNotFound
    let err = bridge.unregister_death_recipient(client, handle, 0x1234);
    assert_eq!(err, Err(BridgeError::HandleNotFound(handle, client)));

    // Multi-threaded double-release race test
    let bridge_arc = Arc::new(HandleBridge::new());
    let drop_flag2 = Arc::new(AtomicBool::new(false));
    let service2: Arc<dyn IBinder> = Arc::new(AdversarialMockService::new(
        "android.os.IRaceRelease",
        Arc::clone(&drop_flag2),
    ));

    let race_handle = bridge_arc.register_service(client, "android.os.IRaceRelease", service2);
    let mut race_threads = Vec::new();
    let dropped_count = Arc::new(AtomicUsize::new(0));
    let not_found_count = Arc::new(AtomicUsize::new(0));

    for _ in 0..8 {
        let b = Arc::clone(&bridge_arc);
        let dc = Arc::clone(&dropped_count);
        let nfc = Arc::clone(&not_found_count);

        let t = thread::spawn(move || {
            match b.release_ref(client, race_handle, 1) {
                Ok(true) => {
                    dc.fetch_add(1, Ordering::SeqCst);
                }
                Ok(false) => {
                    panic!("Unexpected Ok(false) for count 1 on initial count 1");
                }
                Err(BridgeError::HandleNotFound(..)) => {
                    nfc.fetch_add(1, Ordering::SeqCst);
                }
                Err(e) => {
                    panic!("Unexpected error in double-release race: {:?}", e);
                }
            }
        });
        race_threads.push(t);
    }

    for t in race_threads {
        t.join().expect("Race release thread joined cleanly");
    }

    // Exactly 1 thread must have successfully dropped the handle, and remaining 7 must get HandleNotFound
    assert_eq!(dropped_count.load(Ordering::SeqCst), 1);
    assert_eq!(not_found_count.load(Ordering::SeqCst), 7);
    assert!(drop_flag2.load(Ordering::SeqCst));
    assert_eq!(bridge_arc.total_handles(), 0);
}

// =============================================================================
// 5. Handle ID allocation wrapping and boundary behavior
// =============================================================================

#[test]
fn test_handle_id_allocation_wrapping_and_collision_resistance() {
    let mut table = binder_handle_bridge::HandleTable::new();
    let client_id: ClientId = 99;

    let flag = Arc::new(AtomicBool::new(false));
    let svc: Arc<dyn IBinder> =
        Arc::new(AdversarialMockService::new("android.os.IWrapTest", Arc::clone(&flag)));

    // Allocate first handle
    let h1 = table.allocate_handle_id(client_id);
    assert_eq!(h1, 1);
    let entry1 = binder_handle_bridge::HostHandleEntry::new(h1, client_id, "desc", Arc::clone(&svc));
    table.insert(entry1).unwrap();

    // Allocate handle with ID close to u32::MAX
    let h_max_minus_1 = u32::MAX - 1;
    let entry_max = binder_handle_bridge::HostHandleEntry::new(
        h_max_minus_1,
        client_id,
        "desc",
        Arc::clone(&svc),
    );
    table.insert(entry_max).unwrap();

    // Allocate handle IDs sequentially across the boundary
    let mut allocated = Vec::new();
    for _ in 0..10 {
        let h = table.allocate_handle_id(client_id);
        allocated.push(h);
        let entry = binder_handle_bridge::HostHandleEntry::new(h, client_id, "desc", Arc::clone(&svc));
        table.insert(entry).unwrap();
    }

    // Verify all allocated handles are distinct and valid (never 0)
    for &h in &allocated {
        assert_ne!(h, 0, "Allocated handle ID must never be 0");
    }
    assert_eq!(table.client_handle_count(client_id), 12);
}
