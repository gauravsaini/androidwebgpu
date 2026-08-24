//! Adversarial stress, concurrency, and property test suite for Milestone M2 (`crates/aidl_compat`).
//! Validates thread-safety, nested AIDL dispatches, deep custom Parcelable hierarchies,
//! high-frequency DeathRecipient churn, extreme exception roundtripping, weak pointer races,
//! and empirical bug reproduction harnesses.

use aidl_compat as binder;
use binder::traits::Parcelable;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

// =============================================================================
// 1. Math and Compute Service for Multi-Threaded Stress
// =============================================================================

pub mod math_service {
    use super::*;

    pub trait IMathStressService: binder::Interface + Send + Sync {
        fn compute(&self, a: i64, b: i64, op: i32) -> binder::Result<i64>;
        fn hash_payload(&self, payload: &[u8]) -> binder::Result<u64>;
        fn echo_large_string(&self, input: &str) -> binder::Result<String>;
        fn fail_with_service_code(&self, code: i32, msg: &str) -> binder::Result<()>;
    }

    pub const TX_COMPUTE: binder::TransactionCode = binder::FIRST_CALL_TRANSACTION + 10;
    pub const TX_HASH_PAYLOAD: binder::TransactionCode = binder::FIRST_CALL_TRANSACTION + 11;
    pub const TX_ECHO_LARGE_STRING: binder::TransactionCode = binder::FIRST_CALL_TRANSACTION + 12;
    pub const TX_FAIL_WITH_CODE: binder::TransactionCode = binder::FIRST_CALL_TRANSACTION + 13;

    pub fn math_on_transact(
        service: &dyn IMathStressService,
        code: binder::TransactionCode,
        data: &binder::Parcel,
        reply: &mut binder::Parcel,
    ) -> binder::Result<()> {
        match code {
            TX_COMPUTE => {
                let mut offset = 0;
                let a = data
                    .read_i64(&mut offset)
                    .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                let b = data
                    .read_i64(&mut offset)
                    .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                let op = data
                    .read_i32(&mut offset)
                    .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                match service.compute(a, b, op) {
                    Ok(res) => {
                        reply
                            .write_status(&binder::Status::ok())
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                        reply
                            .write_i64(res)
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    }
                    Err(e) => {
                        reply
                            .write_status(&e)
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    }
                }
                Ok(())
            }
            TX_HASH_PAYLOAD => {
                let mut offset = 0;
                let bytes = data
                    .read_byte_vec(&mut offset)
                    .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                match service.hash_payload(&bytes.unwrap_or_default()) {
                    Ok(h) => {
                        reply
                            .write_status(&binder::Status::ok())
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                        reply
                            .write_u64(h)
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    }
                    Err(e) => {
                        reply
                            .write_status(&e)
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    }
                }
                Ok(())
            }
            TX_ECHO_LARGE_STRING => {
                let mut offset = 0;
                let s = data
                    .read_utf8(&mut offset)
                    .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                match service.echo_large_string(&s.unwrap_or_default()) {
                    Ok(out) => {
                        reply
                            .write_status(&binder::Status::ok())
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                        reply
                            .write_utf8(Some(&out))
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    }
                    Err(e) => {
                        reply
                            .write_status(&e)
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    }
                }
                Ok(())
            }
            TX_FAIL_WITH_CODE => {
                let mut offset = 0;
                let code_val = data
                    .read_i32(&mut offset)
                    .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                let msg = data
                    .read_utf8(&mut offset)
                    .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                match service.fail_with_service_code(code_val, &msg.unwrap_or_default()) {
                    Ok(()) => {
                        reply
                            .write_status(&binder::Status::ok())
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    }
                    Err(e) => {
                        reply
                            .write_status(&e)
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    }
                }
                Ok(())
            }
            _ => Err(binder::Status::from_status(
                binder::STATUS_UNKNOWN_TRANSACTION,
            )),
        }
    }

    binder::declare_binder_interface! {
        IMathStressService["android.os.IMathStressService"] {
            native: BnMathStressService(math_on_transact),
            proxy: BpMathStressService,
        }
    }

    impl IMathStressService for BpMathStressService {
        fn compute(&self, a: i64, b: i64, op: i32) -> binder::Result<i64> {
            let mut data = binder::Parcel::new();
            data.write_i64(a)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            data.write_i64(b)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            data.write_i32(op)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;

            let (reply, mut offset) =
                binder::transact_sync(&self.binder, TX_COMPUTE, 0, &data)?;
            reply
                .read_i64(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))
        }

        fn hash_payload(&self, payload: &[u8]) -> binder::Result<u64> {
            let mut data = binder::Parcel::new();
            data.write_byte_slice(Some(payload))
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;

            let (reply, mut offset) =
                binder::transact_sync(&self.binder, TX_HASH_PAYLOAD, 0, &data)?;
            reply
                .read_u64(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))
        }

        fn echo_large_string(&self, input: &str) -> binder::Result<String> {
            let mut data = binder::Parcel::new();
            data.write_utf8(Some(input))
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;

            let (reply, mut offset) =
                binder::transact_sync(&self.binder, TX_ECHO_LARGE_STRING, 0, &data)?;
            let res = reply
                .read_utf8(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            Ok(res.unwrap_or_default())
        }

        fn fail_with_service_code(&self, code: i32, msg: &str) -> binder::Result<()> {
            let mut data = binder::Parcel::new();
            data.write_i32(code)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            data.write_utf8(Some(msg))
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;

            let _ = binder::transact_sync(&self.binder, TX_FAIL_WITH_CODE, 0, &data)?;
            Ok(())
        }
    }

    pub struct MathStressServiceImpl {
        pub tx_count: AtomicUsize,
    }

    impl Default for MathStressServiceImpl {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MathStressServiceImpl {
        pub fn new() -> Self {
            Self {
                tx_count: AtomicUsize::new(0),
            }
        }
    }

    impl binder::Interface for MathStressServiceImpl {
        fn as_binder(&self) -> binder::SpIBinder {
            BnMathStressService::new_binder(
                Self {
                    tx_count: AtomicUsize::new(self.tx_count.load(Ordering::SeqCst)),
                },
                binder::BinderFeatures::default(),
            )
        }
    }

    impl IMathStressService for MathStressServiceImpl {
        fn compute(&self, a: i64, b: i64, op: i32) -> binder::Result<i64> {
            self.tx_count.fetch_add(1, Ordering::SeqCst);
            match op {
                0 => Ok(a.wrapping_add(b)),
                1 => Ok(a.wrapping_sub(b)),
                2 => Ok(a.wrapping_mul(b)),
                3 => {
                    if b == 0 {
                        Err(binder::Status::new_exception(
                            binder::ExceptionCode::IllegalArgument,
                            Some("Division by zero"),
                        ))
                    } else {
                        Ok(a / b)
                    }
                }
                _ => Ok(a ^ b),
            }
        }

        fn hash_payload(&self, payload: &[u8]) -> binder::Result<u64> {
            self.tx_count.fetch_add(1, Ordering::SeqCst);
            let mut h: u64 = 0xcbf29ce484222325;
            for &byte in payload {
                h ^= byte as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            Ok(h)
        }

        fn echo_large_string(&self, input: &str) -> binder::Result<String> {
            self.tx_count.fetch_add(1, Ordering::SeqCst);
            Ok(input.to_string())
        }

        fn fail_with_service_code(&self, code: i32, msg: &str) -> binder::Result<()> {
            self.tx_count.fetch_add(1, Ordering::SeqCst);
            Err(binder::Status::new_service_specific_error(code, Some(msg)))
        }
    }
}

// =============================================================================
// 2. Loopback Router & Nested AIDL Definitions
// =============================================================================

#[derive(Default)]
pub struct LoopbackTransport {
    endpoints: RwLock<HashMap<u32, binder::SpIBinder>>,
}

impl LoopbackTransport {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            endpoints: RwLock::new(HashMap::new()),
        })
    }

    pub fn register(&self, handle: u32, binder_sp: binder::SpIBinder) {
        self.endpoints.write().unwrap().insert(handle, binder_sp);
    }
}

impl binder::RemoteTransport for LoopbackTransport {
    fn transact(
        &self,
        handle: u32,
        code: binder::TransactionCode,
        flags: binder::TransactionFlags,
        data: &binder::Parcel,
        reply: &mut binder::Parcel,
    ) -> binder::Result<()> {
        let ep = {
            let guard = self.endpoints.read().unwrap();
            guard.get(&handle).cloned()
        };
        if let Some(target) = ep {
            target.transact(code, flags, data, reply)
        } else {
            Err(binder::Status::from_status(binder::STATUS_DEAD_OBJECT))
        }
    }
}

pub mod progress_callback {
    use super::*;

    pub trait IProgressCallback: binder::Interface + Send + Sync {
        fn on_progress(&self, depth: i32, msg: &str) -> binder::Result<bool>;
    }

    pub const TX_ON_PROGRESS: binder::TransactionCode = binder::FIRST_CALL_TRANSACTION + 20;

    pub fn progress_on_transact(
        service: &dyn IProgressCallback,
        code: binder::TransactionCode,
        data: &binder::Parcel,
        reply: &mut binder::Parcel,
    ) -> binder::Result<()> {
        match code {
            TX_ON_PROGRESS => {
                let mut offset = 0;
                let depth = data
                    .read_i32(&mut offset)
                    .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                let msg = data
                    .read_utf8(&mut offset)
                    .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                match service.on_progress(depth, &msg.unwrap_or_default()) {
                    Ok(continue_flag) => {
                        reply
                            .write_status(&binder::Status::ok())
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                        reply
                            .write_bool(continue_flag)
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    }
                    Err(e) => {
                        reply
                            .write_status(&e)
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    }
                }
                Ok(())
            }
            _ => Err(binder::Status::from_status(
                binder::STATUS_UNKNOWN_TRANSACTION,
            )),
        }
    }

    binder::declare_binder_interface! {
        IProgressCallback["android.os.IProgressCallback"] {
            native: BnProgressCallback(progress_on_transact),
            proxy: BpProgressCallback,
        }
    }

    impl IProgressCallback for BpProgressCallback {
        fn on_progress(&self, depth: i32, msg: &str) -> binder::Result<bool> {
            let mut data = binder::Parcel::new();
            data.write_i32(depth)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            data.write_utf8(Some(msg))
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;

            let (reply, mut offset) =
                binder::transact_sync(&self.binder, TX_ON_PROGRESS, 0, &data)?;
            reply
                .read_bool(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))
        }
    }
}

pub mod nested_worker {
    use super::*;

    pub trait INestedWorker: binder::Interface + Send + Sync {
        fn process_recursive(
            &self,
            depth: i32,
            callback_handle: u32,
        ) -> binder::Result<i32>;
    }

    pub const TX_PROCESS_RECURSIVE: binder::TransactionCode = binder::FIRST_CALL_TRANSACTION + 30;

    pub fn nested_on_transact(
        service: &dyn INestedWorker,
        code: binder::TransactionCode,
        data: &binder::Parcel,
        reply: &mut binder::Parcel,
    ) -> binder::Result<()> {
        match code {
            TX_PROCESS_RECURSIVE => {
                let mut offset = 0;
                let depth = data
                    .read_i32(&mut offset)
                    .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                let fbo = data
                    .read_binder(&mut offset)
                    .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                match service.process_recursive(depth, fbo.handle()) {
                    Ok(acc) => {
                        reply
                            .write_status(&binder::Status::ok())
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                        reply
                            .write_i32(acc)
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    }
                    Err(e) => {
                        reply
                            .write_status(&e)
                            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    }
                }
                Ok(())
            }
            _ => Err(binder::Status::from_status(
                binder::STATUS_UNKNOWN_TRANSACTION,
            )),
        }
    }

    binder::declare_binder_interface! {
        INestedWorker["android.os.INestedWorker"] {
            native: BnNestedWorker(nested_on_transact),
            proxy: BpNestedWorker,
        }
    }

    impl INestedWorker for BpNestedWorker {
        fn process_recursive(
            &self,
            depth: i32,
            callback_handle: u32,
        ) -> binder::Result<i32> {
            let mut data = binder::Parcel::new();
            data.write_i32(depth)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            data.write_binder(callback_handle, 0)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;

            let (reply, mut offset) =
                binder::transact_sync(&self.binder, TX_PROCESS_RECURSIVE, 0, &data)?;
            reply
                .read_i32(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))
        }
    }

    pub struct NestedWorkerImpl {
        pub router: Arc<LoopbackTransport>,
        pub self_handle: u32,
    }

    impl NestedWorkerImpl {
        pub fn new(router: Arc<LoopbackTransport>, self_handle: u32) -> Self {
            Self {
                router,
                self_handle,
            }
        }
    }

    impl binder::Interface for NestedWorkerImpl {
        fn as_binder(&self) -> binder::SpIBinder {
            BnNestedWorker::new_binder(
                Self {
                    router: self.router.clone(),
                    self_handle: self.self_handle,
                },
                binder::BinderFeatures::default(),
            )
        }
    }

    impl INestedWorker for NestedWorkerImpl {
        fn process_recursive(
            &self,
            depth: i32,
            callback_handle: u32,
        ) -> binder::Result<i32> {
            if depth <= 0 {
                return Ok(0);
            }

            // Create proxy to callback via loopback router
            let cb_remote_sp = binder::RemoteBinder::new_with_transport(
                callback_handle,
                0x100,
                Some("android.os.IProgressCallback"),
                self.router.clone(),
            );
            let cb_proxy: binder::Strong<dyn progress_callback::IProgressCallback> =
                <dyn progress_callback::IProgressCallback as binder::FromIBinder>::try_from(cb_remote_sp)?;

            let cont = cb_proxy.on_progress(depth, &format!("Depth {depth} step"))?;
            if !cont {
                return Err(binder::Status::new_exception(
                    binder::ExceptionCode::IllegalState,
                    Some("Aborted by callback"),
                ));
            }

            // Recursive call to self through loopback proxy if depth > 1
            if depth > 1 {
                let self_remote_sp = binder::RemoteBinder::new_with_transport(
                    self.self_handle,
                    0x200,
                    Some("android.os.INestedWorker"),
                    self.router.clone(),
                );
                let worker_proxy: binder::Strong<dyn INestedWorker> =
                    <dyn INestedWorker as binder::FromIBinder>::try_from(self_remote_sp)?;
                let sub_res = worker_proxy.process_recursive(depth - 1, callback_handle)?;
                Ok(depth + sub_res)
            } else {
                Ok(depth)
            }
        }
    }
}

// =============================================================================
// 3. Test Cases
// =============================================================================

/// Stress Test 1: Highly concurrent multi-threaded transactions hitting a single SpIBinder.
#[test]
fn test_stress_concurrent_transact_across_shared_sp_binder() {
    let service_impl = math_service::MathStressServiceImpl::new();
    let binder_sp = math_service::BnMathStressService::new_binder(service_impl, binder::BinderFeatures::default());
    let proxy: binder::Strong<dyn math_service::IMathStressService> =
        <dyn math_service::IMathStressService as binder::FromIBinder>::try_from(binder_sp).unwrap();

    let num_threads = 16;
    let iters_per_thread = 250;
    let total_tx = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for thread_id in 0..num_threads {
        let p = proxy.clone();
        let counter = Arc::clone(&total_tx);
        let h = thread::spawn(move || {
            for i in 0..iters_per_thread {
                let a = (thread_id * 1000 + i) as i64;
                let b = (i + 1) as i64;

                // 1. Add
                let sum = p.compute(a, b, 0).expect("Add failed");
                assert_eq!(sum, a + b);

                // 2. Multiply
                let prod = p.compute(a, b, 2).expect("Mul failed");
                assert_eq!(prod, a * b);

                // 3. XOR
                let xor_val = p.compute(a, b, 4).expect("XOR failed");
                assert_eq!(xor_val, a ^ b);

                // 4. Hash payload
                let payload = format!("Thread_{thread_id}_Iter_{i}_WebGPU");
                let hash = p.hash_payload(payload.as_bytes()).expect("Hash failed");
                assert_ne!(hash, 0);

                counter.fetch_add(4, Ordering::SeqCst);
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().expect("Thread joined cleanly");
    }

    assert_eq!(total_tx.load(Ordering::SeqCst), num_threads * iters_per_thread * 4);
}

/// Stress Test 2: Nested AIDL recursive callbacks across interfaces.
#[test]
fn test_stress_nested_aidl_calls_and_callbacks() {
    struct CallbackImpl {
        invocations: AtomicUsize,
    }

    impl binder::Interface for CallbackImpl {
        fn as_binder(&self) -> binder::SpIBinder {
            progress_callback::BnProgressCallback::new_binder(
                Self {
                    invocations: AtomicUsize::new(self.invocations.load(Ordering::SeqCst)),
                },
                binder::BinderFeatures::default(),
            )
        }
    }

    impl progress_callback::IProgressCallback for CallbackImpl {
        fn on_progress(&self, _depth: i32, _msg: &str) -> binder::Result<bool> {
            self.invocations.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        }
    }

    let router = LoopbackTransport::new();
    let worker_handle = 100;
    let cb_handle = 200;

    let worker_impl = nested_worker::NestedWorkerImpl::new(router.clone(), worker_handle);
    let worker_sp = nested_worker::BnNestedWorker::new_binder(worker_impl, binder::BinderFeatures::default());
    router.register(worker_handle, worker_sp.clone());

    let callback_impl = CallbackImpl {
        invocations: AtomicUsize::new(0),
    };
    let callback_sp = progress_callback::BnProgressCallback::new_binder(
        callback_impl,
        binder::BinderFeatures::default(),
    );
    router.register(cb_handle, callback_sp);

    let worker_proxy: binder::Strong<dyn nested_worker::INestedWorker> =
        <dyn nested_worker::INestedWorker as binder::FromIBinder>::try_from(worker_sp).unwrap();

    let depth = 15;
    let total_sum = worker_proxy
        .process_recursive(depth, cb_handle)
        .expect("Recursive nested call should succeed");

    // Expected sum: 15 + 14 + ... + 1 = (15 * 16) / 2 = 120
    assert_eq!(total_sum, (depth * (depth + 1)) / 2);
}

/// Stress Test 3: High-frequency DeathRecipient registration and unregistration under concurrent death notification.
#[test]
fn test_stress_high_frequency_death_recipient_churn() {
    let rb = Arc::new(binder::RemoteBinder::new_raw(42, 0xcafe));
    let remote_sp = binder::SpIBinder::from_arc(Arc::clone(&rb) as Arc<dyn binder::IBinder>);

    let num_threads = 8;
    let ops_per_thread = 200;
    let death_callbacks_fired = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();

    for _ in 0..num_threads {
        let sp = remote_sp.clone();
        let fired = Arc::clone(&death_callbacks_fired);
        let h = thread::spawn(move || {
            for _ in 0..ops_per_thread {
                let f_clone = Arc::clone(&fired);
                let recipient: Arc<dyn binder::DeathRecipient> =
                    Arc::new(binder::DeathCallback(move || {
                        f_clone.fetch_add(1, Ordering::SeqCst);
                    }));

                // Link
                let link_res = sp.link_to_death(Arc::clone(&recipient));
                if link_res.is_ok() {
                    // 50% chance to unlink immediately, 50% chance to leave linked
                    if ops_per_thread % 2 == 0 {
                        let _ = sp.unlink_to_death(&recipient);
                    }
                }
            }
        });
        handles.push(h);
    }

    // Let threads churn for a moment, then trigger death
    thread::sleep(Duration::from_millis(10));
    rb.trigger_death();

    for h in handles {
        h.join().expect("Churn thread joined cleanly");
    }

    assert!(!remote_sp.is_binder_alive());

    // Linking to dead object must fail with DEAD_OBJECT
    let dummy_recipient: Arc<dyn binder::DeathRecipient> =
        Arc::new(binder::DeathCallback(|| {}));
    let err = remote_sp.link_to_death(dummy_recipient).unwrap_err();
    assert_eq!(err.status_code(), binder::StatusCode::DeadObject);

    // Unlinking non-existent recipient must return BAD_VALUE
    let non_existent: Arc<dyn binder::DeathRecipient> =
        Arc::new(binder::DeathCallback(|| {}));
    let unlink_err = remote_sp.unlink_to_death(&non_existent).unwrap_err();
    assert_eq!(unlink_err.status_code(), binder::StatusCode::BadValue);
}

/// Stress Test 4: Exception code round-tripping with extreme payload sizes, special characters, and service-specific codes.
#[test]
fn test_stress_exception_roundtrip_extremes() {
    let test_cases = vec![
        (binder::ExceptionCode::Security, None),
        (binder::ExceptionCode::BadParcelable, Some("")),
        (binder::ExceptionCode::IllegalArgument, Some("Illegal Argument!")),
        (binder::ExceptionCode::NullPointer, Some("Null Pointer Dereference in GPU Buffer")),
        (binder::ExceptionCode::IllegalState, Some("Pipeline in Invalid State")),
        (binder::ExceptionCode::NetworkMainThread, Some("Networking on UI thread")),
        (binder::ExceptionCode::UnsupportedOperation, Some("Vulkan Extension Not Supported")),
        (binder::ExceptionCode::Parcelable, Some("Custom Parcelable Exception")),
        (binder::ExceptionCode::HasReplyHeader, Some("Header Exception")),
        (binder::ExceptionCode::TransactionFailed, Some("Kernel binder transaction failed")),
    ];

    for (code, msg) in test_cases {
        let status = binder::Status::new_exception(code, msg);
        let mut parcel = binder::Parcel::new();
        parcel.write_status(&status).unwrap();

        let mut offset = 0;
        let decoded = parcel.read_status(&mut offset).unwrap();
        assert_eq!(decoded.exception_code(), code);
        assert_eq!(decoded.message(), msg);
    }

    // Extreme service specific codes
    let service_codes = vec![
        0,
        1,
        -1,
        -404,
        -2147483648,
        i32::MAX,
        i32::MIN,
    ];

    for sc in service_codes {
        let status = binder::Status::new_service_specific_error(sc, Some("Service error detail"));
        let mut parcel = binder::Parcel::new();
        parcel.write_status(&status).unwrap();

        let mut offset = 0;
        let decoded = parcel.read_status(&mut offset).unwrap();
        assert_eq!(decoded.exception_code(), binder::ExceptionCode::ServiceSpecific);
        assert_eq!(decoded.service_specific_error(), Some(sc));
        assert_eq!(decoded.message(), Some("Service error detail"));
    }

    // Huge 64 KB and 256 KB error messages with multilingual UTF-8 and emoji
    let mut large_msg = String::with_capacity(70_000);
    for i in 0..1000 {
        large_msg.push_str(&format!("Err_{i}_🔥_🦀_WebGPU_GPU_FAULT_"));
    }

    let large_status = binder::Status::new_exception(
        binder::ExceptionCode::IllegalState,
        Some(&large_msg),
    );
    let mut parcel = binder::Parcel::new();
    parcel.write_status(&large_status).unwrap();

    let mut offset = 0;
    let decoded_large = parcel.read_status(&mut offset).unwrap();
    assert_eq!(decoded_large.exception_code(), binder::ExceptionCode::IllegalState);
    assert_eq!(decoded_large.message(), Some(large_msg.as_str()));
}

/// Stress Test 5: Weak pointer (WpIBinder) upgrade and downgrade concurrent races under deallocation.
#[test]
fn test_stress_weak_pointer_concurrency_race() {
    let iters = 100;
    for _ in 0..iters {
        let rb = Arc::new(binder::RemoteBinder::new_raw(100, 0xbeef));
        let sp = binder::SpIBinder::from_arc(Arc::clone(&rb) as Arc<dyn binder::IBinder>);
        let wp = sp.downgrade();

        let wp_clone = wp.clone();
        let upgraded_count = Arc::new(AtomicUsize::new(0));
        let dropped_count = Arc::new(AtomicUsize::new(0));

        let uc = Arc::clone(&upgraded_count);
        let dc = Arc::clone(&dropped_count);

        let h = thread::spawn(move || {
            for _ in 0..50 {
                match wp_clone.upgrade() {
                    Some(strong) => {
                        assert!(strong.is_binder_alive());
                        uc.fetch_add(1, Ordering::SeqCst);
                    }
                    None => {
                        dc.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        });

        // Drop strong pointer in main thread while background thread upgrades
        drop(sp);
        drop(rb);

        h.join().expect("Race thread joined cleanly");
        assert!(upgraded_count.load(Ordering::SeqCst) + dropped_count.load(Ordering::SeqCst) == 50);
    }
}

/// Stress Test 6: SpIBinder equality and hashing in collections across threads.
#[test]
fn test_stress_sp_binder_hash_and_equality_in_collections() {
    let mut map = HashMap::new();
    let mut set = HashSet::new();

    let b1 = binder::RemoteBinder::new(10, 0x1111);
    let b1_dup = binder::RemoteBinder::new(10, 0x2222); // Same handle 10 -> equal
    let b2 = binder::RemoteBinder::new(20, 0x3333);

    map.insert(b1.clone(), "Service_10");
    map.insert(b2.clone(), "Service_20");

    assert_eq!(map.get(&b1_dup), Some(&"Service_10"));
    assert_eq!(map.len(), 2);

    set.insert(b1.clone());
    set.insert(b1_dup.clone());
    set.insert(b2.clone());
    assert_eq!(set.len(), 2);
}

/// Stress Test 7: Remote transport concurrent failure and timeout simulation.
#[test]
fn test_stress_remote_transport_concurrent_failures() {
    struct FlakyTransport {
        failure_counter: AtomicI32,
    }

    impl binder::RemoteTransport for FlakyTransport {
        fn transact(
            &self,
            handle: u32,
            code: binder::TransactionCode,
            _flags: binder::TransactionFlags,
            _data: &binder::Parcel,
            reply: &mut binder::Parcel,
        ) -> binder::Result<()> {
            let count = self.failure_counter.fetch_add(1, Ordering::SeqCst);
            if count % 3 == 0 {
                Err(binder::Status::from_status(binder::STATUS_TIMED_OUT))
            } else if count % 3 == 1 {
                Err(binder::Status::from_status(binder::STATUS_NO_MEMORY))
            } else {
                reply
                    .write_status(&binder::Status::ok())
                    .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                reply
                    .write_i64((handle as i64) * 1000 + (code as i64))
                    .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                Ok(())
            }
        }
    }

    let transport = Arc::new(FlakyTransport {
        failure_counter: AtomicI32::new(0),
    });

    let remote_sp = binder::RemoteBinder::new_with_transport(
        99,
        0x5555,
        Some("android.os.IMathStressService"),
        transport,
    );

    let proxy: binder::Strong<dyn math_service::IMathStressService> =
        <dyn math_service::IMathStressService as binder::FromIBinder>::try_from(remote_sp).unwrap();

    let num_threads = 8;
    let iters = 60;
    let timeouts = Arc::new(AtomicUsize::new(0));
    let nomems = Arc::new(AtomicUsize::new(0));
    let successes = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..num_threads {
        let p = proxy.clone();
        let to = Arc::clone(&timeouts);
        let nm = Arc::clone(&nomems);
        let sc = Arc::clone(&successes);
        let h = thread::spawn(move || {
            for _ in 0..iters {
                match p.compute(10, 20, 0) {
                    Ok(val) => {
                        assert_eq!(val, 99 * 1000 + (math_service::TX_COMPUTE as i64));
                        sc.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(e) => {
                        if e.status_code() == binder::StatusCode::TimedOut {
                            to.fetch_add(1, Ordering::SeqCst);
                        } else if e.status_code() == binder::StatusCode::NoMemory {
                            nm.fetch_add(1, Ordering::SeqCst);
                        } else {
                            panic!("Unexpected error: {:?}", e);
                        }
                    }
                }
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(
        timeouts.load(Ordering::SeqCst) + nomems.load(Ordering::SeqCst) + successes.load(Ordering::SeqCst),
        num_threads * iters
    );
    assert!(timeouts.load(Ordering::SeqCst) > 0);
    assert!(nomems.load(Ordering::SeqCst) > 0);
    assert!(successes.load(Ordering::SeqCst) > 0);
}

// =============================================================================
// 4. Empirical Bug Remediation Tests
// =============================================================================

/// VERIFICATION 1: `Parcelable::read_from_parcel_at` properly advances `offset` and reads from non-zero offsets.
#[test]
fn test_bug_reproduce_primitive_read_from_parcel_at_discards_offset() {
    let mut p = binder::Parcel::new();
    p.write_i32(111).unwrap();
    p.write_i32(222).unwrap();

    let mut offset = 4; // Read second i32 (222) at byte offset 4
    let mut val: i32 = 0;
    val.read_from_parcel_at(&p, &mut offset).unwrap();

    assert_eq!(val, 222, "read_from_parcel_at correctly reads second integer from offset 4");
    assert_eq!(offset, 8, "offset advanced by 4 bytes to 8");
}

/// VERIFICATION 2: `ParcelFileDescriptor::read_from_parcel_at` succeeds on composite parcels at non-zero offsets.
#[test]
fn test_bug_reproduce_parcel_file_descriptor_at_nonzero_offset() {
    let mut p = binder::Parcel::new();
    p.write_i32(9999).unwrap(); // Preceding header / field (4 bytes)
    let pfd = binder::ParcelFileDescriptor::new(55);
    pfd.write_to_parcel(&mut p).unwrap();

    let mut offset = 4; // Offset of the ParcelFileDescriptor
    let mut decoded_pfd = binder::ParcelFileDescriptor::default();

    decoded_pfd.read_from_parcel_at(&p, &mut offset).unwrap();
    assert_eq!(decoded_pfd.as_raw_fd(), 55);
    assert_eq!(offset, 4 + 24); // FlatBinderObject is 24 bytes on wire
}

/// VERIFICATION 3: `ParcelableHolder::read_from_parcel_at` succeeds on composite parcels at non-zero offsets.
#[test]
fn test_bug_reproduce_parcelable_holder_at_nonzero_offset() {
    let mut p = binder::Parcel::new();
    p.write_i32(7777).unwrap(); // Preceding field (4 bytes)
    let mut holder = binder::ParcelableHolder::new(10);
    holder.set_parcelable(&42i32, "android.os.Int").unwrap();
    holder.write_to_parcel(&mut p).unwrap();

    let mut offset = 4;
    let mut decoded_holder = binder::ParcelableHolder::default();

    decoded_holder.read_from_parcel_at(&p, &mut offset).unwrap();
    assert_eq!(decoded_holder.get_stability(), 10);
    assert_eq!(decoded_holder.get_parcelable_name(), Some("android.os.Int"));
    let int_val: Option<i32> = decoded_holder.get_parcelable().unwrap();
    assert_eq!(int_val, Some(42));
    assert!(offset > 4, "offset was advanced past the ParcelableHolder payload");
}

/// VERIFICATION 4: Multiple AIDL interfaces declared in the same module scope compile and work without `_BnWrapper` name collisions.
mod multi_interface_same_scope {
    use super::*;

    pub trait IAlphaService: binder::Interface + Send + Sync {
        fn get_alpha(&self) -> binder::Result<i32>;
    }

    pub fn alpha_on_transact(
        service: &dyn IAlphaService,
        code: binder::TransactionCode,
        _data: &binder::Parcel,
        reply: &mut binder::Parcel,
    ) -> binder::Result<()> {
        if code == binder::FIRST_CALL_TRANSACTION {
            let val = service.get_alpha()?;
            reply.write_status(&binder::Status::ok()).unwrap();
            reply.write_i32(val).unwrap();
            Ok(())
        } else {
            Err(binder::Status::from_status(binder::STATUS_UNKNOWN_TRANSACTION))
        }
    }

    pub trait IBetaService: binder::Interface + Send + Sync {
        fn get_beta(&self) -> binder::Result<String>;
    }

    pub fn beta_on_transact(
        service: &dyn IBetaService,
        code: binder::TransactionCode,
        _data: &binder::Parcel,
        reply: &mut binder::Parcel,
    ) -> binder::Result<()> {
        if code == binder::FIRST_CALL_TRANSACTION {
            let val = service.get_beta()?;
            reply.write_status(&binder::Status::ok()).unwrap();
            reply.write_utf8(Some(&val)).unwrap();
            Ok(())
        } else {
            Err(binder::Status::from_status(binder::STATUS_UNKNOWN_TRANSACTION))
        }
    }

    binder::declare_binder_interface! {
        IAlphaService["android.os.IAlphaService"] {
            native: BnAlphaService(alpha_on_transact),
            proxy: BpAlphaService,
        }
    }

    impl IAlphaService for BpAlphaService {
        fn get_alpha(&self) -> binder::Result<i32> {
            let data = binder::Parcel::new();
            let (reply, mut offset) = binder::transact_sync(&self.binder, binder::FIRST_CALL_TRANSACTION, 0, &data)?;
            reply.read_i32(&mut offset).map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))
        }
    }

    binder::declare_binder_interface! {
        IBetaService["android.os.IBetaService"] {
            native: BnBetaService(beta_on_transact),
            proxy: BpBetaService,
        }
    }

    impl IBetaService for BpBetaService {
        fn get_beta(&self) -> binder::Result<String> {
            let data = binder::Parcel::new();
            let (reply, mut offset) = binder::transact_sync(&self.binder, binder::FIRST_CALL_TRANSACTION, 0, &data)?;
            let s = reply.read_utf8(&mut offset).map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            Ok(s.unwrap_or_default())
        }
    }

    struct AlphaImpl;
    impl binder::Interface for AlphaImpl {
        fn as_binder(&self) -> binder::SpIBinder {
            BnAlphaService::new_binder(AlphaImpl, binder::BinderFeatures::default())
        }
    }
    impl IAlphaService for AlphaImpl {
        fn get_alpha(&self) -> binder::Result<i32> {
            Ok(100)
        }
    }

    struct BetaImpl;
    impl binder::Interface for BetaImpl {
        fn as_binder(&self) -> binder::SpIBinder {
            BnBetaService::new_binder(BetaImpl, binder::BinderFeatures::default())
        }
    }
    impl IBetaService for BetaImpl {
        fn get_beta(&self) -> binder::Result<String> {
            Ok("beta_ok".to_string())
        }
    }

    #[test]
    fn test_multiple_interfaces_in_same_module_scope_no_collision() {
        let alpha_binder = BnAlphaService::new_binder(AlphaImpl, binder::BinderFeatures::default());
        let beta_binder = BnBetaService::new_binder(BetaImpl, binder::BinderFeatures::default());

        assert_eq!(
            alpha_binder.get_class_descriptor(),
            Some("android.os.IAlphaService")
        );
        assert_eq!(
            beta_binder.get_class_descriptor(),
            Some("android.os.IBetaService")
        );

        let alpha_proxy: binder::Strong<dyn IAlphaService> =
            <dyn IAlphaService as binder::FromIBinder>::try_from(alpha_binder).unwrap();
        let beta_proxy: binder::Strong<dyn IBetaService> =
            <dyn IBetaService as binder::FromIBinder>::try_from(beta_binder).unwrap();

        assert_eq!(
            alpha_proxy.as_binder().get_class_descriptor(),
            Some("android.os.IAlphaService")
        );
        assert_eq!(
            beta_proxy.as_binder().get_class_descriptor(),
            Some("android.os.IBetaService")
        );
    }
}
