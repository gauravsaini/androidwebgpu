//! Test ActivityManager AIDL Interface, Parcel Roundtrips, and Client Proxy.

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::IBinder;
use ams_rs::app_thread::MockApplicationThread;
use ams_rs::types::START_SUCCESS;
use ams_rs::{ActivityManagerClient, ActivityManagerService, IActivityManager};
use pms_rs::service::PackageManagerClient;
use pms_rs::types::{ActivityInfo, ApplicationInfo, ComponentName, Intent, PackageInfo};
use pms_rs::PackageManagerService;
use std::sync::Arc;
use zygote_client::socket::ZygoteClient;

#[test]
fn test_activity_manager_aidl_proxy_transact_roundtrip() {
    let pms = Arc::new(PackageManagerService::new());
    let (zygote_client, _zygote_handler) = ZygoteClient::new_mock_default();
    let zygote = Arc::new(zygote_client);

    // Install package in PMS
    let app_info = ApplicationInfo {
        package_name: "com.test.demo".to_string(),
        name: Some("DemoApp".to_string()),
        target_sdk_version: 33,
        uid: 10050,
        ..Default::default()
    };
    let act_info = ActivityInfo {
        name: "com.test.demo.MainActivity".to_string(),
        package_name: "com.test.demo".to_string(),
        exported: true,
        enabled: true,
        ..Default::default()
    };
    let pkg_info = PackageInfo {
        package_name: "com.test.demo".to_string(),
        version_code: 1,
        version_name: Some("1.0".to_string()),
        application_info: Some(app_info),
        activities: vec![act_info],
        requested_permissions: vec![],
        first_install_time: 0,
        last_update_time: 0,
        ..Default::default()
    };
    pms.install_package_info(pkg_info, None);

    let pms_client = Arc::new(PackageManagerClient::new(SpIBinder::from_arc(
        pms as Arc<dyn IBinder>,
    )));

    let service = Arc::new(ActivityManagerService::new(pms_client, zygote));
    let service_binder = SpIBinder::from_arc(service.clone() as Arc<dyn IBinder>);
    let client = ActivityManagerClient::new(service_binder);

    // 1. startActivity via AIDL client proxy
    let mut intent = Intent::new(Some("android.intent.action.MAIN"));
    intent.component = Some(ComponentName::new("com.test.demo", "com.test.demo.MainActivity"));
    intent.package = Some("com.test.demo".to_string());

    let res = client
        .start_activity(
            None,
            Some("com.launcher".to_string()),
            &intent,
            None,
            None,
            None,
            0,
            0,
            None,
            None,
        )
        .expect("Proxy startActivity failed");
    assert_eq!(res, START_SUCCESS);

    let top_act = service.lifecycle().top_activity().expect("Top activity not found");
    let token = top_act.read().unwrap().token.clone();

    // 2. attachApplication via AIDL client proxy
    let mock_thread = Arc::new(MockApplicationThread::new());
    let thread_binder = SpIBinder::from_arc(mock_thread as Arc<dyn IBinder>);
    client
        .attach_application(thread_binder, 1)
        .expect("Proxy attachApplication failed");

    // 3. activityResumed via AIDL client proxy
    client
        .activity_resumed(token.clone())
        .expect("Proxy activityResumed failed");

    // 4. activityPaused via AIDL client proxy
    client
        .activity_paused(token.clone())
        .expect("Proxy activityPaused failed");

    // 5. activityStopped via AIDL client proxy
    client
        .activity_stopped(token.clone(), None)
        .expect("Proxy activityStopped failed");

    // 6. finishActivity via AIDL client proxy
    let finished = client
        .finish_activity(token, 0, None, 0)
        .expect("Proxy finishActivity failed");
    assert!(finished);
}
