//! Test `IApplicationThread` AIDL Serialization, Parcel Codecs, and Stub Dispatches.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::{IBinder, Parcelable};
use ams_rs::app_thread::{
    ActivityTokenBinder, ApplicationThreadProxy, ClientTransaction, ClientTransactionItem,
    IApplicationThread, MockApplicationThread,
};
use binder_rt::Parcel;
use pms_rs::types::ApplicationInfo;
use std::sync::Arc;

#[test]
fn test_client_transaction_parcel_roundtrip() {
    let original = ClientTransaction {
        activity_token_id: 1042,
        items: vec![
            ClientTransactionItem::ResumeActivity { is_forward: true },
            ClientTransactionItem::PauseActivity {
                is_finishing: false,
                user_leaving: true,
            },
            ClientTransactionItem::StopActivity,
            ClientTransactionItem::DestroyActivity { is_finishing: true },
        ],
    };

    let mut p = Parcel::new();
    original.write_to_parcel(&mut p).unwrap();

    let mut decoded = ClientTransaction {
        activity_token_id: 0,
        items: Vec::new(),
    };
    decoded.read_from_parcel(&p).unwrap();

    assert_eq!(original, decoded);
}

#[test]
fn test_application_thread_proxy_and_stub_direct_dispatch() {
    let mock = Arc::new(MockApplicationThread::new());
    let binder = SpIBinder::from_arc(mock.clone() as Arc<dyn IBinder>);
    let proxy = ApplicationThreadProxy::new(binder);

    let app_info = ApplicationInfo {
        package_name: "com.androidwebgpu.arcade".to_string(),
        name: Some("ArcadeApp".to_string()),
        target_sdk_version: 33,
        uid: 10042,
        ..Default::default()
    };

    // 1. bind_application
    proxy
        .bind_application("com.androidwebgpu.arcade", &app_info, "com.androidwebgpu.arcade")
        .expect("bind_application failed");

    {
        let bound = mock.bound_applications.read().unwrap();
        assert_eq!(bound.len(), 1);
        assert_eq!(bound[0].0, "com.androidwebgpu.arcade");
        assert_eq!(bound[0].2, "com.androidwebgpu.arcade");
    }

    // 2. schedule_resume_activity
    let token = ActivityTokenBinder::new(555);
    proxy
        .schedule_resume_activity(&token, true)
        .expect("schedule_resume failed");

    {
        let resumed = mock.resumed_activities.read().unwrap();
        assert_eq!(resumed.len(), 1);
        assert_eq!(resumed[0], (555, true));
    }

    // 3. schedule_pause_activity
    proxy
        .schedule_pause_activity(&token, false, true)
        .expect("schedule_pause failed");

    {
        let paused = mock.paused_activities.read().unwrap();
        assert_eq!(paused.len(), 1);
        assert_eq!(paused[0], (555, false, true));
    }

    // 4. schedule_stop_activity
    proxy
        .schedule_stop_activity(&token)
        .expect("schedule_stop failed");

    {
        let stopped = mock.stopped_activities.read().unwrap();
        assert_eq!(stopped.len(), 1);
        assert_eq!(stopped[0], 555);
    }

    // 5. schedule_destroy_activity
    proxy
        .schedule_destroy_activity(&token, true)
        .expect("schedule_destroy failed");

    {
        let destroyed = mock.destroyed_activities.read().unwrap();
        assert_eq!(destroyed.len(), 1);
        assert_eq!(destroyed[0], (555, true));
    }

    // 6. schedule_transaction
    let tx = ClientTransaction {
        activity_token_id: 555,
        items: vec![ClientTransactionItem::ResumeActivity { is_forward: false }],
    };
    proxy.schedule_transaction(&tx).expect("schedule_transaction failed");

    {
        let transactions = mock.scheduled_transactions.read().unwrap();
        assert_eq!(transactions.len(), 1);
        assert_eq!(transactions[0].activity_token_id, 555);
    }
}
