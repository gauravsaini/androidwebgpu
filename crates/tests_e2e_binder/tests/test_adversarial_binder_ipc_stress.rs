//! Comprehensive Adversarial & Empirical Stress Test Suite for Binder IPC, Zygote, and System Services.
//!
//! ASD-STE100, /ponytail, /caveman

use aidl_compat::traits::IBinder;
use aidl_compat::Remotable;
use binder_handle_bridge::HandleBridge;
use binder_rt::parcel::{Parcel, ParcelError};
use binder_rt::status::StatusCode;
use binder_rt::types::FlatBinderObject;
use pms_rs::PackageManagerService;
use std::sync::Arc;
use std::thread;
use tests_e2e_binder::harness::EchoService;
use wms_rs::WindowManagerService;
use zygote_client::error::ZygoteError;
use zygote_client::protocol::{format_pid_response, parse_pid_response, ZygoteSpawnArgs};

// =============================================================================
// 1. Malformed / Corrupted Binder Parcels & Out-of-Bounds Offsets
// =============================================================================

#[test]
fn test_adversarial_parcel_fuzzing_and_bounds() {
    // 1.1 Out of bounds reads on empty parcel
    let empty_parcel = Parcel::new();
    let mut off = 0;
    assert!(matches!(
        empty_parcel.read_i32(&mut off),
        Err(ParcelError::NotEnoughData { .. })
    ));
    assert_eq!(off, 0);

    let mut off = 0;
    assert!(matches!(
        empty_parcel.read_i64(&mut off),
        Err(ParcelError::NotEnoughData { .. })
    ));
    assert_eq!(off, 0);

    let mut off = 0;
    assert!(matches!(
        empty_parcel.read_utf8(&mut off),
        Err(ParcelError::NotEnoughData { .. })
    ));
    assert_eq!(off, 0);

    // 1.2 Negative string length headers
    let bad_lengths = [-2i32, -10, -9999, i32::MIN, i32::MIN + 1];
    for neg_len in bad_lengths {
        let mut p = Parcel::new();
        p.write_i32(neg_len).unwrap();
        let mut off = 0;
        assert_eq!(p.read_utf8(&mut off).unwrap_err(), ParcelError::BadParcelable(neg_len));
        let mut off = 0;
        assert_eq!(p.read_utf16(&mut off).unwrap_err(), ParcelError::BadParcelable(neg_len));
        let mut off = 0;
        assert_eq!(p.read_byte_vec(&mut off).unwrap_err(), ParcelError::BadParcelable(neg_len));
    }

    // 1.3 String payload claim exceeding available buffer
    let mut oob_parcel = Parcel::new();
    oob_parcel.write_i32(10000).unwrap(); // claims 10,000 bytes
    oob_parcel.data_mut().extend_from_slice(b"small"); // only provides 5 bytes
    let mut off = 0;
    assert!(matches!(
        oob_parcel.read_utf8(&mut off),
        Err(ParcelError::NotEnoughData { .. })
    ));

    // 1.4 Missing null terminator in UTF-8 string
    let mut non_null_parcel = Parcel::new();
    non_null_parcel.write_i32(4).unwrap(); // len = 4
    non_null_parcel.data_mut().extend_from_slice(b"abcd"); // string payload
    non_null_parcel.data_mut().extend_from_slice(&[0x58, 0x00, 0x00, 0x00]); // 'X' instead of 0x00 null terminator
    let mut off = 0;
    assert!(matches!(
        non_null_parcel.read_utf8(&mut off),
        Err(ParcelError::MissingNullTerminator(8))
    ));

    // 1.5 Binder object offset verification & invalid offset table
    let mut binder_parcel = Parcel::new();
    let obj = FlatBinderObject::new_handle(10, 0, 0x1234);
    binder_parcel.write_binder_object(&obj).unwrap();
    assert_eq!(binder_parcel.offsets().len(), 1);
    assert_eq!(binder_parcel.offsets()[0], 0);

    // Corrupt offset table to point elsewhere
    binder_parcel.offsets_mut()[0] = 8;
    let mut off = 0;
    assert!(matches!(
        binder_parcel.read_binder_object(&mut off),
        Err(ParcelError::ObjectOffsetNotFound(0))
    ));
}

// =============================================================================
// 2. Unexpected Transaction Codes and Stub Resilience
// =============================================================================

#[test]
fn test_adversarial_unexpected_transaction_codes() {
    let service = EchoService::new();
    let unexpected_codes = [0u32, 0xFFFFFFFF, 0x80000000, 999999, 12345];

    for code in unexpected_codes {
        let in_parcel = Parcel::new();
        let mut out_parcel = Parcel::new();
        let res = service.on_transact(code, &in_parcel, &mut out_parcel);
        assert!(res.is_err(), "Unexpected transaction code {code:#x} must return error");
        let status = res.unwrap_err();
        assert_eq!(
            status.status,
            StatusCode::UnknownTransaction,
            "Must return UnknownTransaction for code {code:#x}"
        );
    }
}

#[test]
fn test_wms_and_pms_transact_resilience() {
    let wms = Arc::new(WindowManagerService::new());
    let unexpected_codes = [0u32, 0xFFFFFFFF, 0x7FFFFFFF, 88888];

    for code in unexpected_codes {
        let in_parcel = Parcel::new();
        let mut out_parcel = Parcel::new();
        let res = wms.on_transact(code, &in_parcel, &mut out_parcel);
        assert!(res.is_ok());
        let mut offset = 0;
        let status = out_parcel.read_status(&mut offset).expect("Reply must contain valid status");
        assert!(!status.is_ok(), "WMS reply must indicate failure for unexpected transaction code {code:#x}");
    }

    let pms = Arc::new(PackageManagerService::new());
    for code in unexpected_codes {
        let in_parcel = Parcel::new();
        let mut out_parcel = Parcel::new();
        let res = pms.on_transact(code, &in_parcel, &mut out_parcel);
        assert!(res.is_err(), "PMS must return Err for unexpected transaction code {code:#x}");
        let status = res.unwrap_err();
        assert_eq!(
            status.status,
            StatusCode::UnknownTransaction,
            "PMS error must be StatusCode::UnknownTransaction for code {code:#x}"
        );
    }
}

// =============================================================================
// 3. Thread Safety & Concurrency Stress Simulation
// =============================================================================

#[test]
fn test_concurrent_binder_handle_bridge_stress() {
    let bridge = Arc::new(HandleBridge::new());
    let iterations_per_thread = 200;
    let num_threads = 8;

    let mut handles = Vec::new();
    for thread_id in 0..num_threads {
        let t_bridge = bridge.clone();

        handles.push(thread::spawn(move || {
            let client_id = (thread_id + 1) as u32;
            for _ in 0..iterations_per_thread {
                let echo = Arc::new(EchoService::new());
                
                // Register service
                let handle = t_bridge.register_service(
                    client_id,
                    EchoService::DESCRIPTOR,
                    echo as Arc<dyn IBinder>,
                );
                assert!(handle > 0);

                // Retrieve service
                let resolved = t_bridge.get_service(client_id, handle);
                assert!(resolved.is_some());

                // Acquire additional ref
                t_bridge.acquire_ref(client_id, handle, 1).expect("Acquire ref must succeed");

                // Release refs
                t_bridge.release_ref(client_id, handle, 1).expect("Release ref 1 must succeed");
                t_bridge.release_ref(client_id, handle, 1).expect("Release ref 2 must succeed");
            }
        }));
    }

    for h in handles {
        h.join().expect("Thread must finish cleanly without panic or deadlock");
    }
}

// =============================================================================
// 4. Zygote Wire Framing & Edge Cases Simulation
// =============================================================================

#[test]
fn test_zygote_wire_framing_adversarial_suite() {
    // 4.1 Empty payload
    assert!(ZygoteSpawnArgs::parse_wire_bytes(b"").is_err());

    // 4.2 Argument count header parsing
    let bad_headers = [
        "not_a_number\n--setuid=1000\n",
        "-10\n--setuid=1000\n",
        "0\n--setuid=1000\n", // count 0 but provided 1 line
        "5\n--setuid=1000\n--setgid=1000\n", // count 5 but provided 2 lines
    ];
    for bad in bad_headers {
        assert!(
            ZygoteSpawnArgs::parse_wire_bytes(bad.as_bytes()).is_err(),
            "Bad wire header '{bad}' must fail parsing"
        );
    }

    // 4.3 Valid wire encoding with supplementary groups & app data dir
    let args = ZygoteSpawnArgs::new("com.example.secure", "secure_process")
        .with_uid(10099)
        .with_gid(10099)
        .with_gids(vec![1015, 1028, 3003])
        .with_target_sdk_version(33)
        .with_entry_point("com.example.secure.MainEntry")
        .with_se_info("u:r:untrusted_app:s0")
        .with_app_data_dir("/data/user/0/com.example.secure");

    let wire_bytes = args.encode_wire_bytes();
    let parsed = ZygoteSpawnArgs::parse_wire_bytes(&wire_bytes).expect("Round trip parse must succeed");
    assert_eq!(parsed.uid, 10099);
    assert_eq!(parsed.gid, 10099);
    assert_eq!(parsed.gids, vec![1015, 1028, 3003]);
    assert_eq!(parsed.target_sdk_version, 33);
    assert_eq!(parsed.package_name, "com.example.secure");
    assert_eq!(parsed.nice_name, "secure_process");
    assert_eq!(parsed.entry_point, "com.example.secure.MainEntry");
    assert_eq!(parsed.se_info, Some("u:r:untrusted_app:s0".to_string()));
    assert_eq!(parsed.app_data_dir, Some("/data/user/0/com.example.secure".to_string()));
}

#[test]
fn test_zygote_pid_response_adversarial_bounds() {
    // 4.4 Truncated buffers
    for len in [0, 1, 2, 3, 5, 10] {
        let buf = vec![0u8; len];
        assert!(parse_pid_response(&buf).is_err(), "Buffer len {len} must be rejected");
    }

    // 4.5 Negative error codes
    for neg_pid in [-1, -42, -9999, i32::MIN] {
        let buf = format_pid_response(neg_pid);
        let res = parse_pid_response(&buf);
        match res {
            Err(ZygoteError::ForkFailed { pid, .. }) => assert_eq!(pid, neg_pid),
            other => panic!("Expected ForkFailed for {neg_pid}, got {:?}", other),
        }
    }

    // 4.6 Valid PID response
    let valid_buf = format_pid_response(54321);
    assert_eq!(parse_pid_response(&valid_buf).unwrap(), 54321);
}
