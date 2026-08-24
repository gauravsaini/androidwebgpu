//! Adversarial challenge test suite for crates/binder_rt.
//! Tests C-ABI alignments, struct offsets, zero-copy safety, opcode constants,
//! status/exception round-tripping, parcel padding, and boundary edge cases.

use binder_rt::*;
use std::mem::{align_of, offset_of, size_of};

// =============================================================================
// 1. C-ABI Struct Alignments, Sizes, and Field Offsets
// =============================================================================

#[test]
fn challenge_c_abi_flat_binder_object_layout() {
    // 64-bit Binder ABI requires FlatBinderObject to be exactly 24 bytes with 8-byte alignment
    assert_eq!(size_of::<FlatBinderObject>(), 24, "FlatBinderObject size must be 24 bytes");
    assert_eq!(align_of::<FlatBinderObject>(), 8, "FlatBinderObject align must be 8 bytes");

    // Verify exact C-ABI struct offsets
    assert_eq!(offset_of!(FlatBinderObject, hdr), 0, "hdr offset must be 0");
    assert_eq!(offset_of!(FlatBinderObject, flags), 4, "flags offset must be 4");
    assert_eq!(offset_of!(FlatBinderObject, binder), 8, "binder offset must be 8");
    assert_eq!(offset_of!(FlatBinderObject, cookie), 16, "cookie offset must be 16");

    // Zero-copy byte casting test with bytemuck
    let obj = FlatBinderObject::new_handle(0x12345678, 0x00000100, 0xfeedface_cafebabe);
    let bytes: &[u8] = bytemuck::bytes_of(&obj);
    assert_eq!(bytes.len(), 24);

    // Verify little-endian byte pattern directly
    // hdr.type_ = BINDER_TYPE_HANDLE = 0x73682a85 -> [0x85, 0x2a, 0x68, 0x73]
    assert_eq!(&bytes[0..4], &[0x85, 0x2a, 0x68, 0x73]);
    // flags = 0x00000100 -> [0x00, 0x01, 0x00, 0x00]
    assert_eq!(&bytes[4..8], &[0x00, 0x01, 0x00, 0x00]);
    // binder = 0x12345678 -> [0x78, 0x56, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00]
    assert_eq!(&bytes[8..16], &[0x78, 0x56, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00]);
    // cookie = 0xfeedface_cafebabe -> [0xbe, 0xba, 0xfe, 0xca, 0xce, 0xfa, 0xed, 0xfe]
    assert_eq!(&bytes[16..24], &[0xbe, 0xba, 0xfe, 0xca, 0xce, 0xfa, 0xed, 0xfe]);

    // Round-trip from bytes
    let restored: FlatBinderObject = *bytemuck::from_bytes(bytes);
    assert_eq!(restored, obj);
    assert_eq!(restored.handle(), 0x12345678);
    assert_eq!(restored.cookie, 0xfeedface_cafebabe);
    assert!(restored.is_handle());
    assert!(!restored.is_binder());
    assert!(!restored.is_fd());
}

#[test]
fn challenge_c_abi_binder_transaction_data_layout() {
    // 64-bit Binder ABI requires BinderTransactionData to be exactly 64 bytes with 8-byte alignment
    assert_eq!(size_of::<BinderTransactionData>(), 64, "BinderTransactionData size must be 64 bytes");
    assert_eq!(align_of::<BinderTransactionData>(), 8, "BinderTransactionData align must be 8 bytes");

    // Verify exact C-ABI struct offsets
    assert_eq!(offset_of!(BinderTransactionData, target), 0, "target offset must be 0");
    assert_eq!(offset_of!(BinderTransactionData, cookie), 8, "cookie offset must be 8");
    assert_eq!(offset_of!(BinderTransactionData, code), 16, "code offset must be 16");
    assert_eq!(offset_of!(BinderTransactionData, flags), 20, "flags offset must be 20");
    assert_eq!(offset_of!(BinderTransactionData, sender_pid), 24, "sender_pid offset must be 24");
    assert_eq!(offset_of!(BinderTransactionData, sender_euid), 28, "sender_euid offset must be 28");
    assert_eq!(offset_of!(BinderTransactionData, data_size), 32, "data_size offset must be 32");
    assert_eq!(offset_of!(BinderTransactionData, offsets_size), 40, "offsets_size offset must be 40");
    assert_eq!(offset_of!(BinderTransactionData, data_buffer), 48, "data_buffer offset must be 48");
    assert_eq!(offset_of!(BinderTransactionData, offsets_buffer), 56, "offsets_buffer offset must be 56");

    let tr = BinderTransactionData::new(
        0x01234567_89abcdef,
        0xfedcba98_76543210,
        FIRST_CALL_TRANSACTION + 42,
        TF_ONE_WAY | TF_ACCEPT_FDS,
        12345,
        1000,
        256,
        32,
        0x1000_0000,
        0x2000_0000,
    );

    let bytes = tr.as_bytes();
    assert_eq!(bytes.len(), 64);

    let restored = BinderTransactionData::from_bytes(bytes).expect("Should deserialize from 64-byte slice");
    assert_eq!(restored, tr);
    assert_eq!(restored.target_handle(), 0x89abcdef);
    assert!(restored.is_one_way());

    // Incomplete buffer rejection
    assert!(BinderTransactionData::from_bytes(&bytes[..63]).is_none());
}

#[test]
fn challenge_c_abi_auxiliary_structs() {
    // BinderObjectHeader: 4 bytes, align 4
    assert_eq!(size_of::<BinderObjectHeader>(), 4);
    assert_eq!(align_of::<BinderObjectHeader>(), 4);
    assert_eq!(offset_of!(BinderObjectHeader, type_), 0);

    // BinderPtrCookie: 16 bytes, align 8
    assert_eq!(size_of::<BinderPtrCookie>(), 16);
    assert_eq!(align_of::<BinderPtrCookie>(), 8);
    assert_eq!(offset_of!(BinderPtrCookie, ptr), 0);
    assert_eq!(offset_of!(BinderPtrCookie, cookie), 8);

    // BinderHandleCookie: 16 bytes, align 8
    assert_eq!(size_of::<BinderHandleCookie>(), 16);
    assert_eq!(align_of::<BinderHandleCookie>(), 8);
    assert_eq!(offset_of!(BinderHandleCookie, handle), 0);
    assert_eq!(offset_of!(BinderHandleCookie, padding), 4);
    assert_eq!(offset_of!(BinderHandleCookie, cookie), 8);
}

// =============================================================================
// 2. Command and Reply Opcodes & Well-Known Constants
// =============================================================================

#[test]
fn challenge_opcode_and_type_constants() {
    // Binder Object Types
    assert_eq!(BINDER_TYPE_BINDER, u32::from_le_bytes([0x85, b'*', b'b', b's']));
    assert_eq!(BINDER_TYPE_WEAK_BINDER, u32::from_le_bytes([0x85, b'*', b'b', b'w']));
    assert_eq!(BINDER_TYPE_HANDLE, u32::from_le_bytes([0x85, b'*', b'h', b's']));
    assert_eq!(BINDER_TYPE_WEAK_HANDLE, u32::from_le_bytes([0x85, b'*', b'h', b'w']));
    assert_eq!(BINDER_TYPE_FD, u32::from_le_bytes([0x85, b'*', b'd', b'f']));
    assert_eq!(BINDER_TYPE_FDA, u32::from_le_bytes([0x85, b'a', b'd', b'f']));
    assert_eq!(BINDER_TYPE_PTR, u32::from_le_bytes([0x85, b'*', b't', b'p']));

    // Special Transaction Codes (FourCC ASCII representation)
    assert_eq!(PING_TRANSACTION, u32::from_be_bytes(*b"_PNG"));
    assert_eq!(DUMP_TRANSACTION, u32::from_be_bytes(*b"_DMP"));
    assert_eq!(SHELL_CMD_TRANSACTION, u32::from_be_bytes(*b"_CMD"));
    assert_eq!(INTERFACE_TRANSACTION, u32::from_be_bytes(*b"_NTF"));
    assert_eq!(SYSPROPS_TRANSACTION, u32::from_be_bytes(*b"_SPR"));

    // Transaction Flags
    assert_eq!(TF_ONE_WAY, 0x01);
    assert_eq!(TF_ROOT_OBJECT, 0x04);
    assert_eq!(TF_STATUS_CODE, 0x08);
    assert_eq!(TF_ACCEPT_FDS, 0x10);
    assert_eq!(TF_CLEAR_BUF, 0x20);

    // BC_* commands
    assert_eq!(BC_TRANSACTION, 0x40406300);
    assert_eq!(BC_REPLY, 0x40406301);
    assert_eq!(BC_ACQUIRE_RESULT, 0x40046302);
    assert_eq!(BC_FREE_BUFFER, 0x40086303);
    assert_eq!(BC_INCREFS, 0x40046304);
    assert_eq!(BC_ACQUIRE, 0x40046305);
    assert_eq!(BC_RELEASE, 0x40046306);
    assert_eq!(BC_DECREFS, 0x40046307);
    assert_eq!(BC_INCREFS_DONE, 0x40106308);
    assert_eq!(BC_ACQUIRE_DONE, 0x40106309);
    assert_eq!(BC_ATTEMPT_ACQUIRE, 0x4018630a);
    assert_eq!(BC_REGISTER_LOOPER, 0x0000630b);
    assert_eq!(BC_ENTER_LOOPER, 0x0000630c);
    assert_eq!(BC_EXIT_LOOPER, 0x0000630d);
    assert_eq!(BC_REQUEST_DEATH_NOTIFICATION, 0x4010630e);
    assert_eq!(BC_CLEAR_DEATH_NOTIFICATION, 0x4010630f);
    assert_eq!(BC_DEAD_BINDER_DONE, 0x40086310);

    // BR_* return opcodes
    assert_eq!(BR_ERROR, 0x80047200);
    assert_eq!(BR_OK, 0x00007201);
    assert_eq!(BR_TRANSACTION, 0x80407202);
    assert_eq!(BR_REPLY, 0x80407203);
    assert_eq!(BR_ACQUIRE_RESULT, 0x80047204);
    assert_eq!(BR_DEAD_REPLY, 0x00007205);
    assert_eq!(BR_TRANSACTION_COMPLETE, 0x00007206);
    assert_eq!(BR_INCREFS, 0x80107207);
    assert_eq!(BR_ACQUIRE, 0x80107208);
    assert_eq!(BR_RELEASE, 0x80107209);
    assert_eq!(BR_DECREFS, 0x8010720a);
    assert_eq!(BR_NOOP, 0x0000720c);
    assert_eq!(BR_SPAWN_LOOPER, 0x0000720d);
    assert_eq!(BR_FINISHED, 0x0000720e);
    assert_eq!(BR_DEAD_BINDER, 0x8008720f);
    assert_eq!(BR_CLEAR_DEATH_NOTIFICATION_DONE, 0x80087210);
    assert_eq!(BR_FAILED_REPLY, 0x00007211);
}

// =============================================================================
// 3. Status Code and Exception Round-tripping & Fallbacks
// =============================================================================

#[test]
fn challenge_status_code_roundtripping() {
    let all_statuses = [
        (STATUS_OK, StatusCode::Ok),
        (STATUS_UNKNOWN_ERROR, StatusCode::UnknownError),
        (STATUS_NO_MEMORY, StatusCode::NoMemory),
        (STATUS_INVALID_OPERATION, StatusCode::InvalidOperation),
        (STATUS_BAD_VALUE, StatusCode::BadValue),
        (STATUS_BAD_TYPE, StatusCode::BadType),
        (STATUS_NAME_NOT_FOUND, StatusCode::NameNotFound),
        (STATUS_PERMISSION_DENIED, StatusCode::PermissionDenied),
        (STATUS_DEAD_OBJECT, StatusCode::DeadObject),
        (STATUS_FAILED_TRANSACTION, StatusCode::FailedTransaction),
        (STATUS_TIMED_OUT, StatusCode::TimedOut),
        (STATUS_ALREADY_EXISTS, StatusCode::AlreadyExists),
        (STATUS_UNKNOWN_TRANSACTION, StatusCode::UnknownTransaction),
    ];

    for (raw, expected_enum) in all_statuses {
        let sc = StatusCode::from(raw);
        assert_eq!(sc, expected_enum, "Mismatch for raw status {}", raw);
        assert_eq!(sc as i32, raw, "Enum representation does not match raw value {}", raw);
    }

    // Unmapped / unknown integer fallback
    assert_eq!(StatusCode::from(-9999), StatusCode::UnknownError);
    assert_eq!(StatusCode::from(12345), StatusCode::UnknownError);
}

#[test]
fn challenge_exception_code_roundtripping() {
    let all_exceptions = [
        (EX_NONE, ExceptionCode::None),
        (EX_SECURITY, ExceptionCode::Security),
        (EX_BAD_PARCELABLE, ExceptionCode::BadParcelable),
        (EX_ILLEGAL_ARGUMENT, ExceptionCode::IllegalArgument),
        (EX_NULL_POINTER, ExceptionCode::NullPointer),
        (EX_ILLEGAL_STATE, ExceptionCode::IllegalState),
        (EX_NETWORK_MAIN_THREAD, ExceptionCode::NetworkMainThread),
        (EX_UNSUPPORTED_OPERATION, ExceptionCode::UnsupportedOperation),
        (EX_SERVICE_SPECIFIC, ExceptionCode::ServiceSpecific),
        (EX_PARCELABLE, ExceptionCode::Parcelable),
        (EX_HAS_REPLY_HEADER, ExceptionCode::HasReplyHeader),
        (EX_TRANSACTION_FAILED, ExceptionCode::TransactionFailed),
    ];

    for (raw, expected_enum) in all_exceptions {
        let ec = ExceptionCode::from(raw);
        assert_eq!(ec, expected_enum, "Mismatch for raw exception {}", raw);
        assert_eq!(ec as i32, raw, "Enum representation does not match raw value {}", raw);
    }

    // Unmapped / unknown exception fallback
    assert_eq!(ExceptionCode::from(-9999), ExceptionCode::TransactionFailed);
    assert_eq!(ExceptionCode::from(555), ExceptionCode::TransactionFailed);
}

#[test]
fn challenge_status_struct_serialization_comprehensive() {
    // 1. Status::ok() -> exactly 4 bytes (EX_NONE)
    {
        let mut p = Parcel::new();
        p.write_status(&Status::ok()).unwrap();
        assert_eq!(p.data(), &[0x00, 0x00, 0x00, 0x00]);
        let mut off = 0;
        let s = p.read_status(&mut off).unwrap();
        assert!(s.is_ok());
        assert_eq!(s.exception_code(), ExceptionCode::None);
        assert_eq!(s.status_code(), StatusCode::Ok);
        assert_eq!(s.message(), None);
        assert_eq!(off, 4);
    }

    // 2. Status::new_service_specific_error with message
    {
        let mut p = Parcel::new();
        let status = Status::new_service_specific_error(-303, Some("Shader compilation failure"));
        p.write_status(&status).unwrap();

        let mut off = 0;
        let s = p.read_status(&mut off).unwrap();
        assert!(!s.is_ok());
        assert_eq!(s.exception_code(), ExceptionCode::ServiceSpecific);
        assert_eq!(s.service_specific_error(), Some(-303));
        assert_eq!(s.message(), Some("Shader compilation failure"));
    }

    // 3. Status::new_service_specific_error with None message
    {
        let mut p = Parcel::new();
        let status = Status::new_service_specific_error(42, None);
        p.write_status(&status).unwrap();

        let mut off = 0;
        let s = p.read_status(&mut off).unwrap();
        assert_eq!(s.exception_code(), ExceptionCode::ServiceSpecific);
        assert_eq!(s.service_specific_error(), Some(42));
        assert_eq!(s.message(), None);
    }

    // 4. Status::from_status error conversion
    {
        let s = Status::from_status(STATUS_DEAD_OBJECT);
        assert_eq!(s.exception_code(), ExceptionCode::TransactionFailed);
        assert_eq!(s.status_code(), StatusCode::DeadObject);
        assert!(!s.is_ok());
    }
}

// =============================================================================
// 4. Parcel Padding, Edge Cases, Boundary Errors & Generators
// =============================================================================

#[test]
fn challenge_parcel_scalars_extremes() {
    let mut p = Parcel::new();

    // Extreme integer values
    p.write_i8(i8::MIN).unwrap();
    p.write_i8(i8::MAX).unwrap();
    p.write_u8(u8::MIN).unwrap();
    p.write_u8(u8::MAX).unwrap();

    p.write_i16(i16::MIN).unwrap();
    p.write_i16(i16::MAX).unwrap();
    p.write_u16(u16::MIN).unwrap();
    p.write_u16(u16::MAX).unwrap();

    p.write_i32(i32::MIN).unwrap();
    p.write_i32(i32::MAX).unwrap();
    p.write_u32(u32::MIN).unwrap();
    p.write_u32(u32::MAX).unwrap();

    p.write_i64(i64::MIN).unwrap();
    p.write_i64(i64::MAX).unwrap();
    p.write_u64(u64::MIN).unwrap();
    p.write_u64(u64::MAX).unwrap();

    // Extreme floating point values
    p.write_f32(f32::INFINITY).unwrap();
    p.write_f32(f32::NEG_INFINITY).unwrap();
    p.write_f32(f32::MIN_POSITIVE).unwrap();
    p.write_f32(f32::MAX).unwrap();

    p.write_f64(f64::INFINITY).unwrap();
    p.write_f64(f64::NEG_INFINITY).unwrap();
    p.write_f64(f64::MIN_POSITIVE).unwrap();
    p.write_f64(f64::MAX).unwrap();

    // Read back and verify
    let mut off = 0;
    assert_eq!(p.read_i8(&mut off).unwrap(), i8::MIN);
    assert_eq!(p.read_i8(&mut off).unwrap(), i8::MAX);
    assert_eq!(p.read_u8(&mut off).unwrap(), u8::MIN);
    assert_eq!(p.read_u8(&mut off).unwrap(), u8::MAX);

    assert_eq!(p.read_i16(&mut off).unwrap(), i16::MIN);
    assert_eq!(p.read_i16(&mut off).unwrap(), i16::MAX);
    assert_eq!(p.read_u16(&mut off).unwrap(), u16::MIN);
    assert_eq!(p.read_u16(&mut off).unwrap(), u16::MAX);

    assert_eq!(p.read_i32(&mut off).unwrap(), i32::MIN);
    assert_eq!(p.read_i32(&mut off).unwrap(), i32::MAX);
    assert_eq!(p.read_u32(&mut off).unwrap(), u32::MIN);
    assert_eq!(p.read_u32(&mut off).unwrap(), u32::MAX);

    assert_eq!(p.read_i64(&mut off).unwrap(), i64::MIN);
    assert_eq!(p.read_i64(&mut off).unwrap(), i64::MAX);
    assert_eq!(p.read_u64(&mut off).unwrap(), u64::MIN);
    assert_eq!(p.read_u64(&mut off).unwrap(), u64::MAX);

    assert_eq!(p.read_f32(&mut off).unwrap(), f32::INFINITY);
    assert_eq!(p.read_f32(&mut off).unwrap(), f32::NEG_INFINITY);
    assert_eq!(p.read_f32(&mut off).unwrap(), f32::MIN_POSITIVE);
    assert_eq!(p.read_f32(&mut off).unwrap(), f32::MAX);

    assert_eq!(p.read_f64(&mut off).unwrap(), f64::INFINITY);
    assert_eq!(p.read_f64(&mut off).unwrap(), f64::NEG_INFINITY);
    assert_eq!(p.read_f64(&mut off).unwrap(), f64::MIN_POSITIVE);
    assert_eq!(p.read_f64(&mut off).unwrap(), f64::MAX);

    assert_eq!(off, p.data_size());
    assert_eq!(p.data_size() % 4, 0);
}

#[test]
fn challenge_string_padding_all_mod4_cases() {
    // Strings of length 0, 1, 2, 3, 4, 5, 6, 7, 8
    // UTF-8 payload size = len + 1.
    // len=0: 1 byte + 3 pad = 4 bytes payload -> total 8
    // len=1: 2 bytes + 2 pad = 4 bytes payload -> total 8
    // len=2: 3 bytes + 1 pad = 4 bytes payload -> total 8
    // len=3: 4 bytes + 0 pad = 4 bytes payload -> total 8
    // len=4: 5 bytes + 3 pad = 8 bytes payload -> total 12
    for len in 0..=32 {
        let original: String = (0..len).map(|i| ((b'a' + (i % 26) as u8) as char)).collect();
        let mut p = Parcel::new();
        p.write_utf8(Some(&original)).unwrap();

        let payload_and_term = len + 1;
        let expected_pad = (4 - (payload_and_term % 4)) % 4;
        let expected_total = 4 + payload_and_term + expected_pad;
        assert_eq!(p.data_size(), expected_total, "Failed total for len {}", len);
        assert_eq!(p.data_size() % 4, 0);

        let mut off = 0;
        let read = p.read_utf8(&mut off).unwrap();
        assert_eq!(read, Some(original));
        assert_eq!(off, expected_total);
    }
}

#[test]
fn challenge_utf16_multilingual_and_emoji() {
    let test_cases = [
        "",
        "A",
        "AB",
        "ABC",
        "ABCD",
        "Hello, World!",
        "你好世界",
        "こんにちは世界",
        "안녕 세상",
        "Здравствуй, мир",
        "🔥⚡✨🎉🚀👾🧠",
        "Complex 混合 텍스트 with 🎯 emojis and symbols ∭ ∇×B",
    ];

    for s in test_cases {
        let mut p = Parcel::new();
        p.write_utf16(Some(s)).unwrap();

        assert_eq!(p.data_size() % 4, 0);

        let mut off = 0;
        let read = p.read_utf16(&mut off).unwrap();
        assert_eq!(read.as_deref(), Some(s));
        assert_eq!(off, p.data_size());
    }
}

#[test]
fn challenge_corrupted_inputs_and_bounds_checking() {
    // 1. Missing null terminator in UTF-8 string
    {
        let mut p = Parcel::new();
        p.write_i32(4).unwrap(); // claims length 4
        p.data_mut().extend_from_slice(b"abcd"); // 4 bytes without null terminator
        p.data_mut().extend_from_slice(&[0xff, 0xff, 0xff, 0xff]); // padding/garbage without 0x00

        let mut off = 0;
        let err = p.read_utf8(&mut off).unwrap_err();
        assert!(matches!(err, ParcelError::MissingNullTerminator(_)));
    }

    // 2. Buffer underflow on read_i64 with only 4 bytes
    {
        let mut p = Parcel::new();
        p.write_i32(1234).unwrap();
        let mut off = 0;
        let err = p.read_i64(&mut off).unwrap_err();
        assert_eq!(
            err,
            ParcelError::NotEnoughData {
                offset: 0,
                requested: 8,
                available: 4
            }
        );
    }

    // 3. Bad negative count in vector
    {
        let mut p = Parcel::new();
        p.write_i32(-5).unwrap(); // invalid negative count
        let mut off = 0;
        let err = p.read_vector(&mut off, |parcel, o| parcel.read_i32(o)).unwrap_err();
        assert_eq!(err, ParcelError::BadParcelable(-5));
    }

    // 4. Invalid Binder object read when object not in offsets
    {
        let mut p = Parcel::new();
        let obj = FlatBinderObject::new_handle(1, 0, 100);
        // Write raw bytes without registering offset
        p.data_mut().extend_from_slice(bytemuck::bytes_of(&obj));
        // Register a fake offset at 100
        p.offsets_mut().push(100);

        let mut off = 0;
        let err = p.read_binder_object(&mut off).unwrap_err();
        assert_eq!(err, ParcelError::ObjectOffsetNotFound(0));
    }
}

// =============================================================================
// 5. High-Volume Generator & Oracle Stress Harness
// =============================================================================

#[test]
fn challenge_high_volume_pseudo_random_oracle() {
    let mut rng_state: u64 = 0x123456789abcdef0;
    let mut next_u32 = || {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        rng_state as u32
    };

    let mut parcel = Parcel::new();
    let num_iterations = 2000;
    let mut expected_values = Vec::new();

    for i in 0..num_iterations {
        let tag = next_u32() % 6;
        match tag {
            0 => {
                let v = next_u32() as i32;
                parcel.write_i32(v).unwrap();
                expected_values.push((0, v as i64, String::new()));
            }
            1 => {
                let v = ((next_u32() as u64) << 32) | (next_u32() as u64);
                parcel.write_u64(v).unwrap();
                expected_values.push((1, v as i64, String::new()));
            }
            2 => {
                let s = format!("payload_{}_{}", i, next_u32() % 1000);
                parcel.write_utf8(Some(&s)).unwrap();
                expected_values.push((2, 0, s));
            }
            3 => {
                let s = format!("utf16_🚀_{}_{}", i, next_u32() % 1000);
                parcel.write_utf16(Some(&s)).unwrap();
                expected_values.push((3, 0, s));
            }
            4 => {
                let handle = next_u32() % 500;
                let cookie = ((next_u32() as u64) << 32) | (next_u32() as u64);
                parcel.write_binder(handle, cookie).unwrap();
                expected_values.push((4, handle as i64, cookie.to_string()));
            }
            5 => {
                let fd = (next_u32() % 100) as i32;
                let cookie = next_u32() as u64;
                parcel.write_file_descriptor(fd, cookie).unwrap();
                expected_values.push((5, fd as i64, cookie.to_string()));
            }
            _ => unreachable!(),
        }
    }

    assert_eq!(parcel.data_size() % 4, 0);

    let mut off = 0;
    for (tag, expected_num, expected_str) in expected_values {
        match tag {
            0 => {
                let val = parcel.read_i32(&mut off).unwrap();
                assert_eq!(val, expected_num as i32);
            }
            1 => {
                let val = parcel.read_u64(&mut off).unwrap();
                assert_eq!(val, expected_num as u64);
            }
            2 => {
                let s = parcel.read_utf8(&mut off).unwrap();
                assert_eq!(s, Some(expected_str));
            }
            3 => {
                let s = parcel.read_utf16(&mut off).unwrap();
                assert_eq!(s, Some(expected_str));
            }
            4 => {
                let obj = parcel.read_binder(&mut off).unwrap();
                assert_eq!(obj.handle(), expected_num as u32);
                assert_eq!(obj.cookie.to_string(), expected_str);
            }
            5 => {
                let fd = parcel.read_file_descriptor(&mut off).unwrap();
                assert_eq!(fd, expected_num as i32);
            }
            _ => unreachable!(),
        }
    }

    assert_eq!(off, parcel.data_size());
}
