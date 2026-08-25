//! Spawn-before-block concurrency and re-entrant looper deadlock prevention tests.

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

// Service A which calls Service B synchronously during on_transact
struct ServiceA {
    service_b_handle: AtomicU32,
    process_state: Arc<ProcessState>,
    completed_calls: AtomicU32,
}

impl Interface for ServiceA {
    fn as_binder(&self) -> SpIBinder {
        Binder::new(Self {
            service_b_handle: AtomicU32::new(0),
            process_state: Arc::clone(&self.process_state),
            completed_calls: AtomicU32::new(0),
        })
    }
}

impl Remotable for ServiceA {
    fn get_class_descriptor() -> &'static str {
        "android.test.IServiceA"
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
                let val = data.read_i32(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                // Make a nested synchronous call to Service B from within this looper thread!
                let b_handle = self.service_b_handle.load(Ordering::SeqCst);
                let transport = Arc::new(BinderKernelTransport::with_process(Arc::clone(&self.process_state)));
                let service_b = RemoteBinder::new_with_transport(b_handle, 0, None, transport);

                let mut b_data = Parcel::new();
                b_data.write_utf16(Some("android.test.IServiceB")).unwrap();
                b_data.write_i32(val * 2).unwrap();

                let mut b_reply = Parcel::new();
                service_b.transact(1, 0, &b_data, &mut b_reply)?;

                let mut b_offset = 0;
                let b_result = b_reply.read_i32(&mut b_offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                reply.write_i32(b_result + 1).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                self.completed_calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            _ => Err(Status::from_status(aidl_compat::status::STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

// Service B which answers calls
struct ServiceB {
    calls: AtomicU32,
}

impl Interface for ServiceB {
    fn as_binder(&self) -> SpIBinder {
        Binder::new(Self {
            calls: AtomicU32::new(0),
        })
    }
}

impl Remotable for ServiceB {
    fn get_class_descriptor() -> &'static str {
        "android.test.IServiceB"
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
                let val = data.read_i32(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                self.calls.fetch_add(1, Ordering::SeqCst);
                reply.write_i32(val + 10).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => Err(Status::from_status(aidl_compat::status::STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

#[test]
fn test_spawn_before_block_nested_binder_calls() {
    let mock_driver = Arc::new(MockBinderDriver::new());

    // 1. Process A (hosting Service A)
    let ps_a = ProcessState::init_mock(Arc::clone(&mock_driver));
    let service_a = Arc::new(ServiceA {
        service_b_handle: AtomicU32::new(0),
        process_state: Arc::clone(&ps_a),
        completed_calls: AtomicU32::new(0),
    });
    let cookie_a = 0xAAAA;
    ps_a.register_service_object(cookie_a, Binder::new_with_arc(Arc::clone(&service_a)));

    // 2. Process B (hosting Service B)
    let ps_b = ProcessState::init_mock(Arc::clone(&mock_driver));
    let service_b = Arc::new(ServiceB {
        calls: AtomicU32::new(0),
    });
    let cookie_b = 0xBBBB;
    ps_b.register_service_object(cookie_b, Binder::new_with_arc(Arc::clone(&service_b)));

    // Start worker loopers in Process A and Process B
    let ps_a_worker = Arc::clone(&ps_a);
    let _worker_a = thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(ps_a_worker);
        let _ = ts.enter_looper();
    });

    let ps_b_worker = Arc::clone(&ps_b);
    let _worker_b = thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(ps_b_worker);
        let _ = ts.enter_looper();
    });

    // Give loopers time to start
    thread::sleep(Duration::from_millis(50));

    // Register handle to Service B in Process A
    let handle_b_in_a = mock_driver.add_handle_for_client(
        &mock_driver.get_client(ps_a.pid()).unwrap(),
        ps_b.pid(),
        0,
        cookie_b,
    );
    service_a.service_b_handle.store(handle_b_in_a, Ordering::SeqCst);

    // 3. Client Process invoking Service A
    let client_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let handle_a_in_client = mock_driver.add_handle_for_client(
        &mock_driver.get_client(client_ps.pid()).unwrap(),
        ps_a.pid(),
        0,
        cookie_a,
    );

    let client_transport = Arc::new(BinderKernelTransport::with_process(Arc::clone(&client_ps)));
    let proxy_a = RemoteBinder::new_with_transport(handle_a_in_client, 0, None, client_transport);

    // Call Service A with value 5
    // Service A computes: (5 * 2) -> sent to B -> B computes (10 + 10 = 20) -> A adds 1 -> 21
    let mut data = Parcel::new();
    data.write_utf16(Some("android.test.IServiceA")).unwrap();
    data.write_i32(5).unwrap();

    let mut reply = Parcel::new();
    let res = proxy_a.transact(1, 0, &data, &mut reply);

    assert!(res.is_ok(), "Nested transaction call succeeded");
    let mut offset = 0;
    let final_val = reply.read_i32(&mut offset).expect("Read reply failed");
    assert_eq!(final_val, 21);
    assert_eq!(service_b.calls.load(Ordering::SeqCst), 1);
    assert_eq!(service_a.completed_calls.load(Ordering::SeqCst), 1);
}
