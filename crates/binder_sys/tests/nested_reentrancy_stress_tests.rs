//! Adversarial deeply nested synchronous re-entrant calls stress tests.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{Result, Status, STATUS_BAD_VALUE};
use aidl_compat::stub::{Binder, RemoteBinder};
use aidl_compat::traits::{Interface, Remotable};
use binder_rt::types::TransactionCode;
use binder_rt::Parcel;
use binder_sys::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Recursive re-entrant service that calls another service (or ping-pongs between two services)
/// to depth `N`, requiring `spawn_worker_thread_if_needed()` to dynamically expand the looper threadpool.
struct PingPongRecursiveService {
    service_id: u32,
    peer_handle: AtomicU32,
    process_state: Arc<ProcessState>,
    executions: AtomicU32,
}

impl Interface for PingPongRecursiveService {
    fn as_binder(&self) -> SpIBinder {
        Binder::new(Self {
            service_id: self.service_id,
            peer_handle: AtomicU32::new(0),
            process_state: Arc::clone(&self.process_state),
            executions: AtomicU32::new(0),
        })
    }
}

impl Remotable for PingPongRecursiveService {
    fn get_class_descriptor() -> &'static str {
        "android.test.IPingPongRecursiveService"
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()> {
        let mut offset = 0;
        let _ = data.read_utf16(&mut offset);

        match code {
            1 => {
                let remaining_depth = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let accumulator = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                self.executions.fetch_add(1, Ordering::SeqCst);

                if remaining_depth <= 1 {
                    // Base case: return accumulated value + 10
                    reply
                        .write_i32(accumulator + 10)
                        .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                    return Ok(());
                }

                // Recursive case: make nested synchronous call to peer service
                let peer_h = self.peer_handle.load(Ordering::SeqCst);
                let transport =
                    Arc::new(BinderKernelTransport::with_process(Arc::clone(&self.process_state)));
                let peer_proxy = RemoteBinder::new_with_transport(peer_h, 0, None, transport);

                let mut next_data = Parcel::new();
                next_data
                    .write_utf16(Some("android.test.IPingPongRecursiveService"))
                    .unwrap();
                next_data.write_i32(remaining_depth - 1).unwrap();
                next_data.write_i32(accumulator + 1).unwrap();

                let mut next_reply = Parcel::new();
                peer_proxy.transact(1, 0, &next_data, &mut next_reply)?;

                let mut rep_offset = 0;
                let child_result = next_reply
                    .read_i32(&mut rep_offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                reply
                    .write_i32(child_result + 1)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => Err(Status::from_status(aidl_compat::status::STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

#[test]
fn test_deeply_nested_reentrant_threadpool_spawning() {
    let mock_driver = Arc::new(MockBinderDriver::new());

    // Process A hosting Service 1
    let ps_a = ProcessState::init_mock(Arc::clone(&mock_driver));
    let service_1 = Arc::new(PingPongRecursiveService {
        service_id: 1,
        peer_handle: AtomicU32::new(0),
        process_state: Arc::clone(&ps_a),
        executions: AtomicU32::new(0),
    });
    let cookie_1 = 0x1111;
    ps_a.register_service_object(cookie_1, Binder::new_with_arc(Arc::clone(&service_1)));

    // Process B hosting Service 2
    let ps_b = ProcessState::init_mock(Arc::clone(&mock_driver));
    let service_2 = Arc::new(PingPongRecursiveService {
        service_id: 2,
        peer_handle: AtomicU32::new(0),
        process_state: Arc::clone(&ps_b),
        executions: AtomicU32::new(0),
    });
    let cookie_2 = 0x2222;
    ps_b.register_service_object(cookie_2, Binder::new_with_arc(Arc::clone(&service_2)));

    // Start initial looper thread in Process A and B
    let ps_a_clone = Arc::clone(&ps_a);
    thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(ps_a_clone);
        let _ = ts.enter_looper();
    });

    let ps_b_clone = Arc::clone(&ps_b);
    thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(ps_b_clone);
        let _ = ts.enter_looper();
    });

    thread::sleep(Duration::from_millis(50));

    // Register cross-process handles
    let handle_2_in_a = mock_driver.add_handle_for_client(
        &mock_driver.get_client(ps_a.pid()).unwrap(),
        ps_b.pid(),
        0,
        cookie_2,
    );
    service_1.peer_handle.store(handle_2_in_a, Ordering::SeqCst);

    let handle_1_in_b = mock_driver.add_handle_for_client(
        &mock_driver.get_client(ps_b.pid()).unwrap(),
        ps_a.pid(),
        0,
        cookie_1,
    );
    service_2.peer_handle.store(handle_1_in_b, Ordering::SeqCst);

    // Client process invoking recursion depth of 6:
    // Call 1: Client -> S1 (depth 6)
    // Call 2: S1 -> S2 (depth 5) [spawns replacement in A before blocking]
    // Call 3: S2 -> S1 (depth 4) [spawns replacement in B before blocking]
    // Call 4: S1 -> S2 (depth 3) [spawns replacement in A before blocking]
    // Call 5: S2 -> S1 (depth 2) [spawns replacement in B before blocking]
    // Call 6: S1 -> S2 (depth 1) [base case returns 10 + 5 = 15]
    let client_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let handle_1_in_client = mock_driver.add_handle_for_client(
        &mock_driver.get_client(client_ps.pid()).unwrap(),
        ps_a.pid(),
        0,
        cookie_1,
    );

    let client_transport =
        Arc::new(BinderKernelTransport::with_process(Arc::clone(&client_ps)));
    let proxy = RemoteBinder::new_with_transport(handle_1_in_client, 0, None, client_transport);

    let target_depth = 6;
    let mut data = Parcel::new();
    data.write_utf16(Some("android.test.IPingPongRecursiveService"))
        .unwrap();
    data.write_i32(target_depth).unwrap();
    data.write_i32(0).unwrap();

    let mut reply = Parcel::new();
    let res = proxy.transact(1, 0, &data, &mut reply);

    assert!(
        res.is_ok(),
        "Deeply nested re-entrant transact failed: {:?}",
        res.err()
    );

    let mut offset = 0;
    let final_val = reply.read_i32(&mut offset).expect("Failed to read result");

    // Calculation:
    // depth 6: acc=0
    // depth 5: acc=1
    // depth 4: acc=2
    // depth 3: acc=3
    // depth 2: acc=4
    // depth 1: base case returns acc + 10 = 5 + 10 = 15
    // unwinds 5 layers adding +1 each = 15 + 5 = 20
    assert_eq!(final_val, 20);

    assert_eq!(
        service_1.executions.load(Ordering::SeqCst) + service_2.executions.load(Ordering::SeqCst),
        target_depth as u32
    );

    // Verify that replacement threads were spawned dynamically
    assert!(
        ps_a.active_worker_count() > 0,
        "Process A must have active workers spawned"
    );
    assert!(
        ps_b.active_worker_count() > 0,
        "Process B must have active workers spawned"
    );
}
