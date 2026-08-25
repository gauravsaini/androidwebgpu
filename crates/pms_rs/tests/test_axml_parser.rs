//! Test Binary Android XML (`AndroidManifest.xml` / AXML) Parser.

use pms_rs::axml::*;
use pms_rs::types::GET_ACTIVITIES;
use pms_rs::PackageManagerService;
use std::fs;

// Helper to build synthetic AXML buffer
fn build_synthetic_axml(
    package_name: &str,
    version_code: i32,
    version_name: &str,
    min_sdk: i32,
    target_sdk: i32,
    activity_name: &str,
    action: &str,
    category: &str,
    permission: &str,
) -> Vec<u8> {
    let strings = vec![
        "manifest".to_string(),
        "package".to_string(),
        "versionCode".to_string(),
        "versionName".to_string(),
        package_name.to_string(),
        version_name.to_string(),
        "uses-sdk".to_string(),
        "minSdkVersion".to_string(),
        "targetSdkVersion".to_string(),
        "uses-permission".to_string(),
        "name".to_string(),
        permission.to_string(),
        "application".to_string(),
        "label".to_string(),
        "icon".to_string(),
        "hasCode".to_string(),
        "activity".to_string(),
        activity_name.to_string(),
        "exported".to_string(),
        "intent-filter".to_string(),
        "action".to_string(),
        action.to_string(),
        "category".to_string(),
        category.to_string(),
    ];

    // Build String Pool Chunk (UTF-8)
    let mut str_data = Vec::new();
    let mut str_offsets = Vec::new();
    for s in &strings {
        str_offsets.push(str_data.len() as u32);
        let bytes = s.as_bytes();
        str_data.push(bytes.len() as u8); // UTF-16 len
        str_data.push(bytes.len() as u8); // UTF-8 len
        str_data.extend_from_slice(bytes);
        str_data.push(0); // null terminator
    }
    // Pad string data to 4-byte boundary
    while str_data.len() % 4 != 0 {
        str_data.push(0);
    }

    let header_size = 28u16;
    let strings_start = 28 + (strings.len() * 4) as u32;
    let pool_size = strings_start + str_data.len() as u32;

    let mut string_pool_chunk = Vec::new();
    string_pool_chunk.extend_from_slice(&RES_STRING_POOL_TYPE.to_le_bytes());
    string_pool_chunk.extend_from_slice(&header_size.to_le_bytes());
    string_pool_chunk.extend_from_slice(&pool_size.to_le_bytes());
    string_pool_chunk.extend_from_slice(&(strings.len() as u32).to_le_bytes()); // string_count
    string_pool_chunk.extend_from_slice(&0u32.to_le_bytes()); // style_count
    string_pool_chunk.extend_from_slice(&0x00000100u32.to_le_bytes()); // UTF-8 flag
    string_pool_chunk.extend_from_slice(&strings_start.to_le_bytes());
    string_pool_chunk.extend_from_slice(&0u32.to_le_bytes()); // styles_start
    for off in str_offsets {
        string_pool_chunk.extend_from_slice(&off.to_le_bytes());
    }
    string_pool_chunk.extend_from_slice(&str_data);

    // Build Resource Map Chunk
    let mut res_map_chunk = Vec::new();
    let res_ids = vec![
        0u32, // manifest
        0u32, // package
        ATTR_VERSION_CODE,
        ATTR_VERSION_NAME,
        0u32, // package_name
        0u32, // version_name
        0u32, // uses-sdk
        ATTR_MIN_SDK_VERSION,
        ATTR_TARGET_SDK_VERSION,
        0u32, // uses-permission
        ATTR_NAME,
        0u32, // permission
        0u32, // application
        ATTR_LABEL,
        ATTR_ICON,
        ATTR_HAS_CODE,
        0u32, // activity
        0u32, // activity_name
        ATTR_EXPORTED,
        0u32, // intent-filter
        0u32, // action
        0u32, // action_name
        0u32, // category
        0u32, // category_name
    ];
    let res_map_header_size = 8u16;
    let res_map_size = 8 + (res_ids.len() * 4) as u32;
    res_map_chunk.extend_from_slice(&RES_XML_RESOURCE_MAP_TYPE.to_le_bytes());
    res_map_chunk.extend_from_slice(&res_map_header_size.to_le_bytes());
    res_map_chunk.extend_from_slice(&res_map_size.to_le_bytes());
    for rid in res_ids {
        res_map_chunk.extend_from_slice(&rid.to_le_bytes());
    }

    // Helper to build Start Element Chunk
    fn make_start_element(
        tag_idx: u32,
        attrs: Vec<(u32, u32, u8, u32)>, // (name_idx, raw_idx, type, data)
    ) -> Vec<u8> {
        let mut chunk = Vec::new();
        let attr_count = attrs.len() as u16;
        let chunk_size = 36 + (attr_count as u32 * 20);

        chunk.extend_from_slice(&RES_XML_START_ELEMENT_TYPE.to_le_bytes());
        chunk.extend_from_slice(&16u16.to_le_bytes()); // header size
        chunk.extend_from_slice(&chunk_size.to_le_bytes());
        chunk.extend_from_slice(&1u32.to_le_bytes()); // line number
        chunk.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // comment
        chunk.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // ns
        chunk.extend_from_slice(&tag_idx.to_le_bytes());
        chunk.extend_from_slice(&20u16.to_le_bytes()); // attr start
        chunk.extend_from_slice(&20u16.to_le_bytes()); // attr size
        chunk.extend_from_slice(&attr_count.to_le_bytes());
        chunk.extend_from_slice(&0u16.to_le_bytes()); // id
        chunk.extend_from_slice(&0u16.to_le_bytes()); // class
        chunk.extend_from_slice(&0u16.to_le_bytes()); // style

        for (name_idx, raw_idx, data_type, data) in attrs {
            chunk.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // ns
            chunk.extend_from_slice(&name_idx.to_le_bytes());
            chunk.extend_from_slice(&raw_idx.to_le_bytes());
            chunk.extend_from_slice(&8u16.to_le_bytes()); // typed size
            chunk.push(0); // res0
            chunk.push(data_type);
            chunk.extend_from_slice(&data.to_le_bytes());
        }
        chunk
    }

    // Helper to build End Element Chunk
    fn make_end_element(tag_idx: u32) -> Vec<u8> {
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&RES_XML_END_ELEMENT_TYPE.to_le_bytes());
        chunk.extend_from_slice(&16u16.to_le_bytes());
        chunk.extend_from_slice(&24u32.to_le_bytes());
        chunk.extend_from_slice(&1u32.to_le_bytes());
        chunk.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        chunk.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        chunk.extend_from_slice(&tag_idx.to_le_bytes());
        chunk
    }

    let mut body = Vec::new();

    // 1. <manifest package="pkg" versionCode=123 versionName="1.0">
    body.extend(make_start_element(
        0, // manifest
        vec![
            (1, 4, TYPE_STRING, 4),                     // package = package_name
            (2, 0xFFFFFFFF, TYPE_INT_DEC, version_code as u32), // versionCode
            (3, 5, TYPE_STRING, 5),                     // versionName = version_name
        ],
    ));

    // 2. <uses-sdk minSdkVersion=min targetSdkVersion=target>
    body.extend(make_start_element(
        6, // uses-sdk
        vec![
            (7, 0xFFFFFFFF, TYPE_INT_DEC, min_sdk as u32),
            (8, 0xFFFFFFFF, TYPE_INT_DEC, target_sdk as u32),
        ],
    ));
    body.extend(make_end_element(6));

    // 3. <uses-permission name=permission>
    body.extend(make_start_element(
        9, // uses-permission
        vec![(10, 11, TYPE_STRING, 11)],
    ));
    body.extend(make_end_element(9));

    // 4. <application hasCode=true>
    body.extend(make_start_element(
        12, // application
        vec![(15, 0xFFFFFFFF, TYPE_INT_BOOLEAN, 1)],
    ));

    // 5. <activity name=activity_name exported=true>
    body.extend(make_start_element(
        16, // activity
        vec![
            (10, 17, TYPE_STRING, 17), // name = activity_name
            (18, 0xFFFFFFFF, TYPE_INT_BOOLEAN, 1), // exported = true
        ],
    ));

    // 6. <intent-filter>
    body.extend(make_start_element(19, vec![]));

    // 7. <action name=action>
    body.extend(make_start_element(
        20, // action
        vec![(10, 21, TYPE_STRING, 21)],
    ));
    body.extend(make_end_element(20));

    // 8. <category name=category>
    body.extend(make_start_element(
        22, // category
        vec![(10, 23, TYPE_STRING, 23)],
    ));
    body.extend(make_end_element(22));

    body.extend(make_end_element(19)); // </intent-filter>
    body.extend(make_end_element(16)); // </activity>
    body.extend(make_end_element(12)); // </application>
    body.extend(make_end_element(0));  // </manifest>

    let total_file_size = 8 + string_pool_chunk.len() + res_map_chunk.len() + body.len();
    let mut file = Vec::new();
    file.extend_from_slice(&RES_XML_TYPE.to_le_bytes());
    file.extend_from_slice(&8u16.to_le_bytes());
    file.extend_from_slice(&(total_file_size as u32).to_le_bytes());
    file.extend_from_slice(&string_pool_chunk);
    file.extend_from_slice(&res_map_chunk);
    file.extend_from_slice(&body);

    file
}

#[test]
fn test_synthetic_axml_parsing() {
    let axml_bytes = build_synthetic_axml(
        "com.androidwebgpu.demo",
        42,
        "1.2.3",
        26,
        33,
        ".MainActivity",
        "android.intent.action.MAIN",
        "android.intent.category.LAUNCHER",
        "android.permission.INTERNET",
    );

    let parsed = AxmlParser::parse(&axml_bytes).expect("AXML parsing must succeed");
    assert_eq!(parsed.package_name, "com.androidwebgpu.demo");
    assert_eq!(parsed.version_code, 42);
    assert_eq!(parsed.version_name.as_deref(), Some("1.2.3"));
    assert_eq!(parsed.min_sdk_version, 26);
    assert_eq!(parsed.target_sdk_version, 33);
    assert!(parsed.has_code);
    assert_eq!(parsed.uses_permissions, vec!["android.permission.INTERNET".to_string()]);

    assert_eq!(parsed.activities.len(), 1);
    let act = &parsed.activities[0];
    assert_eq!(act.name, "com.androidwebgpu.demo.MainActivity");
    assert!(act.exported);
    assert_eq!(act.intent_filters.len(), 1);
    let filter = &act.intent_filters[0];
    assert!(filter.actions.contains(&"android.intent.action.MAIN".to_string()));
    assert!(filter.categories.contains(&"android.intent.category.LAUNCHER".to_string()));

    let pkg_info = parsed.to_package_info();
    assert_eq!(pkg_info.package_name, "com.androidwebgpu.demo");
    assert_eq!(pkg_info.version_code, 42);
    assert_eq!(pkg_info.activities.len(), 1);
}

#[test]
fn test_real_apk_manifest_parsing() {
    let apk_paths = [
        "fixtures/godot_gles2.apk",
        "fixtures/unity_cube.apk",
        "fixtures/unity_cube.vulkan.apk",
    ];

    let pms = PackageManagerService::new();

    for path in &apk_paths {
        let bytes = fs::read(path)
            .or_else(|_| fs::read(format!("../../{}", path)))
            .unwrap_or_else(|_| panic!("Failed to read {}", path));

        let info = pms.install_apk(&bytes).expect("Install APK must succeed");
        assert!(!info.package_name.is_empty());
        assert!(info.application_info.is_some());

        let app = info.application_info.unwrap();
        assert_eq!(app.package_name, info.package_name);
        assert!(app.target_sdk_version >= 1);

        let queried = pms.get_package_info(&info.package_name, GET_ACTIVITIES, 0);
        assert!(queried.is_some());
    }
}

#[test]
fn test_axml_error_handling() {
    // Too short
    assert!(AxmlParser::parse(&[0x03, 0x00]).is_err());

    // Invalid Magic
    let mut bad_magic = vec![0x00; 16];
    bad_magic[0] = 0x02; // RES_TABLE_TYPE instead of RES_XML_TYPE
    assert!(matches!(
        AxmlParser::parse(&bad_magic),
        Err(AxmlError::InvalidMagic { .. })
    ));
}
