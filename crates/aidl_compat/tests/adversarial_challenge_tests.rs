//! Adversarial stress and verification tests for `aidl_compat`.
//!
//! Covers:
//! 1. Macro-generated AIDL stubs matching AOSP Rust codegen patterns (`declare_binder_interface!`, `declare_binder_enum!`, `ParcelableHolder`).
//! 2. Dead object recovery workflows, death recipient lifecycle, and error state transitions.
//! 3. Ping operations (`PING_TRANSACTION` / `ping_binder()`) on local, remote, and dead binders.
//! 4. Dump operations (`DUMP_TRANSACTION` / `Interface::dump`) and argument handling.
//! 5. Interface descriptor verification, querying (`INTERFACE_TRANSACTION`), and mismatch rejection.
//! 6. Concurrency stress, weak pointer lifecycle, exception propagation under load, and equality/hashing semantics.
//! 7. Empirical challenge verification of `Parcelable::read_from_parcel_at` contract.

use aidl_compat as binder;
use aidl_compat::traits::Parcelable;
use aidl_compat::Interface;
use std::collections::HashSet;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

// =============================================================================
// 1. Macro Variations Verification (AOSP-style module-scoped AIDL stubs)
// =============================================================================

pub mod foo_service {
    use super::*;

    pub trait IFooService: binder::Interface + Send + Sync {
        fn foo_op(&self, val: i32) -> binder::Result<i32>;
    }

    pub fn foo_on_transact(
        service: &dyn IFooService,
        code: binder::TransactionCode,
        data: &binder::Parcel,
        reply: &mut binder::Parcel,
    ) -> binder::Result<()> {
        if code == binder::FIRST_CALL_TRANSACTION {
            let mut offset = 0;
            let v = data
                .read_i32(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            let res = service.foo_op(v)?;
            reply
                .write_status(&binder::Status::ok())
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            reply
                .write_i32(res)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            Ok(())
        } else {
            Err(binder::Status::from_status(
                binder::STATUS_UNKNOWN_TRANSACTION,
            ))
        }
    }

    // Pattern 1: Native with on_transact path + async ident
    binder::declare_binder_interface! {
        IFooService["android.os.IFooService"] {
            native: BnFooService(foo_on_transact),
            proxy: BpFooService,
            async: IAsyncFooService,
        }
    }

    impl IFooService for BpFooService {
        fn foo_op(&self, val: i32) -> binder::Result<i32> {
            let mut data = binder::Parcel::new();
            data.write_i32(val)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            let (reply, mut offset) = binder::transact_sync(
                &self.binder,
                binder::FIRST_CALL_TRANSACTION,
                0,
                &data,
            )?;
            reply
                .read_i32(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))
        }
    }
}

pub mod bar_service {
    use super::*;

    pub trait IBarService: binder::Interface + Send + Sync {
        fn bar_op(&self, text: &str) -> binder::Result<String>;
    }

    pub fn bar_on_transact(
        service: &dyn IBarService,
        code: binder::TransactionCode,
        data: &binder::Parcel,
        reply: &mut binder::Parcel,
    ) -> binder::Result<()> {
        if code == binder::FIRST_CALL_TRANSACTION {
            let mut offset = 0;
            let s = data
                .read_utf8(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?
                .unwrap_or_default();
            let res = service.bar_op(&s)?;
            reply
                .write_status(&binder::Status::ok())
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            reply
                .write_utf8(Some(&res))
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            Ok(())
        } else {
            Err(binder::Status::from_status(
                binder::STATUS_UNKNOWN_TRANSACTION,
            ))
        }
    }

    // Pattern 2: Native with on_transact path, simple proxy
    binder::declare_binder_interface! {
        IBarService["android.os.IBarService"] {
            native: BnBarService(bar_on_transact),
            proxy: BpBarService
        }
    }

    impl IBarService for BpBarService {
        fn bar_op(&self, text: &str) -> binder::Result<String> {
            let mut data = binder::Parcel::new();
            data.write_utf8(Some(text))
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            let (reply, mut offset) = binder::transact_sync(
                &self.binder,
                binder::FIRST_CALL_TRANSACTION,
                0,
                &data,
            )?;
            let res = reply
                .read_utf8(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            Ok(res.unwrap_or_default())
        }
    }
}

pub mod baz_service {
    use super::*;

    pub trait IBazService: binder::Interface + Send + Sync {
        fn baz_op(&self) -> binder::Result<bool>;
    }

    pub fn on_transact(
        service: &dyn IBazService,
        code: binder::TransactionCode,
        _data: &binder::Parcel,
        reply: &mut binder::Parcel,
    ) -> binder::Result<()> {
        if code == binder::FIRST_CALL_TRANSACTION {
            let res = service.baz_op()?;
            reply
                .write_status(&binder::Status::ok())
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            reply
                .write_bool(res)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            Ok(())
        } else {
            Err(binder::Status::from_status(
                binder::STATUS_UNKNOWN_TRANSACTION,
            ))
        }
    }

    // Pattern 3: Native without on_transact path (defaults to in-scope on_transact) + async ident
    binder::declare_binder_interface! {
        IBazService["android.os.IBazService"] {
            native: BnBazService,
            proxy: BpBazService,
            async: IAsyncBazService,
        }
    }

    impl IBazService for BpBazService {
        fn baz_op(&self) -> binder::Result<bool> {
            let data = binder::Parcel::new();
            let (reply, mut offset) = binder::transact_sync(
                &self.binder,
                binder::FIRST_CALL_TRANSACTION,
                0,
                &data,
            )?;
            reply
                .read_bool(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))
        }
    }
}

pub mod qux_service {
    use super::*;

    pub trait IQuxService: binder::Interface + Send + Sync {
        fn quux(&self) -> binder::Result<u64>;
    }

    pub fn on_transact(
        service: &dyn IQuxService,
        code: binder::TransactionCode,
        _data: &binder::Parcel,
        reply: &mut binder::Parcel,
    ) -> binder::Result<()> {
        if code == binder::FIRST_CALL_TRANSACTION {
            let res = service.quux()?;
            reply
                .write_status(&binder::Status::ok())
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            reply
                .write_u64(res)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
            Ok(())
        } else {
            Err(binder::Status::from_status(
                binder::STATUS_UNKNOWN_TRANSACTION,
            ))
        }
    }

    // Pattern 4: Native without on_transact path, simple proxy
    binder::declare_binder_interface! {
        IQuxService["android.os.IQuxService"] {
            native: BnQuxService,
            proxy: BpQuxService
        }
    }

    impl IQuxService for BpQuxService {
        fn quux(&self) -> binder::Result<u64> {
            let data = binder::Parcel::new();
            let (reply, mut offset) = binder::transact_sync(
                &self.binder,
                binder::FIRST_CALL_TRANSACTION,
                0,
                &data,
            )?;
            reply
                .read_u64(&mut offset)
                .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))
        }
    }
}

use bar_service::{BnBarService, IBarService};
use baz_service::{BnBazService, IBazService};
use foo_service::{BnFooService, IFooService};
use qux_service::{BnQuxService, IQuxService};

// Enums with various backing types and negative/boundary values
binder::declare_binder_enum! {
    #[allow(non_camel_case_types)]
    #[derive(PartialOrd, Ord)]
    GpuFormat : [i32; 4] {
        RGBA8888 = 1,
        RGBX8888 = 2,
        RGB565 = 4,
        BGRA8888 = 5,
    }
}

binder::declare_binder_enum! {
    #[allow(non_camel_case_types)]
    GpuError : i32 {
        NONE = 0,
        DEVICE_LOST = -100,
        OUT_OF_MEMORY = -101,
        MAX_INT = i32::MAX,
    }
}

// Struct implementing Parcelable
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SurfaceConfig {
    pub width: u32,
    pub height: u32,
    pub format: i32,
    pub title: String,
}

impl binder::Parcelable for SurfaceConfig {
    fn write_to_parcel(&self, parcel: &mut binder::Parcel) -> binder::Result<()> {
        parcel
            .write_u32(self.width)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        parcel
            .write_u32(self.height)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        parcel
            .write_i32(self.format)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        parcel
            .write_utf8(Some(&self.title))
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        Ok(())
    }

    fn read_from_parcel(&mut self, parcel: &binder::Parcel) -> binder::Result<()> {
        let mut offset = 0;
        self.read_from_parcel_at(parcel, &mut offset)
    }

    fn read_from_parcel_at(
        &mut self,
        parcel: &binder::Parcel,
        offset: &mut usize,
    ) -> binder::Result<()> {
        self.width = parcel
            .read_u32(offset)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        self.height = parcel
            .read_u32(offset)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        self.format = parcel
            .read_i32(offset)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        self.title = parcel
            .read_utf8(offset)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?
            .unwrap_or_default();
        Ok(())
    }
}

// Concrete Service Implementations
struct FooServiceImpl;
impl binder::Interface for FooServiceImpl {
    fn as_binder(&self) -> binder::SpIBinder {
        BnFooService::new_binder(Self, binder::BinderFeatures::default())
    }
}
impl IFooService for FooServiceImpl {
    fn foo_op(&self, val: i32) -> binder::Result<i32> {
        Ok(val * 2)
    }
}

struct BarServiceImpl;
impl binder::Interface for BarServiceImpl {
    fn as_binder(&self) -> binder::SpIBinder {
        BnBarService::new_binder(Self, binder::BinderFeatures::default())
    }
}
impl IBarService for BarServiceImpl {
    fn bar_op(&self, text: &str) -> binder::Result<String> {
        Ok(format!("Bar:{}", text))
    }
}

struct BazServiceImpl;
impl binder::Interface for BazServiceImpl {
    fn as_binder(&self) -> binder::SpIBinder {
        BnBazService::new_binder(Self, binder::BinderFeatures::default())
    }
}
impl IBazService for BazServiceImpl {
    fn baz_op(&self) -> binder::Result<bool> {
        Ok(true)
    }
}

struct QuxServiceImpl;
impl binder::Interface for QuxServiceImpl {
    fn as_binder(&self) -> binder::SpIBinder {
        BnQuxService::new_binder(Self, binder::BinderFeatures::default())
    }
}
impl IQuxService for QuxServiceImpl {
    fn quux(&self) -> binder::Result<u64> {
        Ok(0xDEADBEEFCAFE)
    }
}

#[test]
fn test_all_macro_interface_patterns_execution() {
    // Test Pattern 1 (IFooService)
    let foo_sp = BnFooService::new_binder(FooServiceImpl, binder::BinderFeatures::default());
    let foo_proxy = <dyn IFooService as binder::FromIBinder>::try_from(foo_sp).unwrap();
    assert_eq!(foo_proxy.foo_op(21).unwrap(), 42);

    // Test Pattern 2 (IBarService)
    let bar_sp = BnBarService::new_binder(BarServiceImpl, binder::BinderFeatures::default());
    let bar_proxy = <dyn IBarService as binder::FromIBinder>::try_from(bar_sp).unwrap();
    assert_eq!(bar_proxy.bar_op("Hello").unwrap(), "Bar:Hello");

    // Test Pattern 3 (IBazService)
    let baz_sp = BnBazService::new_binder(BazServiceImpl, binder::BinderFeatures::default());
    let baz_proxy = <dyn IBazService as binder::FromIBinder>::try_from(baz_sp).unwrap();
    assert!(baz_proxy.baz_op().unwrap());

    // Test Pattern 4 (IQuxService)
    let qux_sp = BnQuxService::new_binder(QuxServiceImpl, binder::BinderFeatures::default());
    let qux_proxy = <dyn IQuxService as binder::FromIBinder>::try_from(qux_sp).unwrap();
    assert_eq!(qux_proxy.quux().unwrap(), 0xDEADBEEFCAFE);
}

#[test]
fn test_declare_binder_enum_edge_cases() {
    // 1. Serialization / Deserialization of valid enum variants
    let mut parcel = binder::Parcel::new();
    let fmt = GpuFormat::BGRA8888;
    fmt.write_to_parcel(&mut parcel).unwrap();

    let mut decoded_fmt = GpuFormat::RGBA8888;
    decoded_fmt.read_from_parcel(&parcel).unwrap();
    assert_eq!(decoded_fmt, GpuFormat::BGRA8888);

    // 2. Negative enum value roundtrip
    let mut parcel2 = binder::Parcel::new();
    let err = GpuError::DEVICE_LOST;
    err.write_to_parcel(&mut parcel2).unwrap();

    let mut decoded_err = GpuError::NONE;
    decoded_err.read_from_parcel(&parcel2).unwrap();
    assert_eq!(decoded_err, GpuError::DEVICE_LOST);

    // 3. Boundary i32::MAX enum variant
    let mut parcel3 = binder::Parcel::new();
    let max_err = GpuError::MAX_INT;
    max_err.write_to_parcel(&mut parcel3).unwrap();

    let mut decoded_max = GpuError::NONE;
    decoded_max.read_from_parcel(&parcel3).unwrap();
    assert_eq!(decoded_max, GpuError::MAX_INT);

    // 4. Invalid integer value rejected during enum unmarshaling
    let mut invalid_parcel = binder::Parcel::new();
    invalid_parcel.write_i32(99999).unwrap(); // Not a valid GpuFormat variant

    let mut bad_enum = GpuFormat::RGBA8888;
    let res = bad_enum.read_from_parcel(&invalid_parcel);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().status_code(), binder::StatusCode::BadValue);
}

#[test]
fn test_parcelable_holder_edge_cases_and_custom_structs() {
    let mut holder = binder::ParcelableHolder::new(2);
    assert_eq!(holder.get_stability(), 2);
    assert_eq!(holder.get_parcelable_name(), None);

    // Empty holder get returns Ok(None)
    assert!(holder.get_parcelable::<SurfaceConfig>().unwrap().is_none());

    // Set custom parcelable
    let cfg = SurfaceConfig {
        width: 1920,
        height: 1080,
        format: 1,
        title: "MainDisplay".to_string(),
    };
    holder
        .set_parcelable(&cfg, "android.gui.SurfaceConfig")
        .unwrap();
    assert_eq!(
        holder.get_parcelable_name(),
        Some("android.gui.SurfaceConfig")
    );

    let retrieved: Option<SurfaceConfig> = holder.get_parcelable().unwrap();
    assert_eq!(retrieved, Some(cfg.clone()));

    // Overwrite with ParcelFileDescriptor
    let pfd = binder::ParcelFileDescriptor::new(100);
    holder
        .set_parcelable(&pfd, "android.os.ParcelFileDescriptor")
        .unwrap();
    assert_eq!(
        holder.get_parcelable_name(),
        Some("android.os.ParcelFileDescriptor")
    );
    let pfd_out: Option<binder::ParcelFileDescriptor> = holder.get_parcelable().unwrap();
    assert_eq!(pfd_out, Some(binder::ParcelFileDescriptor::new(100)));

    // Serialize and deserialize holder
    let mut parcel = binder::Parcel::new();
    holder.write_to_parcel(&mut parcel).unwrap();

    let mut restored_holder = binder::ParcelableHolder::default();
    restored_holder.read_from_parcel(&parcel).unwrap();
    assert_eq!(restored_holder.get_stability(), 2);
    assert_eq!(
        restored_holder.get_parcelable_name(),
        Some("android.os.ParcelFileDescriptor")
    );

    // Reset clears state
    holder.reset();
    assert_eq!(holder.get_parcelable_name(), None);
    assert!(holder.get_parcelable::<SurfaceConfig>().unwrap().is_none());
}

// =============================================================================
// 2. Dead Object Recovery and Error States
// =============================================================================

#[test]
fn test_dead_object_error_states_and_client_recovery_lifecycle() {
    // 1. Setup service and client proxy
    let service_raw = Arc::new(binder::RemoteBinder::new_raw(42, 0x5555));
    let service_sp =
        binder::SpIBinder::from_arc(Arc::clone(&service_raw) as Arc<dyn binder::IBinder>);

    let death_notification_fired = Arc::new(AtomicBool::new(false));
    let dnf_clone = Arc::clone(&death_notification_fired);

    let death_recipient: Arc<dyn binder::DeathRecipient> =
        Arc::new(binder::DeathCallback(move || {
            dnf_clone.store(true, Ordering::SeqCst);
        }));

    service_sp.link_to_death(death_recipient).unwrap();
    assert!(service_sp.is_binder_alive());
    assert!(service_sp.ping_binder().is_ok());

    // 2. Trigger object death (simulating remote process crash / disconnection)
    service_raw.trigger_death();

    // 3. Verify death recipient fired and binder marked dead
    assert!(death_notification_fired.load(Ordering::SeqCst));
    assert!(!service_sp.is_binder_alive());

    // 4. Verify any transaction returns STATUS_DEAD_OBJECT
    let dummy_data = binder::Parcel::new();
    let mut reply = binder::Parcel::new();
    let tx_err = service_sp
        .transact(binder::FIRST_CALL_TRANSACTION, 0, &dummy_data, &mut reply)
        .unwrap_err();
    assert_eq!(tx_err.status_code(), binder::StatusCode::DeadObject);

    // 5. Verify ping_binder on dead object returns STATUS_DEAD_OBJECT
    let ping_err = service_sp.ping_binder().unwrap_err();
    assert_eq!(ping_err.status_code(), binder::StatusCode::DeadObject);

    // 6. Verify link_to_death on dead object immediately fails with STATUS_DEAD_OBJECT
    let dead_recipient: Arc<dyn binder::DeathRecipient> =
        Arc::new(binder::DeathCallback(|| {}));
    let link_err = service_sp.link_to_death(dead_recipient).unwrap_err();
    assert_eq!(link_err.status_code(), binder::StatusCode::DeadObject);

    // 7. Client Recovery Workflow:
    // When dead object is detected, client reconnects / instantiates new RemoteBinder,
    // re-links death recipient, and resumes healthy transactions.
    let recovered_raw = Arc::new(binder::RemoteBinder::new_raw(43, 0x6666));
    let recovered_sp =
        binder::SpIBinder::from_arc(Arc::clone(&recovered_raw) as Arc<dyn binder::IBinder>);

    let recovery_death_fired = Arc::new(AtomicBool::new(false));
    let rdf_clone = Arc::clone(&recovery_death_fired);
    let new_recipient: Arc<dyn binder::DeathRecipient> =
        Arc::new(binder::DeathCallback(move || {
            rdf_clone.store(true, Ordering::SeqCst);
        }));

    recovered_sp.link_to_death(new_recipient).unwrap();
    assert!(recovered_sp.is_binder_alive());
    assert!(recovered_sp.ping_binder().is_ok());
    assert!(!recovery_death_fired.load(Ordering::SeqCst));
}

// =============================================================================
// 3. Ping Operations Verification
// =============================================================================

#[test]
fn test_ping_operations_local_and_remote() {
    // 1. Local Stub Ping (must succeed without executing on_transact)
    let foo_sp = BnFooService::new_binder(FooServiceImpl, binder::BinderFeatures::default());
    assert!(foo_sp.ping_binder().is_ok());

    let mut direct_ping_reply = binder::Parcel::new();
    assert!(foo_sp
        .transact(
            binder::PING_TRANSACTION,
            0,
            &binder::Parcel::new(),
            &mut direct_ping_reply
        )
        .is_ok());

    // 2. Remote Stub Ping (default transport)
    let remote_sp = binder::RemoteBinder::new(12, 0xABCD);
    assert!(remote_sp.ping_binder().is_ok());

    // 3. Remote Stub Ping with Transport Interception
    struct PingRecordingTransport {
        ping_count: AtomicU32,
    }
    impl binder::RemoteTransport for PingRecordingTransport {
        fn transact(
            &self,
            _handle: u32,
            code: binder::TransactionCode,
            _flags: binder::TransactionFlags,
            _data: &binder::Parcel,
            _reply: &mut binder::Parcel,
        ) -> binder::Result<()> {
            if code == binder::PING_TRANSACTION {
                self.ping_count.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }
    }

    let transport = Arc::new(PingRecordingTransport {
        ping_count: AtomicU32::new(0),
    });
    let custom_remote = binder::RemoteBinder::new_with_transport(
        99,
        0xFEED,
        Some("android.os.IPingTest"),
        transport.clone(),
    );

    assert!(custom_remote.ping_binder().is_ok());
    assert_eq!(transport.ping_count.load(Ordering::SeqCst), 1);
}

// =============================================================================
// 4. Dump Operations Verification
// =============================================================================

struct DiagnosticService {
    counter: AtomicU32,
}

impl binder::Interface for DiagnosticService {
    fn as_binder(&self) -> binder::SpIBinder {
        BnFooService::new_binder(
            FooServiceImpl, // Stub wrapper
            binder::BinderFeatures::default(),
        )
    }

    fn dump(&self, writer: &mut dyn Write, args: &[&str]) -> binder::Result<()> {
        self.counter.fetch_add(1, Ordering::SeqCst);
        writeln!(
            writer,
            "DiagnosticService dump: counter={}, args={:?}",
            self.counter.load(Ordering::SeqCst),
            args
        )
        .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?;
        Ok(())
    }
}

#[test]
fn test_dump_operations_and_transactions() {
    let diag = DiagnosticService {
        counter: AtomicU32::new(10),
    };

    // 1. Interface dump method call with arguments
    let mut buffer = Vec::new();
    let dump_res = diag.dump(&mut buffer, &["--all", "--verbose"]);
    assert!(dump_res.is_ok());
    let dump_str = String::from_utf8(buffer).unwrap();
    assert!(dump_str.contains("DiagnosticService dump: counter=11"));
    assert!(dump_str.contains("--all"));

    // 2. DUMP_TRANSACTION dispatch on local Binder
    let foo_sp = BnFooService::new_binder(FooServiceImpl, binder::BinderFeatures::default());
    let mut reply = binder::Parcel::new();
    let res = foo_sp.transact(
        binder::DUMP_TRANSACTION,
        0,
        &binder::Parcel::new(),
        &mut reply,
    );
    assert!(res.is_ok());
}

// =============================================================================
// 5. Interface Descriptor Verification and Mismatch Rejection
// =============================================================================

#[test]
fn test_interface_descriptor_query_and_mismatch_rejection() {
    // 1. Local Stub INTERFACE_TRANSACTION returns exact UTF-16 descriptor
    let foo_sp = BnFooService::new_binder(FooServiceImpl, binder::BinderFeatures::default());
    assert_eq!(
        foo_sp.get_class_descriptor(),
        Some("android.os.IFooService")
    );

    let mut reply = binder::Parcel::new();
    foo_sp
        .transact(
            binder::INTERFACE_TRANSACTION,
            0,
            &binder::Parcel::new(),
            &mut reply,
        )
        .expect("INTERFACE_TRANSACTION must succeed");

    let mut offset = 0;
    let desc = reply
        .read_utf16(&mut offset)
        .expect("Reading descriptor must succeed");
    assert_eq!(desc, Some("android.os.IFooService".to_string()));

    // 2. RemoteBinder with descriptor
    let remote_sp = binder::RemoteBinder::new_with_transport(
        15,
        0x9999,
        Some("android.gui.ISurfaceComposer"),
        Arc::new(PingRecordingTransportNoop),
    );
    assert_eq!(
        remote_sp.get_class_descriptor(),
        Some("android.gui.ISurfaceComposer")
    );

    // 3. RemoteBinder without descriptor returns STATUS_UNKNOWN_TRANSACTION on INTERFACE_TRANSACTION
    let raw_remote = binder::RemoteBinder::new(20, 0x1111);
    let mut no_desc_reply = binder::Parcel::new();
    let err = raw_remote.transact(
        binder::INTERFACE_TRANSACTION,
        0,
        &binder::Parcel::new(),
        &mut no_desc_reply,
    );
    assert!(err.is_err());
    assert_eq!(
        err.unwrap_err().status_code(),
        binder::StatusCode::UnknownTransaction
    );

    // 4. Descriptor Mismatch Rejection Pattern:
    // Verify client token validation logic that guards against descriptor confusion.
    fn validate_and_transact(
        binder: &binder::SpIBinder,
        expected_descriptor: &str,
    ) -> binder::Result<()> {
        let mut desc_reply = binder::Parcel::new();
        binder.transact(
            binder::INTERFACE_TRANSACTION,
            0,
            &binder::Parcel::new(),
            &mut desc_reply,
        )?;
        let mut offset = 0;
        let actual_desc = desc_reply
            .read_utf16(&mut offset)
            .map_err(|_| binder::Status::from_status(binder::STATUS_BAD_VALUE))?
            .unwrap_or_default();

        if actual_desc != expected_descriptor {
            let msg = format!(
                "Descriptor mismatch: expected {}, got {}",
                expected_descriptor, actual_desc
            );
            return Err(binder::Status::new_exception(
                binder::ExceptionCode::Security,
                Some(&msg),
            ));
        }
        Ok(())
    }

    // Matching descriptor passes
    assert!(validate_and_transact(&foo_sp, "android.os.IFooService").is_ok());

    // Mismatched descriptor is rejected with Security exception
    let mismatch_err = validate_and_transact(&foo_sp, "android.os.IWrongService").unwrap_err();
    assert_eq!(
        mismatch_err.exception_code(),
        binder::ExceptionCode::Security
    );
    assert!(mismatch_err
        .message()
        .unwrap()
        .contains("Descriptor mismatch"));
}

struct PingRecordingTransportNoop;
impl binder::RemoteTransport for PingRecordingTransportNoop {
    fn transact(
        &self,
        _handle: u32,
        _code: binder::TransactionCode,
        _flags: binder::TransactionFlags,
        _data: &binder::Parcel,
        _reply: &mut binder::Parcel,
    ) -> binder::Result<()> {
        Ok(())
    }
}

// =============================================================================
// 6. Concurrency Stress and Smart Pointer Lifecycle
// =============================================================================

#[test]
fn test_weak_pointer_downgrade_upgrade_lifecycle() {
    let strong_sp = BnFooService::new_binder(FooServiceImpl, binder::BinderFeatures::default());
    let weak = strong_sp.downgrade();

    // While strong reference exists, upgrade succeeds
    let upgraded = weak.upgrade().expect("Upgrade should succeed");
    assert_eq!(upgraded.get_class_descriptor(), Some("android.os.IFooService"));

    drop(strong_sp);
    drop(upgraded);

    // After dropping all strong references, weak upgrade must fail
    assert!(weak.upgrade().is_none());
}

#[test]
fn test_multithreaded_concurrent_stress_and_death_notifications() {
    let service_raw = Arc::new(binder::RemoteBinder::new_raw(88, 0x8888));
    let service_sp =
        binder::SpIBinder::from_arc(Arc::clone(&service_raw) as Arc<dyn binder::IBinder>);

    let death_count = Arc::new(AtomicUsize::new(0));
    let num_threads = 16;
    let mut handles = Vec::new();

    // Register multiple death recipients concurrently
    for _ in 0..num_threads {
        let dc = Arc::clone(&death_count);
        let sp = service_sp.clone();
        let h = thread::spawn(move || {
            let recipient: Arc<dyn binder::DeathRecipient> =
                Arc::new(binder::DeathCallback(move || {
                    dc.fetch_add(1, Ordering::SeqCst);
                }));
            sp.link_to_death(recipient).unwrap();
            // Ping while alive
            assert!(sp.ping_binder().is_ok());
        });
        handles.push(h);
    }

    for h in handles {
        h.join().unwrap();
    }

    // Trigger death
    service_raw.trigger_death();

    // Verify all recipients notified exactly once
    assert_eq!(death_count.load(Ordering::SeqCst), num_threads);
    assert!(!service_sp.is_binder_alive());
}

#[test]
fn test_sp_ibinder_equality_and_hash_semantics() {
    let remote1 = binder::RemoteBinder::new(50, 0x1000);
    let remote2 = binder::RemoteBinder::new(50, 0x2000); // Same handle 50
    let remote3 = binder::RemoteBinder::new(51, 0x1000); // Different handle 51

    assert_eq!(remote1, remote2);
    assert_ne!(remote1, remote3);

    let mut set = HashSet::new();
    set.insert(remote1.clone());
    assert!(set.contains(&remote2));
    assert!(!set.contains(&remote3));

    let local1 = BnFooService::new_binder(FooServiceImpl, binder::BinderFeatures::default());
    let local2 = local1.clone();
    assert_eq!(local1, local2);
    set.insert(local1.clone());
    assert!(set.contains(&local2));
}

#[test]
fn test_all_status_exceptions_and_service_specific_errors() {
    let ex_illegal = binder::Exception(binder::ExceptionCode::IllegalArgument, "Bad arg");
    assert_eq!(ex_illegal.exception_code(), binder::ExceptionCode::IllegalArgument);
    assert_eq!(ex_illegal.message(), Some("Bad arg"));

    let ex_service = binder::ServiceSpecificException(-999, "Custom service fault");
    assert_eq!(ex_service.exception_code(), binder::ExceptionCode::ServiceSpecific);
    assert_eq!(ex_service.service_specific_error(), Some(-999));
    assert_eq!(ex_service.message(), Some("Custom service fault"));

    let status_dead = binder::Status::from_status(binder::STATUS_DEAD_OBJECT);
    assert_eq!(status_dead.status_code(), binder::StatusCode::DeadObject);
}

#[test]
fn test_empirical_discovery_read_from_parcel_at_behavior() {
    // Write two sequential i32 integers into a Parcel: [42, 99]
    let mut parcel = binder::Parcel::new();
    42i32.write_to_parcel(&mut parcel).unwrap();
    99i32.write_to_parcel(&mut parcel).unwrap();

    let mut offset = 0;
    let mut first_val = 0i32;
    first_val.read_from_parcel_at(&parcel, &mut offset).unwrap();
    assert_eq!(first_val, 42);
    assert_eq!(offset, 4);

    let mut second_val = 0i32;
    second_val.read_from_parcel_at(&parcel, &mut offset).unwrap();
    assert_eq!(second_val, 99);
    assert_eq!(offset, 8);
}
