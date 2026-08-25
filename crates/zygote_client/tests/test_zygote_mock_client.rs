//! Test Zygote client with mock socket/handler.

use zygote_client::error::ZygoteError;
use zygote_client::socket::ZygoteClient;

#[test]
fn test_mock_zygote_client_spawning() {
    let (client, handler) = ZygoteClient::new_mock_default();

    let pid1 = client
        .fork_app_simple("com.example.game", "com.example.game", 10042, 10042, 33)
        .expect("Fork 1 failed");
    assert_eq!(pid1, 10001);

    let pid2 = client
        .fork_app_simple("com.example.util", "com.example.util:service", 10043, 10043, 33)
        .expect("Fork 2 failed");
    assert_eq!(pid2, 10002);

    let requests = handler.get_received_requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].package_name, "com.example.game");
    assert_eq!(requests[1].package_name, "com.example.util");

    // Check tracker
    assert_eq!(client.tracker().count(), 2);
    let proc1 = client.tracker().get_process(pid1).unwrap();
    assert_eq!(proc1.package_name, "com.example.game");
}

#[test]
fn test_mock_zygote_client_failure_injection() {
    let (client, handler) = ZygoteClient::new_mock_default();

    handler.set_fail_next(ZygoteError::ForkFailed {
        pid: -1,
        message: "Out of memory in zygote".to_string(),
    });

    let res = client.fork_app_simple("com.fail.app", "com.fail.app", 10000, 10000, 33);
    assert!(res.is_err());
    match res.unwrap_err() {
        ZygoteError::ForkFailed { pid, .. } => assert_eq!(pid, -1),
        other => panic!("Unexpected error: {:?}", other),
    }

    // Next fork should succeed
    let pid_ok = client
        .fork_app_simple("com.ok.app", "com.ok.app", 10000, 10000, 33)
        .expect("Fork should succeed");
    assert_eq!(pid_ok, 10001);
}
