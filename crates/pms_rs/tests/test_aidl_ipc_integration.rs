//! Test AIDL `IPackageManager` Interface, Parcel Roundtrips, and Binder IPC.

use aidl_compat::traits::{IBinder, Parcelable};
use aidl_compat::{Parcel, INTERFACE_TRANSACTION, PING_TRANSACTION};
use pms_rs::service::{
    ipackage_manager_codes, IPackageManager, PackageManagerClient,
    IPACKAGE_MANAGER_DESCRIPTOR,
};
use pms_rs::types::*;
use pms_rs::PackageManagerService;
use std::sync::Arc;

#[test]
fn test_parcel_roundtrips() {
    // 1. ComponentName
    let original_comp = ComponentName::new("com.androidwebgpu.demo", ".MainActivity");
    let mut p = Parcel::new();
    original_comp.write_to_parcel(&mut p).unwrap();
    let mut decoded_comp = ComponentName::default();
    decoded_comp.read_from_parcel(&p).unwrap();
    assert_eq!(original_comp, decoded_comp);

    // 2. Intent
    let mut original_intent = Intent::new(Some("android.intent.action.MAIN"));
    original_intent.add_category("android.intent.category.LAUNCHER");
    original_intent.data_uri = Some("https://androidwebgpu.org".to_string());
    original_intent.mime_type = Some("text/plain".to_string());
    original_intent.component = Some(original_comp.clone());
    original_intent.flags = 0x10000000;
    original_intent.package = Some("com.androidwebgpu.demo".to_string());

    let mut p2 = Parcel::new();
    original_intent.write_to_parcel(&mut p2).unwrap();
    let mut decoded_intent = Intent::default();
    decoded_intent.read_from_parcel(&p2).unwrap();
    assert_eq!(original_intent, decoded_intent);

    // 3. ApplicationInfo
    let app_info = ApplicationInfo {
        package_name: "com.androidwebgpu.demo".to_string(),
        name: Some("MainApplication".to_string()),
        label: Some("Demo Game".to_string()),
        icon: 0x7f020001,
        target_sdk_version: 33,
        min_sdk_version: 26,
        flags: 1 << 2,
        data_dir: "/data/user/0/com.androidwebgpu.demo".to_string(),
        source_dir: "/data/app/com.androidwebgpu.demo/base.apk".to_string(),
        public_source_dir: "/data/app/com.androidwebgpu.demo/base.apk".to_string(),
        native_library_dir: "/data/app/com.androidwebgpu.demo/lib".to_string(),
        uid: 10042,
        enabled: true,
    };

    let mut p3 = Parcel::new();
    app_info.write_to_parcel(&mut p3).unwrap();
    let mut decoded_app = ApplicationInfo::default();
    decoded_app.read_from_parcel(&p3).unwrap();
    assert_eq!(app_info, decoded_app);

    // 4. PackageInfo
    let pkg_info = PackageInfo {
        package_name: "com.androidwebgpu.demo".to_string(),
        version_code: 101,
        version_name: Some("1.0.1".to_string()),
        application_info: Some(app_info.clone()),
        activities: vec![ActivityInfo {
            name: "com.androidwebgpu.demo.MainActivity".to_string(),
            package_name: "com.androidwebgpu.demo".to_string(),
            label: Some("Demo Game".to_string()),
            icon: 0x7f020001,
            theme: 0x7f080002,
            launch_mode: 0,
            permission: None,
            exported: true,
            enabled: true,
            intent_filters: vec![IntentFilter {
                actions: vec!["android.intent.action.MAIN".to_string()],
                categories: vec!["android.intent.category.LAUNCHER".to_string()],
                data_schemes: vec![],
                priority: 0,
            }],
            application_info: Some(app_info),
        }],
        requested_permissions: vec!["android.permission.INTERNET".to_string()],
        first_install_time: 1234567890,
        last_update_time: 1234567899,
        ..Default::default()
    };

    let mut p4 = Parcel::new();
    pkg_info.write_to_parcel(&mut p4).unwrap();
    let mut decoded_pkg = PackageInfo::default();
    decoded_pkg.read_from_parcel(&p4).unwrap();
    assert_eq!(pkg_info, decoded_pkg);
}

#[test]
fn test_service_binder_transact_roundtrips() {
    let service = Arc::new(PackageManagerService::new());

    let pkg = PackageInfo {
        package_name: "com.test.app".to_string(),
        version_code: 10,
        version_name: Some("2.0".to_string()),
        application_info: Some(ApplicationInfo {
            package_name: "com.test.app".to_string(),
            name: Some("TestApp".to_string()),
            target_sdk_version: 33,
            ..Default::default()
        }),
        activities: vec![ActivityInfo {
            name: "com.test.app.MainActivity".to_string(),
            package_name: "com.test.app".to_string(),
            exported: true,
            enabled: true,
            intent_filters: vec![IntentFilter {
                actions: vec!["android.intent.action.MAIN".to_string()],
                categories: vec!["android.intent.category.LAUNCHER".to_string()],
                data_schemes: vec![],
                priority: 0,
            }],
            ..Default::default()
        }],
        requested_permissions: vec!["android.permission.VIBRATE".to_string()],
        ..Default::default()
    };
    service.install_package_info(pkg, None);

    // 1. PING_TRANSACTION
    let ping_data = Parcel::new();
    let mut ping_reply = Parcel::new();
    assert!(service
        .transact(PING_TRANSACTION, 0, &ping_data, &mut ping_reply)
        .is_ok());

    // 2. INTERFACE_TRANSACTION
    let iface_data = Parcel::new();
    let mut iface_reply = Parcel::new();
    service
        .transact(INTERFACE_TRANSACTION, 0, &iface_data, &mut iface_reply)
        .unwrap();
    let mut off = 0;
    let descriptor = iface_reply.read_utf16(&mut off).unwrap().unwrap();
    assert_eq!(descriptor, IPACKAGE_MANAGER_DESCRIPTOR);

    // 3. GET_PACKAGE_INFO
    let mut req_pkg = Parcel::new();
    req_pkg.write_utf16(Some(IPACKAGE_MANAGER_DESCRIPTOR)).unwrap();
    req_pkg.write_utf8(Some("com.test.app")).unwrap();
    req_pkg.write_i64(GET_ACTIVITIES | GET_PERMISSIONS).unwrap();
    req_pkg.write_i32(0).unwrap();

    let mut rep_pkg = Parcel::new();
    service
        .transact(
            ipackage_manager_codes::GET_PACKAGE_INFO,
            0,
            &req_pkg,
            &mut rep_pkg,
        )
        .unwrap();

    let mut rep_off = 0;
    let status = rep_pkg.read_status(&mut rep_off).unwrap();
    assert!(status.is_ok());
    let has_pkg = rep_pkg.read_bool(&mut rep_off).unwrap();
    assert!(has_pkg);
    let mut pkg_res = PackageInfo::default();
    pkg_res.read_from_parcel_at(&rep_pkg, &mut rep_off).unwrap();
    assert_eq!(pkg_res.package_name, "com.test.app");
    assert_eq!(pkg_res.version_code, 10);
    assert_eq!(pkg_res.activities.len(), 1);

    // 4. CHECK_PERMISSION
    let mut req_perm = Parcel::new();
    req_perm.write_utf16(Some(IPACKAGE_MANAGER_DESCRIPTOR)).unwrap();
    req_perm.write_utf8(Some("android.permission.INTERNET")).unwrap();
    req_perm.write_utf8(Some("com.test.app")).unwrap();
    req_perm.write_i32(0).unwrap();

    let mut rep_perm = Parcel::new();
    service
        .transact(
            ipackage_manager_codes::CHECK_PERMISSION,
            0,
            &req_perm,
            &mut rep_perm,
        )
        .unwrap();

    let mut rep_perm_off = 0;
    assert!(rep_perm.read_status(&mut rep_perm_off).unwrap().is_ok());
    let perm_res = rep_perm.read_i32(&mut rep_perm_off).unwrap();
    assert_eq!(perm_res, PERMISSION_GRANTED);
}

#[test]
fn test_package_manager_client_proxy_local() {
    let service = Arc::new(PackageManagerService::new());
    let pkg = PackageInfo {
        package_name: "com.client.test".to_string(),
        version_code: 5,
        version_name: Some("0.5".to_string()),
        application_info: Some(ApplicationInfo {
            package_name: "com.client.test".to_string(),
            name: Some("ClientApp".to_string()),
            ..Default::default()
        }),
        activities: vec![ActivityInfo {
            name: "com.client.test.HomeActivity".to_string(),
            package_name: "com.client.test".to_string(),
            exported: true,
            enabled: true,
            intent_filters: vec![IntentFilter {
                actions: vec!["android.intent.action.MAIN".to_string()],
                categories: vec!["android.intent.category.LAUNCHER".to_string()],
                data_schemes: vec![],
                priority: 0,
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    service.install_package_info(pkg, None);

    let sp_binder = aidl_compat::pointer::SpIBinder::from_arc(service as Arc<dyn IBinder>);
    let client = PackageManagerClient::new(sp_binder);

    // Call get_package_info
    let info = client.get_package_info("com.client.test", GET_ACTIVITIES, 0).unwrap();
    assert!(info.is_some());
    assert_eq!(info.unwrap().version_code, 5);

    // Call get_application_info
    let app = client.get_application_info("com.client.test", 0, 0).unwrap();
    assert!(app.is_some());
    assert_eq!(app.unwrap().name.as_deref(), Some("ClientApp"));

    // Call check_permission
    let perm = client.check_permission("android.permission.CAMERA", "com.client.test", 0).unwrap();
    assert_eq!(perm, PERMISSION_GRANTED);

    // Call resolve_intent
    let mut intent = Intent::new(Some("android.intent.action.MAIN"));
    intent.add_category("android.intent.category.LAUNCHER");
    let resolved = client.resolve_intent(&intent, "", 0, 0).unwrap();
    assert!(resolved.is_some());
    assert_eq!(
        resolved.unwrap().activity_info.unwrap().name,
        "com.client.test.HomeActivity"
    );

    // Call query_intent_activities
    let list = client.query_intent_activities(&intent, "", 0, 0).unwrap();
    assert_eq!(list.len(), 1);
}
