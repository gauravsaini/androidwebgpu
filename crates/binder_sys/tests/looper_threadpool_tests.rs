//! Looper thread and threadpool expansion tests.

use aidl_compat::status::{Result, Status, STATUS_BAD_VALUE};
use aidl_compat::stub::Binder;
use aidl_compat::traits::{Interface, Remotable};
use binder_rt::types::TransactionCode;
use binder_rt::Parcel;
use binder_sys::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// Test remote calculation service
struct EchoCalculator {
    call_count: AtomicU32,
}

impl Interface for EchoCalculator {
    fn as_binder(&self) -> aidl_compat::pointer::SpIBinder {
        Binder::new(Self {
            call_count: AtomicU32::new(0),
        })
    }
}

impl Remotable for EchoCalculator {
    fn get_class_descriptor() -> &'static str {
        "android.test.IEchoCalculator"
    }

    fn on_transact(
        &self,
        code: TransactionCode,
        data: &Parcel,
        reply: &mut Parcel,
    ) -> Result<()> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let mut offset = 0;
        let _ = data.read_utf16(&mut offset);

        match code {
            1 => {
                // Add two i32
                let a = data.read_i32(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let b = data.read_i32(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply.write_i32(a + b).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            2 => {
                // String echo
                let msg = data.read_utf8(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply.write_utf8(msg.as_deref()).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => Err(Status::from_status(aidl_compat::status::STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

#[test]
fn test_looper_basic_transaction_dispatch() {
    let mock_driver = Arc::new(MockBinderDriver::new());

    // 1. Server Process
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let calc = Arc::new(EchoCalculator {
        call_count: AtomicU32::new(0),
    });
    let calc_binder = Binder::new_with_arc(Arc::clone(&calc));
    let service_cookie = 0x5555;
    server_ps.register_service_object(service_cookie, calc_binder);

    // Start server looper thread
    let server_ps_clone = Arc::clone(&server_ps);
    let _server_thread = thread::spawn(move || {
        let mut ts = IPCThreadState::with_process(server_ps_clone);
        let _ = ts.enter_looper();
    });

    thread::sleep(Duration::from_millis(50));

    // 2. Client Process
    let client_ps = ProcessState::init_mock(Arc::clone(&mock_driver));

    // Register handle in client pointing to server's service_cookie
    let handle = mock_driver.add_handle_for_client(
        &mock_driver.get_client(client_ps.pid()).unwrap(),
        server_ps.pid(),
        0,
        service_cookie,
    );

    // Send transaction (code 1: add(20, 22))
    let mut data = Parcel::new();
    data.write_utf16(Some("android.test.IEchoCalculator")).unwrap();
    data.write_i32(20).unwrap();
    data.write_i32(22).unwrap();

    let mut reply = Parcel::new();
    let mut client_ts = IPCThreadState::with_process(Arc::clone(&client_ps));
    let res = client_ts.transact(handle, 1, 0, &data, &mut reply);

    assert!(res.is_ok(), "Transaction should succeed");
    let mut reply_offset = 0;
    let sum = reply.read_i32(&mut reply_offset).expect("Read sum failed");
    assert_eq!(sum, 42);
    assert_eq!(calc.call_count.load(Ordering::SeqCst), 1);
}

#[test]
fn test_threadpool_dynamic_expansion() {
    let mock_driver = Arc::new(MockBinderDriver::new());
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));

    assert_eq!(server_ps.active_worker_count(), 0);
    assert_eq!(server_ps.max_threads(), 15);

    // Trigger spawn worker
    let spawned = server_ps.spawn_worker_thread_if_needed();
    assert!(spawned);

    // Wait a brief moment for worker thread to increment active counter
    thread::sleep(Duration::from_millis(50));
    assert_eq!(server_ps.active_worker_count(), 1);
}
