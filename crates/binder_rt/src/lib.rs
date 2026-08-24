//! # binder_rt
//!
//! Pure-Rust AOSP Parcel Codec and Binder Wire Protocol Runtime for AndroidWebGPU.
//! Provides binary serialization, transaction envelopes, and status representations
//! compatible with Android Open Source Project (AOSP) Binder specifications.

pub mod parcel;
pub mod status;
pub mod types;
pub mod wire;

pub use parcel::{Parcel, ParcelError};
pub use status::{
    ExceptionCode, Status, StatusCode, EX_BAD_PARCELABLE, EX_HAS_REPLY_HEADER, EX_ILLEGAL_ARGUMENT,
    EX_ILLEGAL_STATE, EX_NETWORK_MAIN_THREAD, EX_NONE, EX_NULL_POINTER, EX_PARCELABLE, EX_SECURITY,
    EX_SERVICE_SPECIFIC, EX_TRANSACTION_FAILED, EX_UNSUPPORTED_OPERATION, STATUS_ALREADY_EXISTS,
    STATUS_BAD_TYPE, STATUS_BAD_VALUE, STATUS_DEAD_OBJECT, STATUS_FAILED_TRANSACTION,
    STATUS_INVALID_OPERATION, STATUS_NAME_NOT_FOUND, STATUS_NO_MEMORY, STATUS_OK,
    STATUS_PERMISSION_DENIED, STATUS_TIMED_OUT, STATUS_UNKNOWN_ERROR, STATUS_UNKNOWN_TRANSACTION,
};
pub use types::{
    BinderObjectHeader, BinderSizeT, BinderUintptrT, FlatBinderObject, TransactionCode,
    TransactionFlags, BINDER_TYPE_BINDER, BINDER_TYPE_FD, BINDER_TYPE_FDA, BINDER_TYPE_HANDLE,
    BINDER_TYPE_PTR, BINDER_TYPE_WEAK_BINDER, BINDER_TYPE_WEAK_HANDLE,
};
pub use wire::{
    BinderHandleCookie, BinderPtrCookie, BinderTransactionData, BC_ACQUIRE, BC_ACQUIRE_DONE,
    BC_ACQUIRE_RESULT, BC_ATTEMPT_ACQUIRE, BC_CLEAR_DEATH_NOTIFICATION, BC_DEAD_BINDER_DONE,
    BC_DECREFS, BC_ENTER_LOOPER, BC_EXIT_LOOPER, BC_FREE_BUFFER, BC_INCREFS, BC_INCREFS_DONE,
    BC_REGISTER_LOOPER, BC_RELEASE, BC_REPLY, BC_REQUEST_DEATH_NOTIFICATION, BC_TRANSACTION,
    BR_ACQUIRE, BR_ACQUIRE_RESULT, BR_CLEAR_DEATH_NOTIFICATION_DONE, BR_DEAD_BINDER, BR_DEAD_REPLY,
    BR_DECREFS, BR_ERROR, BR_FAILED_REPLY, BR_FINISHED, BR_INCREFS, BR_NOOP, BR_OK, BR_RELEASE,
    BR_REPLY, BR_SPAWN_LOOPER, BR_TRANSACTION, BR_TRANSACTION_COMPLETE, DUMP_TRANSACTION,
    FIRST_CALL_TRANSACTION, INTERFACE_TRANSACTION, LAST_CALL_TRANSACTION, PING_TRANSACTION,
    SHELL_CMD_TRANSACTION, SYSPROPS_TRANSACTION, TF_ACCEPT_FDS, TF_CLEAR_BUF, TF_ONE_WAY,
    TF_ROOT_OBJECT, TF_STATUS_CODE,
};

/// Type alias for Results carrying AIDL Status errors.
pub type Result<T> = std::result::Result<T, Status>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_scalars_roundtrip() {
        let mut p = Parcel::new();
        p.write_bool(true).unwrap();
        p.write_bool(false).unwrap();
        p.write_i8(-42).unwrap();
        p.write_u8(200).unwrap();
        p.write_i16(-1234).unwrap();
        p.write_u16(5678).unwrap();
        p.write_i32(-999999).unwrap();
        p.write_u32(0xdeadbeef).unwrap();
        p.write_i64(-0x123456789abcdef).unwrap();
        p.write_u64(0xfedcba9876543210).unwrap();
        p.write_f32(3.14159).unwrap();
        p.write_f64(2.718281828459045).unwrap();
        p.write_char('Z').unwrap();

        let mut offset = 0;
        assert_eq!(p.read_bool(&mut offset).unwrap(), true);
        assert_eq!(p.read_bool(&mut offset).unwrap(), false);
        assert_eq!(p.read_i8(&mut offset).unwrap(), -42);
        assert_eq!(p.read_u8(&mut offset).unwrap(), 200);
        assert_eq!(p.read_i16(&mut offset).unwrap(), -1234);
        assert_eq!(p.read_u16(&mut offset).unwrap(), 5678);
        assert_eq!(p.read_i32(&mut offset).unwrap(), -999999);
        assert_eq!(p.read_u32(&mut offset).unwrap(), 0xdeadbeef);
        assert_eq!(p.read_i64(&mut offset).unwrap(), -0x123456789abcdef);
        assert_eq!(p.read_u64(&mut offset).unwrap(), 0xfedcba9876543210);
        assert!((p.read_f32(&mut offset).unwrap() - 3.14159).abs() < 1e-5);
        assert!((p.read_f64(&mut offset).unwrap() - 2.718281828459045).abs() < 1e-12);
        assert_eq!(p.read_char(&mut offset).unwrap(), 'Z');
        assert_eq!(offset, p.data_size());
    }

    #[test]
    fn test_utf8_string_aosp_fixtures() {
        // Null string -> i32 -1 (4 bytes: 0xff, 0xff, 0xff, 0xff)
        let mut p_null = Parcel::new();
        p_null.write_utf8(None).unwrap();
        assert_eq!(p_null.data(), &[0xff, 0xff, 0xff, 0xff]);
        let mut off = 0;
        assert_eq!(p_null.read_utf8(&mut off).unwrap(), None);
        assert_eq!(off, 4);

        // Empty string -> length 0 (4 bytes), null terminator 0x00 (1 byte), 3 padding bytes = 8 bytes total
        let mut p_empty = Parcel::new();
        p_empty.write_utf8(Some("")).unwrap();
        assert_eq!(
            p_empty.data(),
            &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(p_empty.data_size(), 8);
        let mut off = 0;
        assert_eq!(p_empty.read_utf8(&mut off).unwrap(), Some("".to_string()));
        assert_eq!(off, 8);

        // "android" -> length 7 (4 bytes), 7 UTF-8 bytes + 0x00 (8 bytes) -> total 12 bytes, 0 pad
        let mut p_android = Parcel::new();
        p_android.write_utf8(Some("android")).unwrap();
        let expected_android = [
            0x07, 0x00, 0x00, 0x00, // length = 7
            b'a', b'n', b'd', b'r', b'o', b'i', b'd', 0x00, // bytes + null
        ];
        assert_eq!(p_android.data(), &expected_android);
        assert_eq!(p_android.data_size(), 12);
        let mut off = 0;
        assert_eq!(
            p_android.read_utf8(&mut off).unwrap(),
            Some("android".to_string())
        );

        // Non-ASCII and multi-byte UTF-8 string
        let mut p_multi = Parcel::new();
        let test_str = "Android 🚀 WebGPU";
        p_multi.write_utf8(Some(test_str)).unwrap();
        let mut off = 0;
        assert_eq!(
            p_multi.read_utf8(&mut off).unwrap(),
            Some(test_str.to_string())
        );
        assert_eq!(p_multi.data_size() % 4, 0);
    }

    #[test]
    fn test_utf16_string_aosp_fixtures() {
        // Null string -> i32 -1 (4 bytes: 0xff, 0xff, 0xff, 0xff)
        let mut p_null = Parcel::new();
        p_null.write_utf16(None).unwrap();
        assert_eq!(p_null.data(), &[0xff, 0xff, 0xff, 0xff]);
        let mut off = 0;
        assert_eq!(p_null.read_utf16(&mut off).unwrap(), None);

        // Empty string -> length 0 (4 bytes), 2-byte null terminator (0x00, 0x00), 2 pad bytes = 8 bytes
        let mut p_empty = Parcel::new();
        p_empty.write_utf16(Some("")).unwrap();
        assert_eq!(
            p_empty.data(),
            &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(p_empty.data_size(), 8);
        let mut off = 0;
        assert_eq!(p_empty.read_utf16(&mut off).unwrap(), Some("".to_string()));

        // "android" -> length 7 (4 bytes), 14 bytes UTF-16LE, 2 bytes null terminator = 20 bytes (0 pad)
        let mut p_android = Parcel::new();
        p_android.write_utf16(Some("android")).unwrap();
        let expected_utf16_android: [u8; 20] = [
            0x07, 0x00, 0x00, 0x00, // length = 7
            0x61, 0x00, // 'a'
            0x6e, 0x00, // 'n'
            0x64, 0x00, // 'd'
            0x72, 0x00, // 'r'
            0x6f, 0x00, // 'o'
            0x69, 0x00, // 'i'
            0x64, 0x00, // 'd'
            0x00, 0x00, // null terminator
        ];
        assert_eq!(p_android.data(), &expected_utf16_android);
        assert_eq!(p_android.data_size(), 20);
        let mut off = 0;
        assert_eq!(
            p_android.read_utf16(&mut off).unwrap(),
            Some("android".to_string())
        );
    }

    #[test]
    fn test_byte_vectors_and_slices() {
        let mut p = Parcel::new();
        let data = [1u8, 2, 3, 4, 5]; // 5 bytes -> pad 3 bytes -> 8 bytes payload + 4 bytes header = 12
        p.write_byte_slice(Some(&data)).unwrap();
        assert_eq!(p.data_size(), 12);

        let mut off = 0;
        let read_back = p.read_byte_vec(&mut off).unwrap();
        assert_eq!(read_back, Some(vec![1, 2, 3, 4, 5]));
        assert_eq!(off, 12);
    }

    #[test]
    fn test_generic_vectors_and_presence() {
        let mut p = Parcel::new();
        let nums = vec![10i32, 20, 30, 40];
        p.write_vector(Some(&nums), |parcel, &item| parcel.write_i32(item))
            .unwrap();

        let mut off = 0;
        let decoded: Option<Vec<i32>> = p
            .read_vector(&mut off, |parcel, offset| parcel.read_i32(offset))
            .unwrap();
        assert_eq!(decoded, Some(nums));

        // Presence flag test
        let mut p_pres = Parcel::new();
        p_pres
            .write_nullable_presence(Some(&"hello"), |parcel, val| parcel.write_utf8(Some(val)))
            .unwrap();
        p_pres
            .write_nullable_presence::<String, _>(None, |parcel, val| parcel.write_utf8(Some(val)))
            .unwrap();

        let mut off = 0;
        let res1 = p_pres
            .read_nullable_presence(&mut off, |parcel, offset| {
                parcel.read_utf8(offset).map(|opt| opt.unwrap())
            })
            .unwrap();
        assert_eq!(res1, Some("hello".to_string()));

        let res2 = p_pres
            .read_nullable_presence(&mut off, |parcel, offset| {
                parcel.read_utf8(offset).map(|opt| opt.unwrap())
            })
            .unwrap();
        assert_eq!(res2, None);
    }

    #[test]
    fn test_flat_binder_object_and_offsets() {
        assert_eq!(std::mem::size_of::<FlatBinderObject>(), 24);
        assert_eq!(std::mem::align_of::<FlatBinderObject>(), 8);

        let mut p = Parcel::new();
        p.write_i32(12345).unwrap(); // 4 bytes at offset 0
        p.write_binder(42, 0x1000).unwrap(); // 24 bytes at offset 4
        p.write_file_descriptor(7, 0x2000).unwrap(); // 24 bytes at offset 28

        assert_eq!(p.offsets(), &[4, 28]);
        assert_eq!(p.offsets_size(), 16);
        assert_eq!(p.data_size(), 52);

        let mut off = 0;
        assert_eq!(p.read_i32(&mut off).unwrap(), 12345);

        let obj1 = p.read_binder(&mut off).unwrap();
        assert_eq!(obj1.hdr.type_, BINDER_TYPE_HANDLE);
        assert_eq!(obj1.handle(), 42);
        assert_eq!(obj1.cookie, 0x1000);
        assert!(obj1.is_handle());

        let fd = p.read_file_descriptor(&mut off).unwrap();
        assert_eq!(fd, 7);
    }

    #[test]
    fn test_binder_transaction_data_64byte_envelope() {
        assert_eq!(std::mem::size_of::<BinderTransactionData>(), 64);
        assert_eq!(std::mem::align_of::<BinderTransactionData>(), 8);

        let tr = BinderTransactionData::new(
            1, // target handle 1
            0x12345678_9abcdef0,
            PING_TRANSACTION,
            TF_ACCEPT_FDS,
            1001,
            2001,
            128,
            16,
            0x7fff0000,
            0x7fff1000,
        );

        assert_eq!(tr.target_handle(), 1);
        assert_eq!(tr.code, PING_TRANSACTION);
        assert!(!tr.is_one_way());

        let bytes = tr.as_bytes();
        assert_eq!(bytes.len(), 64);

        let decoded = BinderTransactionData::from_bytes(bytes).unwrap();
        assert_eq!(decoded, tr);
    }

    #[test]
    fn test_aidl_status_and_exception_marshaling() {
        // 1. Success Status
        let mut p_ok = Parcel::new();
        let ok_status = Status::ok();
        p_ok.write_status(&ok_status).unwrap();
        assert_eq!(p_ok.data(), &[0x00, 0x00, 0x00, 0x00]); // EX_NONE = 0
        let mut off = 0;
        let read_ok = p_ok.read_status(&mut off).unwrap();
        assert!(read_ok.is_ok());

        // 2. Exception Status (EX_ILLEGAL_ARGUMENT)
        let mut p_err = Parcel::new();
        let err_status = Status::new_exception(
            ExceptionCode::IllegalArgument,
            Some("Invalid dimensions given"),
        );
        p_err.write_status(&err_status).unwrap();
        let mut off = 0;
        let read_err = p_err.read_status(&mut off).unwrap();
        assert_eq!(read_err.exception_code(), ExceptionCode::IllegalArgument);
        assert_eq!(read_err.message(), Some("Invalid dimensions given"));

        // 3. Service Specific Exception (EX_SERVICE_SPECIFIC)
        let mut p_svc = Parcel::new();
        let svc_status = Status::new_service_specific_error(-404, Some("Layer not found"));
        p_svc.write_status(&svc_status).unwrap();
        let mut off = 0;
        let read_svc = p_svc.read_status(&mut off).unwrap();
        assert_eq!(read_svc.exception_code(), ExceptionCode::ServiceSpecific);
        assert_eq!(read_svc.service_specific_error(), Some(-404));
        assert_eq!(read_svc.message(), Some("Layer not found"));
    }

    #[test]
    fn test_parcel_bounds_and_error_handling() {
        let p = Parcel::from_slice(&[0x01, 0x00, 0x00]); // 3 bytes only
        let mut off = 0;
        let err = p.read_i32(&mut off).unwrap_err();
        assert_eq!(
            err,
            ParcelError::NotEnoughData {
                offset: 0,
                requested: 4,
                available: 3
            }
        );
    }
}
