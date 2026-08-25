//! Challenger Empirical Adversarial & Stress Test Suite for PMS (Milestone 1).

use aidl_compat::pointer::SpIBinder;
use aidl_compat::traits::{IBinder, Parcelable};
use aidl_compat::{Parcel, Status, STATUS_BAD_VALUE};
use pms_rs::service::ipackage_manager_codes;
use pms_rs::types::*;
use pms_rs::{IPackageManager, PackageManagerClient, PackageManagerService};
use std::fs;
use std::sync::Arc;

fn load_fdroid_apk_bytes() -> Vec<u8> {
    let candidate_paths = [
        "/Users/ektasaini/Desktop/androidwebgpu/F-Droid.apk",
        "F-Droid.apk",
        "../../F-Droid.apk",
        "../F-Droid.apk",
    ];

    for path in &candidate_paths {
        if let Ok(bytes) = fs::read(path) {
            return bytes;
        }
    }

    panic!("Could not find F-Droid.apk in candidate paths: {:?}", candidate_paths);
}

#[test]
fn test_adversarial_nonexistent_queries() {
    let pms = PackageManagerService::new();
    let apk_bytes = load_fdroid_apk_bytes();
    pms.install_apk(&apk_bytes).expect("Install F-Droid");

    // 1. Nonexistent package info
    assert!(pms.get_package_info("com.fake.nonexistent.pkg", GET_ACTIVITIES | GET_PROVIDERS, 0).is_none());
    assert!(pms.get_application_info("com.fake.nonexistent.pkg", 0, 0).is_none());

    // 2. Nonexistent activity
    let fake_comp = ComponentName::new("org.fdroid.fdroid", "org.fdroid.fdroid.NonExistentActivity");
    assert!(pms.get_activity_info(&fake_comp, 0, 0).is_none());

    let wrong_pkg_comp = ComponentName::new("com.fake.package", "org.fdroid.fdroid.views.main.MainActivity");
    assert!(pms.get_activity_info(&wrong_pkg_comp, 0, 0).is_none());

    // 3. Nonexistent intent resolution
    let mut fake_intent = Intent::new(Some("com.fake.action.DO_NOTHING"));
    assert!(pms.resolve_intent(&fake_intent, "", 0, 0).is_none());

    fake_intent.package = Some("com.fake.nonexistent.pkg".to_string());
    assert!(pms.query_intent_activities(&fake_intent, "", 0, 0).is_empty());

    // 4. Nonexistent authority resolution
    assert!(pms.resolve_content_provider("com.fake.authority", 0, 0).is_none());
    assert!(pms.resolve_content_provider("", 0, 0).is_none());
    assert!(pms.resolve_content_provider("   ", 0, 0).is_none());
    assert!(pms.resolve_content_provider(";;;", 0, 0).is_none());
    assert!(pms.resolve_content_provider("org.fdroid.fdroid/InvalidClass", 0, 0).is_none());
    assert!(pms.resolve_content_provider("/InvalidClass", 0, 0).is_none());
}

#[test]
fn test_adversarial_multi_authority_resolution() {
    let pms = PackageManagerService::new();

    let mut pkg = PackageInfo {
        package_name: "com.multi.authority.test".to_string(),
        version_code: 42,
        version_name: Some("4.2.0".to_string()),
        ..Default::default()
    };

    let prov = ProviderInfo {
        name: "com.multi.authority.test.MultiProvider".to_string(),
        package_name: "com.multi.authority.test".to_string(),
        authority: "auth.first; auth.second ;  auth.third;auth.fourth  ".to_string(),
        exported: true,
        grant_uri_permissions: true,
        read_permission: Some("com.multi.permission.READ".to_string()),
        write_permission: Some("com.multi.permission.WRITE".to_string()),
        multiprocess: false,
        init_order: 10,
        enabled: true,
        application_info: None,
    };

    pkg.providers.push(prov.clone());
    pms.install_package_info(pkg, None);

    // Resolve by each separated authority (trimmed)
    let p1 = pms.resolve_content_provider("auth.first", 0, 0).expect("auth.first");
    assert_eq!(p1.name, "com.multi.authority.test.MultiProvider");

    let p2 = pms.resolve_content_provider("auth.second", 0, 0).expect("auth.second");
    assert_eq!(p2.name, "com.multi.authority.test.MultiProvider");

    let p3 = pms.resolve_content_provider("auth.third", 0, 0).expect("auth.third");
    assert_eq!(p3.name, "com.multi.authority.test.MultiProvider");

    let p4 = pms.resolve_content_provider("auth.fourth", 0, 0).expect("auth.fourth");
    assert_eq!(p4.name, "com.multi.authority.test.MultiProvider");

    // Resolve by unflattened component name
    let p_comp = pms
        .resolve_content_provider("com.multi.authority.test/com.multi.authority.test.MultiProvider", 0, 0)
        .expect("unflattened component string");
    assert_eq!(p_comp.name, "com.multi.authority.test.MultiProvider");

    // Resolve by relative unflattened component name
    let p_rel = pms
        .resolve_content_provider("com.multi.authority.test/.MultiProvider", 0, 0)
        .expect("relative unflattened component string");
    assert_eq!(p_rel.name, "com.multi.authority.test.MultiProvider");

    // Negative resolution
    assert!(pms.resolve_content_provider("auth.fifth", 0, 0).is_none());
    assert!(pms.resolve_content_provider("auth.first;auth.second", 0, 0).is_none());
}

#[test]
fn test_adversarial_provider_info_parcel_roundtrip() {
    // 1. Fully populated ProviderInfo with ApplicationInfo
    let original = ProviderInfo {
        name: "com.example.provider.TestProvider".to_string(),
        package_name: "com.example.provider".to_string(),
        authority: "com.example.provider.auth1;com.example.provider.auth2".to_string(),
        exported: true,
        grant_uri_permissions: true,
        read_permission: Some("android.permission.READ_EXTERNAL_STORAGE".to_string()),
        write_permission: Some("android.permission.WRITE_EXTERNAL_STORAGE".to_string()),
        multiprocess: true,
        init_order: -100,
        enabled: true,
        application_info: Some(ApplicationInfo {
            package_name: "com.example.provider".to_string(),
            name: Some("com.example.provider.App".to_string()),
            label: Some("Test App".to_string()),
            icon: 0x7f010001,
            target_sdk_version: 34,
            min_sdk_version: 21,
            flags: 0x00000001,
            data_dir: "/data/user/0/com.example.provider".to_string(),
            source_dir: "/data/app/com.example.provider/base.apk".to_string(),
            public_source_dir: "/data/app/com.example.provider/base.apk".to_string(),
            native_library_dir: "/data/app/com.example.provider/lib/arm64".to_string(),
            uid: 10123,
            enabled: true,
        }),
    };

    let mut parcel = Parcel::new();
    original.write_to_parcel(&mut parcel).expect("write_to_parcel");

    let mut decoded = ProviderInfo::default();
    let mut offset = 0;
    decoded.read_from_parcel_at(&parcel, &mut offset).expect("read_from_parcel_at");

    assert_eq!(original, decoded);
    assert_eq!(offset, parcel.data_size());

    // 2. Minimal / Empty ProviderInfo
    let minimal = ProviderInfo::default();
    let mut min_parcel = Parcel::new();
    minimal.write_to_parcel(&mut min_parcel).expect("write minimal");

    let mut min_decoded = ProviderInfo::default();
    let mut min_offset = 0;
    min_decoded.read_from_parcel_at(&min_parcel, &mut min_offset).expect("read minimal");
    assert_eq!(minimal, min_decoded);

    // 3. Truncated parcel error recovery
    let mut trunc_parcel = Parcel::new();
    trunc_parcel.write_utf8(Some("only_name")).unwrap();
    // Missing remaining fields
    let mut trunc_decoded = ProviderInfo::default();
    let mut trunc_offset = 0;
    let err = trunc_decoded.read_from_parcel_at(&trunc_parcel, &mut trunc_offset);
    assert!(err.is_err(), "Truncated parcel must fail cleanly without panic");
}

#[test]
fn test_adversarial_flag_combinations() {
    let apk_bytes = load_fdroid_apk_bytes();
    let pms = PackageManagerService::new();
    pms.install_apk(&apk_bytes).expect("Install F-Droid");

    // 1. Flag 0: No components returned
    let pkg_0 = pms.get_package_info("org.fdroid.fdroid", 0, 0).expect("pkg_0");
    assert_eq!(pkg_0.package_name, "org.fdroid.fdroid");
    assert!(pkg_0.activities.is_empty());
    assert!(pkg_0.services.is_empty());
    assert!(pkg_0.receivers.is_empty());
    assert!(pkg_0.providers.is_empty());
    assert!(pkg_0.requested_permissions.is_empty());

    // 2. Flag GET_PROVIDERS: Only providers returned
    let pkg_prov = pms.get_package_info("org.fdroid.fdroid", GET_PROVIDERS, 0).expect("pkg_prov");
    assert_eq!(pkg_prov.providers.len(), 4);
    assert!(pkg_prov.activities.is_empty());
    assert!(pkg_prov.services.is_empty());
    assert!(pkg_prov.receivers.is_empty());
    assert!(pkg_prov.requested_permissions.is_empty());

    // 3. Flag GET_ACTIVITIES | GET_SERVICES: Only activities and services
    let pkg_act_svc = pms
        .get_package_info("org.fdroid.fdroid", GET_ACTIVITIES | GET_SERVICES, 0)
        .expect("pkg_act_svc");
    assert_eq!(pkg_act_svc.activities.len(), 25);
    assert_eq!(pkg_act_svc.services.len(), 16);
    assert!(pkg_act_svc.receivers.is_empty());
    assert!(pkg_act_svc.providers.is_empty());
    assert!(pkg_act_svc.requested_permissions.is_empty());

    // 4. Flag GET_RECEIVERS | GET_PERMISSIONS: Only receivers and permissions
    let pkg_rcv_perm = pms
        .get_package_info("org.fdroid.fdroid", GET_RECEIVERS | GET_PERMISSIONS, 0)
        .expect("pkg_rcv_perm");
    assert_eq!(pkg_rcv_perm.receivers.len(), 16);
    assert_eq!(pkg_rcv_perm.requested_permissions.len(), 29);
    assert!(pkg_rcv_perm.activities.is_empty());
    assert!(pkg_rcv_perm.services.is_empty());
    assert!(pkg_rcv_perm.providers.is_empty());

    // 5. Flag GET_ACTIVITIES | GET_PROVIDERS:
    let pkg_act_prov = pms
        .get_package_info("org.fdroid.fdroid", GET_ACTIVITIES | GET_PROVIDERS, 0)
        .expect("pkg_act_prov");
    assert_eq!(pkg_act_prov.activities.len(), 25);
    assert_eq!(pkg_act_prov.providers.len(), 4);
    assert!(pkg_act_prov.services.is_empty());
    assert!(pkg_act_prov.receivers.is_empty());
    assert!(pkg_act_prov.requested_permissions.is_empty());

    // 6. All combined:
    let pkg_all = pms
        .get_package_info(
            "org.fdroid.fdroid",
            GET_ACTIVITIES | GET_SERVICES | GET_RECEIVERS | GET_PROVIDERS | GET_PERMISSIONS,
            0,
        )
        .expect("pkg_all");
    assert_eq!(pkg_all.activities.len(), 25);
    assert_eq!(pkg_all.services.len(), 16);
    assert_eq!(pkg_all.receivers.len(), 16);
    assert_eq!(pkg_all.providers.len(), 4);
    assert_eq!(pkg_all.requested_permissions.len(), 29);

    // 7. Unknown / high bits flag:
    let pkg_high = pms
        .get_package_info("org.fdroid.fdroid", 0x7FFF00000000 | GET_PROVIDERS, 0)
        .expect("pkg_high");
    assert_eq!(pkg_high.providers.len(), 4);
    assert!(pkg_high.activities.is_empty());
}

#[test]
fn test_adversarial_service_and_receiver_parcel_roundtrips() {
    // 1. ServiceInfo Parcel Roundtrip
    let svc = ServiceInfo {
        name: "org.fdroid.fdroid.data.DownloaderService".to_string(),
        package_name: "org.fdroid.fdroid".to_string(),
        permission: Some("android.permission.BIND_JOB_SERVICE".to_string()),
        exported: false,
        enabled: true,
    };
    let mut p = Parcel::new();
    svc.write_to_parcel(&mut p).unwrap();
    let mut svc_dec = ServiceInfo::default();
    let mut off = 0;
    svc_dec.read_from_parcel_at(&p, &mut off).unwrap();
    assert_eq!(svc, svc_dec);

    // 2. ReceiverInfo Parcel Roundtrip with IntentFilters
    let rcv = ReceiverInfo {
        name: "org.fdroid.fdroid.receiver.PackageReceiver".to_string(),
        package_name: "org.fdroid.fdroid".to_string(),
        permission: None,
        exported: true,
        enabled: true,
        intent_filters: vec![
            IntentFilter {
                actions: vec![
                    "android.intent.action.PACKAGE_ADDED".to_string(),
                    "android.intent.action.PACKAGE_REMOVED".to_string(),
                ],
                categories: vec![],
                data_schemes: vec!["package".to_string()],
                priority: 100,
            },
        ],
    };
    let mut rp = Parcel::new();
    rcv.write_to_parcel(&mut rp).unwrap();
    let mut rcv_dec = ReceiverInfo::default();
    let mut roff = 0;
    rcv_dec.read_from_parcel_at(&rp, &mut roff).unwrap();
    assert_eq!(rcv, rcv_dec);
}

#[test]
fn test_adversarial_full_package_info_parcel_roundtrip() {
    let apk_bytes = load_fdroid_apk_bytes();
    let pms = PackageManagerService::new();
    let pkg_info = pms.install_apk(&apk_bytes).expect("Install F-Droid");

    let mut parcel = Parcel::new();
    pkg_info.write_to_parcel(&mut parcel).expect("write PackageInfo");

    let mut decoded = PackageInfo::default();
    let mut offset = 0;
    decoded.read_from_parcel_at(&parcel, &mut offset).expect("read PackageInfo");

    assert_eq!(pkg_info.package_name, decoded.package_name);
    assert_eq!(pkg_info.version_code, decoded.version_code);
    assert_eq!(pkg_info.version_name, decoded.version_name);
    assert_eq!(pkg_info.activities.len(), decoded.activities.len());
    assert_eq!(pkg_info.services.len(), decoded.services.len());
    assert_eq!(pkg_info.receivers.len(), decoded.receivers.len());
    assert_eq!(pkg_info.providers.len(), decoded.providers.len());
    assert_eq!(pkg_info.requested_permissions.len(), decoded.requested_permissions.len());
    assert_eq!(pkg_info.application_info, decoded.application_info);
}

#[test]
fn test_adversarial_ipc_transact_and_proxy_edge_cases() {
    let apk_bytes = load_fdroid_apk_bytes();
    let service = Arc::new(PackageManagerService::new());
    service.install_apk(&apk_bytes).expect("Install F-Droid.apk");

    let sp_binder = SpIBinder::from_arc(service as Arc<dyn IBinder>);
    let client = PackageManagerClient::new(sp_binder);

    // 1. Querying nonexistent provider via AIDL
    let nonexistent_prov = client.resolve_content_provider("nonexistent.authority", 0, 0).unwrap();
    assert!(nonexistent_prov.is_none());

    // 2. Querying nonexistent package via AIDL
    let nonexistent_pkg = client.get_package_info("com.nonexistent.app", 0, 0).unwrap();
    assert!(nonexistent_pkg.is_none());

    // 3. Querying nonexistent application info via AIDL
    let nonexistent_app = client.get_application_info("com.nonexistent.app", 0, 0).unwrap();
    assert!(nonexistent_app.is_none());

    // 4. Querying nonexistent activity info via AIDL
    let fake_comp = ComponentName::new("org.fdroid.fdroid", "NonExistentClass");
    let nonexistent_act = client.get_activity_info(&fake_comp, 0, 0).unwrap();
    assert!(nonexistent_act.is_none());

    // 5. Querying empty intent via AIDL
    let empty_intent = Intent::default();
    let resolved_empty = client.resolve_intent(&empty_intent, "", 0, 0).unwrap();
    // Default intent matches any activity without specific filter constraints or none
    let query_empty = client.query_intent_activities(&empty_intent, "", 0, 0).unwrap();
    assert_eq!(resolved_empty.is_some(), !query_empty.is_empty());

    // 6. Direct IBinder::transact with corrupted buffer for RESOLVE_CONTENT_PROVIDER
    let pms_svc = PackageManagerService::new();
    let empty_data = Parcel::new();
    let mut reply = Parcel::new();
    let transact_err = pms_svc.transact(
        ipackage_manager_codes::RESOLVE_CONTENT_PROVIDER,
        0,
        &empty_data,
        &mut reply,
    );
    assert!(transact_err.is_err());
    assert_eq!(transact_err.unwrap_err(), Status::from_status(STATUS_BAD_VALUE));
}

#[test]
fn test_adversarial_extreme_string_payloads() {
    let pms = PackageManagerService::new();

    // 1. 100KB long string in authority and package name
    let huge_str = "a".repeat(100_000);
    assert!(pms.resolve_content_provider(&huge_str, 0, 0).is_none());
    assert!(pms.get_package_info(&huge_str, GET_ACTIVITIES, 0).is_none());

    // 2. Unicode and emoji package registration
    let emoji_pkg = PackageInfo {
        package_name: "📦.🚀.⚡".to_string(),
        version_code: 1,
        version_name: Some("1.0.0-🎉".to_string()),
        providers: vec![ProviderInfo {
            name: "📦.🚀.⚡.Provider".to_string(),
            package_name: "📦.🚀.⚡".to_string(),
            authority: "📦.authority.test; 🔥.authority".to_string(),
            exported: true,
            grant_uri_permissions: false,
            read_permission: None,
            write_permission: None,
            multiprocess: false,
            init_order: 0,
            enabled: true,
            application_info: None,
        }],
        ..Default::default()
    };
    pms.install_package_info(emoji_pkg.clone(), None);

    let found_pkg = pms.get_package_info("📦.🚀.⚡", GET_PROVIDERS, 0).expect("Emoji pkg found");
    assert_eq!(found_pkg.package_name, "📦.🚀.⚡");
    assert_eq!(found_pkg.providers.len(), 1);

    let p1 = pms.resolve_content_provider("📦.authority.test", 0, 0).expect("Emoji auth 1");
    assert_eq!(p1.name, "📦.🚀.⚡.Provider");

    let p2 = pms.resolve_content_provider("🔥.authority", 0, 0).expect("Emoji auth 2");
    assert_eq!(p2.name, "📦.🚀.⚡.Provider");
}

#[test]
fn test_adversarial_disabled_component_filtering() {
    let pms = PackageManagerService::new();

    let pkg = PackageInfo {
        package_name: "com.disguise.test".to_string(),
        version_code: 1,
        activities: vec![
            ActivityInfo {
                name: "com.disguise.test.EnabledActivity".to_string(),
                package_name: "com.disguise.test".to_string(),
                exported: true,
                enabled: true,
                intent_filters: vec![IntentFilter {
                    actions: vec!["android.intent.action.VIEW".to_string()],
                    categories: vec!["android.intent.category.DEFAULT".to_string()],
                    data_schemes: vec![],
                    priority: 50,
                }],
                ..Default::default()
            },
            ActivityInfo {
                name: "com.disguise.test.DisabledActivity".to_string(),
                package_name: "com.disguise.test".to_string(),
                exported: true,
                enabled: false,
                intent_filters: vec![IntentFilter {
                    actions: vec!["android.intent.action.VIEW".to_string()],
                    categories: vec!["android.intent.category.DEFAULT".to_string()],
                    data_schemes: vec![],
                    priority: 100, // Higher priority but disabled
                }],
                ..Default::default()
            },
        ],
        ..Default::default()
    };

    pms.install_package_info(pkg, None);

    let view_intent = Intent::new(Some("android.intent.action.VIEW"));

    // Default query: disabled activity must be skipped despite higher priority
    let resolved = pms.resolve_intent(&view_intent, "", 0, 0).expect("resolve intent");
    assert_eq!(
        resolved.activity_info.unwrap().name,
        "com.disguise.test.EnabledActivity"
    );

    // Query with GET_DISABLED_COMPONENTS: disabled activity should be matched and take priority (priority 100 > 50)
    let resolved_disabled = pms
        .resolve_intent(&view_intent, "", GET_DISABLED_COMPONENTS, 0)
        .expect("resolve with GET_DISABLED_COMPONENTS");
    assert_eq!(
        resolved_disabled.activity_info.unwrap().name,
        "com.disguise.test.DisabledActivity"
    );
}
