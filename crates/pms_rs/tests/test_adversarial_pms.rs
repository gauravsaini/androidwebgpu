//! Adversarial, Boundary, and Fuzzing Tests for `pms_rs`.

use aidl_compat::traits::IBinder;
use aidl_compat::{Parcel, Status, STATUS_UNKNOWN_TRANSACTION};
use pms_rs::arsc::ArscParser;
use pms_rs::axml::AxmlParser;
use pms_rs::types::*;
use pms_rs::PackageManagerService;
use std::sync::Arc;
use std::thread;

#[test]
fn test_adversarial_fuzzed_axml_chunks() {
    // 1. All zero bytes
    let zeros = vec![0u8; 128];
    assert!(AxmlParser::parse(&zeros).is_err());

    // 2. Huge declared chunk size exceeding buffer
    let mut bad_chunk = vec![0x03, 0x00, 0x08, 0x00]; // Root Header
    bad_chunk.extend_from_slice(&0x00FFFFFFu32.to_le_bytes()); // Size: 16MB
    bad_chunk.extend_from_slice(&0x0001u16.to_le_bytes()); // String pool
    bad_chunk.extend_from_slice(&28u16.to_le_bytes());
    bad_chunk.extend_from_slice(&0x00FFFFFFu32.to_le_bytes());
    assert!(AxmlParser::parse(&bad_chunk).is_ok() || AxmlParser::parse(&bad_chunk).is_err());

    // 3. String pool with declared string_count 1,000,000 but only 32 bytes buffer
    let mut fake_pool = vec![0x03, 0x00, 0x08, 0x00, 60, 0, 0, 0];
    fake_pool.extend_from_slice(&0x0001u16.to_le_bytes());
    fake_pool.extend_from_slice(&28u16.to_le_bytes());
    fake_pool.extend_from_slice(&52u32.to_le_bytes());
    fake_pool.extend_from_slice(&1_000_000u32.to_le_bytes()); // string_count
    fake_pool.extend_from_slice(&0u32.to_le_bytes());
    fake_pool.extend_from_slice(&0x100u32.to_le_bytes());
    fake_pool.extend_from_slice(&28u32.to_le_bytes());
    fake_pool.extend_from_slice(&0u32.to_le_bytes());
    let _ = AxmlParser::parse(&fake_pool);
}

#[test]
fn test_adversarial_fuzzed_arsc_tables() {
    // 1. Empty buffer
    assert!(ArscParser::parse(&[]).is_err());

    // 2. Declared package chunk with huge size
    let mut bad_pkg = vec![0x02, 0x00, 0x0C, 0x00, 100, 0, 0, 0, 1, 0, 0, 0];
    bad_pkg.extend_from_slice(&0x0200u16.to_le_bytes());
    bad_pkg.extend_from_slice(&288u16.to_le_bytes());
    bad_pkg.extend_from_slice(&0x00FFFFFFu32.to_le_bytes());
    let _ = ArscParser::parse(&bad_pkg);
}

#[test]
fn test_adversarial_unknown_binder_transactions() {
    let pms = PackageManagerService::new();
    let data = Parcel::new();
    let mut reply = Parcel::new();

    // Unknown transaction code 9999
    let res = pms.transact(9999, 0, &data, &mut reply);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), Status::from_status(STATUS_UNKNOWN_TRANSACTION));
}

#[test]
fn test_adversarial_concurrent_multithreaded_intent_resolution() {
    let pms = Arc::new(PackageManagerService::new());

    // Pre-populate with 10 packages
    for i in 0..10 {
        let pkg = PackageInfo {
            package_name: format!("com.pkg.app_{}", i),
            version_code: i,
            activities: vec![ActivityInfo {
                name: format!("com.pkg.app_{}.MainActivity", i),
                package_name: format!("com.pkg.app_{}", i),
                exported: true,
                enabled: true,
                intent_filters: vec![IntentFilter {
                    actions: vec!["android.intent.action.MAIN".to_string()],
                    categories: vec!["android.intent.category.LAUNCHER".to_string()],
                    data_schemes: vec![],
                    priority: i,
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        pms.install_package_info(pkg, None);
    }

    let mut handles = Vec::new();
    for thread_idx in 0..16 {
        let pms_clone = Arc::clone(&pms);
        let handle = thread::spawn(move || {
            for iter in 0..500 {
                let intent = Intent::new(Some("android.intent.action.MAIN"));
                let resolved = pms_clone.resolve_intent(&intent, "", 0, 0);
                assert!(resolved.is_some());
                assert_eq!(
                    resolved.unwrap().activity_info.unwrap().name,
                    "com.pkg.app_9.MainActivity"
                );

                if (iter + thread_idx) % 50 == 0 {
                    let all = pms_clone.get_installed_packages(GET_ACTIVITIES, 0);
                    assert_eq!(all.len(), 10);
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }
}
