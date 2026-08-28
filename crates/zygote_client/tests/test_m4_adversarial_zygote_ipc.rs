//! Adversarial stress tests for Zygote IPC, wire decoding, and process tracking.
//! ASD-STE100, /ponytail, /caveman

use std::sync::Arc;
use zygote_client::error::ZygoteError;
use zygote_client::process::{ProcessRecord, ProcessState};
use zygote_client::protocol::{format_pid_response, parse_pid_response, ZygoteSpawnArgs};
use zygote_client::socket::{MockZygoteHandler, ZygoteClient};

#[test]
fn test_adversarial_wire_parsing_malformed_payloads() {
    // 1. Empty payload
    let empty_res = ZygoteSpawnArgs::parse_wire_bytes(b"");
    assert!(empty_res.is_err(), "Empty payload must fail");

    // 2. Non-numeric count header
    let nan_res = ZygoteSpawnArgs::parse_wire_bytes(b"invalid\n--setuid=1000\n");
    assert!(nan_res.is_err(), "Non-numeric header must fail");

    // 3. Count mismatch (header says 5 lines, only 2 provided)
    let short_res = ZygoteSpawnArgs::parse_wire_bytes(b"5\n--setuid=1000\n--setgid=1000\n");
    assert!(short_res.is_err(), "Count mismatch must fail");

    // 4. Invalid UTF-8 bytes
    let invalid_utf8 = vec![0xFF, 0xFE, 0xFD];
    let utf8_res = ZygoteSpawnArgs::parse_wire_bytes(&invalid_utf8);
    assert!(utf8_res.is_err(), "Invalid UTF-8 must fail");
}

#[test]
fn test_adversarial_pid_response_extreme_boundaries() {
    // 1. Negative error code -100
    let err_resp = format_pid_response(-100);
    match parse_pid_response(&err_resp) {
        Err(ZygoteError::ForkFailed { pid, .. }) => assert_eq!(pid, -100),
        other => panic!("Expected ForkFailed, got {:?}", other),
    }

    // 2. Max positive i32 PID
    let max_resp = format_pid_response(i32::MAX);
    assert_eq!(parse_pid_response(&max_resp).unwrap(), i32::MAX as u32);

    // 3. Min negative i32 PID
    let min_resp = format_pid_response(i32::MIN);
    assert!(parse_pid_response(&min_resp).is_err());

    // 4. Zero byte buffer
    assert!(parse_pid_response(&[]).is_err());

    // 5. 5-byte buffer (too long)
    assert!(parse_pid_response(&[1, 2, 3, 4, 5]).is_err());
}

#[test]
fn test_adversarial_concurrent_fork_and_process_tracker_churn() {
    let handler = Arc::new(MockZygoteHandler::new(5000));
    let client = ZygoteClient::new_mock(handler.clone());

    // Spawn 50 processes in rapid sequence
    let mut spawned_pids = Vec::new();
    for i in 0..50 {
        let pkg = format!("com.app.worker_{i}");
        let nice = format!("worker_{i}");
        let pid = client
            .fork_app_simple(&pkg, &nice, 10000 + i as u32, 10000 + i as u32, 33)
            .expect("Fork must succeed");
        spawned_pids.push(pid);
    }

    assert_eq!(spawned_pids.len(), 50);
    assert_eq!(client.tracker().count(), 50);

    // Duplicate registration prevention
    let dup_args = ZygoteSpawnArgs::new("com.app.duplicate", "duplicate");
    let duplicate_record = ProcessRecord::from_spawn_args(spawned_pids[0], &dup_args);
    assert!(client.tracker().register_process(duplicate_record).is_err());

    // Terminate and clean up half
    for &pid in &spawned_pids[0..25] {
        assert!(client.tracker().kill_process(pid).is_ok());
        let rec = client.tracker().get_process(pid).unwrap();
        assert_eq!(rec.state, ProcessState::Killed);
        assert!(client.tracker().remove_process(pid).is_some());
    }

    assert_eq!(client.tracker().count(), 25);
}

#[test]
fn test_adversarial_builder_pattern_optional_fields() {
    let args = ZygoteSpawnArgs::new("org.fdroid.fdroid", "org.fdroid.fdroid")
        .with_uid(10042)
        .with_gid(10042)
        .with_target_sdk_version(33)
        .with_entry_point("org.fdroid.fdroid.views.main.MainActivity")
        .with_gids(vec![1000, 1015, 1028, 3003])
        .with_se_info("u:r:untrusted_app:s0:c42,c256")
        .with_app_data_dir("/data/user/0/org.fdroid.fdroid");

    let encoded = args.encode_wire_bytes();
    let decoded = ZygoteSpawnArgs::parse_wire_bytes(&encoded).expect("Must parse cleanly");

    assert_eq!(decoded.package_name, "org.fdroid.fdroid");
    assert_eq!(decoded.uid, 10042);
    assert_eq!(decoded.gids, vec![1000, 1015, 1028, 3003]);
    assert_eq!(decoded.se_info, Some("u:r:untrusted_app:s0:c42,c256".to_string()));
    assert_eq!(decoded.app_data_dir, Some("/data/user/0/org.fdroid.fdroid".to_string()));
}
