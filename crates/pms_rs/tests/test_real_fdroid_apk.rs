//! Integration Tests: Ingestion and Extended PMS Queries for real-world F-Droid.apk.

use aidl_compat::traits::IBinder;
use aidl_compat::pointer::SpIBinder;
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
fn test_real_fdroid_apk_ingestion_and_manifest_parsing() {
    let apk_bytes = load_fdroid_apk_bytes();
    assert!(!apk_bytes.is_empty(), "F-Droid.apk must not be empty");

    let pms = PackageManagerService::new();
    let pkg_info = pms.install_apk(&apk_bytes).expect("F-Droid.apk installation must succeed");

    // 1. Package Metadata Verification
    assert_eq!(pkg_info.package_name, "org.fdroid.fdroid");
    assert_eq!(pkg_info.version_code, 1023051);
    assert_eq!(pkg_info.version_name.as_deref(), Some("1.23.1"));

    // 2. ApplicationInfo Verification
    assert!(pkg_info.application_info.is_some());
    let app_info = pkg_info.application_info.unwrap();
    assert_eq!(app_info.package_name, "org.fdroid.fdroid");
    assert_eq!(app_info.name.as_deref(), Some("org.fdroid.fdroid.FDroidApp"));
    assert_eq!(app_info.label.as_deref(), Some("F-Droid"));
    assert_eq!(app_info.target_sdk_version, 30);
    assert_eq!(app_info.min_sdk_version, 23);
    assert!(app_info.enabled);

    // 3. Activity Component Inventory Verification (25 activities in F-Droid 1.23.1)
    assert_eq!(pkg_info.activities.len(), 25);
    let main_act = pkg_info
        .activities
        .iter()
        .find(|a| a.name == "org.fdroid.fdroid.views.main.MainActivity")
        .expect("MainActivity must exist");
    assert!(main_act.exported, "MainActivity must be exported");
    assert_eq!(main_act.launch_mode, 1, "MainActivity launchMode should be singleTop");

    // 4. ContentProvider Component Inventory Verification (4 providers)
    assert_eq!(pkg_info.providers.len(), 4, "F-Droid declares 4 content providers");
    let apk_file_prov = pkg_info
        .providers
        .iter()
        .find(|p| p.name == "org.fdroid.fdroid.installer.ApkFileProvider")
        .expect("ApkFileProvider must exist");
    assert_eq!(apk_file_prov.authority, "org.fdroid.fdroid.installer.ApkFileProvider");
    assert!(apk_file_prov.grant_uri_permissions);

    let file_prov = pkg_info
        .providers
        .iter()
        .find(|p| p.name == "androidx.core.content.FileProvider")
        .expect("FileProvider must exist");
    assert_eq!(file_prov.authority, "org.fdroid.fdroid.installer");
    assert!(file_prov.grant_uri_permissions);

    let public_dir_prov = pkg_info
        .providers
        .iter()
        .find(|p| p.name == "org.fdroid.fdroid.nearby.PublicSourceDirProvider")
        .expect("PublicSourceDirProvider must exist");
    assert_eq!(public_dir_prov.authority, "org.fdroid.fdroid.nearby.PublicSourceDirProvider");

    let acra_prov = pkg_info
        .providers
        .iter()
        .find(|p| p.name == "org.acra.attachment.AcraContentProvider")
        .expect("AcraContentProvider must exist");
    assert_eq!(acra_prov.authority, "org.fdroid.fdroid.acra");

    // 5. Services & Receivers Inventory Verification (16 services, 16 receivers)
    assert_eq!(pkg_info.services.len(), 16, "F-Droid declares 16 background services");
    assert_eq!(pkg_info.receivers.len(), 16, "F-Droid declares 16 broadcast receivers");

    // 6. Permissions Verification (29 requested permissions including uses-permission-sdk-23)
    assert_eq!(pkg_info.requested_permissions.len(), 29);
    assert!(pkg_info.requested_permissions.contains(&"android.permission.INTERNET".to_string()));
    assert!(pkg_info.requested_permissions.contains(&"android.permission.ACCESS_NETWORK_STATE".to_string()));
    assert!(pkg_info.requested_permissions.contains(&"android.permission.REQUEST_INSTALL_PACKAGES".to_string()));
    assert!(pkg_info.requested_permissions.contains(&"android.permission.POST_NOTIFICATIONS".to_string()));
}

#[test]
fn test_real_fdroid_intent_and_provider_resolution() {
    let apk_bytes = load_fdroid_apk_bytes();
    let pms = PackageManagerService::new();
    pms.install_apk(&apk_bytes).expect("F-Droid.apk installation");

    // 1. Resolve MAIN / LAUNCHER Intent for F-Droid
    let mut launcher_intent = Intent::new(Some("android.intent.action.MAIN"));
    launcher_intent.add_category("android.intent.category.LAUNCHER");
    launcher_intent.package = Some("org.fdroid.fdroid".to_string());

    let resolve_info = pms
        .resolve_intent(&launcher_intent, "", MATCH_DEFAULT_ONLY, 0)
        .expect("Launcher intent must resolve");
    assert!(resolve_info.activity_info.is_some());
    let resolved_act = resolve_info.activity_info.unwrap();
    assert_eq!(resolved_act.name, "org.fdroid.fdroid.views.main.MainActivity");
    assert_eq!(resolved_act.package_name, "org.fdroid.fdroid");

    // 2. Query Intent Activities for VIEW deep link (fdroid.app schema)
    let mut view_intent = Intent::new(Some("android.intent.action.VIEW"));
    view_intent.add_category("android.intent.category.BROWSABLE");
    view_intent.data_uri = Some("fdroid.app:org.example.app".to_string());
    view_intent.package = Some("org.fdroid.fdroid".to_string());

    let view_matches = pms.query_intent_activities(&view_intent, "", 0, 0);
    assert!(!view_matches.is_empty(), "VIEW deep link should match MainActivity or AppDetailsActivity");

    // 3. Resolve Content Provider by Authorities
    let apk_prov = pms
        .resolve_content_provider("org.fdroid.fdroid.installer.ApkFileProvider", 0, 0)
        .expect("ApkFileProvider authority must resolve");
    assert_eq!(apk_prov.name, "org.fdroid.fdroid.installer.ApkFileProvider");
    assert_eq!(apk_prov.package_name, "org.fdroid.fdroid");
    assert!(apk_prov.grant_uri_permissions);

    let file_prov = pms
        .resolve_content_provider("org.fdroid.fdroid.installer", 0, 0)
        .expect("FileProvider authority must resolve");
    assert_eq!(file_prov.name, "androidx.core.content.FileProvider");
    assert_eq!(file_prov.package_name, "org.fdroid.fdroid");

    let acra_prov = pms
        .resolve_content_provider("org.fdroid.fdroid.acra", 0, 0)
        .expect("AcraContentProvider authority must resolve");
    assert_eq!(acra_prov.name, "org.acra.attachment.AcraContentProvider");

    let public_dir_prov = pms
        .resolve_content_provider("org.fdroid.fdroid.nearby.PublicSourceDirProvider", 0, 0)
        .expect("PublicSourceDirProvider authority must resolve");
    assert_eq!(public_dir_prov.name, "org.fdroid.fdroid.nearby.PublicSourceDirProvider");

    // 4. Non-existent authority resolution
    assert!(pms.resolve_content_provider("com.nonexistent.authority", 0, 0).is_none());

    // 5. Query PackageInfo with Flags filtering
    let full_info = pms
        .get_package_info(
            "org.fdroid.fdroid",
            GET_ACTIVITIES | GET_SERVICES | GET_RECEIVERS | GET_PROVIDERS | GET_PERMISSIONS,
            0,
        )
        .expect("Full PackageInfo");
    assert_eq!(full_info.activities.len(), 25);
    assert_eq!(full_info.services.len(), 16);
    assert_eq!(full_info.receivers.len(), 16);
    assert_eq!(full_info.providers.len(), 4);
    assert_eq!(full_info.requested_permissions.len(), 29);

    let providers_only = pms
        .get_package_info("org.fdroid.fdroid", GET_PROVIDERS, 0)
        .expect("Providers only");
    assert_eq!(providers_only.providers.len(), 4);
    assert!(providers_only.activities.is_empty());
    assert!(providers_only.services.is_empty());
    assert!(providers_only.receivers.is_empty());
    assert!(providers_only.requested_permissions.is_empty());
}

#[test]
fn test_real_fdroid_aidl_ipc_client_calls() {
    let apk_bytes = load_fdroid_apk_bytes();
    let service = Arc::new(PackageManagerService::new());
    service.install_apk(&apk_bytes).expect("Install F-Droid.apk");

    let sp_binder = SpIBinder::from_arc(service as Arc<dyn IBinder>);
    let client = PackageManagerClient::new(sp_binder);

    // 1. AIDL get_package_info
    let pkg_info = client
        .get_package_info("org.fdroid.fdroid", GET_ACTIVITIES | GET_PROVIDERS, 0)
        .unwrap()
        .expect("get_package_info via AIDL");
    assert_eq!(pkg_info.package_name, "org.fdroid.fdroid");
    assert_eq!(pkg_info.version_code, 1023051);
    assert_eq!(pkg_info.activities.len(), 25);
    assert_eq!(pkg_info.providers.len(), 4);

    // 2. AIDL get_application_info
    let app_info = client
        .get_application_info("org.fdroid.fdroid", 0, 0)
        .unwrap()
        .expect("get_application_info via AIDL");
    assert_eq!(app_info.package_name, "org.fdroid.fdroid");
    assert_eq!(app_info.label.as_deref(), Some("F-Droid"));

    // 3. AIDL resolve_intent
    let mut launcher_intent = Intent::new(Some("android.intent.action.MAIN"));
    launcher_intent.add_category("android.intent.category.LAUNCHER");
    launcher_intent.package = Some("org.fdroid.fdroid".to_string());

    let resolve_info = client
        .resolve_intent(&launcher_intent, "", MATCH_DEFAULT_ONLY, 0)
        .unwrap()
        .expect("resolve_intent via AIDL");
    assert_eq!(
        resolve_info.activity_info.unwrap().name,
        "org.fdroid.fdroid.views.main.MainActivity"
    );

    // 4. AIDL resolve_content_provider
    let prov = client
        .resolve_content_provider("org.fdroid.fdroid.installer.ApkFileProvider", 0, 0)
        .unwrap()
        .expect("resolve_content_provider via AIDL");
    assert_eq!(prov.name, "org.fdroid.fdroid.installer.ApkFileProvider");
    assert_eq!(prov.authority, "org.fdroid.fdroid.installer.ApkFileProvider");
    assert!(prov.grant_uri_permissions);

    let file_prov = client
        .resolve_content_provider("org.fdroid.fdroid.installer", 0, 0)
        .unwrap()
        .expect("resolve_content_provider FileProvider via AIDL");
    assert_eq!(file_prov.name, "androidx.core.content.FileProvider");
    assert_eq!(file_prov.authority, "org.fdroid.fdroid.installer");

    // 5. AIDL check_permission
    let perm_res = client
        .check_permission("android.permission.INTERNET", "org.fdroid.fdroid", 0)
        .unwrap();
    assert_eq!(perm_res, PERMISSION_GRANTED);
}
