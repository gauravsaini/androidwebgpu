//! Test Intent Resolution Engine and Component Matching.

use pms_rs::types::*;
use pms_rs::PackageManagerService;

#[test]
fn test_standard_main_launcher_intent_resolution() {
    let pms = PackageManagerService::new();

    let act1 = ActivityInfo {
        name: "com.example.game.MainActivity".to_string(),
        package_name: "com.example.game".to_string(),
        label: Some("My Game".to_string()),
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
    };

    let act2 = ActivityInfo {
        name: "com.example.game.SettingsActivity".to_string(),
        package_name: "com.example.game".to_string(),
        label: Some("Settings".to_string()),
        exported: true,
        enabled: true,
        intent_filters: vec![IntentFilter {
            actions: vec!["android.intent.action.VIEW".to_string()],
            categories: vec!["android.intent.category.DEFAULT".to_string()],
            data_schemes: vec!["settings".to_string()],
            priority: 5,
        }],
        ..Default::default()
    };

    let pkg = PackageInfo {
        package_name: "com.example.game".to_string(),
        version_code: 1,
        version_name: Some("1.0".to_string()),
        application_info: Some(ApplicationInfo {
            package_name: "com.example.game".to_string(),
            name: Some("GameApp".to_string()),
            ..Default::default()
        }),
        activities: vec![act1.clone(), act2.clone()],
        ..Default::default()
    };

    pms.install_package_info(pkg, None);

    // 1. Resolve MAIN / LAUNCHER Intent
    let mut main_intent = Intent::new(Some("android.intent.action.MAIN"));
    main_intent.add_category("android.intent.category.LAUNCHER");

    let resolve_res = pms.resolve_intent(&main_intent, "", 0, 0);
    assert!(resolve_res.is_some());
    let res_info = resolve_res.unwrap();
    assert_eq!(
        res_info.activity_info.as_ref().unwrap().name,
        "com.example.game.MainActivity"
    );

    // 2. Query activities with MATCH_DEFAULT_ONLY
    let query_res = pms.query_intent_activities(&main_intent, "", MATCH_DEFAULT_ONLY, 0);
    assert_eq!(query_res.len(), 1);
    assert_eq!(
        query_res[0].activity_info.as_ref().unwrap().name,
        "com.example.game.MainActivity"
    );

    // 3. Resolve Custom Scheme Intent ("settings://")
    let mut view_intent = Intent::new(Some("android.intent.action.VIEW"));
    view_intent.data_uri = Some("settings://general".to_string());
    view_intent.add_category("android.intent.category.DEFAULT");

    let view_res = pms.resolve_intent(&view_intent, "", 0, 0);
    assert!(view_res.is_some());
    assert_eq!(
        view_res.unwrap().activity_info.as_ref().unwrap().name,
        "com.example.game.SettingsActivity"
    );

    // 4. Non-matching Intent
    let bad_intent = Intent::new(Some("android.intent.action.BATTERY_LOW"));
    assert!(pms.resolve_intent(&bad_intent, "", 0, 0).is_none());
}

#[test]
fn test_explicit_component_name_resolution() {
    let pms = PackageManagerService::new();

    let act = ActivityInfo {
        name: "com.androidwebgpu.arcade.ArcadeActivity".to_string(),
        package_name: "com.androidwebgpu.arcade".to_string(),
        label: Some("Arcade 3D".to_string()),
        exported: true,
        enabled: true,
        ..Default::default()
    };

    let pkg = PackageInfo {
        package_name: "com.androidwebgpu.arcade".to_string(),
        version_code: 100,
        activities: vec![act],
        ..Default::default()
    };

    pms.install_package_info(pkg, None);

    let comp = ComponentName::new("com.androidwebgpu.arcade", ".ArcadeActivity");
    let intent = Intent::with_component(comp.clone());

    let resolve_res = pms.resolve_intent(&intent, "", 0, 0);
    assert!(resolve_res.is_some());
    assert_eq!(
        resolve_res.unwrap().activity_info.unwrap().name,
        "com.androidwebgpu.arcade.ArcadeActivity"
    );

    // Query activity info directly
    let act_info = pms.get_activity_info(&comp, 0, 0);
    assert!(act_info.is_some());
    assert_eq!(act_info.unwrap().name, "com.androidwebgpu.arcade.ArcadeActivity");
}

#[test]
fn test_multi_package_priority_ranking() {
    let pms = PackageManagerService::new();

    // App 1 with priority 5
    let app1 = PackageInfo {
        package_name: "com.app.one".to_string(),
        version_code: 1,
        activities: vec![ActivityInfo {
            name: "com.app.one.ShareActivity".to_string(),
            package_name: "com.app.one".to_string(),
            exported: true,
            enabled: true,
            intent_filters: vec![IntentFilter {
                actions: vec!["android.intent.action.SEND".to_string()],
                categories: vec!["android.intent.category.DEFAULT".to_string()],
                data_schemes: vec![],
                priority: 5,
            }],
            ..Default::default()
        }],
        ..Default::default()
    };

    // App 2 with priority 20 (higher)
    let app2 = PackageInfo {
        package_name: "com.app.two".to_string(),
        version_code: 1,
        activities: vec![ActivityInfo {
            name: "com.app.two.FastShareActivity".to_string(),
            package_name: "com.app.two".to_string(),
            exported: true,
            enabled: true,
            intent_filters: vec![IntentFilter {
                actions: vec!["android.intent.action.SEND".to_string()],
                categories: vec!["android.intent.category.DEFAULT".to_string()],
                data_schemes: vec![],
                priority: 20,
            }],
            ..Default::default()
        }],
        ..Default::default()
    };

    pms.install_package_info(app1, None);
    pms.install_package_info(app2, None);

    let send_intent = Intent::new(Some("android.intent.action.SEND"));
    let activities = pms.query_intent_activities(&send_intent, "", 0, 0);
    assert_eq!(activities.len(), 2);
    // Highest priority first
    assert_eq!(
        activities[0].activity_info.as_ref().unwrap().name,
        "com.app.two.FastShareActivity"
    );
    assert_eq!(
        activities[1].activity_info.as_ref().unwrap().name,
        "com.app.one.ShareActivity"
    );

    // Best resolution must be App 2
    let best = pms.resolve_intent(&send_intent, "", 0, 0).unwrap();
    assert_eq!(
        best.activity_info.unwrap().name,
        "com.app.two.FastShareActivity"
    );

    // With target package restriction to app 1
    let mut restricted_intent = send_intent.clone();
    restricted_intent.package = Some("com.app.one".to_string());
    let restricted_res = pms.resolve_intent(&restricted_intent, "", 0, 0).unwrap();
    assert_eq!(
        restricted_res.activity_info.unwrap().name,
        "com.app.one.ShareActivity"
    );
}
