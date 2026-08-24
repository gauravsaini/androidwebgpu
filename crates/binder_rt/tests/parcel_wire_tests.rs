use binder_rt::*;

#[test]
fn test_all_string_padding_permutations_utf8() {
    for len in 0..16 {
        let test_str: String = "A".repeat(len);
        let mut p = Parcel::new();
        p.write_utf8(Some(&test_str)).unwrap();

        // Length header: 4 bytes.
        // Payload: len bytes + 1 byte (0x00).
        // Total before pad: 4 + len + 1.
        // Pad: (4 - ((len + 1) % 4)) % 4.
        let expected_size = 4 + (len + 1) + ((4 - ((len + 1) % 4)) % 4);
        assert_eq!(
            p.data_size(),
            expected_size,
            "Failed size check for UTF-8 string of length {len}"
        );
        assert_eq!(p.data_size() % 4, 0);

        let mut off = 0;
        let decoded = p.read_utf8(&mut off).unwrap();
        assert_eq!(decoded, Some(test_str));
        assert_eq!(off, expected_size);
    }
}

#[test]
fn test_all_string_padding_permutations_utf16() {
    for len in 0..16 {
        let test_str: String = "B".repeat(len);
        let mut p = Parcel::new();
        p.write_utf16(Some(&test_str)).unwrap();

        // Length header: 4 bytes.
        // Payload: (len + 1) * 2 bytes.
        // Pad: (4 - (((len + 1) * 2) % 4)) % 4.
        let byte_count = (len + 1) * 2;
        let expected_size = 4 + byte_count + ((4 - (byte_count % 4)) % 4);
        assert_eq!(
            p.data_size(),
            expected_size,
            "Failed size check for UTF-16 string of length {len}"
        );
        assert_eq!(p.data_size() % 4, 0);

        let mut off = 0;
        let decoded = p.read_utf16(&mut off).unwrap();
        assert_eq!(decoded, Some(test_str));
        assert_eq!(off, expected_size);
    }
}

#[test]
fn test_all_byte_slice_padding_permutations() {
    for len in 0..16 {
        let data: Vec<u8> = (0..len as u8).collect();
        let mut p = Parcel::new();
        p.write_byte_slice(Some(&data)).unwrap();

        let expected_size = 4 + len + ((4 - (len % 4)) % 4);
        assert_eq!(p.data_size(), expected_size);
        assert_eq!(p.data_size() % 4, 0);

        let mut off = 0;
        let decoded = p.read_byte_vec(&mut off).unwrap();
        assert_eq!(decoded, Some(data));
        assert_eq!(off, expected_size);
    }
}

#[test]
fn test_surrogate_pairs_and_unicode_utf16() {
    let complex_str = "Android 🚀 WebGPU 🦀 2026";
    let mut p = Parcel::new();
    p.write_utf16(Some(complex_str)).unwrap();
    assert_eq!(p.data_size() % 4, 0);

    let mut off = 0;
    let decoded = p.read_utf16(&mut off).unwrap();
    assert_eq!(decoded, Some(complex_str.to_string()));
}

#[test]
fn test_interleaved_objects_primitives_strings() {
    let mut p = Parcel::new();
    p.write_i32(100).unwrap();
    p.write_utf8(Some("surface_control")).unwrap();
    p.write_binder(101, 0xcafe_babe).unwrap();
    p.write_f32(1.5).unwrap();
    p.write_file_descriptor(4, 0xdead_beef).unwrap();
    p.write_utf16(Some("graphic_buffer")).unwrap();
    p.write_binder(102, 0x1234_5678).unwrap();

    assert_eq!(p.offsets().len(), 3);
    assert_eq!(p.offsets_size(), 24);

    let mut off = 0;
    assert_eq!(p.read_i32(&mut off).unwrap(), 100);
    assert_eq!(
        p.read_utf8(&mut off).unwrap(),
        Some("surface_control".to_string())
    );

    let b1 = p.read_binder(&mut off).unwrap();
    assert_eq!(b1.handle(), 101);
    assert_eq!(b1.cookie, 0xcafe_babe);

    assert_eq!(p.read_f32(&mut off).unwrap(), 1.5);

    let fd = p.read_file_descriptor(&mut off).unwrap();
    assert_eq!(fd, 4);

    assert_eq!(
        p.read_utf16(&mut off).unwrap(),
        Some("graphic_buffer".to_string())
    );

    let b2 = p.read_binder(&mut off).unwrap();
    assert_eq!(b2.handle(), 102);
    assert_eq!(b2.cookie, 0x1234_5678);

    assert_eq!(off, p.data_size());
}

#[test]
fn test_all_aidl_exception_codes_marshaling() {
    let exceptions = [
        (
            ExceptionCode::Security,
            "SecurityException: permission denied",
        ),
        (ExceptionCode::BadParcelable, "BadParcelableException"),
        (
            ExceptionCode::IllegalArgument,
            "IllegalArgumentException: invalid argument",
        ),
        (
            ExceptionCode::NullPointer,
            "NullPointerException: pointer is null",
        ),
        (ExceptionCode::IllegalState, "IllegalStateException"),
        (
            ExceptionCode::NetworkMainThread,
            "NetworkOnMainThreadException",
        ),
        (
            ExceptionCode::UnsupportedOperation,
            "UnsupportedOperationException",
        ),
        (ExceptionCode::TransactionFailed, "TransactionFailed"),
    ];

    for (code, msg) in exceptions {
        let mut p = Parcel::new();
        let status = Status::new_exception(code, Some(msg));
        p.write_status(&status).unwrap();

        let mut off = 0;
        let read_status = p.read_status(&mut off).unwrap();
        assert_eq!(read_status.exception_code(), code);
        assert_eq!(read_status.message(), Some(msg));
        assert!(!read_status.is_ok());
    }
}

#[test]
fn test_special_transaction_codes_and_flags() {
    let codes = [
        FIRST_CALL_TRANSACTION,
        LAST_CALL_TRANSACTION,
        PING_TRANSACTION,
        DUMP_TRANSACTION,
        SHELL_CMD_TRANSACTION,
        INTERFACE_TRANSACTION,
        SYSPROPS_TRANSACTION,
    ];

    for code in codes {
        let tr =
            BinderTransactionData::new(0, 0, code, TF_ONE_WAY | TF_CLEAR_BUF, 123, 456, 0, 0, 0, 0);
        assert_eq!(tr.code, code);
        assert!(tr.is_one_way());
        assert_eq!(tr.target_handle(), 0);

        let bytes = tr.as_bytes();
        let restored = BinderTransactionData::from_bytes(bytes).unwrap();
        assert_eq!(restored.code, code);
        assert_eq!(restored.flags, TF_ONE_WAY | TF_CLEAR_BUF);
    }
}

#[test]
fn test_parcel_append_all() {
    let mut p1 = Parcel::new();
    p1.write_i32(1).unwrap();
    p1.write_binder(10, 0x100).unwrap();

    let mut p2 = Parcel::new();
    p2.write_i32(2).unwrap();
    p2.write_binder(20, 0x200).unwrap();

    p1.append_all(&p2);

    assert_eq!(p1.offsets().len(), 2);
    let mut off = 0;
    assert_eq!(p1.read_i32(&mut off).unwrap(), 1);
    let b1 = p1.read_binder(&mut off).unwrap();
    assert_eq!(b1.handle(), 10);
    assert_eq!(p1.read_i32(&mut off).unwrap(), 2);
    let b2 = p1.read_binder(&mut off).unwrap();
    assert_eq!(b2.handle(), 20);
}
