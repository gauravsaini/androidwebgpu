//! Adversarial, Malformed Parcel, and Concurrency Test Suite for ams_rs.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::status::{Status, STATUS_UNKNOWN_TRANSACTION};
use aidl_compat::traits::{IBinder, Remotable};
use aidl_compat::Parcel;
use ams_rs::app_thread::ActivityTokenBinder;
use ams_rs::{ActivityManagerService, IActivityManager};
use pms_rs::service::PackageManagerClient;
use pms_rs::PackageManagerService;
use std::sync::Arc;
use zygote_client::socket::ZygoteClient;

fn make_test_ams() -> Arc<ActivityManagerService> {
    let pms = Arc::new(PackageManagerService::new());
    let pms_client = Arc::new(PackageManagerClient::new(SpIBinder::from_arc(
        pms as Arc<dyn IBinder>,
    )));
    let (zygote, _) = ZygoteClient::new_mock_default();
    Arc::new(ActivityManagerService::new(pms_client, Arc::new(zygote)))
}

#[test]
fn test_adversarial_unknown_transaction_code() {
    let service = make_test_ams();

    let data = Parcel::new();
    let mut reply = Parcel::new();

    let res = service.on_transact(999999, &data, &mut reply);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), Status::from_status(STATUS_UNKNOWN_TRANSACTION));
}

#[test]
fn test_adversarial_truncated_parcel_payloads() {
    let service = make_test_ams();

    // Send 2 bytes to START_ACTIVITY which expects a full Intent
    let mut truncated_data = Parcel::new();
    truncated_data.write_bool(true).unwrap();

    let mut reply = Parcel::new();
    let res = service.on_transact(ams_rs::iactivity_manager_codes::START_ACTIVITY, &truncated_data, &mut reply);
    assert!(res.is_err());
}

#[test]
fn test_adversarial_out_of_order_lifecycle_calls() {
    let service = make_test_ams();

    // Call activity_resumed with a token that was never registered
    let phantom_token = ActivityTokenBinder::new(9999);
    let res = service.activity_resumed(phantom_token);
    assert!(res.is_err());
}

#[test]
fn test_concurrent_lifecycle_access() {
    let ams = make_test_ams();

    let mut handles = Vec::new();
    for thread_idx in 0..8 {
        let ams_clone = ams.clone();
        let handle = std::thread::spawn(move || {
            for i in 0..50 {
                let token = ActivityTokenBinder::new(thread_idx * 1000 + i);
                let _ = ams_clone.finish_activity(token, 0, None, 0);
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }
}
