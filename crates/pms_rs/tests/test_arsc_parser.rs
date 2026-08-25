//! Test Binary `resources.arsc` Table Parser and Resource Resolver.

use pms_rs::arsc::*;

// Helper to build a valid synthetic resources.arsc binary buffer
fn build_synthetic_arsc(
    pkg_id: u32,
    pkg_name: &str,
    strings: Vec<&str>,
    type_strings: Vec<&str>,
    key_strings: Vec<&str>,
    entries: Vec<(u8, u16, u32, u8, u32)>, // (type_id, entry_idx, key_idx, data_type, data)
) -> Vec<u8> {
    // 1. Global String Pool (UTF-8)
    let mut str_data = Vec::new();
    let mut str_offsets = Vec::new();
    for s in &strings {
        str_offsets.push(str_data.len() as u32);
        let bytes = s.as_bytes();
        str_data.push(bytes.len() as u8); // UTF-16 len
        str_data.push(bytes.len() as u8); // UTF-8 len
        str_data.extend_from_slice(bytes);
        str_data.push(0); // null byte
    }
    while str_data.len() % 4 != 0 {
        str_data.push(0);
    }
    let str_pool_header_size = 28u16;
    let str_pool_strings_start = 28 + (strings.len() * 4) as u32;
    let str_pool_size = str_pool_strings_start + str_data.len() as u32;

    let mut global_str_pool = Vec::new();
    global_str_pool.extend_from_slice(&RES_STRING_POOL_TYPE.to_le_bytes());
    global_str_pool.extend_from_slice(&str_pool_header_size.to_le_bytes());
    global_str_pool.extend_from_slice(&str_pool_size.to_le_bytes());
    global_str_pool.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    global_str_pool.extend_from_slice(&0u32.to_le_bytes());
    global_str_pool.extend_from_slice(&0x00000100u32.to_le_bytes()); // UTF-8
    global_str_pool.extend_from_slice(&str_pool_strings_start.to_le_bytes());
    global_str_pool.extend_from_slice(&0u32.to_le_bytes());
    for off in str_offsets {
        global_str_pool.extend_from_slice(&off.to_le_bytes());
    }
    global_str_pool.extend_from_slice(&str_data);

    // 2. Type Strings Pool (in Package)
    let mut type_str_data = Vec::new();
    let mut type_str_offsets = Vec::new();
    for s in &type_strings {
        type_str_offsets.push(type_str_data.len() as u32);
        let bytes = s.as_bytes();
        type_str_data.push(bytes.len() as u8);
        type_str_data.push(bytes.len() as u8);
        type_str_data.extend_from_slice(bytes);
        type_str_data.push(0);
    }
    while type_str_data.len() % 4 != 0 {
        type_str_data.push(0);
    }
    let type_pool_strings_start = 28 + (type_strings.len() * 4) as u32;
    let type_pool_size = type_pool_strings_start + type_str_data.len() as u32;

    let mut type_str_pool = Vec::new();
    type_str_pool.extend_from_slice(&RES_STRING_POOL_TYPE.to_le_bytes());
    type_str_pool.extend_from_slice(&28u16.to_le_bytes());
    type_str_pool.extend_from_slice(&type_pool_size.to_le_bytes());
    type_str_pool.extend_from_slice(&(type_strings.len() as u32).to_le_bytes());
    type_str_pool.extend_from_slice(&0u32.to_le_bytes());
    type_str_pool.extend_from_slice(&0x00000100u32.to_le_bytes());
    type_str_pool.extend_from_slice(&type_pool_strings_start.to_le_bytes());
    type_str_pool.extend_from_slice(&0u32.to_le_bytes());
    for off in type_str_offsets {
        type_str_pool.extend_from_slice(&off.to_le_bytes());
    }
    type_str_pool.extend_from_slice(&type_str_data);

    // 3. Key Strings Pool (in Package)
    let mut key_str_data = Vec::new();
    let mut key_str_offsets = Vec::new();
    for s in &key_strings {
        key_str_offsets.push(key_str_data.len() as u32);
        let bytes = s.as_bytes();
        key_str_data.push(bytes.len() as u8);
        key_str_data.push(bytes.len() as u8);
        key_str_data.extend_from_slice(bytes);
        key_str_data.push(0);
    }
    while key_str_data.len() % 4 != 0 {
        key_str_data.push(0);
    }
    let key_pool_strings_start = 28 + (key_strings.len() * 4) as u32;
    let key_pool_size = key_pool_strings_start + key_str_data.len() as u32;

    let mut key_str_pool = Vec::new();
    key_str_pool.extend_from_slice(&RES_STRING_POOL_TYPE.to_le_bytes());
    key_str_pool.extend_from_slice(&28u16.to_le_bytes());
    key_str_pool.extend_from_slice(&key_pool_size.to_le_bytes());
    key_str_pool.extend_from_slice(&(key_strings.len() as u32).to_le_bytes());
    key_str_pool.extend_from_slice(&0u32.to_le_bytes());
    key_str_pool.extend_from_slice(&0x00000100u32.to_le_bytes());
    key_str_pool.extend_from_slice(&key_pool_strings_start.to_le_bytes());
    key_str_pool.extend_from_slice(&0u32.to_le_bytes());
    for off in key_str_offsets {
        key_str_pool.extend_from_slice(&off.to_le_bytes());
    }
    key_str_pool.extend_from_slice(&key_str_data);

    // 4. Type Spec and Type Entries
    let mut type_chunks = Vec::new();
    let mut entries_by_type: std::collections::HashMap<u8, Vec<(u16, u32, u8, u32)>> =
        std::collections::HashMap::new();
    for (tid, eidx, kidx, dtype, data) in entries {
        entries_by_type
            .entry(tid)
            .or_default()
            .push((eidx, kidx, dtype, data));
    }

    for (&tid, t_entries) in &entries_by_type {
        let max_eidx = t_entries.iter().map(|e| e.0).max().unwrap_or(0) as usize + 1;

        // Type Spec Chunk (0x0202)
        let spec_size = 16 + (max_eidx * 4) as u32;
        type_chunks.extend_from_slice(&RES_TABLE_TYPE_SPEC_TYPE.to_le_bytes());
        type_chunks.extend_from_slice(&16u16.to_le_bytes());
        type_chunks.extend_from_slice(&spec_size.to_le_bytes());
        type_chunks.push(tid); // id
        type_chunks.push(0);   // res0
        type_chunks.extend_from_slice(&0u16.to_le_bytes()); // res1
        type_chunks.extend_from_slice(&(max_eidx as u32).to_le_bytes());
        for _ in 0..max_eidx {
            type_chunks.extend_from_slice(&0u32.to_le_bytes()); // config flags
        }

        // Type Entry Chunk (0x0201)
        let header_size = 56u16; // 20 bytes header + 36 bytes config
        let entries_start = (header_size as usize) + (max_eidx * 4);

        let mut entry_payload = Vec::new();
        let mut entry_offsets = vec![NO_ENTRY; max_eidx];

        for (eidx, kidx, dtype, data) in t_entries {
            let cur_off = entry_payload.len() as u32;
            entry_offsets[*eidx as usize] = cur_off;

            // Simple Entry (8 bytes entry header + 8 bytes Res_value = 16 bytes)
            entry_payload.extend_from_slice(&8u16.to_le_bytes()); // size
            entry_payload.extend_from_slice(&0u16.to_le_bytes()); // flags (simple)
            entry_payload.extend_from_slice(&kidx.to_le_bytes()); // key index

            // Res_value (8 bytes)
            entry_payload.extend_from_slice(&8u16.to_le_bytes()); // size
            entry_payload.push(0);                               // res0
            entry_payload.push(*dtype);                          // data_type
            entry_payload.extend_from_slice(&data.to_le_bytes()); // data
        }

        let type_chunk_size = entries_start + entry_payload.len();
        type_chunks.extend_from_slice(&RES_TABLE_TYPE_TYPE.to_le_bytes());
        type_chunks.extend_from_slice(&header_size.to_le_bytes());
        type_chunks.extend_from_slice(&(type_chunk_size as u32).to_le_bytes());
        type_chunks.push(tid); // id
        type_chunks.push(0);   // flags
        type_chunks.extend_from_slice(&0u16.to_le_bytes()); // reserved
        type_chunks.extend_from_slice(&(max_eidx as u32).to_le_bytes());
        type_chunks.extend_from_slice(&(entries_start as u32).to_le_bytes());
        // 36 bytes config zeros
        type_chunks.extend_from_slice(&[0u8; 36]);
        // Offsets
        for off in entry_offsets {
            type_chunks.extend_from_slice(&off.to_le_bytes());
        }
        // Entry payload
        type_chunks.extend_from_slice(&entry_payload);
    }

    // 5. Package Chunk (0x0200)
    let pkg_header_size = 288u16;
    let type_strings_offset = pkg_header_size as u32;
    let key_strings_offset = type_strings_offset + type_str_pool.len() as u32;
    let pkg_chunk_size = key_strings_offset + key_str_pool.len() as u32 + type_chunks.len() as u32;

    let mut pkg_chunk = Vec::new();
    pkg_chunk.extend_from_slice(&RES_TABLE_PACKAGE_TYPE.to_le_bytes());
    pkg_chunk.extend_from_slice(&pkg_header_size.to_le_bytes());
    pkg_chunk.extend_from_slice(&pkg_chunk_size.to_le_bytes());
    pkg_chunk.extend_from_slice(&pkg_id.to_le_bytes());

    // Package Name (128 UTF-16 chars = 256 bytes)
    let mut name_u16 = [0u16; 128];
    for (i, c) in pkg_name.encode_utf16().take(127).enumerate() {
        name_u16[i] = c;
    }
    for ch in name_u16 {
        pkg_chunk.extend_from_slice(&ch.to_le_bytes());
    }

    pkg_chunk.extend_from_slice(&type_strings_offset.to_le_bytes());
    pkg_chunk.extend_from_slice(&0u32.to_le_bytes()); // last_public_type
    pkg_chunk.extend_from_slice(&key_strings_offset.to_le_bytes());
    pkg_chunk.extend_from_slice(&0u32.to_le_bytes()); // last_public_key
    pkg_chunk.extend_from_slice(&0u32.to_le_bytes()); // type_id_offset

    pkg_chunk.extend_from_slice(&type_str_pool);
    pkg_chunk.extend_from_slice(&key_str_pool);
    pkg_chunk.extend_from_slice(&type_chunks);

    // 6. Root Table Chunk (0x0002)
    let table_header_size = 12u16;
    let table_total_size = (table_header_size as usize) + global_str_pool.len() + pkg_chunk.len();

    let mut table_chunk = Vec::new();
    table_chunk.extend_from_slice(&RES_TABLE_TYPE.to_le_bytes());
    table_chunk.extend_from_slice(&table_header_size.to_le_bytes());
    table_chunk.extend_from_slice(&(table_total_size as u32).to_le_bytes());
    table_chunk.extend_from_slice(&1u32.to_le_bytes()); // package_count = 1

    table_chunk.extend_from_slice(&global_str_pool);
    table_chunk.extend_from_slice(&pkg_chunk);

    table_chunk
}

#[test]
fn test_synthetic_arsc_parsing_and_resolution() {
    let global_strings = vec![
        "AndroidWebGPU Super Game",
        "res/drawable/app_icon.png",
        "Landscape Mode",
    ];
    let type_strings = vec!["attr", "drawable", "layout", "string"];
    let key_strings = vec!["app_name", "app_icon", "game_mode", "primary_color"];

    // 0x7F = package 127
    // Type 4 = "string" (1-based index in type_strings)
    // Type 2 = "drawable"
    // Type 1 = "attr"
    let entries = vec![
        // String entry 0: app_name -> global string 0 ("AndroidWebGPU Super Game")
        (4u8, 0u16, 0u32, 3u8, 0u32),
        // Drawable entry 0: app_icon -> global string 1 ("res/drawable/app_icon.png")
        (2u8, 0u16, 1u32, 3u8, 1u32),
        // Integer entry 1: primary_color -> #FF5722
        (4u8, 1u16, 3u32, 17u8, 0x00FF5722u32),
    ];

    let arsc_bytes = build_synthetic_arsc(
        0x7F,
        "com.androidwebgpu.demo",
        global_strings,
        type_strings,
        key_strings,
        entries,
    );

    let table = ArscParser::parse(&arsc_bytes).expect("ARSC parsing must succeed");
    assert_eq!(table.global_strings.len(), 3);
    assert_eq!(table.packages.len(), 1);

    // Resolve by Resource ID: 0x7F 04 0000 (package 0x7f, type 4 = string, entry 0 = app_name)
    let app_name_id = 0x7F040000;
    let resolved_app_name = table.resolve_string(app_name_id);
    assert_eq!(
        resolved_app_name.as_deref(),
        Some("AndroidWebGPU Super Game")
    );

    // Resolve by Name: package="com.androidwebgpu.demo", type="string", key="app_name"
    let resolved_by_name = table.resolve_string_by_name("com.androidwebgpu.demo", "app_name");
    assert_eq!(
        resolved_by_name.as_deref(),
        Some("AndroidWebGPU Super Game")
    );

    // Resolve Drawable: 0x7F 02 0000 (package 0x7f, type 2 = drawable, entry 0 = app_icon)
    let icon_id = 0x7F020000;
    let resolved_icon = table.resolve_string(icon_id);
    assert_eq!(
        resolved_icon.as_deref(),
        Some("res/drawable/app_icon.png")
    );

    // Resolve Integer/Color resource
    let color_id = 0x7F040001;
    let resolved_color = table.resolve_resource(color_id);
    assert_eq!(
        resolved_color,
        Some(ResourceValue::Integer(0x00FF5722))
    );

    // String Reference resolving: "@string/app_name"
    let resolved_ref = table.resolve_string_ref("@string/app_name", "com.androidwebgpu.demo");
    assert_eq!(resolved_ref, "AndroidWebGPU Super Game");
}

#[test]
fn test_arsc_error_handling() {
    // Truncated Buffer
    assert!(ArscParser::parse(&[0x02, 0x00, 0x0C]).is_err());

    // Invalid Table Magic
    let bad_magic = vec![0x03, 0x00, 0x0C, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    assert!(matches!(
        ArscParser::parse(&bad_magic),
        Err(ArscError::InvalidMagic { .. })
    ));
}
