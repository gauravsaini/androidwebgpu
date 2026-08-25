//! Adversarial high-concurrency transaction burst stress tests for `crates/binder_sys`.

use aidl_compat::status::{Result, Status, STATUS_BAD_VALUE};
use aidl_compat::stub::{Binder, RemoteBinder};
use aidl_compat::traits::{Interface, Remotable};
use binder_rt::types::TransactionCode;
use binder_rt::Parcel;
use binder_sys::*;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Server service that echoes the client's request payload back with unique identification,
/// simulating variable processing delays to force out-of-order response completions.
struct VariableLatencyService {
    request_count: AtomicU32,
    checksum_total: AtomicU64,
}

impl Interface for VariableLatencyService {
    fn as_binder(&self) -> aidl_compat::pointer::SpIBinder {
        Binder::new(Self {
            request_count: AtomicU32::new(0),
            checksum_total: AtomicU64::new(0),
        })
    }
}

impl Remotable for VariableLatencyService {
    fn get_class_descriptor() -> &'static str {
        "android.test.IVariableLatencyService"
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
                let client_id = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let seq_no = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let payload_len = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                let delay_us = data
                    .read_i32(&mut offset)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                // Simulate processing latency to force out-of-order thread execution
                if delay_us > 0 {
                    thread::sleep(Duration::from_micros(delay_us as u64));
                }

                self.request_count.fetch_add(1, Ordering::SeqCst);
                self.checksum_total
                    .fetch_add((client_id as u64) ^ (seq_no as u64), Ordering::SeqCst);

                // Write response with exact client_id and seq_no for verification
                reply
                    .write_i32(client_id)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply
                    .write_i32(seq_no)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;
                reply
                    .write_i32(payload_len * 2)
                    .map_err(|_| Status::from_status(STATUS_BAD_VALUE))?;

                Ok(())
            }
            _ => Err(Status::from_status(aidl_compat::status::STATUS_UNKNOWN_TRANSACTION)),
        }
    }
}

#[test]
fn test_high_concurrency_bursts_across_loopers() {
    let mock_driver = Arc::new(MockBinderDriver::new());

    // 1. Setup Server Process with 8 worker looper threads
    let server_ps = ProcessState::init_mock(Arc::clone(&mock_driver));
    let service = Arc::new(VariableLatencyService {
        request_count: AtomicU32::new(0),
        checksum_total: AtomicU64::new(0),
    });
    let service_cookie = 0x9999;
    server_ps.register_service_object(service_cookie, Binder::new_with_arc(Arc::clone(&service)));

    let num_server_workers = 8;
    for _ in 0..num_server_workers {
        let ps_clone = Arc::clone(&server_ps);
        thread::spawn(move || {
            let mut ts = IPCThreadState::with_process(ps_clone);
            let _ = ts.enter_looper();
        });
    }

    thread::sleep(Duration::from_millis(50));

    // 2. Launch 8 concurrent Client processes, each sending 25 transactions with varying delays
    let num_clients = 8;
    let transactions_per_client = 25;
    let mut handles = Vec::new();

    for client_id in 0..num_clients {
        let driver_clone = Arc::clone(&mock_driver);
        let server_pid = server_ps.pid();

        let handle = thread::spawn(move || {
            let client_ps = ProcessState::init_mock(driver_clone.clone());
            let handle_in_client = driver_clone.add_handle_for_client(
                &driver_clone.get_client(client_ps.pid()).unwrap(),
                server_pid,
                0,
                service_cookie,
            );

            let transport =
                Arc::new(BinderKernelTransport::with_process(Arc::clone(&client_ps)));
            let proxy = RemoteBinder::new_with_transport(handle_in_client, 0, None, transport);

            for seq in 0..transactions_per_client {
                // Vary delay based on client_id and seq to cause heavy interleaving
                let delay_us = if (client_id + seq) % 3 == 0 {
                    500
                } else if (client_id + seq) % 2 == 0 {
                    200
                } else {
                    0
                };

                let mut data = Parcel::new();
                data.write_utf16(Some("android.test.IVariableLatencyService"))
                    .unwrap();
                data.write_i32(client_id).unwrap();
                data.write_i32(seq).unwrap();
                data.write_i32(100).unwrap();
                data.write_i32(delay_us).unwrap();

                let mut reply = Parcel::new();
                let res = proxy.transact(1, 0, &data, &mut reply);

                assert!(
                    res.is_ok(),
                    "Client {} tx {} failed with status: {:?}",
                    client_id,
                    seq,
                    res.err()
                );

                let mut offset = 0;
                let reply_client_id = reply
                    .read_i32(&mut offset)
                    .expect("Failed to read client_id");
                let reply_seq = reply.read_i32(&mut offset).expect("Failed to read seq");
                let reply_len = reply
                    .read_i32(&mut offset)
                    .expect("Failed to read payload_len");

                // Verify exact response integrity - no cross-talk or response swapping!
                assert_eq!(
                    reply_client_id, client_id,
                    "Cross-talk detected! Expected client {}, got {}",
                    client_id, reply_client_id
                );
                assert_eq!(
                    reply_seq, seq,
                    "Sequence mismatch! Client {} expected seq {}, got {}",
                    client_id, seq, reply_seq
                );
                assert_eq!(reply_len, 200);
            }
        });

        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Client thread panicked");
    }

    let total_expected_tx = (num_clients * transactions_per_client) as u32;
    assert_eq!(
        service.request_count.load(Ordering::SeqCst),
        total_expected_tx,
        "Total processed transactions must match total sent"
    );
}
