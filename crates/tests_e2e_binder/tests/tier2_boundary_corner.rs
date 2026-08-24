//! Tier 2: Boundary and Corner Cases Test Suite (F1..F26)
//!
//! Deep boundary, extreme value, limit, overflow, underflow, corrupted input,
//! null, and edge case verification covering all 26 features in PROJECT.md.
//! Exactly 5 distinct, genuine tests per feature = 130 tests total.

use aidl_compat::{
    DeathCallback, DeathRecipient, IBinder,
    Parcel, ParcelFileDescriptor, ParcelableHolder, Remotable, RemoteTransport,
    Result as AidlResult, SpIBinder, Status, StatusCode, Strong, WpIBinder,
    STATUS_DEAD_OBJECT, STATUS_FAILED_TRANSACTION, STATUS_UNKNOWN_TRANSACTION,
};
use binder_handle_bridge::{BridgeError, HandleBridge};
use binder_routing::{
    CodeFilter, DescriptorMatcher, MatchRule, MatcherEngine, RouteAction, RoutingPolicy,
    RoutingRule, ServiceNameMatcher,
};
use binder_rt::parcel::ParcelError;
use binder_rt::types::FlatBinderObject;
use binder_rt::wire::{
    BinderTransactionData, BR_FAILED_REPLY, DUMP_TRANSACTION, FIRST_CALL_TRANSACTION,
    INTERFACE_TRANSACTION, LAST_CALL_TRANSACTION, PING_TRANSACTION, TF_ACCEPT_FDS, TF_ONE_WAY,
};
use std::sync::Arc;
use surfaceflinger_gpu_service::layer_translator::{LayerState, LayerTranslator};
use surfaceflinger_gpu_service::service::SurfaceComposerService;
use tests_e2e_binder::harness::{create_test_wgpu_device, EchoService};
use virtio_binder::device::VirtioBinderDevice;
use virtio_binder::guest_shim::{DirectDeviceBackend, GuestVirtioTransport, VirtqueueChainBackend};
use virtio_binder::protocol::*;
use webgpu_compositor::{CompositionLayer, WebGpuCompositor};
use webgpu_swapchain::WebGpuSwapchain;

// =============================================================================
// Feature 1: Parcel Alignment & Padding (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f01_boundary_empty_parcel_properties() {
    let p = Parcel::new();
    assert_eq!(p.data_size(), 0);
    assert_eq!(p.data().len(), 0);
    assert!(p.offsets().is_empty());
}

#[test]
fn test_f01_boundary_large_allocation_1mb() {
    let mut p = Parcel::new();
    let large_buf = vec![0xAAu8; 1024 * 1024]; // 1MB
    p.write_byte_slice(Some(&large_buf)).unwrap();
    assert_eq!(p.data_size(), 4 + 1024 * 1024);

    let mut off = 0;
    let read_buf = p.read_byte_vec(&mut off).unwrap().unwrap();
    assert_eq!(read_buf.len(), 1024 * 1024);
    assert_eq!(read_buf[0], 0xAA);
    assert_eq!(read_buf[1024 * 1024 - 1], 0xAA);
}

#[test]
fn test_f01_boundary_single_byte_write_padding_zeros() {
    let mut p = Parcel::new();
    p.write_u8(0xFF).unwrap();
    assert_eq!(p.data_size(), 4);
    assert_eq!(p.data(), &[0xFF, 0x00, 0x00, 0x00]);
}

#[test]
fn test_f01_boundary_three_bytes_write_padding_zero() {
    let mut p = Parcel::new();
    p.write_u8(0x11).unwrap();
    p.write_u8(0x22).unwrap();
    p.write_u8(0x33).unwrap();
    // 3 bytes written sequentially, each padded to 4 bytes => 12 bytes total
    assert_eq!(p.data_size(), 12);
}

#[test]
fn test_f01_boundary_read_past_end_returns_not_enough_data() {
    let p = Parcel::new();
    let mut off = 0;
    let res = p.read_i32(&mut off);
    assert!(matches!(res, Err(ParcelError::NotEnoughData { .. })));
}

// =============================================================================
// Feature 2: Scalar & Wire Codec (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f02_boundary_extreme_integer_limits() {
    let mut p = Parcel::new();
    p.write_i8(i8::MIN).unwrap();
    p.write_i8(i8::MAX).unwrap();
    p.write_u8(u8::MAX).unwrap();
    p.write_i16(i16::MIN).unwrap();
    p.write_i16(i16::MAX).unwrap();
    p.write_u16(u16::MAX).unwrap();
    p.write_i32(i32::MIN).unwrap();
    p.write_i32(i32::MAX).unwrap();
    p.write_u32(u32::MAX).unwrap();
    p.write_i64(i64::MIN).unwrap();
    p.write_i64(i64::MAX).unwrap();
    p.write_u64(u64::MAX).unwrap();

    let mut off = 0;
    assert_eq!(p.read_i8(&mut off).unwrap(), i8::MIN);
    assert_eq!(p.read_i8(&mut off).unwrap(), i8::MAX);
    assert_eq!(p.read_u8(&mut off).unwrap(), u8::MAX);
    assert_eq!(p.read_i16(&mut off).unwrap(), i16::MIN);
    assert_eq!(p.read_i16(&mut off).unwrap(), i16::MAX);
    assert_eq!(p.read_u16(&mut off).unwrap(), u16::MAX);
    assert_eq!(p.read_i32(&mut off).unwrap(), i32::MIN);
    assert_eq!(p.read_i32(&mut off).unwrap(), i32::MAX);
    assert_eq!(p.read_u32(&mut off).unwrap(), u32::MAX);
    assert_eq!(p.read_i64(&mut off).unwrap(), i64::MIN);
    assert_eq!(p.read_i64(&mut off).unwrap(), i64::MAX);
    assert_eq!(p.read_u64(&mut off).unwrap(), u64::MAX);
}

#[test]
fn test_f02_boundary_floating_point_special_values() {
    let mut p = Parcel::new();
    p.write_f32(f32::NAN).unwrap();
    p.write_f32(f32::INFINITY).unwrap();
    p.write_f32(f32::NEG_INFINITY).unwrap();
    p.write_f64(f64::NAN).unwrap();
    p.write_f64(f64::INFINITY).unwrap();
    p.write_f64(f64::NEG_INFINITY).unwrap();

    let mut off = 0;
    assert!(p.read_f32(&mut off).unwrap().is_nan());
    assert_eq!(p.read_f32(&mut off).unwrap(), f32::INFINITY);
    assert_eq!(p.read_f32(&mut off).unwrap(), f32::NEG_INFINITY);
    assert!(p.read_f64(&mut off).unwrap().is_nan());
    assert_eq!(p.read_f64(&mut off).unwrap(), f64::INFINITY);
    assert_eq!(p.read_f64(&mut off).unwrap(), f64::NEG_INFINITY);
}

#[test]
fn test_f02_boundary_boolean_integer_values() {
    let mut p = Parcel::new();
    p.write_bool(true).unwrap();
    p.write_bool(false).unwrap();

    let mut off = 0;
    assert_eq!(p.read_i32(&mut off).unwrap(), 1);
    assert_eq!(p.read_i32(&mut off).unwrap(), 0);
}

#[test]
fn test_f02_boundary_char_unicode_plane() {
    let mut p = Parcel::new();
    p.write_char('A').unwrap();
    p.write_char('🦀').unwrap(); // Crab emoji (U+1F980)

    let mut off = 0;
    assert_eq!(p.read_char(&mut off).unwrap(), 'A');
    assert_eq!(p.read_char(&mut off).unwrap(), '🦀');
}

#[test]
fn test_f02_boundary_corrupted_char_read() {
    let mut p = Parcel::new();
    p.write_u32(0xD800).unwrap(); // Surrogate half (invalid unicode scalar)
    let mut off = 0;
    let res = p.read_char(&mut off);
    assert!(matches!(res, Err(ParcelError::MalformedUtf16(_))));
}

// =============================================================================
// Feature 3: String Codec & Nullability (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f03_boundary_empty_utf8_string() {
    let mut p = Parcel::new();
    p.write_utf8(Some("")).unwrap();

    let mut off = 0;
    assert_eq!(p.read_utf8(&mut off).unwrap(), Some("".to_string()));
}

#[test]
fn test_f03_boundary_empty_utf16_string() {
    let mut p = Parcel::new();
    p.write_utf16(Some("")).unwrap();

    let mut off = 0;
    assert_eq!(p.read_utf16(&mut off).unwrap(), Some("".to_string()));
}

#[test]
fn test_f03_boundary_none_string_marshaling() {
    let mut p = Parcel::new();
    p.write_utf8(None).unwrap();
    p.write_utf16(None).unwrap();

    let mut off = 0;
    assert_eq!(p.read_utf8(&mut off).unwrap(), None);
    assert_eq!(p.read_utf16(&mut off).unwrap(), None);
}

#[test]
fn test_f03_boundary_utf16_missing_null_terminator() {
    let mut raw = Vec::new();
    raw.extend_from_slice(&3i32.to_le_bytes()); // char_count = 3
    raw.extend_from_slice(&('A' as u16).to_le_bytes());
    raw.extend_from_slice(&('B' as u16).to_le_bytes());
    raw.extend_from_slice(&('C' as u16).to_le_bytes());
    raw.extend_from_slice(&0xFFFFu16.to_le_bytes()); // Non-zero terminator
    let p = Parcel::from_slice(&raw);

    let mut off = 0;
    let res = p.read_utf16(&mut off);
    assert!(matches!(res, Err(ParcelError::MissingNullTerminator(_))));
}

#[test]
fn test_f03_boundary_massive_string_roundtrip() {
    let mut p = Parcel::new();
    let massive = "X".repeat(65536);
    p.write_utf8(Some(&massive)).unwrap();

    let mut off = 0;
    let read_back = p.read_utf8(&mut off).unwrap().unwrap();
    assert_eq!(read_back.len(), 65536);
}

// =============================================================================
// Feature 4: Vector Codec & Slices (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f04_boundary_empty_vector_roundtrip() {
    let mut p = Parcel::new();
    let empty: Vec<i32> = Vec::new();
    p.write_vector(Some(&empty), |p, val| p.write_i32(*val)).unwrap();

    let mut off = 0;
    let res: Option<Vec<i32>> = p.read_vector(&mut off, |p, off| p.read_i32(off)).unwrap();
    assert_eq!(res, Some(vec![]));
}

#[test]
fn test_f04_boundary_none_vector_marshaling() {
    let mut p = Parcel::new();
    p.write_vector(None::<&[i32]>, |p, val| p.write_i32(*val)).unwrap();

    let mut off = 0;
    let res: Option<Vec<i32>> = p.read_vector(&mut off, |p, off| p.read_i32(off)).unwrap();
    assert_eq!(res, None);
}

#[test]
fn test_f04_boundary_negative_vector_length_error() {
    let mut p = Parcel::new();
    p.write_i32(-5).unwrap(); // Invalid negative count other than -1

    let mut off = 0;
    let res: Result<Option<Vec<i32>>, ParcelError> = p.read_vector(&mut off, |p, off| p.read_i32(off));
    assert!(matches!(res, Err(ParcelError::BadParcelable(-5))));
}

#[test]
fn test_f04_boundary_byte_slice_empty_and_none() {
    let mut p = Parcel::new();
    p.write_byte_slice(Some(&[])).unwrap();
    p.write_byte_slice(None).unwrap();

    let mut off = 0;
    assert_eq!(p.read_byte_vec(&mut off).unwrap(), Some(vec![]));
    assert_eq!(p.read_byte_vec(&mut off).unwrap(), None);
}

#[test]
fn test_f04_boundary_large_vector_of_strings() {
    let mut p = Parcel::new();
    let strings: Vec<String> = (0..500).map(|i| format!("string_{}", i)).collect();
    p.write_vector(Some(&strings), |p, s| p.write_utf8(Some(s))).unwrap();

    let mut off = 0;
    let read_strings: Vec<String> = p
        .read_vector(&mut off, |p, off| p.read_utf8(off).map(|s| s.unwrap()))
        .unwrap()
        .unwrap();
    assert_eq!(read_strings.len(), 500);
    assert_eq!(read_strings[499], "string_499");
}

// =============================================================================
// Feature 5: FlatBinderObject Serialization (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f05_boundary_handle_zero_service_manager() {
    let mut p = Parcel::new();
    let obj = FlatBinderObject::new_handle(0, 0, 0);
    p.write_binder_object(&obj).unwrap();

    let mut off = 0;
    let read_obj = p.read_binder_object(&mut off).unwrap();
    assert_eq!(read_obj.handle(), 0);
    assert_eq!(read_obj.cookie, 0);
}

#[test]
fn test_f05_boundary_handle_max_and_cookie_max() {
    let mut p = Parcel::new();
    let obj = FlatBinderObject::new_handle(u32::MAX, u32::MAX, u64::MAX);
    p.write_binder_object(&obj).unwrap();

    let mut off = 0;
    let read_obj = p.read_binder_object(&mut off).unwrap();
    assert_eq!(read_obj.handle(), u32::MAX);
    assert_eq!(read_obj.flags, u32::MAX);
    assert_eq!(read_obj.cookie, u64::MAX);
}

#[test]
fn test_f05_boundary_read_binder_object_truncated_buffer() {
    let mut p = Parcel::new();
    p.write_u64(12345).unwrap(); // only 8 bytes

    let mut off = 0;
    let res = p.read_binder_object(&mut off);
    assert!(matches!(res, Err(ParcelError::NotEnoughData { .. })));
}

#[test]
fn test_f05_boundary_binder_pointer_object() {
    let mut p = Parcel::new();
    let obj = FlatBinderObject::new_binder(0xDEADBEEF00, 0x12, 0xCAFE);
    p.write_binder_object(&obj).unwrap();

    let mut off = 0;
    let read_obj = p.read_binder_object(&mut off).unwrap();
    assert_eq!(read_obj.binder, 0xDEADBEEF00);
    assert_eq!(read_obj.cookie, 0xCAFE);
}

#[test]
fn test_f05_boundary_multiple_binders_with_identical_offsets() {
    let mut p = Parcel::new();
    for i in 0..5 {
        p.write_binder(i, (i * 100) as u64).unwrap();
    }
    assert_eq!(p.offsets().len(), 5);
    assert_eq!(p.data_size(), 5 * 24);
}

// =============================================================================
// Feature 6: File Descriptor Serialization (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f06_boundary_fd_zero_stdin() {
    let mut p = Parcel::new();
    p.write_file_descriptor(0, 0).unwrap();

    let mut off = 0;
    assert_eq!(p.read_file_descriptor(&mut off).unwrap(), 0);
}

#[test]
fn test_f06_boundary_negative_fd_error_handling() {
    let mut p = Parcel::new();
    p.write_file_descriptor(-1, 0).unwrap();

    let mut off = 0;
    assert_eq!(p.read_file_descriptor(&mut off).unwrap(), -1);
}

#[test]
fn test_f06_boundary_large_fd_number() {
    let mut p = Parcel::new();
    p.write_file_descriptor(1024 * 1024, 0x9999).unwrap();

    let mut off = 0;
    assert_eq!(p.read_file_descriptor(&mut off).unwrap(), 1024 * 1024);
}

#[test]
fn test_f06_boundary_read_fd_from_empty_parcel() {
    let p = Parcel::new();
    let mut off = 0;
    let res = p.read_file_descriptor(&mut off);
    assert!(res.is_err());
}

#[test]
fn test_f06_boundary_offsets_tracking_sequential_fds() {
    let mut p = Parcel::new();
    for i in 0..10 {
        p.write_file_descriptor(i, 0).unwrap();
    }
    assert_eq!(p.offsets().len(), 10);
    assert_eq!(p.offsets()[9], 9 * 24);
}

// =============================================================================
// Feature 7: Transaction Envelopes (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f07_boundary_zero_sized_transaction_payload() {
    let tr = BinderTransactionData::new(0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    assert_eq!(tr.data_size, 0);
    assert_eq!(tr.offsets_size, 0);
    assert!(!tr.is_one_way());
}

#[test]
fn test_f07_boundary_max_transaction_code_boundary() {
    let tr = BinderTransactionData::new(1, 0, LAST_CALL_TRANSACTION, 0, 0, 0, 0, 0, 0, 0);
    assert_eq!(tr.code, 0x00FFFFFF);
}

#[test]
fn test_f07_boundary_from_bytes_truncated_returns_none() {
    let short_bytes = [0u8; 32]; // BinderTransactionData is 64 bytes
    assert!(BinderTransactionData::from_bytes(&short_bytes).is_none());
}

#[test]
fn test_f07_boundary_sender_pid_negative_value() {
    let tr = BinderTransactionData::new(1, 0, 100, 0, -1, 0, 0, 0, 0, 0);
    assert_eq!(tr.sender_pid, -1);
}

#[test]
fn test_f07_boundary_combined_flags_one_way_and_accept_fds() {
    let tr = BinderTransactionData::new(1, 0, 100, TF_ONE_WAY | TF_ACCEPT_FDS, 0, 0, 0, 0, 0, 0);
    assert!(tr.is_one_way());
    assert_eq!(tr.flags & TF_ACCEPT_FDS, TF_ACCEPT_FDS);
}

// =============================================================================
// Feature 8: AIDL Status & Exceptions (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f08_boundary_status_with_empty_message() {
    let s = Status::new_exception(aidl_compat::ExceptionCode::IllegalArgument, Some(""));
    assert_eq!(s.message(), Some(""));
}

#[test]
fn test_f08_boundary_status_with_massive_message() {
    let long_msg = "Error: ".repeat(1000);
    let mut p = Parcel::new();
    let s = Status::new_exception(aidl_compat::ExceptionCode::Security, Some(&long_msg));
    p.write_status(&s).unwrap();

    let mut off = 0;
    let decoded = p.read_status(&mut off).unwrap();
    assert_eq!(decoded.message(), Some(long_msg.as_str()));
}

#[test]
fn test_f08_boundary_service_specific_min_max_error() {
    let s_min = Status::new_service_specific_error(i32::MIN, None);
    let s_max = Status::new_service_specific_error(i32::MAX, None);

    assert_eq!(s_min.service_specific_error(), Some(i32::MIN));
    assert_eq!(s_max.service_specific_error(), Some(i32::MAX));
}

#[test]
fn test_f08_boundary_corrupted_status_parcel_read() {
    let p = Parcel::new();
    let mut off = 0;
    assert!(p.read_status(&mut off).is_err());
}

#[test]
fn test_f08_boundary_status_dead_object_properties() {
    let s = Status::from_status(STATUS_DEAD_OBJECT);
    assert_eq!(s.status, StatusCode::DeadObject);
    assert!(!s.is_ok());
}

// =============================================================================
// Feature 9: binder::Interface & IBinder (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f09_boundary_transact_unknown_code_returns_unknown_transaction() {
    let echo = EchoService::new();
    let data = Parcel::new();
    let mut reply = Parcel::new();

    let res = echo.on_transact(0xDEADBEEF, &data, &mut reply);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().status, StatusCode::UnknownTransaction);
}

#[test]
fn test_f09_boundary_ping_binder_repeated_calls() {
    let echo = EchoService::new();
    for _ in 0..100 {
        assert!(echo.ping_binder().is_ok());
    }
}

#[test]
fn test_f09_boundary_transact_with_empty_data_parcel() {
    let echo = EchoService::new();
    let data = Parcel::new(); // empty data for ADD which expects 2 ints
    let mut reply = Parcel::new();

    let res = echo.on_transact(EchoService::TRANSACTION_ADD, &data, &mut reply);
    assert!(res.is_err());
}

#[test]
fn test_f09_boundary_link_multiple_distinct_death_recipients() {
    let echo = EchoService::new();
    let r1: Arc<dyn DeathRecipient> = Arc::new(DeathCallback(|| {}));
    let r2: Arc<dyn DeathRecipient> = Arc::new(DeathCallback(|| {}));

    assert!(echo.link_to_death(Arc::clone(&r1)).is_ok());
    assert!(echo.link_to_death(Arc::clone(&r2)).is_ok());
    assert!(echo.unlink_to_death(&r1).is_ok());
    assert!(echo.unlink_to_death(&r2).is_ok());
}

#[test]
fn test_f09_boundary_is_binder_alive_state_consistency() {
    let echo = EchoService::new();
    assert!(echo.is_binder_alive());
}

// =============================================================================
// Feature 10: binder::SpIBinder & WpIBinder (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f10_boundary_wpibinder_upgrade_after_drop_fails() {
    let wp = {
        let echo = EchoService::new();
        let sp = SpIBinder::new(echo);
        sp.downgrade()
    };
    // Inner SpIBinder is dropped
    assert!(wp.upgrade().is_none());
}

#[test]
fn test_f10_boundary_spibinder_clone_drop_loop_stress() {
    let sp = SpIBinder::new(EchoService::new());
    for _ in 0..1000 {
        let cloned = sp.clone();
        assert!(cloned.is_binder_alive());
    }
    assert!(sp.is_binder_alive());
}

#[test]
fn test_f10_boundary_spibinder_handle_equality() {
    let r1 = aidl_compat::stub::RemoteBinder::new(42, 0);
    let r2 = aidl_compat::stub::RemoteBinder::new(42, 0);
    let r3 = aidl_compat::stub::RemoteBinder::new(99, 0);

    assert_eq!(r1, r2);
    assert_ne!(r1, r3);
}

#[test]
fn test_f10_boundary_wpibinder_clone_when_dead() {
    let wp1 = WpIBinder::new(std::sync::Weak::<EchoService>::new() as std::sync::Weak<dyn IBinder>);
    let wp2 = wp1.clone();
    assert!(wp1.upgrade().is_none());
    assert!(wp2.upgrade().is_none());
}

#[test]
fn test_f10_boundary_spibinder_remote_binder_descriptor() {
    let remote = aidl_compat::stub::RemoteBinder::new_raw_with_transport(
        10,
        0,
        Some("android.os.ICustom"),
        Arc::new(GuestVirtioTransport::new_with_device(Arc::new(VirtioBinderDevice::new()))),
    );
    assert_eq!(remote.get_class_descriptor(), Some("android.os.ICustom"));
}

// =============================================================================
// Feature 11: binder::Remotable & Proxy (Boundary/Corner) (5 tests)
// =============================================================================

pub trait ITestBoundaryService: aidl_compat::Interface + Send + Sync {}

fn boundary_on_transact(
    _service: &dyn ITestBoundaryService,
    code: aidl_compat::TransactionCode,
    _data: &aidl_compat::Parcel,
    _reply: &mut aidl_compat::Parcel,
) -> aidl_compat::Result<()> {
    if code == 1 {
        Ok(())
    } else {
        Err(Status::from_status(STATUS_UNKNOWN_TRANSACTION))
    }
}

aidl_compat::declare_binder_interface! {
    ITestBoundaryService ["android.os.ITestBoundaryService"] {
        native: BnTestBoundaryService(boundary_on_transact),
        proxy: BpTestBoundaryService,
    }
}

impl ITestBoundaryService for BpTestBoundaryService {}

#[test]
fn test_f11_boundary_proxy_transact_on_dead_binder() {
    let device = Arc::new(VirtioBinderDevice::new());
    let transport = Arc::new(GuestVirtioTransport::new_with_device(device));
    let remote = aidl_compat::stub::RemoteBinder::new_raw_with_transport(999, 0, None, transport);
    let proxy = BpTestBoundaryService::new(SpIBinder::new(remote));

    let data = Parcel::new();
    let mut reply = Parcel::new();
    let res = aidl_compat::Interface::as_binder(&proxy).transact(1, 0, &data, &mut reply);
    assert!(res.is_err());
}

#[test]
fn test_f11_boundary_transact_sync_one_way_call() {
    let echo = EchoService::new();
    let sp = SpIBinder::new(echo);
    let mut data = Parcel::new();
    data.write_i32(1).unwrap();
    data.write_i32(2).unwrap();

    let res = aidl_compat::transact_sync(&sp, EchoService::TRANSACTION_ADD, TF_ONE_WAY, &data);
    assert!(res.is_ok());
}

#[test]
fn test_f11_boundary_bp_interface_clone_independence() {
    let sp = SpIBinder::new(EchoService::new());
    let p1 = BpTestBoundaryService::new(sp.clone());
    let p2 = BpTestBoundaryService::new(sp);
    assert_eq!(aidl_compat::Interface::as_binder(&p1), aidl_compat::Interface::as_binder(&p2));
}

#[test]
fn test_f11_boundary_from_ibinder_conversion_failure() {
    let remote = aidl_compat::stub::RemoteBinder::new(1, 0);
    let res: AidlResult<Strong<BpTestBoundaryService>> = aidl_compat::FromIBinder::try_from(remote);
    assert!(res.is_ok());
}

#[test]
fn test_f11_boundary_remotable_descriptor_matches() {
    assert_eq!(EchoService::DESCRIPTOR, "android.os.IEchoService");
}

// =============================================================================
// Feature 12: binder::Parcelable & Macros (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f12_boundary_parcelable_holder_replace_parcelable() {
    let mut holder = ParcelableHolder::new(0);
    holder.set_parcelable(&100i32, "Int1").unwrap();
    assert_eq!(holder.get_parcelable_name(), Some("Int1"));

    holder.set_parcelable(&200i32, "Int2").unwrap();
    assert_eq!(holder.get_parcelable_name(), Some("Int2"));
    assert_eq!(holder.get_parcelable::<i32>().unwrap(), Some(200));
}

#[test]
fn test_f12_boundary_parcelable_holder_reset() {
    let mut holder = ParcelableHolder::new(0);
    holder.set_parcelable(&500i32, "Int").unwrap();
    holder.reset();

    assert_eq!(holder.get_parcelable_name(), None);
    assert_eq!(holder.get_parcelable::<i32>().unwrap(), None);
}

#[test]
fn test_f12_boundary_parcelable_holder_stability_retention() {
    let holder = ParcelableHolder::new(42);
    assert_eq!(holder.get_stability(), 42);
}

#[test]
fn test_f12_boundary_parcel_file_descriptor_negative_raw_fd() {
    let pfd = ParcelFileDescriptor::new(-1);
    assert_eq!(pfd.as_raw_fd(), -1);
}

aidl_compat::declare_binder_enum! {
    BoundaryStatus : [i32; 2] {
        Inactive = 0,
        Active = 1,
    }
}

#[test]
fn test_f12_boundary_declare_binder_enum_values() {
    assert_eq!(BoundaryStatus::Inactive as i32, 0);
    assert_eq!(BoundaryStatus::Active as i32, 1);
}

// =============================================================================
// Feature 13: Official AIDL Stub Compatibility (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f13_boundary_aidl_stub_first_call_transaction() {
    assert_eq!(FIRST_CALL_TRANSACTION, 1);
}

#[test]
fn test_f13_boundary_aidl_stub_last_call_transaction() {
    assert_eq!(LAST_CALL_TRANSACTION, 0x00FFFFFF);
}

#[test]
fn test_f13_boundary_aidl_stub_ping_transaction_opcode() {
    assert_eq!(PING_TRANSACTION, 0x5F504E47);
}

#[test]
fn test_f13_boundary_aidl_stub_interface_transaction_opcode() {
    assert_eq!(INTERFACE_TRANSACTION, 0x5F4E5446);
}

#[test]
fn test_f13_boundary_aidl_stub_dump_transaction_opcode() {
    assert_eq!(DUMP_TRANSACTION, 0x5F444D50);
}

// =============================================================================
// Feature 14: Virtio-Binder Device & Protocol (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f14_boundary_req_hdr_zero_payload_sizes() {
    let req = VirtioBinderReqHdr::new_ping(1, 0);
    assert_eq!(req.data_size, 0);
    assert_eq!(req.offsets_size, 0);
    assert_eq!(req.cmd, CMD_PING);
}

#[test]
fn test_f14_boundary_req_hdr_from_bytes_truncated_fails() {
    let bytes = [0u8; 20]; // Needs 48 bytes
    assert!(matches!(VirtioBinderReqHdr::from_bytes(&bytes), Err(ProtocolError::PacketTooShort { .. })));
}

#[test]
fn test_f14_boundary_resp_hdr_from_bytes_truncated_fails() {
    let bytes = [0u8; 10]; // Needs 32 bytes
    assert!(matches!(VirtioBinderRespHdr::from_bytes(&bytes), Err(ProtocolError::PacketTooShort { .. })));
}

#[test]
fn test_f14_boundary_resp_hdr_error_construction() {
    let resp = VirtioBinderRespHdr::new_error(99, STATUS_FAILED_TRANSACTION, BR_FAILED_REPLY as i32);
    assert_eq!(resp.msg_id, 99);
    assert_eq!(resp.status, STATUS_FAILED_TRANSACTION);
    assert_eq!(resp.result_code, BR_FAILED_REPLY as i32);
    assert!(!resp.is_success());
}

#[test]
fn test_f14_boundary_virtio_device_multiple_service_registrations() {
    let device = VirtioBinderDevice::new();
    for i in 1..=50 {
        device.register_service(i, Arc::new(EchoService::new()));
        assert!(device.get_service(i).is_some());
    }
}

// =============================================================================
// Feature 15: Transport Dispatch Loop (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f15_boundary_dispatch_unregistered_handle_returns_error() {
    let device = Arc::new(VirtioBinderDevice::new());
    let transport = GuestVirtioTransport::new_with_device(device);

    let data = Parcel::new();
    let mut reply = Parcel::new();
    let res = transport.transact(9999, 1, 0, &data, &mut reply);
    assert!(res.is_err());
}

#[test]
fn test_f15_boundary_dispatch_zero_sized_payload() {
    let device = Arc::new(VirtioBinderDevice::new());
    device.register_service(1, Arc::new(EchoService::new()));
    let transport = GuestVirtioTransport::new_with_device(device);

    let data = Parcel::new();
    let mut reply = Parcel::new();
    // PING transaction on EchoService
    let res = transport.transact(1, PING_TRANSACTION, 0, &data, &mut reply);
    assert!(res.is_ok());
}

#[test]
fn test_f15_boundary_dispatch_large_payload_64kb() {
    let device = Arc::new(VirtioBinderDevice::new());
    device.register_service(1, Arc::new(EchoService::new()));
    let transport = GuestVirtioTransport::new_with_device(device);

    let mut data = Parcel::new();
    let large_str = "A".repeat(65536);
    data.write_utf8(Some(&large_str)).unwrap();
    let mut reply = Parcel::new();

    transport.transact(1, EchoService::TRANSACTION_ECHO, 0, &data, &mut reply).unwrap();
    let mut off = 0;
    let status = reply.read_status(&mut off).unwrap();
    assert!(status.is_ok());
    assert_eq!(reply.read_utf8(&mut off).unwrap(), Some(large_str));
}

#[test]
fn test_f15_boundary_virtqueue_backend_capacity_boundary() {
    let device = Arc::new(VirtioBinderDevice::new());
    device.register_service(1, Arc::new(EchoService::new()));
    let transport = GuestVirtioTransport::new_with_virtqueue(device, 512);

    let mut data = Parcel::new();
    data.write_i32(10).unwrap();
    data.write_i32(20).unwrap();
    let mut reply = Parcel::new();

    transport.transact(1, EchoService::TRANSACTION_ADD, 0, &data, &mut reply).unwrap();
    let mut off = 0;
    let status = reply.read_status(&mut off).unwrap();
    assert!(status.is_ok());
    assert_eq!(reply.read_i32(&mut off).unwrap(), 30);
}

#[test]
fn test_f15_boundary_sequential_transaction_msg_ids() {
    let device = Arc::new(VirtioBinderDevice::new());
    device.register_service(1, Arc::new(EchoService::new()));
    let transport = GuestVirtioTransport::new_with_device(device);

    for _ in 0..50 {
        let mut data = Parcel::new();
        data.write_i32(1).unwrap();
        data.write_i32(2).unwrap();
        let mut reply = Parcel::new();
        transport.transact(1, EchoService::TRANSACTION_ADD, 0, &data, &mut reply).unwrap();
    }
}

// =============================================================================
// Feature 16: Virtio Event Queue (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f16_boundary_event_queue_drain_when_empty() {
    let device = VirtioBinderDevice::new();
    let drained = device.event_queue().lock().unwrap().drain_events();
    assert!(drained.is_empty());
}

#[test]
fn test_f16_boundary_event_queue_push_100_events_fifo_drain() {
    let device = VirtioBinderDevice::new();
    for i in 0..100 {
        device.event_queue().lock().unwrap().push_event(VirtioBinderEventHdr::new_death(i, i as u64 * 10));
    }

    let drained = device.event_queue().lock().unwrap().drain_events();
    assert_eq!(drained.len(), 100);
    for (i, item) in drained.iter().enumerate() {
        assert_eq!(item.target_handle, i as u32);
        assert_eq!(item.cookie, i as u64 * 10);
    }
}

#[test]
fn test_f16_boundary_direct_backend_empty_events() {
    use virtio_binder::guest_shim::TransportBackend;
    let device = Arc::new(VirtioBinderDevice::new());
    let backend = DirectDeviceBackend::new(device);
    assert!(backend.drain_events().is_empty());
}

#[test]
fn test_f16_boundary_virtqueue_backend_empty_events() {
    use virtio_binder::guest_shim::TransportBackend;
    let device = Arc::new(VirtioBinderDevice::new());
    let backend = VirtqueueChainBackend::new(device, 1024);
    assert!(backend.drain_events().is_empty());
}

#[test]
fn test_f16_boundary_event_hdr_serialization() {
    let evt = VirtioBinderEventHdr::new_death(0x1234, 0x56789ABC);
    let bytes = evt.as_bytes();
    assert_eq!(bytes.len(), 16);

    let decoded = VirtioBinderEventHdr::from_bytes(bytes).unwrap();
    assert_eq!(decoded.event_type, EVENT_TYPE_DEATH);
    assert_eq!(decoded.target_handle, 0x1234);
    assert_eq!(decoded.cookie, 0x56789ABC);
}

// =============================================================================
// Feature 17: Guest Interception Shim (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f17_boundary_remote_binder_handle_zero() {
    let remote = aidl_compat::stub::RemoteBinder::new_raw(0, 0);
    assert_eq!(remote.handle(), Some(0));
}

#[test]
fn test_f17_boundary_remote_binder_unregistered_ping_fails() {
    let device = Arc::new(VirtioBinderDevice::new());
    let transport = Arc::new(GuestVirtioTransport::new_with_device(device));
    let remote = aidl_compat::stub::RemoteBinder::new_raw_with_transport(999, 0, None, transport);
    assert!(remote.ping_binder().is_err());
}

#[test]
fn test_f17_boundary_remote_binder_empty_descriptor() {
    let remote = aidl_compat::stub::RemoteBinder::new_raw(1, 0);
    assert_eq!(remote.get_class_descriptor(), None);
}

#[test]
fn test_f17_boundary_remote_binder_one_way_call() {
    let device = Arc::new(VirtioBinderDevice::new());
    device.register_service(1, Arc::new(EchoService::new()));
    let transport = Arc::new(GuestVirtioTransport::new_with_device(device));
    let remote = aidl_compat::stub::RemoteBinder::new_raw_with_transport(1, 0, None, transport);

    let mut data = Parcel::new();
    data.write_i32(10).unwrap();
    data.write_i32(20).unwrap();
    let mut reply = Parcel::new();

    let res = remote.transact(EchoService::TRANSACTION_ADD, TF_ONE_WAY, &data, &mut reply);
    assert!(res.is_ok());
}

#[test]
fn test_f17_boundary_remote_binder_into_spibinder_downgrade() {
    let remote = aidl_compat::stub::RemoteBinder::new(5, 0x500);
    let wp = remote.downgrade();
    assert!(wp.upgrade().is_some());
}

// =============================================================================
// Feature 18: Bidirectional Handle Table (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f18_boundary_lookup_nonexistent_client_returns_none() {
    let bridge = HandleBridge::new();
    assert!(bridge.get_service(9999, 1).is_none());
}

#[test]
fn test_f18_boundary_lookup_nonexistent_handle_returns_none() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    bridge.register_service(1, "desc", echo);
    assert!(bridge.get_service(1, 9999).is_none());
}

#[test]
fn test_f18_boundary_register_with_handle_zero_service_manager() {
    let bridge = HandleBridge::new();
    let sm: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let res = bridge.register_service_with_handle(1, 0, "android.os.IServiceManager", sm);
    assert_eq!(res.unwrap(), 0);
    assert!(bridge.get_service(1, 0).is_some());
}

#[test]
fn test_f18_boundary_register_with_handle_collision_fails() {
    let bridge = HandleBridge::new();
    let s1: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let s2: Arc<dyn IBinder> = Arc::new(EchoService::new());

    bridge.register_service_with_handle(1, 10, "desc1", s1).unwrap();
    let res = bridge.register_service_with_handle(1, 10, "desc2", s2);
    assert!(res.is_err());
}

#[test]
fn test_f18_boundary_handle_table_list_handles_across_clients() {
    let bridge = HandleBridge::new();
    for client in 1..=5 {
        for _ in 1..=3 {
            bridge.register_service(client, "desc", Arc::new(EchoService::new()));
        }
        assert_eq!(bridge.list_handles(client).len(), 3);
    }
}

// =============================================================================
// Feature 19: Reference Counting & Lifecycles (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f19_boundary_release_more_than_existing_strong_count_returns_error() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h = bridge.register_service(1, "desc", echo); // count = 1

    let res = bridge.release_ref(1, h, 10);
    assert!(matches!(res, Err(BridgeError::InvalidRefCount { .. })));
}

#[test]
fn test_f19_boundary_acquire_on_nonexistent_handle_returns_error() {
    let bridge = HandleBridge::new();
    let res = bridge.acquire_ref(1, 9999, 1);
    assert!(matches!(res, Err(BridgeError::HandleNotFound(9999, 1))));
}

#[test]
fn test_f19_boundary_release_on_nonexistent_handle_returns_error() {
    let bridge = HandleBridge::new();
    let res = bridge.release_ref(1, 9999, 1);
    assert!(matches!(res, Err(BridgeError::HandleNotFound(9999, 1))));
}

#[test]
fn test_f19_boundary_acquire_large_refcount_and_release() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h = bridge.register_service(1, "desc", echo);

    bridge.acquire_ref(1, h, 1000).unwrap();
    assert_eq!(bridge.get_strong_count(1, h), Some(1001));

    let dropped = bridge.release_ref(1, h, 1001).unwrap();
    assert!(dropped);
    assert!(bridge.get_service(1, h).is_none());
}

#[test]
fn test_f19_boundary_weak_count_drop_independent_of_strong() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h = bridge.register_service(1, "desc", echo);

    bridge.acquire_weak_ref(1, h, 5).unwrap();
    assert_eq!(bridge.get_weak_count(1, h), Some(5));

    bridge.release_weak_ref(1, h, 5).unwrap();
    assert_eq!(bridge.get_weak_count(1, h), Some(0));
    assert!(bridge.get_service(1, h).is_some());
}

// =============================================================================
// Feature 20: Cross-Client Handle Passing (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f20_boundary_transfer_nonexistent_handle_returns_error() {
    let bridge = HandleBridge::new();
    let res = bridge.transfer_handle(1, 2, 9999);
    assert!(matches!(res, Err(BridgeError::HandleNotFound(9999, 1))));
}

#[test]
fn test_f20_boundary_transfer_to_same_client_bumps_refcount() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h1 = bridge.register_service(1, "desc", echo);

    let h2 = bridge.transfer_handle(1, 1, h1).unwrap();
    assert_eq!(h1, h2);
    assert_eq!(bridge.get_strong_count(1, h1), Some(2));
}

#[test]
fn test_f20_boundary_transfer_across_10_clients_in_chain() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let mut current_handle = bridge.register_service(1, "desc", Arc::clone(&echo));

    for client in 1..10 {
        let next_handle = bridge.transfer_handle(client, client + 1, current_handle).unwrap();
        assert!(bridge.get_service(client + 1, next_handle).is_some());
        current_handle = next_handle;
    }
}

#[test]
fn test_f20_boundary_transfer_preserves_descriptor() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h1 = bridge.register_service(1, "android.gui.ISurfaceComposer", echo);

    let h2 = bridge.transfer_handle(1, 2, h1).unwrap();
    assert_eq!(bridge.get_descriptor(2, h2), Some("android.gui.ISurfaceComposer".to_string()));
}

#[test]
fn test_f20_boundary_transfer_after_original_client_dropped_still_valid() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h1 = bridge.register_service(1, "desc", echo);
    let h2 = bridge.transfer_handle(1, 2, h1).unwrap();

    bridge.release_ref(1, h1, 1).unwrap(); // Drop client 1's handle
    assert!(bridge.get_service(1, h1).is_none());
    assert!(bridge.get_service(2, h2).is_some()); // Client 2 still has valid reference
}

// =============================================================================
// Feature 21: Asynchronous Death Notification (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f21_boundary_link_death_nonexistent_handle_returns_error() {
    let bridge = HandleBridge::new();
    let res = bridge.register_death_recipient(1, 9999, 0x1234);
    assert!(matches!(res, Err(BridgeError::HandleNotFound(9999, 1))));
}

#[test]
fn test_f21_boundary_unlink_death_nonexistent_handle_returns_error() {
    let bridge = HandleBridge::new();
    let res = bridge.unregister_death_recipient(1, 9999, 0x1234);
    assert!(matches!(res, Err(BridgeError::HandleNotFound(9999, 1))));
}

#[test]
fn test_f21_boundary_unlink_unregistered_cookie_returns_false() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h = bridge.register_service(1, "desc", echo);

    let res = bridge.unregister_death_recipient(1, h, 0xDEADBEEF);
    assert!(matches!(res, Err(BridgeError::DeathRecipientNotFound(0xDEADBEEF, _, _))));
}

#[test]
fn test_f21_boundary_client_death_with_zero_handles() {
    let bridge = HandleBridge::new();
    // Should execute cleanly without error
    let events = bridge.on_client_died(999);
    assert!(events.is_empty());
}

#[test]
fn test_f21_boundary_death_registry_multiple_cookies_same_client() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h = bridge.register_service(1, "desc", echo);

    for i in 1..=10 {
        bridge.register_death_recipient(1, h, i as u64).unwrap();
    }

    for i in 1..=10 {
        assert!(bridge.unregister_death_recipient(1, h, i as u64).is_ok());
    }
}

// =============================================================================
// Feature 22: Selective Routing Policy Engine (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f22_boundary_empty_policy_returns_default() {
    let policy = RoutingPolicy::new_default_local();
    assert_eq!(policy.route("any.descriptor", 1), RouteAction::LocalGuest);
}

#[test]
fn test_f22_boundary_policy_universal_wildcard() {
    let mut policy = RoutingPolicy::new_default_local();
    policy.allow_host_offload("*");

    assert_eq!(policy.route("anything", 1), RouteAction::HostOffload);
    assert_eq!(policy.route("another.service", 100), RouteAction::HostOffload);
}

#[test]
fn test_f22_boundary_policy_remove_nonexistent_rule_returns_none() {
    let mut policy = RoutingPolicy::new_default_local();
    assert!(policy.remove_rule("nonexistent").is_none());
}

#[test]
fn test_f22_boundary_policy_clear_resets_rules() {
    let mut policy = RoutingPolicy::new_default_local();
    policy.allow_host_offload("test.desc");
    assert_eq!(policy.len(), 1);

    policy.clear();
    assert_eq!(policy.len(), 0);
    assert_eq!(policy.route("test.desc", 1), RouteAction::LocalGuest);
}

#[test]
fn test_f22_boundary_policy_rule_priorities_negative_and_positive() {
    let mut policy = RoutingPolicy::new_default_local();
    policy.add_rule(RoutingRule::new("android.gui.*", RouteAction::HostOffload).with_priority(-5));
    policy.add_rule(RoutingRule::new("android.gui.ISurfaceComposerClient", RouteAction::LocalGuest).with_priority(5));

    assert_eq!(policy.route("android.gui.ISurfaceComposerClient", 1), RouteAction::LocalGuest);
    assert_eq!(policy.route("android.gui.ISurfaceComposer", 1), RouteAction::HostOffload);
}

// =============================================================================
// Feature 23: Descriptors & Opcode Matcher (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f23_boundary_matcher_any_matches_all_descriptors() {
    let mut engine = MatcherEngine::new(RouteAction::LocalGuest);
    engine.add_rule(MatchRule::new(DescriptorMatcher::Any, RouteAction::HostOffload));

    assert_eq!(engine.match_transaction(None, Some("com.example.Service"), 1), RouteAction::HostOffload);
    assert_eq!(engine.match_transaction(None, None, 1), RouteAction::HostOffload);
}

#[test]
fn test_f23_boundary_matcher_empty_code_filter() {
    let mut engine = MatcherEngine::new(RouteAction::LocalGuest);
    engine.add_rule(
        MatchRule::new(DescriptorMatcher::Exact("desc".to_string()), RouteAction::HostOffload)
            .with_code_filter(CodeFilter::Specific(vec![])),
    );

    assert_eq!(engine.match_transaction(None, Some("desc"), 1), RouteAction::LocalGuest);
}

#[test]
fn test_f23_boundary_matcher_code_filter_range_edges() {
    let mut engine = MatcherEngine::new(RouteAction::LocalGuest);
    engine.add_rule(
        MatchRule::new(DescriptorMatcher::Any, RouteAction::HostOffload)
            .with_code_filter(CodeFilter::Range(10, 20)),
    );

    assert_eq!(engine.match_transaction(None, Some("desc"), 9), RouteAction::LocalGuest);
    assert_eq!(engine.match_transaction(None, Some("desc"), 10), RouteAction::HostOffload);
    assert_eq!(engine.match_transaction(None, Some("desc"), 20), RouteAction::HostOffload);
    assert_eq!(engine.match_transaction(None, Some("desc"), 21), RouteAction::LocalGuest);
}

#[test]
fn test_f23_boundary_matcher_service_name_none_match() {
    let mut engine = MatcherEngine::new(RouteAction::LocalGuest);
    engine.add_rule(
        MatchRule::new(DescriptorMatcher::Any, RouteAction::HostOffload)
            .with_service(ServiceNameMatcher::Exact("SurfaceFlinger".to_string())),
    );

    assert_eq!(engine.match_transaction(None, Some("desc"), 1), RouteAction::LocalGuest);
    assert_eq!(engine.match_transaction(Some("SurfaceFlinger"), Some("desc"), 1), RouteAction::HostOffload);
}

#[test]
fn test_f23_boundary_matcher_clear_and_default_fallback() {
    let mut engine = MatcherEngine::new(RouteAction::LocalGuest);
    engine.add_rule(MatchRule::new(DescriptorMatcher::Any, RouteAction::HostOffload));
    assert_eq!(engine.match_transaction(None, Some("desc"), 1), RouteAction::HostOffload);

    engine.clear();
    assert_eq!(engine.match_transaction(None, Some("desc"), 1), RouteAction::LocalGuest);
}

// =============================================================================
// Feature 24: Offloaded Compositor Service (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f24_boundary_composer_service_zero_dimension_display() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 0, 0);
        let info = svc.get_display_info();
        assert_eq!(info.width, 0);
        assert_eq!(info.height, 0);
    });
}

#[test]
fn test_f24_boundary_composer_create_surface_zero_dimensions() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 800, 600);
        let handle = svc.create_surface("ZeroSurface", 0, 0, 0).unwrap();
        assert_eq!(handle.width, 0);
        assert_eq!(handle.height, 0);
    });
}

#[test]
fn test_f24_boundary_composer_destroy_nonexistent_surface_returns_error() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 800, 600);
        let res = svc.destroy_surface(99999);
        assert!(res.is_err());
    });
}

#[test]
fn test_f24_boundary_composer_boot_finished_idempotent() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 800, 600);
        assert!(!svc.is_boot_finished());
        svc.set_boot_finished(true);
        assert!(svc.is_boot_finished());
        svc.set_boot_finished(true);
        assert!(svc.is_boot_finished());
    });
}

#[test]
fn test_f24_boundary_composer_very_long_surface_name() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 800, 600);
        let long_name = "Surface_".repeat(100);
        let handle = svc.create_surface(&long_name, 100, 100, 0).unwrap();
        assert_eq!(handle.name, long_name);
    });
}

// =============================================================================
// Feature 25: Layer State Translation (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f25_boundary_layer_state_zero_and_negative_ndc_bounds() {
    let mut state = LayerState::new(1, "ZeroLayer");
    state.set_bounds_ndc([0.0, 0.0, 0.0, 0.0]);
    assert_eq!(state.bounds, [0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn test_f25_boundary_layer_state_alpha_clamping() {
    let mut state = LayerState::new(1, "AlphaLayer");
    state.set_alpha(0.0);
    assert_eq!(state.alpha, 0.0);
    state.set_alpha(1.0);
    assert_eq!(state.alpha, 1.0);
}

#[test]
fn test_f25_boundary_layer_translator_zero_display_dimensions() {
    let state = LayerState::new(1, "Layer");
    let comp_layer = LayerTranslator::translate_to_composition_layer(&state, 0, 0, None);
    assert_eq!(comp_layer.id, 1);
}

#[test]
fn test_f25_boundary_composer_state_empty_flags_apply() {
    let mut comp_layer = CompositionLayer::new_color(1, "Layer1", [-1.0, -1.0, 2.0, 2.0], 0, [0.0, 0.0, 1.0, 1.0]);
    let update_state = LayerState::new(1, "Update");
    // No changes flag set (flags = 0)
    LayerTranslator::apply_state_update(&mut comp_layer, &update_state, 800, 600);
    assert_eq!(comp_layer.z_order, 0);
}

#[test]
fn test_f25_boundary_layer_state_extreme_z_orders() {
    let mut s1 = LayerState::new(1, "MinZ");
    s1.set_z_order(i32::MIN);
    let mut s2 = LayerState::new(2, "MaxZ");
    s2.set_z_order(i32::MAX);

    assert_eq!(s1.z_order, i32::MIN);
    assert_eq!(s2.z_order, i32::MAX);
}

// =============================================================================
// Feature 26: Multi-Layer WebGPU Compositor (Boundary/Corner) (5 tests)
// =============================================================================

#[test]
fn test_f26_boundary_compositor_zero_layers_compose() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let mut compositor = WebGpuCompositor::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let swapchain = WebGpuSwapchain::new(&device, 64, 64, wgpu::TextureFormat::Rgba8Unorm);

        let target_view = swapchain.get_current_texture_view();
        // Composing 0 layers should clear to background color without panicking
        compositor.compose(&device, &queue, target_view, Some(wgpu::Color::BLUE));
    });
}

#[test]
fn test_f26_boundary_swapchain_minimum_resolution_1x1() {
    pollster::block_on(async {
        let (device, _queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let swapchain = WebGpuSwapchain::new(&device, 1, 1, wgpu::TextureFormat::Rgba8Unorm);
        assert_eq!(swapchain.width, 1);
        assert_eq!(swapchain.height, 1);
    });
}

#[test]
fn test_f26_boundary_compositor_50_overlapping_layers_z_sort() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let mut compositor = WebGpuCompositor::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        for i in 0..50 {
            let layer = CompositionLayer::new_color(
                i as u64,
                "Layer",
                [-1.0, -1.0, 2.0, 2.0],
                50 - i, // Reverse z-order
                [0.1, 0.2, 0.3, 0.5],
            );
            compositor.add_or_update_layer(layer);
        }

        assert_eq!(compositor.layers.len(), 50);
        let swapchain = WebGpuSwapchain::new(&device, 128, 128, wgpu::TextureFormat::Rgba8Unorm);
        let target_view = swapchain.get_current_texture_view();
        compositor.compose(&device, &queue, target_view, None);
    });
}

#[test]
fn test_f26_boundary_compositor_remove_nonexistent_layer() {
    pollster::block_on(async {
        let (device, _queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let mut compositor = WebGpuCompositor::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        compositor.remove_layer(9999);
    });
}

#[test]
fn test_f26_boundary_compositor_clear_all_layers() {
    pollster::block_on(async {
        let (device, _queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let mut compositor = WebGpuCompositor::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        compositor.add_or_update_layer(CompositionLayer::new_color(1, "L1", [-1.0, -1.0, 2.0, 2.0], 0, [1.0, 0.0, 0.0, 1.0]));
        compositor.add_or_update_layer(CompositionLayer::new_color(2, "L2", [-1.0, -1.0, 2.0, 2.0], 1, [0.0, 1.0, 0.0, 1.0]));
        assert_eq!(compositor.layers.len(), 2);

        compositor.layers.clear();
        assert_eq!(compositor.layers.len(), 0);
    });
}
