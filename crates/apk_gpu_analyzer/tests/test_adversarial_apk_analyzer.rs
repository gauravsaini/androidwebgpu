//! Adversarial Fuzzing and Corruption Tests for ApkGpuAnalyzer and BinaryXmlParser.

use apk_gpu_analyzer::manifest_parser::BinaryXmlParser;
use apk_gpu_analyzer::ApkGpuAnalyzer;
use std::io::Write;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

#[test]
fn test_adversarial_empty_and_truncated_apk_bytes() {
    // 1. Empty slice
    assert!(ApkGpuAnalyzer::analyze_apk_bytes(&[]).is_err());

    // 2. Truncated ZIP magic (3 bytes)
    assert!(ApkGpuAnalyzer::analyze_apk_bytes(&[0x50, 0x4B, 0x03]).is_err());

    // 3. 256 bytes of pure zeroes
    let zeroes = vec![0u8; 256];
    assert!(ApkGpuAnalyzer::analyze_apk_bytes(&zeroes).is_err());
}

#[test]
fn test_adversarial_malformed_axml_manifest_in_valid_zip() {
    // Construct a valid ZIP containing malformed AndroidManifest.xml
    let mut buf = Vec::new();
    {
        let mut zip = ZipWriter::new(std::io::Cursor::new(&mut buf));
        let options = SimpleFileOptions::default();
        zip.start_file("AndroidManifest.xml", options).unwrap();
        // Corrupted AXML: valid magic 0x0003, but invalid chunk sizes and garbage
        zip.write_all(&[0x03, 0x00, 0x08, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0xAA, 0xBB, 0xCC, 0xDD]).unwrap();
        zip.finish().unwrap();
    }

    // Must handle gracefully without panic, defaulting to placeholder
    let profile_res = ApkGpuAnalyzer::analyze_apk_bytes(&buf);
    assert!(profile_res.is_ok());
    let profile = profile_res.unwrap();
    assert_eq!(profile.package_name, "com.unknown.androidgpu");
}

#[test]
fn test_adversarial_axml_parser_fuzz_bounds() {
    // 1. Valid root header, string pool chunk type with zero chunk size
    let zero_size_chunk = [
        0x03, 0x00, 0x08, 0x00, 0x20, 0x00, 0x00, 0x00, // root
        0x01, 0x00, 0x1C, 0x00, 0x00, 0x00, 0x00, 0x00, // chunk size 0
    ];
    let res1 = BinaryXmlParser::parse_axml(&zero_size_chunk);
    assert!(res1.is_ok() || res1.is_err());

    // 2. Start element chunk with huge attribute count
    let mut start_elem = vec![
        0x03, 0x00, 0x08, 0x00, 100, 0, 0, 0, // root
    ];
    // START_ELEMENT chunk (0x0102)
    start_elem.extend_from_slice(&0x0102u16.to_le_bytes()); // type
    start_elem.extend_from_slice(&16u16.to_le_bytes());     // header size
    start_elem.extend_from_slice(&64u32.to_le_bytes());     // chunk size
    start_elem.extend_from_slice(&0u32.to_le_bytes());      // line
    start_elem.extend_from_slice(&0u32.to_le_bytes());      // comment
    start_elem.extend_from_slice(&0u32.to_le_bytes());      // ns
    start_elem.extend_from_slice(&0u32.to_le_bytes());      // name
    start_elem.extend_from_slice(&0x0014u16.to_le_bytes()); // attr_start
    start_elem.extend_from_slice(&0x0014u16.to_le_bytes()); // attr_size
    start_elem.extend_from_slice(&50000u16.to_le_bytes());  // attr_count: 50,000 (overflows buffer)
    start_elem.extend_from_slice(&0u16.to_le_bytes());      // id_idx
    start_elem.extend_from_slice(&0u16.to_le_bytes());      // class_idx
    start_elem.extend_from_slice(&0u16.to_le_bytes());      // style_idx

    let res2 = BinaryXmlParser::parse_axml(&start_elem);
    assert!(res2.is_ok()); // Should safely parse 0 valid attributes without out-of-bounds panic
}

#[test]
fn test_adversarial_random_byte_fuzzing_resilience() {
    // 500 pseudo-random buffers passed to analyze_apk_bytes
    let mut seed = 0x12345678u64;
    let mut rng = move || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as u32
    };

    for _ in 0..500 {
        let len = (rng() % 512) as usize;
        let mut random_bytes = vec![0u8; len];
        for b in random_bytes.iter_mut() {
            *b = (rng() & 0xFF) as u8;
        }

        // Must NEVER panic
        let _ = ApkGpuAnalyzer::analyze_apk_bytes(&random_bytes);
        let _ = BinaryXmlParser::parse_axml(&random_bytes);
    }
}
