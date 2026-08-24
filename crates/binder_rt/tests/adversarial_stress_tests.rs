//! Adversarial, Stress, Property, and Fuzzing Test Suite for binder_rt.

use binder_rt::*;

/// Fast deterministic PRNG (Xorshift64) for reproducible stress testing without dependencies.
struct SimplePrng {
    state: u64,
}

impl SimplePrng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xdeadbeefcafe1337 } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    fn next_u8(&mut self) -> u8 {
        self.next_u64() as u8
    }

    fn next_bool(&mut self) -> bool {
        (self.next_u64() & 1) != 0
    }

    fn next_range(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        let span = (max - min) as u64;
        min + (self.next_u64() % span) as usize
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for byte in dest.iter_mut() {
            *byte = self.next_u8();
        }
    }
}

// =============================================================================
// 1. EXTREME BUFFER SIZES AND LENGTH PREFIX ATTACKS
// =============================================================================

#[test]
fn test_extreme_buffer_size_and_overflow_defense() {
    // Malicious length header claiming i32::MAX (2GB) in a 4-byte buffer
    let mut bad_utf8_buf = Vec::new();
    bad_utf8_buf.extend_from_slice(&i32::MAX.to_le_bytes());
    let p = Parcel::from_slice(&bad_utf8_buf);
    let mut off = 0;
    let err = p.read_utf8(&mut off).unwrap_err();
    assert!(matches!(err, ParcelError::NotEnoughData { .. }));
    // Offset consumed the 4-byte length prefix before checking payload bounds
    assert_eq!(off, 4);

    // Malicious length header claiming i32::MAX in UTF-16 read
    let mut off = 0;
    let err = p.read_utf16(&mut off).unwrap_err();
    assert!(matches!(err, ParcelError::NotEnoughData { .. }));
    assert_eq!(off, 4);

    // Malicious length header claiming i32::MAX in byte_vec read
    let mut off = 0;
    let err = p.read_byte_vec(&mut off).unwrap_err();
    assert!(matches!(err, ParcelError::NotEnoughData { .. }));
    assert_eq!(off, 4);

    // Negative counts other than -1 (must return BadParcelable)
    let negative_lengths = [-2i32, -5, -100, -9999, i32::MIN, i32::MIN + 1];
    for neg in negative_lengths {
        let mut neg_buf = Vec::new();
        neg_buf.extend_from_slice(&neg.to_le_bytes());
        let p_neg = Parcel::from_slice(&neg_buf);

        let mut off1 = 0;
        assert_eq!(
            p_neg.read_utf8(&mut off1).unwrap_err(),
            ParcelError::BadParcelable(neg)
        );
        assert_eq!(off1, 4);

        let mut off2 = 0;
        assert_eq!(
            p_neg.read_utf16(&mut off2).unwrap_err(),
            ParcelError::BadParcelable(neg)
        );
        assert_eq!(off2, 4);

        let mut off3 = 0;
        assert_eq!(
            p_neg.read_byte_vec(&mut off3).unwrap_err(),
            ParcelError::BadParcelable(neg)
        );
        assert_eq!(off3, 4);

        let mut off4 = 0;
        assert_eq!(
            p_neg
                .read_vector(&mut off4, |p, off| p.read_i32(off))
                .unwrap_err(),
            ParcelError::BadParcelable(neg)
        );
        assert_eq!(off4, 4);
    }
}

#[test]
fn test_large_valid_payload_roundtrip() {
    // 512 KB byte buffer stress test
    let large_size = 512 * 1024;
    let mut rng = SimplePrng::new(0x123456);
    let mut large_data = vec![0u8; large_size];
    rng.fill_bytes(&mut large_data);

    let mut p = Parcel::new();
    p.write_byte_slice(Some(&large_data)).unwrap();
    assert_eq!(p.data_size(), 4 + large_size);

    let mut off = 0;
    let decoded = p.read_byte_vec(&mut off).unwrap();
    assert_eq!(decoded, Some(large_data));
    assert_eq!(off, p.data_size());
}

// =============================================================================
// 2. FUZZING WITH RANDOM BIT PATTERNS AND MALFORMED ENCODINGS
// =============================================================================

#[test]
fn test_fuzzed_random_bytes_resilience() {
    let mut rng = SimplePrng::new(0xabcdef0123456789);

    // Run 5000 iterations of random fuzzed byte buffers of varied lengths (0 to 512 bytes)
    for _ in 0..5000 {
        let len = rng.next_range(0, 512);
        let mut buf = vec![0u8; len];
        rng.fill_bytes(&mut buf);

        let p = Parcel::from_slice(&buf);

        // Every reader method must return Ok or Err, but NEVER panic or trigger memory faults
        let mut off = 0;
        let _ = p.read_bool(&mut off);

        let mut off = 0;
        let _ = p.read_i8(&mut off);

        let mut off = 0;
        let _ = p.read_u8(&mut off);

        let mut off = 0;
        let _ = p.read_i16(&mut off);

        let mut off = 0;
        let _ = p.read_u16(&mut off);

        let mut off = 0;
        let _ = p.read_i32(&mut off);

        let mut off = 0;
        let _ = p.read_u32(&mut off);

        let mut off = 0;
        let _ = p.read_i64(&mut off);

        let mut off = 0;
        let _ = p.read_u64(&mut off);

        let mut off = 0;
        let _ = p.read_f32(&mut off);

        let mut off = 0;
        let _ = p.read_f64(&mut off);

        let mut off = 0;
        let _ = p.read_char(&mut off);

        let mut off = 0;
        let _ = p.read_utf8(&mut off);

        let mut off = 0;
        let _ = p.read_utf16(&mut off);

        let mut off = 0;
        let _ = p.read_byte_vec(&mut off);

        let mut off = 0;
        let _ = p.read_binder_object(&mut off);

        let mut off = 0;
        let _ = p.read_file_descriptor(&mut off);

        let mut off = 0;
        let _ = p.read_status(&mut off);

        let _ = BinderTransactionData::from_bytes(&buf);
    }
}

// =============================================================================
// 3. UNICODE, SURROGATE PAIRS, MALFORMED TERMINATORS
// =============================================================================

#[test]
fn test_utf8_missing_null_terminator_and_invalid_utf8() {
    // 4-byte length = 4, string = "ABCD" (no 0x00 null terminator, ended with 'E')
    let invalid_term = [
        0x04, 0x00, 0x00, 0x00, // length = 4
        b'A', b'B', b'C', b'D', // 4 bytes
        b'E', 0x00, 0x00, 0x00, // missing null terminator at index 4 (has 'E')
    ];
    let p = Parcel::from_slice(&invalid_term);
    let mut off = 0;
    let err = p.read_utf8(&mut off).unwrap_err();
    assert_eq!(err, ParcelError::MissingNullTerminator(8));

    // Valid null terminator, but invalid UTF-8 bytes (0xFF, 0xC0)
    let invalid_utf8_bytes = [
        0x02, 0x00, 0x00, 0x00, // length = 2
        0xff, 0xc0, 0x00, 0x00, // 0xff, 0xc0 (invalid utf8), null terminator, pad
    ];
    let p_utf8 = Parcel::from_slice(&invalid_utf8_bytes);
    let mut off = 0;
    let err = p_utf8.read_utf8(&mut off).unwrap_err();
    assert_eq!(err, ParcelError::MalformedUtf8(4));
}

#[test]
fn test_utf16_surrogate_pairs_and_malformed_surrogates() {
    // 1. High Surrogate without Low Surrogate (lone 0xD83D)
    let lone_high = [
        0x01, 0x00, 0x00, 0x00, // length = 1 code unit
        0x3d, 0xd8, // 0xD83D (lone high surrogate)
        0x00, 0x00, // null terminator 0x0000
    ];
    let p_high = Parcel::from_slice(&lone_high);
    let mut off = 0;
    let err = p_high.read_utf16(&mut off).unwrap_err();
    assert_eq!(err, ParcelError::MalformedUtf16(4));

    // 2. Low Surrogate without High Surrogate (lone 0xDE80)
    let lone_low = [
        0x01, 0x00, 0x00, 0x00, // length = 1 code unit
        0x80, 0xde, // 0xDE80 (lone low surrogate)
        0x00, 0x00, // null terminator
    ];
    let p_low = Parcel::from_slice(&lone_low);
    let mut off = 0;
    let err = p_low.read_utf16(&mut off).unwrap_err();
    assert_eq!(err, ParcelError::MalformedUtf16(4));

    // 3. Inverted Surrogate Pair (0xDE80 followed by 0xD83D)
    let inverted_pair = [
        0x02, 0x00, 0x00, 0x00, // length = 2 code units
        0x80, 0xde, 0x3d, 0xd8, // low then high
        0x00, 0x00, 0x00, 0x00, // null terminator + pad
    ];
    let p_inv = Parcel::from_slice(&inverted_pair);
    let mut off = 0;
    let err = p_inv.read_utf16(&mut off).unwrap_err();
    assert_eq!(err, ParcelError::MalformedUtf16(4));

    // 4. Correct surrogate pair: 🚀 (U+1F680 -> UTF-16: 0xD83D 0xDE80)
    let valid_surrogate = [
        0x02, 0x00, 0x00, 0x00, // length = 2 code units
        0x3d, 0xd8, // 0xD83D
        0x80, 0xde, // 0xDE80
        0x00, 0x00, 0x00, 0x00, // null terminator + 2 pad bytes
    ];
    let p_valid = Parcel::from_slice(&valid_surrogate);
    let mut off = 0;
    let s = p_valid.read_utf16(&mut off).unwrap().unwrap();
    assert_eq!(s, "🚀");
    assert_eq!(off, 12);

    // 5. UTF-16 Missing Null Terminator (has non-zero 0x0001 where 0x0000 expected)
    let missing_null_utf16 = [
        0x01, 0x00, 0x00, 0x00, // length = 1
        0x41, 0x00, // 'A'
        0x01, 0x00, // 0x0001 instead of 0x0000 null terminator
    ];
    let p_term = Parcel::from_slice(&missing_null_utf16);
    let mut off = 0;
    let err = p_term.read_utf16(&mut off).unwrap_err();
    assert_eq!(err, ParcelError::MissingNullTerminator(6));
}

#[test]
fn test_char_surrogate_and_out_of_range_handling() {
    // Char encoded as u32: valid scalar
    let mut p = Parcel::new();
    p.write_char('🦀').unwrap();
    let mut off = 0;
    assert_eq!(p.read_char(&mut off).unwrap(), '🦀');

    // Invalid char: surrogate code point 0xD800 (not a valid unicode scalar)
    let mut p_bad = Parcel::new();
    p_bad.write_u32(0xD800).unwrap();
    let mut off = 0;
    let err = p_bad.read_char(&mut off).unwrap_err();
    assert_eq!(err, ParcelError::MalformedUtf16(4));

    // Invalid char: out of range (> 0x10FFFF)
    let mut p_oor = Parcel::new();
    p_oor.write_u32(0x110000).unwrap();
    let mut off = 0;
    let err = p_oor.read_char(&mut off).unwrap_err();
    assert_eq!(err, ParcelError::MalformedUtf16(4));
}

// =============================================================================
// 4. RANDOM PERMUTATIONS OF MIXED PRIMITIVES (PROPERTY ROUND-TRIP)
// =============================================================================

#[derive(Debug, Clone, PartialEq)]
enum TestToken {
    Bool(bool),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(u32), // compare as bits to avoid NaN != NaN issues
    F64(u64),
    Char(char),
    Utf8(Option<String>),
    Utf16(Option<String>),
    ByteVec(Option<Vec<u8>>),
    BinderHandle(u32, u64),
    FileDescriptor(i32, u64),
}

#[test]
fn test_randomized_mixed_stream_property_roundtrip() {
    let mut rng = SimplePrng::new(0xfeedface98765432);

    for iter in 0..100 {
        let token_count = rng.next_range(20, 100);
        let mut tokens = Vec::with_capacity(token_count);

        for _ in 0..token_count {
            let choice = rng.next_range(0, 17);
            let token = match choice {
                0 => TestToken::Bool(rng.next_bool()),
                1 => TestToken::I8(rng.next_u8() as i8),
                2 => TestToken::U8(rng.next_u8()),
                3 => TestToken::I16(rng.next_u32() as i16),
                4 => TestToken::U16(rng.next_u32() as u16),
                5 => TestToken::I32(rng.next_u32() as i32),
                6 => TestToken::U32(rng.next_u32()),
                7 => TestToken::I64(rng.next_u64() as i64),
                8 => TestToken::U64(rng.next_u64()),
                9 => TestToken::F32(rng.next_u32()),
                10 => TestToken::F64(rng.next_u64()),
                11 => {
                    let chars = ['A', 'z', '9', '🚀', '🦀', '日', '本', '語', '—'];
                    let c = chars[rng.next_range(0, chars.len())];
                    TestToken::Char(c)
                }
                12 => {
                    if rng.next_range(0, 5) == 0 {
                        TestToken::Utf8(None)
                    } else {
                        let len = rng.next_range(0, 32);
                        let s: String = (0..len)
                            .map(|_| {
                                let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789 _-!#";
                                alphabet[rng.next_range(0, alphabet.len())] as char
                            })
                            .collect();
                        TestToken::Utf8(Some(s))
                    }
                }
                13 => {
                    if rng.next_range(0, 5) == 0 {
                        TestToken::Utf16(None)
                    } else {
                        let phrases = [
                            "",
                            "a",
                            "ab",
                            "abc",
                            "SurfaceFlinger",
                            "GraphicBufferProducer",
                            "AndroidWebGPU 🚀 2026",
                        ];
                        let s = phrases[rng.next_range(0, phrases.len())].to_string();
                        TestToken::Utf16(Some(s))
                    }
                }
                14 => {
                    if rng.next_range(0, 5) == 0 {
                        TestToken::ByteVec(None)
                    } else {
                        let len = rng.next_range(0, 64);
                        let mut b = vec![0u8; len];
                        rng.fill_bytes(&mut b);
                        TestToken::ByteVec(Some(b))
                    }
                }
                15 => {
                    let handle = rng.next_u32();
                    let cookie = rng.next_u64();
                    TestToken::BinderHandle(handle, cookie)
                }
                _ => {
                    let fd = rng.next_range(0, 1024) as i32;
                    let cookie = rng.next_u64();
                    TestToken::FileDescriptor(fd, cookie)
                }
            };
            tokens.push(token);
        }

        // Encode all tokens into parcel
        let mut parcel = Parcel::new();
        for token in &tokens {
            match token {
                TestToken::Bool(v) => parcel.write_bool(*v).unwrap(),
                TestToken::I8(v) => parcel.write_i8(*v).unwrap(),
                TestToken::U8(v) => parcel.write_u8(*v).unwrap(),
                TestToken::I16(v) => parcel.write_i16(*v).unwrap(),
                TestToken::U16(v) => parcel.write_u16(*v).unwrap(),
                TestToken::I32(v) => parcel.write_i32(*v).unwrap(),
                TestToken::U32(v) => parcel.write_u32(*v).unwrap(),
                TestToken::I64(v) => parcel.write_i64(*v).unwrap(),
                TestToken::U64(v) => parcel.write_u64(*v).unwrap(),
                TestToken::F32(v) => parcel.write_f32(f32::from_bits(*v)).unwrap(),
                TestToken::F64(v) => parcel.write_f64(f64::from_bits(*v)).unwrap(),
                TestToken::Char(v) => parcel.write_char(*v).unwrap(),
                TestToken::Utf8(v) => parcel.write_utf8(v.as_deref()).unwrap(),
                TestToken::Utf16(v) => parcel.write_utf16(v.as_deref()).unwrap(),
                TestToken::ByteVec(v) => parcel.write_byte_slice(v.as_deref()).unwrap(),
                TestToken::BinderHandle(h, c) => parcel.write_binder(*h, *c).unwrap(),
                TestToken::FileDescriptor(fd, c) => parcel.write_file_descriptor(*fd, *c).unwrap(),
            }
        }

        assert_eq!(
            parcel.data_size() % 4,
            0,
            "Iter {iter}: Parcel data must always be 4-byte aligned"
        );

        // Decode and verify all tokens
        let mut offset = 0;
        for (idx, token) in tokens.iter().enumerate() {
            match token {
                TestToken::Bool(expected) => {
                    let val = parcel.read_bool(&mut offset).unwrap();
                    assert_eq!(&val, expected, "Iter {iter}, item {idx}");
                }
                TestToken::I8(expected) => {
                    let val = parcel.read_i8(&mut offset).unwrap();
                    assert_eq!(&val, expected, "Iter {iter}, item {idx}");
                }
                TestToken::U8(expected) => {
                    let val = parcel.read_u8(&mut offset).unwrap();
                    assert_eq!(&val, expected, "Iter {iter}, item {idx}");
                }
                TestToken::I16(expected) => {
                    let val = parcel.read_i16(&mut offset).unwrap();
                    assert_eq!(&val, expected, "Iter {iter}, item {idx}");
                }
                TestToken::U16(expected) => {
                    let val = parcel.read_u16(&mut offset).unwrap();
                    assert_eq!(&val, expected, "Iter {iter}, item {idx}");
                }
                TestToken::I32(expected) => {
                    let val = parcel.read_i32(&mut offset).unwrap();
                    assert_eq!(&val, expected, "Iter {iter}, item {idx}");
                }
                TestToken::U32(expected) => {
                    let val = parcel.read_u32(&mut offset).unwrap();
                    assert_eq!(&val, expected, "Iter {iter}, item {idx}");
                }
                TestToken::I64(expected) => {
                    let val = parcel.read_i64(&mut offset).unwrap();
                    assert_eq!(&val, expected, "Iter {iter}, item {idx}");
                }
                TestToken::U64(expected) => {
                    let val = parcel.read_u64(&mut offset).unwrap();
                    assert_eq!(&val, expected, "Iter {iter}, item {idx}");
                }
                TestToken::F32(expected_bits) => {
                    let val = parcel.read_f32(&mut offset).unwrap();
                    assert_eq!(val.to_bits(), *expected_bits, "Iter {iter}, item {idx}");
                }
                TestToken::F64(expected_bits) => {
                    let val = parcel.read_f64(&mut offset).unwrap();
                    assert_eq!(val.to_bits(), *expected_bits, "Iter {iter}, item {idx}");
                }
                TestToken::Char(expected) => {
                    let val = parcel.read_char(&mut offset).unwrap();
                    assert_eq!(&val, expected, "Iter {iter}, item {idx}");
                }
                TestToken::Utf8(expected) => {
                    let val = parcel.read_utf8(&mut offset).unwrap();
                    assert_eq!(&val, expected, "Iter {iter}, item {idx}");
                }
                TestToken::Utf16(expected) => {
                    let val = parcel.read_utf16(&mut offset).unwrap();
                    assert_eq!(&val, expected, "Iter {iter}, item {idx}");
                }
                TestToken::ByteVec(expected) => {
                    let val = parcel.read_byte_vec(&mut offset).unwrap();
                    assert_eq!(&val, expected, "Iter {iter}, item {idx}");
                }
                TestToken::BinderHandle(h, c) => {
                    let val = parcel.read_binder(&mut offset).unwrap();
                    assert_eq!(val.handle(), *h);
                    assert_eq!(val.cookie, *c);
                }
                TestToken::FileDescriptor(fd, c) => {
                    let obj = parcel.read_binder_object(&mut offset).unwrap();
                    assert_eq!(obj.fd(), *fd);
                    assert_eq!(obj.cookie, *c);
                }
            }
        }
        assert_eq!(offset, parcel.data_size());
    }
}

// =============================================================================
// 5. INTERLEAVED FLAT_BINDER_OBJECTS AND OFFSETS TABLE DEFENSE
// =============================================================================

#[test]
fn test_flat_binder_object_offsets_table_enforcement() {
    let mut p = Parcel::new();
    p.write_i32(42).unwrap(); // 4 bytes at offset 0
    p.write_binder(10, 0x100).unwrap(); // 24 bytes at offset 4..28
    p.write_i64(999).unwrap(); // 8 bytes at offset 28..36

    assert_eq!(p.offsets(), &[4]);

    // Reading binder object at unregistered offset 0 must fail with ObjectOffsetNotFound
    let mut bad_off = 0;
    let err = p.read_binder_object(&mut bad_off).unwrap_err();
    assert_eq!(err, ParcelError::ObjectOffsetNotFound(0));

    // Reading binder object at correct offset 4 must succeed
    let mut good_off = 4;
    let obj = p.read_binder_object(&mut good_off).unwrap();
    assert_eq!(obj.handle(), 10);
    assert_eq!(good_off, 28);

    // Reading binder object at unregistered offset 28 must fail
    let mut bad_off2 = 28;
    let err = p.read_binder_object(&mut bad_off2).unwrap_err();
    assert!(matches!(
        err,
        ParcelError::ObjectOffsetNotFound(28) | ParcelError::NotEnoughData { .. }
    ));
}

#[test]
fn test_file_descriptor_type_safety() {
    let mut p = Parcel::new();
    // Write a binder handle object
    p.write_binder(99, 0xabc).unwrap();

    let mut off = 0;
    // Attempting to read it as a file descriptor must fail with InvalidObjectType
    let err = p.read_file_descriptor(&mut off).unwrap_err();
    assert_eq!(err, ParcelError::InvalidObjectType(BINDER_TYPE_HANDLE));
}

// =============================================================================
// 6. ENVELOPE ABI, SIZE, AND ALIGNMENT ASSERTS
// =============================================================================

#[test]
fn test_wire_struct_exact_abi_sizes_and_alignments() {
    // Android kernel 64-bit ABI guarantees
    assert_eq!(
        std::mem::size_of::<BinderTransactionData>(),
        64,
        "BinderTransactionData must be exactly 64 bytes"
    );
    assert_eq!(
        std::mem::align_of::<BinderTransactionData>(),
        8,
        "BinderTransactionData must be 8-byte aligned"
    );

    assert_eq!(
        std::mem::size_of::<FlatBinderObject>(),
        24,
        "FlatBinderObject must be exactly 24 bytes"
    );
    assert_eq!(
        std::mem::align_of::<FlatBinderObject>(),
        8,
        "FlatBinderObject must be 8-byte aligned"
    );

    assert_eq!(
        std::mem::size_of::<BinderPtrCookie>(),
        16,
        "BinderPtrCookie must be exactly 16 bytes"
    );
    assert_eq!(
        std::mem::align_of::<BinderPtrCookie>(),
        8,
        "BinderPtrCookie must be 8-byte aligned"
    );

    assert_eq!(
        std::mem::size_of::<BinderHandleCookie>(),
        16,
        "BinderHandleCookie must be exactly 16 bytes"
    );
    assert_eq!(
        std::mem::align_of::<BinderHandleCookie>(),
        8,
        "BinderHandleCookie must be 8-byte aligned"
    );
}

// =============================================================================
// 7. NULLABLE VS EMPTY CONTAINER DISTINCTION TESTS
// =============================================================================

#[test]
fn test_nullable_vs_empty_container_semantics() {
    let mut p = Parcel::new();

    // UTF-8 None vs Some("")
    p.write_utf8(None).unwrap();
    p.write_utf8(Some("")).unwrap();

    // UTF-16 None vs Some("")
    p.write_utf16(None).unwrap();
    p.write_utf16(Some("")).unwrap();

    // Byte slice None vs Some(&[])
    p.write_byte_slice(None).unwrap();
    p.write_byte_slice(Some(&[])).unwrap();

    // Generic vector None vs Some(&[])
    p.write_vector::<i32, _>(None, |p, v| p.write_i32(*v)).unwrap();
    p.write_vector::<i32, _>(Some(&[]), |p, v| p.write_i32(*v)).unwrap();

    // Nullable presence None vs Some(0)
    p.write_nullable_presence::<i32, _>(None, |p, v| p.write_i32(*v)).unwrap();
    p.write_nullable_presence(Some(&0i32), |p, v| p.write_i32(*v)).unwrap();

    let mut off = 0;

    // Read back and assert clear distinction between None and Some(empty)
    assert_eq!(p.read_utf8(&mut off).unwrap(), None);
    assert_eq!(p.read_utf8(&mut off).unwrap(), Some("".to_string()));

    assert_eq!(p.read_utf16(&mut off).unwrap(), None);
    assert_eq!(p.read_utf16(&mut off).unwrap(), Some("".to_string()));

    assert_eq!(p.read_byte_vec(&mut off).unwrap(), None);
    assert_eq!(p.read_byte_vec(&mut off).unwrap(), Some(vec![]));

    assert_eq!(
        p.read_vector::<i32, _>(&mut off, |p, o| p.read_i32(o)).unwrap(),
        None
    );
    assert_eq!(
        p.read_vector::<i32, _>(&mut off, |p, o| p.read_i32(o)).unwrap(),
        Some(vec![])
    );

    assert_eq!(
        p.read_nullable_presence(&mut off, |p, o| p.read_i32(o)).unwrap(),
        None
    );
    assert_eq!(
        p.read_nullable_presence(&mut off, |p, o| p.read_i32(o)).unwrap(),
        Some(0)
    );

    assert_eq!(off, p.data_size());
}

#[test]
fn test_status_display_and_error_trait() {
    let ok = Status::ok();
    assert_eq!(format!("{}", ok), "Status::Ok");

    let svc = Status::new_service_specific_error(-42, Some("Resource busy"));
    assert!(format!("{}", svc).contains("Status::ServiceSpecific(code=-42, msg=Some(\"Resource busy\"))"));

    let exc = Status::new_exception(ExceptionCode::Security, Some("Permission denied"));
    assert!(format!("{}", exc).contains("Status::Exception(Security, msg=Some(\"Permission denied\"))"));

    let err = Status::from_status(STATUS_NO_MEMORY);
    assert_eq!(format!("{}", err), "Status::Exception(TransactionFailed, msg=None)");
}
