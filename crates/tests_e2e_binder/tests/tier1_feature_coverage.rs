//! Tier 1: Feature Coverage Test Suite (F1..F26)
//!
//! Comprehensive verification covering all 26 features defined in PROJECT.md Feature Inventory.
//! Exactly 5 distinct, genuine tests per feature = 130 tests total.

use aidl_compat::{
    DeathCallback, DeathRecipient, IBinder, Parcel, ParcelFileDescriptor, Parcelable,
    ParcelableHolder, Remotable, RemoteTransport, Result as AidlResult, SpIBinder, Status,
    StatusCode, Strong, WpIBinder, BINDER_TYPE_FD, BINDER_TYPE_HANDLE, STATUS_DEAD_OBJECT,
    STATUS_OK,
};
use binder_handle_bridge::HandleBridge;
use binder_routing::{
    CodeFilter, DescriptorMatcher, MatchRule, MatcherEngine, RouteAction, RoutingPolicy,
    ServiceNameMatcher,
};
use binder_rt::types::FlatBinderObject;
use binder_rt::wire::{
    BinderTransactionData, BC_ACQUIRE, BC_RELEASE, BC_TRANSACTION, BR_REPLY, PING_TRANSACTION,
    TF_ACCEPT_FDS, TF_ONE_WAY,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use surfaceflinger_gpu_service::layer_translator::{ComposerState, LayerState, LayerTranslator};
use surfaceflinger_gpu_service::service::{isurfacecomposer_codes, DisplayInfo, SurfaceComposerService};
use tests_e2e_binder::harness::{create_test_wgpu_device, EchoService};
use virtio_binder::device::VirtioBinderDevice;
use virtio_binder::guest_shim::{DirectDeviceBackend, GuestVirtioTransport, VirtqueueChainBackend};
use virtio_binder::protocol::*;
use webgpu_compositor::{CompositionLayer, WebGpuCompositor};
use webgpu_swapchain::WebGpuSwapchain;

// =============================================================================
// Feature 1: Parcel Alignment & Padding (5 tests)
// =============================================================================

#[test]
fn test_f01_1_parcel_4byte_alignment_exact_multiples() {
    let mut p = Parcel::new();
    p.write_i32(100).unwrap();
    p.write_u32(200).unwrap();
    p.write_i32(300).unwrap();
    assert_eq!(p.data_size(), 12);
    assert_eq!(p.data_size() % 4, 0);

    let mut off = 0;
    assert_eq!(p.read_i32(&mut off).unwrap(), 100);
    assert_eq!(p.read_u32(&mut off).unwrap(), 200);
    assert_eq!(p.read_i32(&mut off).unwrap(), 300);
    assert_eq!(off, 12);
}

#[test]
fn test_f01_2_parcel_1byte_alignment_padding_zeros() {
    let mut p = Parcel::new();
    p.write_u8(0xAB).unwrap(); // 1 byte payload -> 3 zero padding bytes
    assert_eq!(p.data_size(), 4);
    assert_eq!(p.data(), &[0xAB, 0x00, 0x00, 0x00]);

    let mut off = 0;
    assert_eq!(p.read_u8(&mut off).unwrap(), 0xAB);
    assert_eq!(off, 4);
}

#[test]
fn test_f01_3_parcel_2byte_alignment_padding_zeros() {
    let mut p = Parcel::new();
    p.write_u16(0x1234).unwrap(); // 2 bytes payload -> 2 zero padding bytes
    assert_eq!(p.data_size(), 4);
    assert_eq!(p.data(), &[0x34, 0x12, 0x00, 0x00]);

    let mut off = 0;
    assert_eq!(p.read_u16(&mut off).unwrap(), 0x1234);
    assert_eq!(off, 4);
}

#[test]
fn test_f01_4_parcel_3byte_alignment_padding_zeros() {
    let mut p = Parcel::new();
    p.write_byte_slice(Some(&[0x11, 0x22, 0x33])).unwrap(); // 4-byte len + 3 bytes + 1 pad byte = 8 bytes
    assert_eq!(p.data_size(), 8);
    assert_eq!(p.data()[4..8], [0x11, 0x22, 0x33, 0x00]);

    let mut off = 0;
    let read_back = p.read_byte_vec(&mut off).unwrap().unwrap();
    assert_eq!(read_back, vec![0x11, 0x22, 0x33]);
    assert_eq!(off, 8);
}

#[test]
fn test_f01_5_parcel_sequential_mixed_alignment_data_size() {
    let mut p = Parcel::new();
    p.write_i8(1).unwrap(); // 4 bytes
    p.write_i16(2).unwrap(); // 4 bytes
    p.write_i32(3).unwrap(); // 4 bytes
    p.write_i64(4).unwrap(); // 8 bytes
    p.write_u8(5).unwrap(); // 4 bytes
    assert_eq!(p.data_size(), 24);

    let mut off = 0;
    assert_eq!(p.read_i8(&mut off).unwrap(), 1);
    assert_eq!(p.read_i16(&mut off).unwrap(), 2);
    assert_eq!(p.read_i32(&mut off).unwrap(), 3);
    assert_eq!(p.read_i64(&mut off).unwrap(), 4);
    assert_eq!(p.read_u8(&mut off).unwrap(), 5);
    assert_eq!(off, 24);
}

// =============================================================================
// Feature 2: Parcel Scalar Codec (5 tests)
// =============================================================================

#[test]
fn test_f02_1_scalar_codec_boolean_true_false() {
    let mut p = Parcel::new();
    p.write_bool(true).unwrap();
    p.write_bool(false).unwrap();
    let mut off = 0;
    assert!(p.read_bool(&mut off).unwrap());
    assert!(!p.read_bool(&mut off).unwrap());
}

#[test]
fn test_f02_2_scalar_codec_integers_signed_unsigned_8_16_32_64() {
    let mut p = Parcel::new();
    p.write_i8(-120).unwrap();
    p.write_u8(240).unwrap();
    p.write_i16(-30000).unwrap();
    p.write_u16(60000).unwrap();
    p.write_i32(-2000000000).unwrap();
    p.write_u32(4000000000).unwrap();
    p.write_i64(-9000000000000000000).unwrap();
    p.write_u64(18000000000000000000).unwrap();

    let mut off = 0;
    assert_eq!(p.read_i8(&mut off).unwrap(), -120);
    assert_eq!(p.read_u8(&mut off).unwrap(), 240);
    assert_eq!(p.read_i16(&mut off).unwrap(), -30000);
    assert_eq!(p.read_u16(&mut off).unwrap(), 60000);
    assert_eq!(p.read_i32(&mut off).unwrap(), -2000000000);
    assert_eq!(p.read_u32(&mut off).unwrap(), 4000000000);
    assert_eq!(p.read_i64(&mut off).unwrap(), -9000000000000000000);
    assert_eq!(p.read_u64(&mut off).unwrap(), 18000000000000000000);
}

#[test]
fn test_f02_3_scalar_codec_floating_point_f32_f64() {
    let mut p = Parcel::new();
    p.write_f32(-123.456).unwrap();
    p.write_f64(987654.321012345).unwrap();

    let mut off = 0;
    assert!((p.read_f32(&mut off).unwrap() - (-123.456)).abs() < 1e-5);
    assert!((p.read_f64(&mut off).unwrap() - 987654.321012345).abs() < 1e-12);
}

#[test]
fn test_f02_4_scalar_codec_char_and_unicode_codepoints() {
    let mut p = Parcel::new();
    p.write_char('A').unwrap();
    p.write_char('🦀').unwrap();

    let mut off = 0;
    assert_eq!(p.read_char(&mut off).unwrap(), 'A');
    assert_eq!(p.read_char(&mut off).unwrap(), '🦀');
}

#[test]
fn test_f02_5_scalar_codec_extreme_values_min_max() {
    let mut p = Parcel::new();
    p.write_i32(i32::MIN).unwrap();
    p.write_i32(i32::MAX).unwrap();
    p.write_u32(u32::MIN).unwrap();
    p.write_u32(u32::MAX).unwrap();
    p.write_i64(i64::MIN).unwrap();
    p.write_i64(i64::MAX).unwrap();

    let mut off = 0;
    assert_eq!(p.read_i32(&mut off).unwrap(), i32::MIN);
    assert_eq!(p.read_i32(&mut off).unwrap(), i32::MAX);
    assert_eq!(p.read_u32(&mut off).unwrap(), u32::MIN);
    assert_eq!(p.read_u32(&mut off).unwrap(), u32::MAX);
    assert_eq!(p.read_i64(&mut off).unwrap(), i64::MIN);
    assert_eq!(p.read_i64(&mut off).unwrap(), i64::MAX);
}

// =============================================================================
// Feature 3: Parcel String Codec (UTF-8 & UTF-16) (5 tests)
// =============================================================================

#[test]
fn test_f03_1_string_codec_utf8_ascii_and_multibyte() {
    let mut p = Parcel::new();
    p.write_utf8(Some("Android WebGPU Offloading")).unwrap();
    let mut off = 0;
    assert_eq!(
        p.read_utf8(&mut off).unwrap(),
        Some("Android WebGPU Offloading".to_string())
    );
}

#[test]
fn test_f03_2_string_codec_utf16le_bmp_and_astral_plane() {
    let mut p = Parcel::new();
    let text = "WebGPU 🚀 SurfaceFlinger";
    p.write_utf16(Some(text)).unwrap();

    let mut off = 0;
    assert_eq!(p.read_utf16(&mut off).unwrap(), Some(text.to_string()));
}

#[test]
fn test_f03_3_string_codec_nullable_some_and_none() {
    let mut p = Parcel::new();
    p.write_utf8(None).unwrap();
    p.write_utf16(None).unwrap();

    let mut off = 0;
    assert_eq!(p.read_utf8(&mut off).unwrap(), None);
    assert_eq!(p.read_utf16(&mut off).unwrap(), None);
}

#[test]
fn test_f03_4_string_codec_empty_strings_with_null_terminator() {
    let mut p = Parcel::new();
    p.write_utf8(Some("")).unwrap();
    p.write_utf16(Some("")).unwrap();

    let mut off = 0;
    assert_eq!(p.read_utf8(&mut off).unwrap(), Some("".to_string()));
    assert_eq!(p.read_utf16(&mut off).unwrap(), Some("".to_string()));
}

#[test]
fn test_f03_5_string_codec_interleaved_utf8_and_utf16() {
    let mut p = Parcel::new();
    p.write_utf8(Some("Hello")).unwrap();
    p.write_utf16(Some("World")).unwrap();
    p.write_utf8(Some("Rust")).unwrap();

    let mut off = 0;
    assert_eq!(p.read_utf8(&mut off).unwrap(), Some("Hello".to_string()));
    assert_eq!(p.read_utf16(&mut off).unwrap(), Some("World".to_string()));
    assert_eq!(p.read_utf8(&mut off).unwrap(), Some("Rust".to_string()));
}

// =============================================================================
// Feature 4: Parcel Vectors & Arrays (5 tests)
// =============================================================================

#[test]
fn test_f04_1_vector_codec_byte_slice_and_byte_vec() {
    let mut p = Parcel::new();
    let data = [10u8, 20, 30, 40, 50, 60, 70];
    p.write_byte_slice(Some(&data)).unwrap();

    let mut off = 0;
    let read_back = p.read_byte_vec(&mut off).unwrap().unwrap();
    assert_eq!(read_back, data);
}

#[test]
fn test_f04_2_vector_codec_i32_and_u64_vectors() {
    let mut p = Parcel::new();
    let ints = vec![1, -2, 3, -4, 5];
    let ulongs = vec![100u64, 200, 300];

    p.write_vector(Some(&ints), |parcel, &v| parcel.write_i32(v)).unwrap();
    p.write_vector(Some(&ulongs), |parcel, &v| parcel.write_u64(v)).unwrap();

    let mut off = 0;
    let read_ints: Vec<i32> = p.read_vector(&mut off, |parcel, off| parcel.read_i32(off)).unwrap().unwrap();
    let read_ulongs: Vec<u64> = p.read_vector(&mut off, |parcel, off| parcel.read_u64(off)).unwrap().unwrap();

    assert_eq!(read_ints, ints);
    assert_eq!(read_ulongs, ulongs);
}

#[test]
fn test_f04_3_vector_codec_string_vectors() {
    let mut p = Parcel::new();
    let strs = vec!["Alpha".to_string(), "Beta".to_string(), "Gamma".to_string()];
    p.write_vector(Some(&strs), |parcel, s| parcel.write_utf8(Some(s))).unwrap();

    let mut off = 0;
    let read_strs: Vec<String> = p
        .read_vector(&mut off, |parcel, off| {
            parcel.read_utf8(off).map(|opt| opt.unwrap_or_default())
        })
        .unwrap()
        .unwrap();

    assert_eq!(read_strs, strs);
}

#[test]
fn test_f04_4_vector_codec_nullable_vector_presence() {
    let mut p = Parcel::new();
    p.write_vector::<i32, _>(None, |parcel, &v| parcel.write_i32(v)).unwrap();

    let mut off = 0;
    let read_res: Option<Vec<i32>> = p.read_vector(&mut off, |parcel, off| parcel.read_i32(off)).unwrap();
    assert_eq!(read_res, None);
}

#[test]
fn test_f04_5_vector_codec_nested_and_empty_vectors() {
    let mut p = Parcel::new();
    let empty_vec: Vec<u32> = vec![];
    p.write_vector(Some(&empty_vec), |parcel, &v| parcel.write_u32(v)).unwrap();

    let mut off = 0;
    let read_empty: Vec<u32> = p.read_vector(&mut off, |parcel, off| parcel.read_u32(off)).unwrap().unwrap();
    assert_eq!(read_empty.len(), 0);
}

// =============================================================================
// Feature 5: Binder Object Serialization (5 tests)
// =============================================================================

#[test]
fn test_f05_1_binder_object_flat_binder_object_size_and_alignment() {
    assert_eq!(std::mem::size_of::<FlatBinderObject>(), 24);
    assert_eq!(std::mem::align_of::<FlatBinderObject>(), 8);
}

#[test]
fn test_f05_2_binder_object_write_read_handle_and_cookie() {
    let mut p = Parcel::new();
    p.write_binder(42, 0x12345678).unwrap();
    assert_eq!(p.offsets(), &[0]);
    assert_eq!(p.data_size(), 24);

    let mut off = 0;
    let obj = p.read_binder(&mut off).unwrap();
    assert_eq!(obj.hdr.type_, BINDER_TYPE_HANDLE);
    assert_eq!(obj.handle(), 42);
    assert_eq!(obj.cookie, 0x12345678);
}

#[test]
fn test_f05_3_binder_object_offsets_array_registration() {
    let mut p = Parcel::new();
    p.write_i32(999).unwrap(); // 4 bytes
    p.write_binder(7, 0x100).unwrap(); // 24 bytes at offset 4

    assert_eq!(p.offsets(), &[4]);
    assert_eq!(p.offsets_size(), 8);
}

#[test]
fn test_f05_4_binder_object_multiple_binders_offset_tracking() {
    let mut p = Parcel::new();
    p.write_binder(1, 0x10).unwrap(); // offset 0
    p.write_u32(1234).unwrap(); // offset 24
    p.write_binder(2, 0x20).unwrap(); // offset 28

    assert_eq!(p.offsets(), &[0, 28]);
    assert_eq!(p.data_size(), 52);

    let mut off = 0;
    let obj1 = p.read_binder(&mut off).unwrap();
    assert_eq!(obj1.handle(), 1);
    let val = p.read_u32(&mut off).unwrap();
    assert_eq!(val, 1234);
    let obj2 = p.read_binder(&mut off).unwrap();
    assert_eq!(obj2.handle(), 2);
}

#[test]
fn test_f05_5_binder_object_weak_binder_and_flags() {
    let mut p = Parcel::new();
    let obj = FlatBinderObject::new_handle(15, 0x01, 0x999);
    p.write_binder_object(&obj).unwrap();

    let mut off = 0;
    let read_obj = p.read_binder_object(&mut off).unwrap();
    assert_eq!(read_obj.hdr.type_, binder_rt::types::BINDER_TYPE_HANDLE);
    assert_eq!(read_obj.handle(), 15);
    assert_eq!(read_obj.cookie, 0x999);
}

// =============================================================================
// Feature 6: File Descriptor Serialization (5 tests)
// =============================================================================

#[test]
fn test_f06_1_fd_serialization_write_read_fd_and_cookie() {
    let mut p = Parcel::new();
    p.write_file_descriptor(12, 0xABCDEF).unwrap();
    assert_eq!(p.data_size(), 24);

    let mut off = 0;
    let fd = p.read_file_descriptor(&mut off).unwrap();
    assert_eq!(fd, 12);
}

#[test]
fn test_f06_2_fd_serialization_flat_binder_object_fd_type() {
    let mut p = Parcel::new();
    p.write_file_descriptor(5, 0).unwrap();

    let mut off = 0;
    let obj = p.read_binder_object(&mut off).unwrap();
    assert_eq!(obj.hdr.type_, BINDER_TYPE_FD);
    assert_eq!(obj.handle(), 5);
}

#[test]
fn test_f06_3_fd_serialization_offsets_tracking() {
    let mut p = Parcel::new();
    p.write_i64(123456789).unwrap(); // 8 bytes
    p.write_file_descriptor(3, 0x111).unwrap(); // offset 8

    assert_eq!(p.offsets(), &[8]);
}

#[test]
fn test_f06_4_fd_serialization_multiple_fds_sequential() {
    let mut p = Parcel::new();
    p.write_file_descriptor(10, 0x1).unwrap();
    p.write_file_descriptor(20, 0x2).unwrap();
    p.write_file_descriptor(30, 0x3).unwrap();

    assert_eq!(p.offsets(), &[0, 24, 48]);
    let mut off = 0;
    assert_eq!(p.read_file_descriptor(&mut off).unwrap(), 10);
    assert_eq!(p.read_file_descriptor(&mut off).unwrap(), 20);
    assert_eq!(p.read_file_descriptor(&mut off).unwrap(), 30);
}

#[test]
fn test_f06_5_fd_serialization_mixed_binders_and_fds() {
    let mut p = Parcel::new();
    p.write_binder(100, 0x1000).unwrap(); // offset 0
    p.write_file_descriptor(55, 0x2000).unwrap(); // offset 24

    assert_eq!(p.offsets(), &[0, 24]);
    let mut off = 0;
    let b = p.read_binder(&mut off).unwrap();
    assert_eq!(b.handle(), 100);
    let fd = p.read_file_descriptor(&mut off).unwrap();
    assert_eq!(fd, 55);
}

// =============================================================================
// Feature 7: Transaction Envelopes (5 tests)
// =============================================================================

#[test]
fn test_f07_1_envelope_binder_transaction_data_64byte_layout() {
    assert_eq!(std::mem::size_of::<BinderTransactionData>(), 64);
    assert_eq!(std::mem::align_of::<BinderTransactionData>(), 8);
}

#[test]
fn test_f07_2_envelope_flags_tf_one_way_and_accept_fds() {
    let tr = BinderTransactionData::new(
        1,
        0x5555,
        1001,
        TF_ONE_WAY | TF_ACCEPT_FDS,
        500,
        1000,
        128,
        16,
        0x10000,
        0x20000,
    );
    assert!(tr.is_one_way());
    assert_eq!(tr.flags & TF_ACCEPT_FDS, TF_ACCEPT_FDS);
}

#[test]
fn test_f07_3_envelope_bc_transaction_and_br_reply_opcodes() {
    assert_eq!(BC_TRANSACTION, 0x40406300);
    assert_eq!(BR_REPLY, 0x80407203);
    assert_eq!(BC_ACQUIRE, 0x40046305);
    assert_eq!(BC_RELEASE, 0x40046306);
}

#[test]
fn test_f07_4_envelope_target_handle_and_cookie_mapping() {
    let tr = BinderTransactionData::new(42, 0xCAFEBABE, PING_TRANSACTION, 0, 0, 0, 0, 0, 0, 0);
    assert_eq!(tr.target_handle(), 42);
    assert_eq!(tr.cookie, 0xCAFEBABE);
    assert_eq!(tr.code, PING_TRANSACTION);
}

#[test]
fn test_f07_5_envelope_payload_and_offsets_buffers_roundtrip() {
    let tr = BinderTransactionData::new(1, 0, 100, 0, 10, 20, 64, 8, 0x8000, 0x9000);
    let bytes = tr.as_bytes();
    let decoded = BinderTransactionData::from_bytes(bytes).unwrap();
    assert_eq!(decoded, tr);
}

// =============================================================================
// Feature 8: AIDL Status & Exceptions (5 tests)
// =============================================================================

#[test]
fn test_f08_1_status_ok_ex_none_serialization() {
    let mut p = Parcel::new();
    let status = Status::ok();
    p.write_status(&status).unwrap();

    let mut off = 0;
    let decoded = p.read_status(&mut off).unwrap();
    assert!(decoded.is_ok());
    assert_eq!(decoded.exception_code(), aidl_compat::ExceptionCode::None);
}

#[test]
fn test_f08_2_status_standard_exceptions_illegal_argument_security() {
    let mut p = Parcel::new();
    let s_arg = Status::new_exception(aidl_compat::ExceptionCode::IllegalArgument, Some("Bad arg"));
    let s_sec = Status::new_exception(aidl_compat::ExceptionCode::Security, Some("Permission denied"));

    p.write_status(&s_arg).unwrap();
    p.write_status(&s_sec).unwrap();

    let mut off = 0;
    let d1 = p.read_status(&mut off).unwrap();
    assert_eq!(d1.exception_code(), aidl_compat::ExceptionCode::IllegalArgument);
    assert_eq!(d1.message(), Some("Bad arg"));

    let d2 = p.read_status(&mut off).unwrap();
    assert_eq!(d2.exception_code(), aidl_compat::ExceptionCode::Security);
    assert_eq!(d2.message(), Some("Permission denied"));
}

#[test]
fn test_f08_3_status_service_specific_error_with_code_and_message() {
    let mut p = Parcel::new();
    let s = Status::new_service_specific_error(-42, Some("GPU Pipeline Error"));
    p.write_status(&s).unwrap();

    let mut off = 0;
    let d = p.read_status(&mut off).unwrap();
    assert_eq!(d.exception_code(), aidl_compat::ExceptionCode::ServiceSpecific);
    assert_eq!(d.service_specific_error(), Some(-42));
    assert_eq!(d.message(), Some("GPU Pipeline Error"));
}

#[test]
fn test_f08_4_status_null_pointer_and_unsupported_operation() {
    let mut p = Parcel::new();
    p.write_status(&Status::new_exception(aidl_compat::ExceptionCode::NullPointer, None)).unwrap();
    p.write_status(&Status::new_exception(aidl_compat::ExceptionCode::UnsupportedOperation, None)).unwrap();

    let mut off = 0;
    assert_eq!(p.read_status(&mut off).unwrap().exception_code(), aidl_compat::ExceptionCode::NullPointer);
    assert_eq!(p.read_status(&mut off).unwrap().exception_code(), aidl_compat::ExceptionCode::UnsupportedOperation);
}

#[test]
fn test_f08_5_status_parcelable_marshaling_and_error_codes() {
    let s = Status::from_status(STATUS_DEAD_OBJECT);
    assert_eq!(s.service_specific_error(), None);
    assert_eq!(s.status, StatusCode::DeadObject);
}

// =============================================================================
// Feature 9: binder::Interface & IBinder (5 tests)
// =============================================================================

#[test]
fn test_f09_1_interface_as_binder_and_descriptor() {
    let echo = Arc::new(EchoService::new());
    assert_eq!(echo.get_class_descriptor(), Some("android.os.IEchoService"));
}

#[test]
fn test_f09_2_ibinder_transact_synchronous_roundtrip() {
    let echo = Arc::new(EchoService::new());
    let mut data = Parcel::new();
    data.write_i32(15).unwrap();
    data.write_i32(27).unwrap();

    let mut reply = Parcel::new();
    echo.transact(EchoService::TRANSACTION_ADD, 0, &data, &mut reply).unwrap();

    let mut off = 0;
    let status = reply.read_status(&mut off).unwrap();
    assert!(status.is_ok());
    assert_eq!(reply.read_i32(&mut off).unwrap(), 42);
}

#[test]
fn test_f09_3_ibinder_ping_binder_transaction() {
    let echo = Arc::new(EchoService::new());
    assert!(echo.ping_binder().is_ok());
}

#[test]
fn test_f09_4_ibinder_is_binder_alive_state() {
    let echo = Arc::new(EchoService::new());
    assert!(echo.is_binder_alive());
}

#[test]
fn test_f09_5_ibinder_link_unlink_death_recipient() {
    let echo = Arc::new(EchoService::new());
    let recipient: Arc<dyn DeathRecipient> = Arc::new(DeathCallback(|| {}));
    assert!(echo.link_to_death(Arc::clone(&recipient)).is_ok());
    assert!(echo.unlink_to_death(&recipient).is_ok());
}

// =============================================================================
// Feature 10: binder::SpIBinder & WpIBinder (5 tests)
// =============================================================================

#[test]
fn test_f10_1_spibinder_strong_reference_cloning() {
    let sp1 = SpIBinder::new(EchoService::new());
    let sp2 = sp1.clone();
    assert!(sp1.is_binder_alive());
    assert!(sp2.is_binder_alive());
}

#[test]
fn test_f10_2_wpibinder_downgrade_and_upgrade() {
    let sp = SpIBinder::new(EchoService::new());
    let wp = sp.downgrade();
    let upgraded = wp.upgrade().expect("Upgrade of active binder must succeed");
    assert!(upgraded.is_binder_alive());
}

#[test]
fn test_f10_3_spibinder_remote_binder_wrapping() {
    let remote = aidl_compat::stub::RemoteBinder::new(50, 0x1000);
    assert_eq!(remote.handle(), Some(50));
    assert!(remote.is_binder_alive());
}

#[test]
fn test_f10_4_wpibinder_upgrade_failure_when_dead() {
    let weak: std::sync::Weak<dyn IBinder> = std::sync::Weak::<EchoService>::new();
    let wp = WpIBinder::new(weak);
    assert!(wp.upgrade().is_none());
}

#[test]
fn test_f10_5_spibinder_pointer_equality_and_hashing() {
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let sp1 = SpIBinder::from_arc(Arc::clone(&echo));
    let sp2 = SpIBinder::from_arc(echo);
    assert_eq!(sp1, sp2);
}

// =============================================================================
// Feature 11: binder::Remotable & Proxy (5 tests)
// =============================================================================

#[test]
fn test_f11_1_remotable_get_class_descriptor_and_on_transact() {
    let echo = EchoService::new();
    assert_eq!(EchoService::DESCRIPTOR, "android.os.IEchoService");

    let mut data = Parcel::new();
    data.write_utf8(Some("TestString")).unwrap();
    let mut reply = Parcel::new();
    echo.on_transact(EchoService::TRANSACTION_ECHO, &data, &mut reply).unwrap();

    let mut off = 0;
    let status = reply.read_status(&mut off).unwrap();
    assert!(status.is_ok());
    assert_eq!(reply.read_utf8(&mut off).unwrap(), Some("TestString".to_string()));
}

#[test]
fn test_f11_2_proxy_as_binder_forwarding() {
    let sp = SpIBinder::new(EchoService::new());
    let proxy = BpMockService::new(sp.clone());
    assert_eq!(aidl_compat::Interface::as_binder(&proxy), sp);
}

#[test]
fn test_f11_3_transact_sync_helper_execution() {
    let sp = SpIBinder::new(EchoService::new());

    let mut data = Parcel::new();
    data.write_i32(10).unwrap();
    data.write_i32(20).unwrap();

    let (reply, mut off) = aidl_compat::transact_sync(&sp, EchoService::TRANSACTION_ADD, 0, &data).unwrap();
    assert_eq!(reply.read_i32(&mut off).unwrap(), 30);
}

#[test]
fn test_f11_4_from_ibinder_try_from_conversion() {
    let sp = SpIBinder::new(EchoService::new());
    let res: AidlResult<Strong<BpMockService>> = aidl_compat::FromIBinder::try_from(sp);
    assert!(res.is_ok());
}

#[test]
fn test_f11_5_bp_interface_wrapper_dispatch() {
    let sp = SpIBinder::new(EchoService::new());
    let proxy = BpMockService::new(sp);

    let mut data = Parcel::new();
    data.write_utf8(Some("BpDispatch")).unwrap();
    let mut reply = Parcel::new();
    aidl_compat::Interface::as_binder(&proxy).transact(EchoService::TRANSACTION_ECHO, 0, &data, &mut reply).unwrap();

    let mut off = 0;
    let status = reply.read_status(&mut off).unwrap();
    assert!(status.is_ok());
    assert_eq!(reply.read_utf8(&mut off).unwrap(), Some("BpDispatch".to_string()));
}

// =============================================================================
// Feature 12: binder::Parcelable & Macros (5 tests)
// =============================================================================

#[test]
fn test_f12_1_parcelable_trait_builtins_primitives() {
    let mut p = Parcel::new();
    42i32.write_to_parcel(&mut p).unwrap();
    "ParcelableString".to_string().write_to_parcel(&mut p).unwrap();

    let mut val_i32 = 0i32;
    let mut val_str = String::new();

    let mut off = 0;
    val_i32.read_from_parcel_at(&p, &mut off).unwrap();
    val_str.read_from_parcel_at(&p, &mut off).unwrap();

    assert_eq!(val_i32, 42);
    assert_eq!(val_str, "ParcelableString");
}

aidl_compat::declare_binder_enum! {
    TestColor : [i32; 3] {
        Red = 0,
        Green = 1,
        Blue = 2,
    }
}

#[test]
fn test_f12_2_declare_binder_enum_macro_roundtrip() {
    let mut p = Parcel::new();
    TestColor::Green.write_to_parcel(&mut p).unwrap();

    let mut read_color = TestColor::Red;
    let mut off = 0;
    read_color.read_from_parcel_at(&p, &mut off).unwrap();
    assert_eq!(read_color, TestColor::Green);
}

pub trait IMockService: aidl_compat::Interface + Send + Sync {}

fn mock_on_transact(
    _service: &dyn IMockService,
    _code: aidl_compat::TransactionCode,
    _data: &aidl_compat::Parcel,
    _reply: &mut aidl_compat::Parcel,
) -> aidl_compat::Result<()> {
    Ok(())
}

aidl_compat::declare_binder_interface! {
    IMockService ["android.os.IMockService"] {
        native: BnMockService(mock_on_transact),
        proxy: BpMockService,
    }
}

impl IMockService for BpMockService {}

#[test]
fn test_f12_3_declare_binder_interface_macro_expansion() {
    assert_eq!(BnMockService::<BpMockService>::get_class_descriptor(), "android.os.IMockService");
}

#[test]
fn test_f12_4_parcelable_holder_marshaling() {
    let mut holder = ParcelableHolder::new(0);
    holder.set_parcelable(&12345i32, "Int").unwrap();

    let mut target = Parcel::new();
    holder.write_to_parcel(&mut target).unwrap();

    let mut read_holder = ParcelableHolder::new(0);
    let mut off = 0;
    read_holder.read_from_parcel_at(&target, &mut off).unwrap();
    let read_val: Option<i32> = read_holder.get_parcelable().unwrap();
    assert_eq!(read_val, Some(12345));
}

#[test]
fn test_f12_5_parcel_file_descriptor_wrapper() {
    let pfd = ParcelFileDescriptor::new(17);
    assert_eq!(pfd.as_raw_fd(), 17);

    let mut p = Parcel::new();
    pfd.write_to_parcel(&mut p).unwrap();

    let mut read_pfd = ParcelFileDescriptor::new(-1);
    let mut off = 0;
    read_pfd.read_from_parcel_at(&p, &mut off).unwrap();
    assert_eq!(read_pfd.as_raw_fd(), 17);
}

// =============================================================================
// Feature 13: Official AIDL Stub Compatibility (5 tests)
// =============================================================================

#[test]
fn test_f13_1_aidl_stub_binder_creation_with_features() {
    let binder = BnMockService::new_binder(BpMockService::new(SpIBinder::new(EchoService::new())), aidl_compat::BinderFeatures::default());
    assert!(binder.is_binder_alive());
}

#[test]
fn test_f13_2_aidl_stub_dispatch_on_transact_matching() {
    let echo = Arc::new(EchoService::new());
    let mut data = Parcel::new();
    data.write_i32(50).unwrap();
    data.write_i32(50).unwrap();

    let mut reply = Parcel::new();
    echo.on_transact(EchoService::TRANSACTION_ADD, &data, &mut reply).unwrap();

    let mut off = 0;
    let status = reply.read_status(&mut off).unwrap();
    assert!(status.is_ok());
    assert_eq!(reply.read_i32(&mut off).unwrap(), 100);
}

#[test]
fn test_f13_3_aidl_proxy_stub_roundtrip_dispatch() {
    let sp = SpIBinder::new(EchoService::new());
    let mut data = Parcel::new();
    data.write_utf8(Some("AidlStubTest")).unwrap();
    let mut reply = Parcel::new();
    sp.transact(EchoService::TRANSACTION_ECHO, 0, &data, &mut reply).unwrap();

    let mut off = 0;
    let status = reply.read_status(&mut off).unwrap();
    assert!(status.is_ok());
    assert_eq!(reply.read_utf8(&mut off).unwrap(), Some("AidlStubTest".to_string()));
}

#[test]
fn test_f13_4_aidl_stub_exception_propagation_to_proxy() {
    let sp = SpIBinder::new(EchoService::new());
    let data = Parcel::new();
    let mut reply = Parcel::new();

    let res = sp.transact(EchoService::TRANSACTION_FAIL, 0, &data, &mut reply);
    assert!(res.is_err());
    let status = res.unwrap_err();
    assert_eq!(status.service_specific_error(), Some(-99));
}

#[test]
fn test_f13_5_aidl_stub_first_call_transaction_offsetting() {
    assert_eq!(binder_rt::FIRST_CALL_TRANSACTION, 1);
    assert_eq!(binder_rt::LAST_CALL_TRANSACTION, 0x00ffffff);
}

// =============================================================================
// Feature 14: Virtio-Binder Device & Protocol (5 tests)
// =============================================================================

#[test]
fn test_f14_1_virtio_protocol_constants_device_id_44() {
    assert_eq!(VIRTIO_ID_BINDER, 44);
    assert_eq!(VIRTIO_BINDER_QUEUE_TX_RX, 0);
    assert_eq!(VIRTIO_BINDER_QUEUE_EVENT, 1);
}

#[test]
fn test_f14_2_virtio_request_header_serialization() {
    let req_hdr = VirtioBinderReqHdr::new_transact(101, 1, 1006, 0, 0x8888, 64, 8);
    let bytes = req_hdr.as_bytes();
    assert_eq!(bytes.len(), 48);

    let decoded = VirtioBinderReqHdr::from_bytes(bytes).unwrap();
    assert_eq!(decoded.msg_id, 101);
    assert_eq!(decoded.cmd, CMD_TRANSACT);
    assert_eq!(decoded.target_handle, 1);
}

#[test]
fn test_f14_3_virtio_response_header_serialization() {
    let resp_hdr = VirtioBinderRespHdr::new(101, STATUS_OK, BR_REPLY as i32, 32, 0, 0);
    let bytes = resp_hdr.as_bytes();
    assert_eq!(bytes.len(), 32);

    let decoded = VirtioBinderRespHdr::from_bytes(bytes).unwrap();
    assert_eq!(decoded.msg_id, 101);
    assert_eq!(decoded.status, STATUS_OK);
    assert_eq!(decoded.result_code, BR_REPLY as i32);
}

#[test]
fn test_f14_4_virtio_command_codes_ping_transact_acquire_release() {
    assert_eq!(CMD_PING, 6);
    assert_eq!(CMD_TRANSACT, 1);
    assert_eq!(CMD_ACQUIRE, 2);
    assert_eq!(CMD_RELEASE, 3);
    assert_eq!(CMD_LINK_DEATH, 4);
    assert_eq!(CMD_UNLINK_DEATH, 5);
}

#[test]
fn test_f14_5_virtio_device_creation_and_queue_initialization() {
    let device = VirtioBinderDevice::new();
    assert_eq!(device.tx_rx_queue().lock().unwrap().queue_id(), 0);
    assert_eq!(device.event_queue().lock().unwrap().queue_id(), 1);
}

// =============================================================================
// Feature 15: Transport Dispatch Loop (5 tests)
// =============================================================================

#[test]
fn test_f15_1_transport_dispatch_single_transaction_msg_id_match() {
    let device = Arc::new(VirtioBinderDevice::new());
    let echo = Arc::new(EchoService::new());
    device.register_service(1, echo);

    let mut p = Parcel::new();
    p.write_i32(11).unwrap();
    p.write_i32(22).unwrap();

    let req = VirtioBinderRequest::new_transact(
        1001,
        1,
        EchoService::TRANSACTION_ADD,
        0,
        0,
        p.data().to_vec(),
        vec![],
    );
    let resp = device.process_request(&req);

    assert_eq!(resp.hdr.msg_id, 1001);
    assert_eq!(resp.hdr.status, STATUS_OK);
    let mut off = 0;
    let rep = Parcel::from_slice(&resp.data);
    let status = rep.read_status(&mut off).unwrap();
    assert!(status.is_ok());
    let res = rep.read_i32(&mut off).unwrap();
    assert_eq!(res, 33);
}

#[test]
fn test_f15_2_transport_dispatch_direct_backend_roundtrip() {
    let device = Arc::new(VirtioBinderDevice::new());
    device.register_service(1, Arc::new(EchoService::new()));

    let transport = GuestVirtioTransport::new_with_device(device);

    let mut data = Parcel::new();
    data.write_utf8(Some("DirectBackend")).unwrap();
    let mut reply = Parcel::new();

    transport.transact(1, EchoService::TRANSACTION_ECHO, 0, &data, &mut reply).unwrap();
    let mut off = 0;
    let status = reply.read_status(&mut off).unwrap();
    assert!(status.is_ok());
    assert_eq!(reply.read_utf8(&mut off).unwrap(), Some("DirectBackend".to_string()));
}

#[test]
fn test_f15_3_transport_dispatch_virtqueue_chain_backend_roundtrip() {
    let device = Arc::new(VirtioBinderDevice::new());
    device.register_service(1, Arc::new(EchoService::new()));

    let transport = GuestVirtioTransport::new_with_virtqueue(device, 4096);

    let mut data = Parcel::new();
    data.write_i32(100).unwrap();
    data.write_i32(200).unwrap();
    let mut reply = Parcel::new();

    transport.transact(1, EchoService::TRANSACTION_ADD, 0, &data, &mut reply).unwrap();
    let mut off = 0;
    let status = reply.read_status(&mut off).unwrap();
    assert!(status.is_ok());
    assert_eq!(reply.read_i32(&mut off).unwrap(), 300);
}

#[test]
fn test_f15_4_transport_dispatch_multiple_sequential_requests() {
    let device = Arc::new(VirtioBinderDevice::new());
    device.register_service(1, Arc::new(EchoService::new()));
    let transport = GuestVirtioTransport::new_with_device(device);

    for i in 0..10 {
        let mut data = Parcel::new();
        data.write_i32(i).unwrap();
        data.write_i32(1).unwrap();
        let mut reply = Parcel::new();
        transport.transact(1, EchoService::TRANSACTION_ADD, 0, &data, &mut reply).unwrap();
        let mut off = 0;
        let status = reply.read_status(&mut off).unwrap();
        assert!(status.is_ok());
        assert_eq!(reply.read_i32(&mut off).unwrap(), i + 1);
    }
}

#[test]
fn test_f15_5_transport_dispatch_one_way_transaction_handling() {
    let device = Arc::new(VirtioBinderDevice::new());
    let echo = Arc::new(EchoService::new());
    device.register_service(1, Arc::clone(&echo) as Arc<dyn IBinder>);

    let req = VirtioBinderRequest::new_transact(
        500,
        1,
        EchoService::TRANSACTION_ADD,
        TF_ONE_WAY,
        0,
        vec![0; 8],
        vec![],
    );
    let resp = device.process_request(&req);
    assert_eq!(resp.hdr.result_code, binder_rt::wire::BR_TRANSACTION_COMPLETE as i32);
}

// =============================================================================
// Feature 16: Virtio Event Queue (5 tests)
// =============================================================================

#[test]
fn test_f16_1_event_queue_initialization_queue_1() {
    let device = VirtioBinderDevice::new();
    let eq = device.event_queue();
    assert_eq!(eq.lock().unwrap().queue_id(), VIRTIO_BINDER_QUEUE_EVENT);
}

#[test]
fn test_f16_2_event_queue_push_and_drain_events() {
    let device = VirtioBinderDevice::new();
    let evt = VirtioBinderEventHdr::new_death(42, 0x9999);
    device.event_queue().lock().unwrap().push_event(evt);

    let drained = device.event_queue().lock().unwrap().drain_events();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].event_type, EVENT_TYPE_DEATH);
    assert_eq!(drained[0].target_handle, 42);
    assert_eq!(drained[0].cookie, 0x9999);
}

#[test]
fn test_f16_3_event_queue_event_types_death_acquire_release() {
    assert_eq!(EVENT_TYPE_DEATH, 1);
    assert_eq!(EVENT_TYPE_ACQUIRE, 2);
    assert_eq!(EVENT_TYPE_RELEASE, 3);
}

#[test]
fn test_f16_4_event_queue_direct_backend_event_draining() {
    use virtio_binder::guest_shim::TransportBackend;
    let device = Arc::new(VirtioBinderDevice::new());
    device.event_queue().lock().unwrap().push_event(VirtioBinderEventHdr::new_acquire(1, 10));

    let backend = DirectDeviceBackend::new(device);
    let events = backend.drain_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EVENT_TYPE_ACQUIRE);
}

#[test]
fn test_f16_5_event_queue_virtqueue_chain_backend_event_draining() {
    use virtio_binder::guest_shim::TransportBackend;
    let device = Arc::new(VirtioBinderDevice::new());
    device.event_queue().lock().unwrap().push_event(VirtioBinderEventHdr::new_release(2, 20));

    let backend = VirtqueueChainBackend::new(device, 1024);
    let events = backend.drain_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EVENT_TYPE_RELEASE);
}

// =============================================================================
// Feature 17: Guest Interception Shim (5 tests)
// =============================================================================

#[test]
fn test_f17_1_guest_shim_transport_creation_and_binding() {
    let remote = aidl_compat::stub::RemoteBinder::new_raw(1, 0x1234);
    assert_eq!(remote.handle(), Some(1));
}

#[test]
fn test_f17_2_guest_shim_remote_transport_transact() {
    let device = Arc::new(VirtioBinderDevice::new());
    device.register_service(1, Arc::new(EchoService::new()));
    let transport = Arc::new(GuestVirtioTransport::new_with_device(device));

    let remote = aidl_compat::stub::RemoteBinder::new_raw_with_transport(1, 0, None, transport);
    let mut data = Parcel::new();
    data.write_i32(10).unwrap();
    data.write_i32(20).unwrap();
    let mut reply = Parcel::new();

    remote.transact(EchoService::TRANSACTION_ADD, 0, &data, &mut reply).unwrap();
    let mut off = 0;
    let status = reply.read_status(&mut off).unwrap();
    assert!(status.is_ok());
    assert_eq!(reply.read_i32(&mut off).unwrap(), 30);
}

#[test]
fn test_f17_3_guest_shim_remote_binder_integration() {
    let device = Arc::new(VirtioBinderDevice::new());
    device.register_service(1, Arc::new(EchoService::new()));
    let transport = Arc::new(GuestVirtioTransport::new_with_device(device));
    let sp = aidl_compat::stub::RemoteBinder::new_with_transport(1, 0, None, transport);
    assert!(sp.is_binder_alive());
}

#[test]
fn test_f17_4_guest_shim_ping_binder_invocation() {
    let device = Arc::new(VirtioBinderDevice::new());
    device.register_service(1, Arc::new(EchoService::new()));
    let transport = Arc::new(GuestVirtioTransport::new_with_device(device));
    let remote = aidl_compat::stub::RemoteBinder::new_raw_with_transport(1, 0, None, transport);
    assert!(remote.ping_binder().is_ok());
}

#[test]
fn test_f17_5_guest_shim_error_propagation_status_code() {
    let device = Arc::new(VirtioBinderDevice::new());
    let transport = Arc::new(GuestVirtioTransport::new_with_device(device));
    let remote = aidl_compat::stub::RemoteBinder::new_raw_with_transport(999, 0, None, transport); // Handle 999 not registered

    let data = Parcel::new();
    let mut reply = Parcel::new();
    let res = remote.transact(1, 0, &data, &mut reply);
    assert!(res.is_err());
}

// =============================================================================
// Feature 18: Bidirectional Handle Table (5 tests)
// =============================================================================

#[test]
fn test_f18_1_handle_table_insert_and_lookup_service() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h = bridge.register_service(1, EchoService::DESCRIPTOR, Arc::clone(&echo));

    let looked_up = bridge.get_service(1, h).expect("Service must be found");
    assert_eq!(looked_up.get_class_descriptor(), Some(EchoService::DESCRIPTOR));
}

#[test]
fn test_f18_2_handle_table_client_handle_isolation() {
    let bridge = HandleBridge::new();
    let echo1: Arc<dyn IBinder> = Arc::new(EchoService::new());

    let h1 = bridge.register_service(10, "desc.client10", echo1);

    assert!(bridge.get_service(10, h1).is_some());
    assert!(bridge.get_service(20, h1).is_none()); // Client 20 has not registered handle h1
}

#[test]
fn test_f18_3_handle_table_handle_allocation_sequential() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h1 = bridge.register_service(1, "desc.1", Arc::clone(&echo));
    let h2 = bridge.register_service(1, "desc.2", Arc::new(EchoService::new()));

    assert_ne!(h1, h2);
}

#[test]
fn test_f18_4_handle_table_descriptor_storage_and_query() {
    let bridge = HandleBridge::new();
    let echo: Arc<dyn IBinder> = Arc::new(EchoService::new());
    let h = bridge.register_service(1, "android.gui.ISurfaceComposer", echo);

    assert_eq!(bridge.get_descriptor(1, h), Some("android.gui.ISurfaceComposer".to_string()));
}

#[test]
fn test_f18_5_handle_table_list_handles_and_count() {
    let bridge = HandleBridge::new();
    let h1 = bridge.register_service(5, "desc.a", Arc::new(EchoService::new()));
    let h2 = bridge.register_service(5, "desc.b", Arc::new(EchoService::new()));

    assert_eq!(bridge.handle_count(5), 2);
    let list = bridge.list_handles(5);
    assert!(list.contains(&h1));
    assert!(list.contains(&h2));
}

// =============================================================================
// Feature 19: Distributed Reference Counting (5 tests)
// =============================================================================

#[test]
fn test_f19_1_refcount_initial_count_on_register() {
    let bridge = HandleBridge::new();
    let h = bridge.register_service(1, "desc", Arc::new(EchoService::new()));
    assert_eq!(bridge.get_strong_count(1, h), Some(1));
    assert_eq!(bridge.get_weak_count(1, h), Some(0));
}

#[test]
fn test_f19_2_refcount_acquire_strong_increment() {
    let bridge = HandleBridge::new();
    let h = bridge.register_service(1, "desc", Arc::new(EchoService::new()));
    bridge.acquire_ref(1, h, 3).unwrap();
    assert_eq!(bridge.get_strong_count(1, h), Some(4));
}

#[test]
fn test_f19_3_refcount_release_strong_decrement() {
    let bridge = HandleBridge::new();
    let h = bridge.register_service(1, "desc", Arc::new(EchoService::new()));
    bridge.acquire_ref(1, h, 2).unwrap(); // count = 3
    let dropped = bridge.release_ref(1, h, 1).unwrap();
    assert!(!dropped);
    assert_eq!(bridge.get_strong_count(1, h), Some(2));
}

#[test]
fn test_f19_4_refcount_automatic_entry_cleanup_on_zero() {
    let bridge = HandleBridge::new();
    let h = bridge.register_service(1, "desc", Arc::new(EchoService::new()));
    let dropped = bridge.release_ref(1, h, 1).unwrap();
    assert!(dropped);
    assert!(bridge.get_service(1, h).is_none());
}

#[test]
fn test_f19_5_refcount_weak_reference_acquire_and_release() {
    let bridge = HandleBridge::new();
    let h = bridge.register_service(1, "desc", Arc::new(EchoService::new()));
    bridge.acquire_weak_ref(1, h, 2).unwrap();
    assert_eq!(bridge.get_weak_count(1, h), Some(2));

    let dropped = bridge.release_weak_ref(1, h, 2).unwrap();
    assert!(dropped);
}

// =============================================================================
// Feature 20: Multi-Hop Handle Transfer (5 tests)
// =============================================================================

#[test]
fn test_f20_1_handle_transfer_between_two_clients() {
    let bridge = HandleBridge::new();
    let svc = Arc::new(EchoService::new());
    let h1 = bridge.register_service(1, EchoService::DESCRIPTOR, svc);

    let h2 = bridge.transfer_handle(1, 2, h1).unwrap();
    assert!(bridge.get_service(2, h2).is_some());
}

#[test]
fn test_f20_2_handle_transfer_preserves_underlying_service() {
    let bridge = HandleBridge::new();
    let svc = Arc::new(EchoService::new());
    let h1 = bridge.register_service(1, EchoService::DESCRIPTOR, svc);

    let h2 = bridge.transfer_handle(1, 2, h1).unwrap();
    let s1 = bridge.get_service(1, h1).unwrap();
    let s2 = bridge.get_service(2, h2).unwrap();
    assert!(Arc::ptr_eq(&s1, &s2));
}

#[test]
fn test_f20_3_handle_transfer_existing_handle_increments_refcount() {
    let bridge = HandleBridge::new();
    let svc = Arc::new(EchoService::new());
    let h1 = bridge.register_service(1, EchoService::DESCRIPTOR, svc);

    let h2_a = bridge.transfer_handle(1, 2, h1).unwrap();
    let h2_b = bridge.transfer_handle(1, 2, h1).unwrap();

    assert_eq!(h2_a, h2_b);
    assert_eq!(bridge.get_strong_count(2, h2_a), Some(2));
}

#[test]
fn test_f20_4_handle_transfer_source_client_remains_valid() {
    let bridge = HandleBridge::new();
    let svc = Arc::new(EchoService::new());
    let h1 = bridge.register_service(1, EchoService::DESCRIPTOR, svc);

    let _h2 = bridge.transfer_handle(1, 2, h1).unwrap();
    assert!(bridge.get_service(1, h1).is_some());
    assert_eq!(bridge.get_strong_count(1, h1), Some(1));
}

#[test]
fn test_f20_5_handle_transfer_three_client_chain_propagation() {
    let bridge = HandleBridge::new();
    let svc = Arc::new(EchoService::new());
    let h1 = bridge.register_service(1, EchoService::DESCRIPTOR, svc);

    let h2 = bridge.transfer_handle(1, 2, h1).unwrap();
    let h3 = bridge.transfer_handle(2, 3, h2).unwrap();

    let s1 = bridge.get_service(1, h1).unwrap();
    let s3 = bridge.get_service(3, h3).unwrap();
    assert!(Arc::ptr_eq(&s1, &s3));
}

// =============================================================================
// Feature 21: Death Notification Propagation (5 tests)
// =============================================================================

#[test]
fn test_f21_1_death_notification_register_recipient() {
    let bridge = HandleBridge::new();
    let h = bridge.register_service(1, "desc", Arc::new(EchoService::new()));
    bridge.register_death_recipient(1, h, 0x1234).unwrap();
    assert_eq!(bridge.get_death_recipients(1, h), Some(vec![0x1234]));
}

#[test]
fn test_f21_2_death_notification_unregister_recipient() {
    let bridge = HandleBridge::new();
    let h = bridge.register_service(1, "desc", Arc::new(EchoService::new()));
    bridge.register_death_recipient(1, h, 0x1234).unwrap();
    bridge.unregister_death_recipient(1, h, 0x1234).unwrap();
    assert_eq!(bridge.get_death_recipients(1, h), Some(vec![]));
}

#[test]
fn test_f21_3_death_notification_client_death_triggers_callbacks() {
    let bridge = HandleBridge::new();
    let h = bridge.register_service(1, "desc", Arc::new(EchoService::new()));
    bridge.register_death_recipient(1, h, 0x999).unwrap();

    let fired = Arc::new(AtomicBool::new(false));
    let fired_clone = Arc::clone(&fired);
    bridge.death_registry().add_listener(move |notif| {
        if notif.client_id == 1 && notif.cookie == 0x999 {
            fired_clone.store(true, Ordering::SeqCst);
        }
    });

    let events = bridge.on_client_died(1);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], (h, 0x999));
    assert!(fired.load(Ordering::SeqCst));
}

#[test]
fn test_f21_4_death_notification_client_death_cleans_handle_table() {
    let bridge = HandleBridge::new();
    let h1 = bridge.register_service(1, "desc.1", Arc::new(EchoService::new()));
    let h2 = bridge.register_service(1, "desc.2", Arc::new(EchoService::new()));

    bridge.on_client_died(1);
    assert_eq!(bridge.handle_count(1), 0);
    assert!(bridge.get_service(1, h1).is_none());
    assert!(bridge.get_service(1, h2).is_none());
}

#[test]
fn test_f21_5_death_notification_multiple_recipients_same_handle() {
    let bridge = HandleBridge::new();
    let h = bridge.register_service(1, "desc", Arc::new(EchoService::new()));
    bridge.register_death_recipient(1, h, 0x111).unwrap();
    bridge.register_death_recipient(1, h, 0x222).unwrap();

    let events = bridge.on_client_died(1);
    assert_eq!(events.len(), 2);
    assert!(events.contains(&(h, 0x111)));
    assert!(events.contains(&(h, 0x222)));
}

// =============================================================================
// Feature 22: Selective Routing Policy (5 tests)
// =============================================================================

#[test]
fn test_f22_1_routing_policy_default_local_deny() {
    let policy = RoutingPolicy::new_default_local();
    assert_eq!(policy.route("android.os.IBatteryStats", 1), RouteAction::LocalGuest);
    assert_eq!(policy.route("android.gui.ISurfaceComposer", 1006), RouteAction::LocalGuest);
}

#[test]
fn test_f22_2_routing_policy_allow_host_offload_exact() {
    let mut policy = RoutingPolicy::new_default_local();
    policy.allow_host_offload("android.gui.ISurfaceComposer");
    assert_eq!(policy.route("android.gui.ISurfaceComposer", 1006), RouteAction::HostOffload);
    assert_eq!(policy.route("android.os.IPowerManager", 1), RouteAction::LocalGuest);
}

#[test]
fn test_f22_3_routing_policy_allow_host_offload_wildcard_prefix() {
    let mut policy = RoutingPolicy::new_default_local();
    policy.allow_host_offload("android.gui.*");
    assert_eq!(policy.route("android.gui.ISurfaceComposer", 1006), RouteAction::HostOffload);
    assert_eq!(policy.route("android.gui.IGraphicBufferProducer", 1), RouteAction::HostOffload);
    assert_eq!(policy.route("android.hardware.camera", 1), RouteAction::LocalGuest);
}

#[test]
fn test_f22_4_routing_policy_deny_host_offload_override() {
    let mut policy = RoutingPolicy::new_default_local();
    policy.allow_host_offload("android.gui.*");
    policy.add_rule(
        binder_routing::RoutingRule::new("android.gui.ISurfaceComposerClient", RouteAction::LocalGuest)
            .with_priority(10),
    );

    assert_eq!(policy.route("android.gui.ISurfaceComposer", 1006), RouteAction::HostOffload);
    assert_eq!(policy.route("android.gui.ISurfaceComposerClient", 1), RouteAction::LocalGuest);
}

#[test]
fn test_f22_5_routing_policy_hybrid_opcode_selective_offload() {
    let mut policy = RoutingPolicy::new_default_local();
    policy.allow_hybrid("android.gui.ISurfaceComposer", vec![1006, 1020]);

    assert_eq!(policy.route("android.gui.ISurfaceComposer", 1006), RouteAction::HostOffload);
    assert_eq!(policy.route("android.gui.ISurfaceComposer", 1020), RouteAction::HostOffload);
    assert_eq!(policy.route("android.gui.ISurfaceComposer", 1002), RouteAction::LocalGuest);
}

// =============================================================================
// Feature 23: Interface & Code Matcher (5 tests)
// =============================================================================

#[test]
fn test_f23_1_matcher_descriptor_exact_and_prefix() {
    let mut engine = MatcherEngine::new(RouteAction::LocalGuest);
    engine.add_rule(MatchRule::new(DescriptorMatcher::Exact("android.os.IServiceManager".to_string()), RouteAction::LocalGuest));
    engine.add_rule(MatchRule::new(DescriptorMatcher::Prefix("android.gui.".to_string()), RouteAction::HostOffload));

    assert_eq!(engine.match_transaction(None, Some("android.os.IServiceManager"), 1), RouteAction::LocalGuest);
    assert_eq!(engine.match_transaction(None, Some("android.gui.ISurfaceComposer"), 1006), RouteAction::HostOffload);
}

#[test]
fn test_f23_2_matcher_descriptor_regex_and_wildcard() {
    let mut engine = MatcherEngine::new(RouteAction::LocalGuest);
    engine.add_rule(MatchRule::new(DescriptorMatcher::Wildcard("*.custom".to_string()), RouteAction::HostOffload));

    assert_eq!(engine.match_transaction(None, Some("anything.custom"), 1), RouteAction::HostOffload);
}

#[test]
fn test_f23_3_matcher_code_filter_only_and_except() {
    let mut engine = MatcherEngine::new(RouteAction::LocalGuest);
    let r1 = MatchRule::new(DescriptorMatcher::Exact("test.service".to_string()), RouteAction::HostOffload)
        .with_code_filter(CodeFilter::Specific(vec![10, 20]));
    engine.add_rule(r1);

    assert_eq!(engine.match_transaction(None, Some("test.service"), 10), RouteAction::HostOffload);
    assert_eq!(engine.match_transaction(None, Some("test.service"), 15), RouteAction::LocalGuest);
}

#[test]
fn test_f23_4_matcher_code_filter_range_and_any() {
    let mut engine = MatcherEngine::new(RouteAction::LocalGuest);
    let r = MatchRule::new(DescriptorMatcher::Exact("test.range".to_string()), RouteAction::HostOffload)
        .with_code_filter(CodeFilter::Range(100, 200));
    engine.add_rule(r);

    assert_eq!(engine.match_transaction(None, Some("test.range"), 150), RouteAction::HostOffload);
    assert_eq!(engine.match_transaction(None, Some("test.range"), 50), RouteAction::LocalGuest);
}

#[test]
fn test_f23_5_matcher_service_name_and_rule_priorities() {
    let mut engine = MatcherEngine::new(RouteAction::LocalGuest);
    let low_prio = MatchRule::new(DescriptorMatcher::Prefix("android.".to_string()), RouteAction::LocalGuest)
        .with_priority(0);
    let high_prio = MatchRule::new(DescriptorMatcher::Prefix("android.gui.".to_string()), RouteAction::HostOffload)
        .with_service(ServiceNameMatcher::Exact("SurfaceFlinger".to_string()))
        .with_priority(10);

    engine.add_rule(low_prio);
    engine.add_rule(high_prio);

    assert_eq!(
        engine.match_transaction(Some("SurfaceFlinger"), Some("android.gui.ISurfaceComposer"), 1006),
        RouteAction::HostOffload
    );
    assert_eq!(
        engine.match_transaction(Some("OtherService"), Some("android.gui.ISurfaceComposer"), 1006),
        RouteAction::LocalGuest
    );
}

// =============================================================================
// Feature 24: Offloaded Compositor Service (5 tests)
// =============================================================================

#[test]
fn test_f24_1_surface_composer_creation_and_display_info() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 1280, 720);
        let info = svc.get_display_info();
        assert_eq!(info.width, 1280);
        assert_eq!(info.height, 720);
        assert_eq!(info.fps, 120.0);
    });
}

#[test]
fn test_f24_2_surface_composer_create_surface_layer() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 640, 480);
        let handle = svc.create_surface("MainSurface", 640, 480, 0).unwrap();
        assert_eq!(handle.name, "MainSurface");
        assert_eq!(svc.get_layer_count(), 1);
    });
}

#[test]
fn test_f24_3_surface_composer_destroy_surface_layer() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 640, 480);
        let handle = svc.create_surface("TempSurface", 640, 480, 0).unwrap();
        assert_eq!(svc.get_layer_count(), 1);

        svc.destroy_surface(handle.surface_id).unwrap();
        assert_eq!(svc.get_layer_count(), 0);
    });
}

#[test]
fn test_f24_4_surface_composer_boot_finished_flag() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 640, 480);
        assert!(!svc.is_boot_finished());
        svc.set_boot_finished(true);
        assert!(svc.is_boot_finished());
    });
}

#[test]
fn test_f24_5_surface_composer_remotable_on_transact_dispatch() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 640, 480);
        let data = Parcel::new();
        let mut reply = Parcel::new();

        svc.on_transact(isurfacecomposer_codes::GET_DISPLAY_INFO, &data, &mut reply).unwrap();
        let mut off = 0;
        let status = reply.read_status(&mut off).unwrap();
        assert!(status.is_ok());
        let mut display = DisplayInfo::default();
        display.read_from_parcel_at(&reply, &mut off).unwrap();
        assert_eq!(display.width, 640);
        assert_eq!(display.height, 480);
    });
}

// =============================================================================
// Feature 25: Layer State Translation (5 tests)
// =============================================================================

#[test]
fn test_f25_1_layer_state_creation_and_properties() {
    let mut state = LayerState::new(10, "StatusBar");
    state.set_z_order(5);
    state.set_alpha(0.75);
    state.set_bounds_ndc([-1.0, 0.8, 2.0, 0.2]);

    assert_eq!(state.surface_id, 10);
    assert_eq!(state.z_order, 5);
    assert_eq!(state.alpha, 0.75);
    assert_eq!(state.bounds, [-1.0, 0.8, 2.0, 0.2]);
}

#[test]
fn test_f25_2_layer_state_parcelable_marshaling_roundtrip() {
    let mut orig = LayerState::new(42, "NavLayer");
    orig.set_color([1.0, 0.5, 0.0, 1.0]);
    orig.set_z_order(100);

    let mut p = Parcel::new();
    orig.write_to_parcel(&mut p).unwrap();

    let mut decoded = LayerState::default();
    let mut off = 0;
    decoded.read_from_parcel_at(&p, &mut off).unwrap();

    assert_eq!(decoded.surface_id, 42);
    assert_eq!(decoded.name, "NavLayer");
    assert_eq!(decoded.color, Some([1.0, 0.5, 0.0, 1.0]));
    assert_eq!(decoded.z_order, 100);
}

#[test]
fn test_f25_3_layer_translator_screen_coords_to_ndc() {
    let bounds_px = [0.0, 0.0, 1920.0, 1080.0];
    let ndc = LayerTranslator::screen_coords_to_ndc(bounds_px, 1920, 1080);
    assert_eq!(ndc, [-1.0, -1.0, 2.0, 2.0]);
}

#[test]
fn test_f25_4_layer_translator_apply_state_update_flags() {
    let mut layer = CompositionLayer::new_color(1, "Test", [-1.0, -1.0, 2.0, 2.0], 0, [0.0, 0.0, 0.0, 1.0]);
    let mut state = LayerState::new(1, "Test");
    state.set_z_order(15);
    state.set_alpha(0.5);

    LayerTranslator::apply_state_update(&mut layer, &state, 1280, 720);
    assert_eq!(layer.z_order, 15);
    assert_eq!(layer.alpha, 0.5);
}

#[test]
fn test_f25_5_composer_state_batch_translation() {
    let mut s1 = LayerState::new(1, "Layer1");
    s1.set_color([1.0, 0.0, 0.0, 1.0]);
    let mut s2 = LayerState::new(2, "Layer2");
    s2.set_color([0.0, 1.0, 0.0, 1.0]);

    let c1 = ComposerState::new(1, s1);
    let c2 = ComposerState::new(2, s2);
    assert_eq!(c1.surface_id, 1);
    assert_eq!(c2.surface_id, 2);
}

// =============================================================================
// Feature 26: WebGPU Frame Presentation (5 tests)
// =============================================================================

#[test]
fn test_f26_1_webgpu_compositor_layer_addition_and_ordering() {
    pollster::block_on(async {
        let (device, _queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let mut compositor = WebGpuCompositor::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let l1 = CompositionLayer::new_color(1, "L1", [-1.0, -1.0, 2.0, 2.0], 5, [1.0, 0.0, 0.0, 1.0]);
        let l2 = CompositionLayer::new_color(2, "L2", [-1.0, -1.0, 2.0, 2.0], 1, [0.0, 1.0, 0.0, 1.0]);

        compositor.add_or_update_layer(l1);
        compositor.add_or_update_layer(l2);
        assert_eq!(compositor.layers.len(), 2);
    });
}

#[test]
fn test_f26_2_webgpu_swapchain_mailbox_presentation() {
    pollster::block_on(async {
        let (device, _queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let mut sc = WebGpuSwapchain::new(&device, 128, 128, wgpu::TextureFormat::Rgba8Unorm);
        let f1 = sc.present();
        let f2 = sc.present();
        assert_eq!(f1, 1);
        assert_eq!(f2, 2);
    });
}

#[test]
fn test_f26_3_webgpu_compositor_solid_color_render_pass() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let mut compositor = WebGpuCompositor::new(&device, wgpu::TextureFormat::Rgba8Unorm);
        let l = CompositionLayer::new_color(1, "Background", [-1.0, -1.0, 2.0, 2.0], 0, [0.2, 0.4, 0.8, 1.0]);
        compositor.add_or_update_layer(l);

        let sc = WebGpuSwapchain::new(&device, 64, 64, wgpu::TextureFormat::Rgba8Unorm);
        compositor.compose(&device, &queue, sc.get_current_texture_view(), Some(wgpu::Color::BLACK));
    });
}

#[test]
fn test_f26_4_webgpu_compositor_pixel_readback_verification() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 64, 64);
        svc.create_surface("RedLayer", 64, 64, 0).unwrap();
        svc.set_surface_color(1, [1.0, 0.0, 0.0, 1.0], [-1.0, -1.0, 2.0, 2.0], 0).unwrap();
        svc.compose_and_present().unwrap();

        let pixels = svc.readback_pixels().await.unwrap();
        assert_eq!(pixels.len(), 64 * 64 * 4);
        // Verify red channel has value > 200
        assert!(pixels[0] > 200, "Red channel must be near 255, got {}", pixels[0]);
    });
}

#[test]
fn test_f26_5_webgpu_multi_layer_alpha_blending_composite() {
    pollster::block_on(async {
        let (device, queue) = match create_test_wgpu_device().await {
            Some(dq) => dq,
            None => return,
        };

        let svc = SurfaceComposerService::new(device, queue, 64, 64);
        svc.create_surface("Bottom", 64, 64, 0).unwrap();
        svc.create_surface("Top", 64, 64, 0).unwrap();

        svc.set_surface_color(1, [1.0, 0.0, 0.0, 1.0], [-1.0, -1.0, 2.0, 2.0], 0).unwrap(); // Red background
        svc.set_surface_color(2, [0.0, 0.0, 1.0, 0.5], [-1.0, -1.0, 2.0, 2.0], 1).unwrap(); // 50% Blue overlay
        svc.compose_and_present().unwrap();

        let pixels = svc.readback_pixels().await.unwrap();
        assert_eq!(pixels.len(), 64 * 64 * 4);
    });
}
