//! Adversarial Multithreaded Looper Concurrency and Contention Stress Tests.

use aidl_compat::death::DeathRecipient;
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

struct StressCalculator {
    call_count: AtomicU32,
}

impl Interface for StressCalculator {
    fn as_binder(&self) -> aidl_compat::pointer::SpIBinder {
        Binder::new(Self {
            call_count: AtomicU32::new(0),
        })
    }
}

impl Remotable for StressCalculator {
    fn get_class_descriptor() -> &'static str {
        "android.test.IStressCalculator"
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
                let a = data.read_i32(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let b = data.read_i32(&mut offset).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply.write_i32(a * 2 + b).map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                Ok(())
            }
            _ => Err(Status::from_status(aidl_compat::status::STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

struct TestDeathHandler {
    trigger_count: AtomicU32,
}

impl DeathRecipient for TestDeathHandler {
    fn binder_died(&self) {
        self.trigger_count.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn test_adversarial_threadpool_saturation_and_limit_enforcement() {
    let mock_driver = Arc::new(MockBinderDriver::new());
    let max_threads = 8;
    let backend = driver::MockDriverBackend::new(Arc::clone(&mock_driver), mmap::BINDER_DEFAULT_MMAP_SIZE);
    let server_ps = ProcessState::new(Arc::new(backend), max_threads);

    assert_eq!(server_ps.active_worker_count(), 0);
    assert_eq!(server_ps.max_threads(), max_threads);

    // Rapidly request threadpool spawn 20 times (exceeding max_threads of 8)
    let mut spawned_count = 0;
    for _ in 0..20 {
        if server_ps.spawn_worker_thread_if_needed() {
            spawned_count += 1;
        }
    }

    // Give threads time to spin up
    thread::sleep(Duration::from_millis(100));

    // Active workers must NEVER exceed max_threads
    let active = server_ps.active_worker_count();
    assert!(
        active <= max_threads,
        "Active workers {} exceeded max_threads {}",
        active,
        max_threads
    );
    assert!(
        spawned_count <= max_threads as usize + 2, // Accounting for race before worker increments active
        "Spawned count {} exceeded reasonable bounds",
        spawned_count
    );
}

#[test]
fn test_adversarial_concurrent_death_recipient_lifecycle() {
    let mock_driver = Arc::new(MockBinderDriver::new());
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let death_handler = Arc::new(TestDeathHandler {
        trigger_count: AtomicU32::new(0),
    });

    let num_threads = 16;
    let iterations = 200;
    let mut handles = Vec::new();

    for t in 0..num_threads {
        let ps = Arc::clone(&server_ps);
        let handler = Arc::clone(&death_handler);
        handles.push(thread::spawn(move || {
            for i in 0..iterations {
                let cookie = (t * 1000 + i) as u64;
                ps.register_death_recipient(cookie, handler.clone());
                if i % 3 == 0 {
                    ps.notify_death(cookie);
                }
                ps.unregister_death_recipient(cookie);
            }
        }));
    }

    for h in handles {
        h.join().expect("Death recipient worker thread panicked");
    }

    assert!(death_handler.trigger_count.load(Ordering::SeqCst) > 0);
}

#[test]
fn test_adversarial_high_throughput_concurrent_transact_burst() {
    let mock_driver = Arc::new(MockBinderDriver::new());

    // 1. Server setup with 4 worker threads
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let calc = Arc::new(StressCalculator {
        call_count: AtomicU32::new(0),
    });
    let calc_binder = Binder::new_with_arc(Arc::clone(&calc));
    let service_cookie = 0x8888;
    server_ps.register_service_object(service_cookie, calc_binder);

    // Spawn 4 worker threads in server
    for _ in 0..4 {
        let sps = Arc::clone(&server_ps);
        thread::spawn(move || {
            let mut ts = IPCThreadState::with_process(sps);
            let _ = ts.enter_looper();
        });
    }

    thread::sleep(Duration::from_millis(50));

    // 2. Client processes hammering the service
    let num_clients = 8;
    let transactions_per_client = 100;
    let mut client_handles = Vec::new();

    for client_id in 0..num_clients {
        let driver_clone = Arc::clone(&mock_driver);
        let s_pid = server_ps.pid();

        client_handles.push(thread::spawn(move || {
            let client_ps = ProcessState::init_mock(driver_clone.clone());
            let handle = driver_clone.add_handle_for_client(
                &driver_clone.get_client(client_ps.pid()).unwrap(),
                s_pid,
                0,
                service_cookie,
            );

            let mut client_ts = IPCThreadState::with_process(Arc::clone(&client_ps));

            for iter in 0..transactions_per_client {
                let mut data = Parcel::new();
                data.write_utf16(Some("android.test.IStressCalculator")).unwrap();
                data.write_i32(client_id as i32).unwrap();
                data.write_i32(iter as i32).unwrap();

                let mut reply = Parcel::new();
                let res = client_ts.transact(handle, 1, 0, &data, &mut reply);
                assert!(res.is_ok(), "Client transact failed at iter {}", iter);

                let mut reply_offset = 0;
                let expected = (client_id as i32) * 2 + (iter as i32);
                let actual = reply.read_i32(&mut reply_offset).expect("Read reply failed");
                assert_eq!(actual, expected);
            }
        }));
    }

    for ch in client_handles {
        ch.join().expect("Client transact worker panicked");
    }

    assert_eq!(
        calc.call_count.load(Ordering::SeqCst),
        (num_clients * transactions_per_client) as u32
    );
}

#[test]
fn test_adversarial_threadpool_zero_max_threads() {
    let mock_driver = Arc::new(MockBinderDriver::new());
    let backend = driver::MockDriverBackend::new(Arc::clone(&mock_driver), mmap::BINDER_DEFAULT_MMAP_SIZE);
    let ps = ProcessState::new(Arc::new(backend), 0);

    assert_eq!(ps.max_threads(), 0);
    assert_eq!(ps.active_worker_count(), 0);

    // 20 threads racing to spawn on max_threads=0
    let mut handles = Vec::new();
    for _ in 0..20 {
        let ps_clone = Arc::clone(&ps);
        handles.push(thread::spawn(move || {
            for _ in 0..10 {
                let spawned = ps_clone.spawn_worker_thread_if_needed();
                assert!(!spawned);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(ps.active_worker_count(), 0);
}

#[test]
fn test_adversarial_threadpool_massive_concurrent_spawn_race() {
    let mock_driver = Arc::new(MockBinderDriver::new());
    let max_threads = 4;
    let backend = driver::MockDriverBackend::new(Arc::clone(&mock_driver), mmap::BINDER_DEFAULT_MMAP_SIZE);
    let ps = ProcessState::new(Arc::new(backend), max_threads);

    let num_threads = 50;
    let mut handles = Vec::new();

    for _ in 0..num_threads {
        let ps_clone = Arc::clone(&ps);
        handles.push(thread::spawn(move || {
            for _ in 0..50 {
                let _ = ps_clone.spawn_worker_thread_if_needed();
                let active = ps_clone.active_worker_count();
                assert!(active <= max_threads, "Active {} exceeded max {}", active, max_threads);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // Wait a brief period and ensure active count never exceeds max_threads
    thread::sleep(Duration::from_millis(50));
    assert!(ps.active_worker_count() <= max_threads);
}

