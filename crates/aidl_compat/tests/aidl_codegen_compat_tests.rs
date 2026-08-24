//! Integration tests validating AOSP AIDL Rust compatibility.

use aidl_compat as binder;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

// =============================================================================
// 1. AOSP-Style AIDL Generated Interface Definition
// =============================================================================

/// Simulated generated trait for `android.os.ITestService.aidl`.
pub trait ITestService: binder::Interface + Send + Sync {
    fn add(&self, a: i32, b: i32) -> binder::Result<i32>;
    fn repeat(&self, msg: &str, count: i32) -> binder::Result<String>;
    fn trigger_exception(&self, error_type: i32) -> binder::Result<()>;
    fn send_data(&self, data: &[u8], reply_data: &mut Vec<u8>) -> binder::Result<()>;
    fn get_gpu_status(&self) -> binder::Result<GpuStatusCode>;
}

// Enum declared with AOSP AIDL macro
binder::declare_binder_enum! {
    #[derive(PartialOrd, Ord)]
    GpuStatusCode : [i32; 3] {
        READY = 0,
        BUSY = 1,
        ERROR = 2,
    }
}

pub const TRANSACTION_ADD: binder::TransactionCode = binder::FIRST_CALL_TRANSACTION;
pub const TRANSACTION_REPEAT: binder::TransactionCode = binder::FIRST_CALL_TRANSACTION + 1;
pub const TRANSACTION_TRIGGER_EXCEPTION: binder::TransactionCode =
    binder::FIRST_CALL_TRANSACTION + 2;
pub const TRANSACTION_SEND_DATA: binder::TransactionCode = binder::FIRST_CALL_TRANSACTION + 3;
pub const TRANSACTION_GET_GPU_STATUS: binder::TransactionCode = binder::FIRST_CALL_TRANSACTION + 4;

/// Generated `on_transact` dispatcher function.
fn on_transact(
    service: &dyn ITestService,
    code: binder::TransactionCode,
    data: &binder::Parcel,
    reply: &mut binder::Parcel,
) -> binder::Result<()> {
    match code {
        TRANSACTION_ADD => {
            let mut offset = 0;
            let a = data
                .read_i32(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            let b = data
                .read_i32(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            match service.add(a, b) {
                Ok(val) => {
                    reply
                        .write_status(&binder::Status::ok())
                        .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    reply
                        .write_i32(val)
                        .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                }
                Err(err) => {
                    reply
                        .write_status(&err)
                        .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                }
            }
            Ok(())
        }
        TRANSACTION_REPEAT => {
            let mut offset = 0;
            let msg = data
                .read_utf8(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            let count = data
                .read_i32(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            match service.repeat(&msg.unwrap_or_default(), count) {
                Ok(val) => {
                    reply
                        .write_status(&binder::Status::ok())
                        .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    reply
                        .write_utf8(Some(&val))
                        .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                }
                Err(err) => {
                    reply
                        .write_status(&err)
                        .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                }
            }
            Ok(())
        }
        TRANSACTION_TRIGGER_EXCEPTION => {
            let mut offset = 0;
            let error_type = data
                .read_i32(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            match service.trigger_exception(error_type) {
                Ok(()) => {
                    reply
                        .write_status(&binder::Status::ok())
                        .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                }
                Err(err) => {
                    reply
                        .write_status(&err)
                        .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                }
            }
            Ok(())
        }
        TRANSACTION_SEND_DATA => {
            let mut offset = 0;
            let in_bytes = data
                .read_byte_vec(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            let mut out_bytes = Vec::new();
            match service.send_data(&in_bytes.unwrap_or_default(), &mut out_bytes) {
                Ok(()) => {
                    reply
                        .write_status(&binder::Status::ok())
                        .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    reply
                        .write_byte_slice(Some(&out_bytes))
                        .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                }
                Err(err) => {
                    reply
                        .write_status(&err)
                        .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                }
            }
            Ok(())
        }
        TRANSACTION_GET_GPU_STATUS => {
            match service.get_gpu_status() {
                Ok(status) => {
                    reply
                        .write_status(&binder::Status::ok())
                        .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
                    status.write_to_parcel(reply)?;
                }
                Err(err) => {
                    reply
                        .write_status(&err)
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

// Interface declaration matching official `aidl --lang=rust`
binder::declare_binder_interface! {
    ITestService["android.os.ITestService"] {
        native: BnTestService(on_transact),
        proxy: BpTestService,
    }
}

impl ITestService for BpTestService {
    fn add(&self, a: i32, b: i32) -> binder::Result<i32> {
        let mut data = binder::Parcel::new();
        data.write_i32(a)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        data.write_i32(b)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        let mut reply = binder::Parcel::new();
        self.binder.transact(TRANSACTION_ADD, 0, &data, &mut reply)?;
        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        reply
            .read_i32(&mut offset)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))
    }

    fn repeat(&self, msg: &str, count: i32) -> binder::Result<String> {
        let mut data = binder::Parcel::new();
        data.write_utf8(Some(msg))
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        data.write_i32(count)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        let mut reply = binder::Parcel::new();
        self.binder
            .transact(TRANSACTION_REPEAT, 0, &data, &mut reply)?;
        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        let res = reply
            .read_utf8(&mut offset)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        Ok(res.unwrap_or_default())
    }

    fn trigger_exception(&self, error_type: i32) -> binder::Result<()> {
        let mut data = binder::Parcel::new();
        data.write_i32(error_type)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        let mut reply = binder::Parcel::new();
        self.binder
            .transact(TRANSACTION_TRIGGER_EXCEPTION, 0, &data, &mut reply)?;
        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    fn send_data(&self, in_data: &[u8], reply_data: &mut Vec<u8>) -> binder::Result<()> {
        let mut data = binder::Parcel::new();
        data.write_byte_slice(Some(in_data))
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        let mut reply = binder::Parcel::new();
        self.binder
            .transact(TRANSACTION_SEND_DATA, 0, &data, &mut reply)?;
        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        let res = reply
            .read_byte_vec(&mut offset)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        *reply_data = res.unwrap_or_default();
        Ok(())
    }

    fn get_gpu_status(&self) -> binder::Result<GpuStatusCode> {
        let data = binder::Parcel::new();
        let mut reply = binder::Parcel::new();
        self.binder
            .transact(TRANSACTION_GET_GPU_STATUS, 0, &data, &mut reply)?;
        let mut offset = 0;
        let status = reply
            .read_status(&mut offset)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        if !status.is_ok() {
            return Err(status);
        }
        let mut gpu_status = GpuStatusCode::READY;
        gpu_status.read_from_parcel_at(&reply, &mut offset)?;
        Ok(gpu_status)
    }
}

// =============================================================================
// 2. Concrete Server Implementation
// =============================================================================

struct TestServiceImpl {
    calls: AtomicU32,
}

impl TestServiceImpl {
    fn new() -> Self {
        Self {
            calls: AtomicU32::new(0),
        }
    }
}

impl binder::Interface for TestServiceImpl {
    fn as_binder(&self) -> binder::SpIBinder {
        BnTestService::new_binder(
            Self {
                calls: AtomicU32::new(self.calls.load(Ordering::SeqCst)),
            },
            binder::BinderFeatures::default(),
        )
    }
}

impl ITestService for TestServiceImpl {
    fn add(&self, a: i32, b: i32) -> binder::Result<i32> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(a + b)
    }

    fn repeat(&self, msg: &str, count: i32) -> binder::Result<String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(msg.repeat(count as usize))
    }

    fn trigger_exception(&self, error_type: i32) -> binder::Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match error_type {
            1 => Err(binder::Status::new_exception(
                binder::ExceptionCode::IllegalArgument,
                Some("Invalid argument provided to TestService"),
            )),
            2 => Err(binder::Status::new_service_specific_error(
                -404,
                Some("Surface layer not found"),
            )),
            3 => Err(binder::Status::new_exception(
                binder::ExceptionCode::NullPointer,
                Some("Null buffer handle"),
            )),
            _ => Ok(()),
        }
    }

    fn send_data(&self, in_data: &[u8], reply_data: &mut Vec<u8>) -> binder::Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut reversed = in_data.to_vec();
        reversed.reverse();
        *reply_data = reversed;
        Ok(())
    }

    fn get_gpu_status(&self) -> binder::Result<GpuStatusCode> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(GpuStatusCode::BUSY)
    }
}

// =============================================================================
// 3. Integration Tests
// =============================================================================

#[test]
fn test_synchronous_method_calls_and_returns() {
    let service_impl = TestServiceImpl::new();
    let binder_sp = BnTestService::new_binder(service_impl, binder::BinderFeatures::default());

    // Convert SpIBinder to Strong<dyn ITestService> via FromIBinder
    let proxy: binder::Strong<dyn ITestService> =
        <dyn ITestService as binder::FromIBinder>::try_from(binder_sp)
            .expect("FromIBinder must succeed");

    // 1. Primitive arithmetic call
    let sum = proxy.add(100, 42).expect("add should succeed");
    assert_eq!(sum, 142);

    // 2. String repeat call
    let repeated = proxy
        .repeat("WebGPU-", 3)
        .expect("repeat should succeed");
    assert_eq!(repeated, "WebGPU-WebGPU-WebGPU-");

    // 3. Byte buffer in/out manipulation
    let mut reply_data = Vec::new();
    proxy
        .send_data(&[1, 2, 3, 4, 5], &mut reply_data)
        .expect("send_data should succeed");
    assert_eq!(reply_data, vec![5, 4, 3, 2, 1]);

    // 4. AIDL Enum return
    let gpu_status = proxy.get_gpu_status().expect("get_gpu_status must succeed");
    assert_eq!(gpu_status, GpuStatusCode::BUSY);
}

#[test]
fn test_exception_propagation_across_transactions() {
    let service_impl = TestServiceImpl::new();
    let binder_sp = BnTestService::new_binder(service_impl, binder::BinderFeatures::default());
    let proxy: binder::Strong<dyn ITestService> =
        <dyn ITestService as binder::FromIBinder>::try_from(binder_sp).unwrap();

    // 1. Test EX_ILLEGAL_ARGUMENT
    let err1 = proxy.trigger_exception(1).unwrap_err();
    assert_eq!(
        err1.exception_code(),
        binder::ExceptionCode::IllegalArgument
    );
    assert_eq!(
        err1.message(),
        Some("Invalid argument provided to TestService")
    );

    // 2. Test EX_SERVICE_SPECIFIC
    let err2 = proxy.trigger_exception(2).unwrap_err();
    assert_eq!(
        err2.exception_code(),
        binder::ExceptionCode::ServiceSpecific
    );
    assert_eq!(err2.service_specific_error(), Some(-404));
    assert_eq!(err2.message(), Some("Surface layer not found"));

    // 3. Test EX_NULL_POINTER
    let err3 = proxy.trigger_exception(3).unwrap_err();
    assert_eq!(err3.exception_code(), binder::ExceptionCode::NullPointer);
    assert_eq!(err3.message(), Some("Null buffer handle"));

    // 4. Test success (error_type 0)
    let ok_res = proxy.trigger_exception(0);
    assert!(ok_res.is_ok());
}

#[test]
fn test_death_recipient_lifecycle_and_invalidation() {
    let service_impl = TestServiceImpl::new();
    let binder_sp = BnTestService::new_binder(service_impl, binder::BinderFeatures::default());

    let died_flag = Arc::new(AtomicBool::new(false));
    let died_clone = Arc::clone(&died_flag);

    let recipient: Arc<dyn binder::DeathRecipient> =
        Arc::new(binder::DeathCallback(move || {
            died_clone.store(true, Ordering::SeqCst);
        }));

    // Link recipient to binder
    binder_sp
        .link_to_death(Arc::clone(&recipient))
        .expect("Linking death recipient must succeed");

    assert!(binder_sp.is_binder_alive());
    assert!(!died_flag.load(Ordering::SeqCst));

    // Ping must succeed while alive
    assert!(binder_sp.ping_binder().is_ok());

    // Unlink test
    binder_sp
        .unlink_to_death(&recipient)
        .expect("Unlinking recipient must succeed");

    // Re-link
    binder_sp
        .link_to_death(Arc::clone(&recipient))
        .expect("Re-linking recipient must succeed");

    // Test RemoteBinder death trigger lifecycle
    let rb = Arc::new(binder::RemoteBinder::new_raw(5, 0x9999));
    let remote_sp = binder::SpIBinder::from_arc(Arc::clone(&rb) as Arc<dyn binder::IBinder>);

    let remote_died = Arc::new(AtomicBool::new(false));
    let rd_clone = Arc::clone(&remote_died);
    let remote_recipient: Arc<dyn binder::DeathRecipient> =
        Arc::new(binder::DeathCallback(move || {
            rd_clone.store(true, Ordering::SeqCst);
        }));

    remote_sp
        .link_to_death(Arc::clone(&remote_recipient))
        .expect("Link remote death must succeed");
    assert!(!remote_died.load(Ordering::SeqCst));
    assert!(remote_sp.is_binder_alive());

    // Trigger death on remote proxy
    rb.trigger_death();

    // Verify death recipient callback fired
    assert!(remote_died.load(Ordering::SeqCst));
    assert!(!remote_sp.is_binder_alive());

    // Transactions and linking to dead object must fail with DEAD_OBJECT
    let data = binder::Parcel::new();
    let mut reply = binder::Parcel::new();
    let err = remote_sp
        .transact(TRANSACTION_ADD, 0, &data, &mut reply)
        .unwrap_err();
    assert_eq!(err.status_code(), binder::StatusCode::DeadObject);

    let link_err = remote_sp
        .link_to_death(Arc::clone(&remote_recipient))
        .unwrap_err();
    assert_eq!(link_err.status_code(), binder::StatusCode::DeadObject);
}

#[test]
fn test_parcelable_holder_and_fd_roundtrip() {
    let mut holder = binder::ParcelableHolder::new(1);
    assert_eq!(holder.get_stability(), 1);
    assert!(holder
        .get_parcelable::<binder::ParcelFileDescriptor>()
        .unwrap()
        .is_none());

    let pfd = binder::ParcelFileDescriptor::new(42);
    holder
        .set_parcelable(&pfd, "android.os.ParcelFileDescriptor")
        .expect("set_parcelable must succeed");

    assert_eq!(
        holder.get_parcelable_name(),
        Some("android.os.ParcelFileDescriptor")
    );

    let retrieved: Option<binder::ParcelFileDescriptor> = holder
        .get_parcelable()
        .expect("get_parcelable must succeed");
    assert_eq!(retrieved, Some(binder::ParcelFileDescriptor::new(42)));

    // Serialize and deserialize holder
    let mut p = binder::Parcel::new();
    holder
        .write_to_parcel(&mut p)
        .expect("Holder write must succeed");

    let mut new_holder = binder::ParcelableHolder::default();
    new_holder
        .read_from_parcel(&p)
        .expect("Holder read must succeed");
    assert_eq!(new_holder.get_stability(), 1);
    assert_eq!(
        new_holder.get_parcelable_name(),
        Some("android.os.ParcelFileDescriptor")
    );

    let from_deserialized: Option<binder::ParcelFileDescriptor> = new_holder
        .get_parcelable()
        .expect("get_parcelable from deserialized holder must succeed");
    assert_eq!(
        from_deserialized,
        Some(binder::ParcelFileDescriptor::new(42))
    );
}

#[test]
fn test_concurrent_multithreaded_transactions() {
    let service_impl = TestServiceImpl::new();
    let binder_sp = BnTestService::new_binder(service_impl, binder::BinderFeatures::default());
    let proxy: binder::Strong<dyn ITestService> =
        <dyn ITestService as binder::FromIBinder>::try_from(binder_sp).unwrap();

    let completed_count = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for thread_idx in 0..8 {
        let p = proxy.clone();
        let cc = Arc::clone(&completed_count);
        let h = thread::spawn(move || {
            for i in 0..50 {
                let res = p.add(thread_idx * 100, i).expect("concurrent add failed");
                assert_eq!(res, thread_idx * 100 + i);
                cc.fetch_add(1, Ordering::SeqCst);
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().expect("Thread must join cleanly");
    }

    assert_eq!(completed_count.load(Ordering::SeqCst), 8 * 50);
}

#[test]
fn test_remote_binder_transport_dispatch() {
    struct MockTransport {
        calls: AtomicU32,
    }

    impl binder::RemoteTransport for MockTransport {
        fn transact(
            &self,
            handle: u32,
            code: binder::TransactionCode,
            _flags: binder::TransactionFlags,
            _data: &binder::Parcel,
            reply: &mut binder::Parcel,
        ) -> binder::Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(handle, 7);
            assert_eq!(code, TRANSACTION_ADD);
            reply
                .write_status(&binder::Status::ok())
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            reply
                .write_i32(999)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            Ok(())
        }
    }

    let transport = Arc::new(MockTransport {
        calls: AtomicU32::new(0),
    });

    let remote_sp = binder::RemoteBinder::new_with_transport(
        7,
        0x1000,
        Some("android.os.ITestService"),
        transport.clone(),
    );

    let proxy: binder::Strong<dyn ITestService> =
        <dyn ITestService as binder::FromIBinder>::try_from(remote_sp).unwrap();

    let result = proxy.add(1, 2).expect("Remote call should succeed");
    assert_eq!(result, 999);
    assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
}
