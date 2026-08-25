//! Test Zygote argument formatting, wire bytes encoding, and PID response parsing.

use zygote_client::error::ZygoteError;
use zygote_client::protocol::{format_pid_response, parse_pid_response, ZygoteSpawnArgs};

#[test]
fn test_zygote_argument_lines_formatting() {
    let args = ZygoteSpawnArgs::new("com.androidwebgpu.arcade", "com.androidwebgpu.arcade")
        .with_uid(10042)
        .with_gid(10042)
        .with_target_sdk_version(33)
        .with_gids(vec![1015, 1028])
        .with_se_info("default:targetSdkVersion=33")
        .with_app_data_dir("/data/user/0/com.androidwebgpu.arcade");

    let lines = args.format_command_lines();

    assert!(lines.contains(&"--setuid=10042".to_string()));
    assert!(lines.contains(&"--setgid=10042".to_string()));
    assert!(lines.contains(&"--setgroups=1015,1028".to_string()));
    assert!(lines.contains(&"--target-sdk-version=33".to_string()));
    assert!(lines.contains(&"--package-name=com.androidwebgpu.arcade".to_string()));
    assert!(lines.contains(&"--nice-name=com.androidwebgpu.arcade".to_string()));
    assert!(lines.contains(&"--seinfo=default:targetSdkVersion=33".to_string()));
    assert!(lines.contains(&"--app-data-dir=/data/user/0/com.androidwebgpu.arcade".to_string()));
    assert_eq!(lines.last().unwrap(), "android.app.ActivityThread");
}

#[test]
fn test_zygote_wire_roundtrip() {
    let original = ZygoteSpawnArgs::new("com.example.testapp", "com.example.testapp:ui")
        .with_uid(10099)
        .with_gid(10099)
        .with_target_sdk_version(33)
        .with_gids(vec![3003])
        .with_se_info("untrusted_app:targetSdkVersion=33")
        .with_app_data_dir("/data/user/0/com.example.testapp");

    let encoded_bytes = original.encode_wire_bytes();
    let decoded = ZygoteSpawnArgs::parse_wire_bytes(&encoded_bytes).unwrap();

    assert_eq!(original.package_name, decoded.package_name);
    assert_eq!(original.nice_name, decoded.nice_name);
    assert_eq!(original.uid, decoded.uid);
    assert_eq!(original.gid, decoded.gid);
    assert_eq!(original.target_sdk_version, decoded.target_sdk_version);
    assert_eq!(original.entry_point, decoded.entry_point);
    assert_eq!(original.gids, decoded.gids);
    assert_eq!(original.se_info, decoded.se_info);
    assert_eq!(original.app_data_dir, decoded.app_data_dir);
}

#[test]
fn test_pid_response_parsing_success() {
    let pid_buf = format_pid_response(12345);
    let parsed_pid = parse_pid_response(&pid_buf).unwrap();
    assert_eq!(parsed_pid, 12345);

    let pid_buf_max = format_pid_response(999999);
    let parsed_pid_max = parse_pid_response(&pid_buf_max).unwrap();
    assert_eq!(parsed_pid_max, 999999);
}

#[test]
fn test_pid_response_parsing_failures() {
    // Negative error code from Zygote
    let err_buf = format_pid_response(-1);
    match parse_pid_response(&err_buf) {
        Err(ZygoteError::ForkFailed { pid, .. }) => assert_eq!(pid, -1),
        other => panic!("Expected ForkFailed, got {:?}", other),
    }

    // Zero PID (invalid)
    let zero_buf = format_pid_response(0);
    match parse_pid_response(&zero_buf) {
        Err(ZygoteError::ForkFailed { pid, .. }) => assert_eq!(pid, 0),
        other => panic!("Expected ForkFailed, got {:?}", other),
    }

    // Truncated buffer
    let short_buf = [1, 2, 3];
    match parse_pid_response(&short_buf) {
        Err(ZygoteError::InvalidPidResponse(3)) => (),
        other => panic!("Expected InvalidPidResponse, got {:?}", other),
    }
}
