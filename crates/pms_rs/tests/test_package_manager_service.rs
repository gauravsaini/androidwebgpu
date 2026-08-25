//! Test PackageManagerService Core APIs, Multi-Package Registry, and Resource Table Integration.

use pms_rs::types::*;
use pms_rs::PackageManagerService;
use std::fs;

#[test]
fn test_package_manager_service_query_apis() {
    let pms = PackageManagerService::new();

    let app1 = PackageInfo {
        package_name: "com.game.arcade".to_string(),
        version_code: 10,
        version_name: Some("1.0.0".to_string()),
        application_info: Some(ApplicationInfo {
            package_name: "com.game.arcade".to_string(),
            name: Some("ArcadeApp".to_string()),
            label: Some("Arcade 3D Flight".to_string()),
            target_sdk_version: 33,
            min_sdk_version: 28,
            flags: 1 << 2,
            data_dir: "/data/user/0/com.game.arcade".to_string(),
            source_dir: "/data/app/com.game.arcade/base.apk".to_string(),
            public_source_dir: "/data/app/com.game.arcade/base.apk".to_string(),
            native_library_dir: "/data/app/com.game.arcade/lib".to_string(),
            uid: 10001,
            enabled: true,
            icon: 0x7f020001,
        }),
        activities: vec![
            ActivityInfo {
                name: "com.game.arcade.FlightActivity".to_string(),
                package_name: "com.game.arcade".to_string(),
                label: Some("Flight Simulation".to_string()),
                exported: true,
                enabled: true,
                intent_filters: vec![IntentFilter {
                    actions: vec!["android.intent.action.MAIN".to_string()],
                    categories: vec![
                        "android.intent.category.LAUNCHER".to_string(),
                        "android.intent.category.DEFAULT".to_string(),
                    ],
                    data_schemes: vec![],
                    priority: 10,
                }],
                ..Default::default()
            },
            ActivityInfo {
                name: "com.game.arcade.ScoreActivity".to_string(),
                package_name: "com.game.arcade".to_string(),
                label: Some("High Scores".to_string()),
                exported: false,
                enabled: true,
                intent_filters: vec![],
                ..Default::default()
            },
        ],
        requested_permissions: vec![
            "android.permission.INTERNET".to_string(),
            "android.permission.VIBRATE".to_string(),
        ],
        first_install_time: 1000,
        last_update_time: 2000,
        ..Default::default()
    };

    let app2 = PackageInfo {
        package_name: "com.media.player".to_string(),
        version_code: 20,
        version_name: Some("2.0.0".to_string()),
        application_info: Some(ApplicationInfo {
            package_name: "com.media.player".to_string(),
            name: Some("MediaPlayerApp".to_string()),
            label: Some("Media Player".to_string()),
            target_sdk_version: 33,
            min_sdk_version: 26,
            flags: 1 << 2,
            data_dir: "/data/user/0/com.media.player".to_string(),
            source_dir: "/data/app/com.media.player/base.apk".to_string(),
            public_source_dir: "/data/app/com.media.player/base.apk".to_string(),
            native_library_dir: "/data/app/com.media.player/lib".to_string(),
            uid: 10002,
            enabled: true,
            icon: 0x7f020002,
        }),
        activities: vec![ActivityInfo {
            name: "com.media.player.PlayerActivity".to_string(),
            package_name: "com.media.player".to_string(),
            label: Some("Video Player".to_string()),
            exported: true,
            enabled: true,
            intent_filters: vec![IntentFilter {
                actions: vec!["android.intent.action.VIEW".to_string()],
                categories: vec!["android.intent.category.DEFAULT".to_string()],
                data_schemes: vec!["content".to_string(), "file".to_string()],
                priority: 5,
            }],
            ..Default::default()
        }],
        requested_permissions: vec!["android.permission.READ_EXTERNAL_STORAGE".to_string()],
        first_install_time: 1500,
        last_update_time: 2500,
        ..Default::default()
    };

    pms.install_package_info(app1, None);
    pms.install_package_info(app2, None);

    assert_eq!(pms.package_count(), 2);

    // 1. getPackageInfo with GET_ACTIVITIES and GET_PERMISSIONS
    let pkg1 = pms
        .get_package_info("com.game.arcade", GET_ACTIVITIES | GET_PERMISSIONS, 0)
        .expect("com.game.arcade must exist");
    assert_eq!(pkg1.package_name, "com.game.arcade");
    assert_eq!(pkg1.version_code, 10);
    assert_eq!(pkg1.activities.len(), 2);
    assert_eq!(pkg1.requested_permissions.len(), 2);

    // 2. getPackageInfo with flags = 0 (no activities or permissions returned)
    let pkg1_no_flags = pms
        .get_package_info("com.game.arcade", 0, 0)
        .expect("com.game.arcade must exist");
    assert_eq!(pkg1_no_flags.activities.len(), 0);
    assert_eq!(pkg1_no_flags.requested_permissions.len(), 0);

    // 3. Non-existent package
    assert!(pms.get_package_info("com.nonexistent.app", 0, 0).is_none());

    // 4. getApplicationInfo
    let app1_info = pms
        .get_application_info("com.game.arcade", 0, 0)
        .expect("ApplicationInfo must exist");
    assert_eq!(app1_info.name.as_deref(), Some("ArcadeApp"));
    assert_eq!(app1_info.target_sdk_version, 33);
    assert_eq!(app1_info.uid, 10001);

    // 5. getActivityInfo (exact and relative)
    let comp_exact = ComponentName::new("com.game.arcade", "com.game.arcade.FlightActivity");
    let act_exact = pms.get_activity_info(&comp_exact, 0, 0).expect("Exact match");
    assert_eq!(act_exact.name, "com.game.arcade.FlightActivity");
    assert_eq!(act_exact.label.as_deref(), Some("Flight Simulation"));

    let comp_relative = ComponentName::new("com.game.arcade", ".ScoreActivity");
    let act_relative = pms.get_activity_info(&comp_relative, 0, 0).expect("Relative match");
    assert_eq!(act_relative.name, "com.game.arcade.ScoreActivity");
    assert_eq!(act_relative.label.as_deref(), Some("High Scores"));

    // 6. getInstalledPackages and getInstalledApplications
    let all_pkgs = pms.get_installed_packages(GET_ACTIVITIES, 0);
    assert_eq!(all_pkgs.len(), 2);

    let all_apps = pms.get_installed_applications(0, 0);
    assert_eq!(all_apps.len(), 2);

    // 7. checkPermission
    assert_eq!(
        pms.check_permission("android.permission.INTERNET", "com.game.arcade", 0),
        PERMISSION_GRANTED
    );
    assert_eq!(
        pms.check_permission("android.permission.CAMERA", "com.game.arcade", 0),
        PERMISSION_GRANTED
    );
}

#[test]
fn test_package_manager_real_apk_install_and_query() {
    let pms = PackageManagerService::new();

    let paths = [
        "fixtures/godot_gles2.apk",
        "fixtures/unity_cube.apk",
        "fixtures/unity_cube.vulkan.apk",
    ];

    for path in &paths {
        let bytes = fs::read(path)
            .or_else(|_| fs::read(format!("../../{}", path)))
            .unwrap_or_else(|_| panic!("Failed to read {}", path));

        let installed = pms.install_apk(&bytes).expect("Install APK");
        assert!(!installed.package_name.is_empty());

        let retrieved = pms.get_package_info(&installed.package_name, GET_ACTIVITIES, 0);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().package_name, installed.package_name);
    }

    assert!(pms.package_count() >= 2);
}
