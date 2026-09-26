//! U10 `apk-pipeline` — analyzer conformance adapter (PURE).
//!
//! Maps the existing `apk_gpu_analyzer` crate's public output onto the frozen
//! Path N contracts ([`ApkMeta`], [`EngineKind`], [`ApkError`]).
//!
//! No detection logic lives here: engine classification is owned by the
//! analyzer crate. This unit only translates types and classifies the
//! analyzer's real failure modes into the exact contract variants:
//!
//! - Unreadable / corrupt zip container            -> [`ApkError::BadZip`]
//! - Manifest entry present but corrupt binary data -> [`ApkError::BadManifest`]
//! - Valid zip with no APK structure at all        -> [`ApkError::Unsupported`]
//!
//! Purity: deterministic, no I/O, no clock, no threads, no hidden state.
//! Corrupt input yields typed errors, never panics.

use apk_gpu_analyzer::{ApkGpuAnalyzer, BinaryXmlParser, EngineType};
use pathn_contracts::machine::{ApkError, ApkMeta, EngineKind};

/// Analyze raw APK bytes into the frozen [`ApkMeta`] contract.
pub fn analyze(apk: &[u8]) -> Result<ApkMeta, ApkError> {
    let profile = match ApkGpuAnalyzer::analyze_apk_bytes(apk) {
        Ok(profile) => profile,
        Err(message) => return Err(map_analyzer_error(&message)),
    };
    // A corrupt manifest is more specific than "not an APK": report it first.
    if let Some(reason) = corrupt_manifest_reason(apk) {
        return Err(ApkError::BadManifest(reason));
    }
    if !has_apk_hallmarks(apk) {
        return Err(ApkError::Unsupported);
    }
    Ok(ApkMeta {
        package: profile.package_name,
        engine: map_engine(profile.engine),
        gles_version: decode_gles(profile.min_gles_version),
    })
}

/// The analyzer reports failures as formatted strings; classify them into the
/// exact contract variants.
fn map_analyzer_error(message: &str) -> ApkError {
    // `analyze_apk_bytes` fails only on zip-container integrity problems
    // ("Zip error: ..." / "Zip entry error: ..."), hence BadZip. Catch any
    // future analyzer error format loudly in debug builds instead of silently
    // misclassifying it.
    debug_assert!(
        message.contains("Zip error") || message.contains("Zip entry error"),
        "unrecognized apk analyzer error: {message}"
    );
    ApkError::BadZip
}

/// [`EngineType`] (analyzer) -> [`EngineKind`] (frozen contract).
///
/// The contract has no `Native`/`Unknown` variants: both the analyzer's
/// `CustomNativeGles` and `Unknown` map to [`EngineKind::Other`].
fn map_engine(engine: EngineType) -> EngineKind {
    match engine {
        EngineType::Unity => EngineKind::Unity,
        EngineType::UnrealEngine => EngineKind::Unreal,
        EngineType::Godot => EngineKind::Godot,
        EngineType::CustomNativeGles | EngineType::Unknown => EngineKind::Other,
    }
}

/// Decode Android's packed GLES version (`0xMMmm0000`) into `(major, minor)`.
///
/// `(0, 0)` is the honest encoding of "no version found" — the fixtures carry
/// plaintext XML manifests which the analyzer's binary-AXML parser does not
/// read, so no version is reported for them.
fn decode_gles(packed: u32) -> (u8, u8) {
    (((packed >> 16) & 0xFF) as u8, (packed & 0xFF) as u8)
}

/// True when the raw bytes contain at least one APK structural hallmark.
///
/// ZIP stores entry names uncompressed, so a byte scan is a sound signal here.
/// This only runs after the analyzer has already accepted the bytes as a zip,
/// so matches come from real entry names, not random data.
fn has_apk_hallmarks(apk: &[u8]) -> bool {
    const HALLMARKS: [&[u8]; 5] = [
        b"AndroidManifest.xml",
        b"classes.dex",
        b"lib/",
        b"assets/",
        b"res/",
    ];
    HALLMARKS
        .iter()
        .any(|mark| find_subslice(apk, mark).is_some())
}

/// Detect a corrupt `AndroidManifest.xml` entry.
///
/// The analyzer tolerates manifest problems by falling back to defaults, so a
/// genuinely corrupt manifest would otherwise degrade silently into
/// `package = "com.unknown.androidgpu"`. This scan walks the zip local file
/// headers (no new dependencies: entry names are stored uncompressed), and for
/// a stored (uncompressed) manifest entry re-runs the analyzer's own public
/// [`BinaryXmlParser`] over the entry bytes:
///
/// - valid binary AXML  -> parses, no error (real-world APKs)
/// - plaintext XML      -> tolerated, no error (matches the repo fixtures,
///   which the existing `apk_real` test expects to analyze successfully)
/// - empty entry        -> tolerated, no error (analyzer default applies)
/// - anything else      -> [`ApkError::BadManifest`] with the parser's reason
///
/// Deflated entries cannot be inspected without a decompressor; those are
/// trusted to the analyzer. Every index is bounds-checked: corrupt input can
/// never panic here.
fn corrupt_manifest_reason(apk: &[u8]) -> Option<String> {
    let mut cursor = 0;
    while cursor + 30 <= apk.len() {
        let rel = find_subslice(&apk[cursor..], b"PK\x03\x04")?;
        let header = cursor + rel;
        // A local header needs 30 bytes; a signature closer to the end than
        // that cannot start a valid header (nor can any later one).
        if header + 30 > apk.len() {
            break;
        }
        let name_len = u16::from_le_bytes([apk[header + 26], apk[header + 27]]) as usize;
        let extra_len = u16::from_le_bytes([apk[header + 28], apk[header + 29]]) as usize;
        let method = u16::from_le_bytes([apk[header + 8], apk[header + 9]]);
        let comp_size = u32::from_le_bytes([
            apk[header + 18],
            apk[header + 19],
            apk[header + 20],
            apk[header + 21],
        ]) as usize;
        let name_start = header + 30;
        let name_end = name_start.saturating_add(name_len);
        let data_start = name_end.saturating_add(extra_len);
        let data_end = data_start.saturating_add(comp_size);
        if name_end > apk.len() || data_end > apk.len() {
            cursor = header + 4;
            continue;
        }
        if &apk[name_start..name_end] == b"AndroidManifest.xml"
            && method == 0 // stored: entry bytes are the manifest bytes
            && comp_size > 0
            && !apk[data_start..].starts_with(b"<?")
        {
            let entry = &apk[data_start..data_end];
            if let Err(reason) = BinaryXmlParser::parse_axml(entry) {
                return Some(reason);
            }
        }
        cursor = header + 4;
    }
    None
}

/// Byte-slice substring search.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Test-only minimal ZIP writer (STORE method, no compression).
    // Keeps conformance tests dependency-free and fully deterministic.
    // ------------------------------------------------------------------

    fn crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }
        !crc
    }

    /// Build a minimal valid zip from `(name, bytes)` entries, STORE method.
    fn zip_store(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in entries {
            let local_offset = out.len() as u32;
            let crc = crc32(data);
            let name_bytes = name.as_bytes();
            // Local file header.
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&0u16.to_le_bytes()); // method: stored
            out.extend_from_slice(&0u16.to_le_bytes()); // mod time
            out.extend_from_slice(&0u16.to_le_bytes()); // mod date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(data);
            // Central directory entry.
            central.extend_from_slice(b"PK\x01\x02");
            central.extend_from_slice(&20u16.to_le_bytes()); // version made by
            central.extend_from_slice(&20u16.to_le_bytes()); // version needed
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes()); // extra
            central.extend_from_slice(&0u16.to_le_bytes()); // comment
            central.extend_from_slice(&0u16.to_le_bytes()); // disk
            central.extend_from_slice(&0u16.to_le_bytes()); // int attrs
            central.extend_from_slice(&0u32.to_le_bytes()); // ext attrs
            central.extend_from_slice(&local_offset.to_le_bytes());
            central.extend_from_slice(name_bytes);
        }
        let central_offset = out.len() as u32;
        let central_size = central.len() as u32;
        out.extend_from_slice(&central);
        // End of central directory.
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&0u16.to_le_bytes()); // disk number
        out.extend_from_slice(&0u16.to_le_bytes()); // central dir disk
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&central_size.to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment len
        out
    }

    // ------------------------------------------------------------------
    // Conformance: real repo fixtures -> expected EngineKind.
    // Fixtures live at the repo root: ../../fixtures relative to this crate.
    // ------------------------------------------------------------------

    #[test]
    fn conform_unity_cube_fixture_maps_to_unity() {
        let bytes = include_bytes!("../../../fixtures/unity_cube.apk");
        let meta = analyze(bytes).expect("unity_cube.apk must analyze cleanly");
        assert_eq!(meta.engine, EngineKind::Unity);
        // Fixtures carry plaintext XML manifests, which the analyzer's
        // binary-AXML parser does not read: package falls back to the
        // analyzer's default and no GLES version is reported.
        assert_eq!(meta.package, "com.unknown.androidgpu");
        assert_eq!(meta.gles_version, (0, 0));
    }

    #[test]
    fn conform_godot_gles2_fixture_maps_to_godot() {
        let bytes = include_bytes!("../../../fixtures/godot_gles2.apk");
        let meta = analyze(bytes).expect("godot_gles2.apk must analyze cleanly");
        assert_eq!(meta.engine, EngineKind::Godot);
        assert_eq!(meta.package, "com.unknown.androidgpu");
        assert_eq!(meta.gles_version, (0, 0));
    }

    #[test]
    fn conform_non_apk_bytes_yield_bad_zip() {
        assert_eq!(
            analyze(b"this is definitely not a zip file"),
            Err(ApkError::BadZip)
        );
        assert_eq!(analyze(b""), Err(ApkError::BadZip));
        assert_eq!(analyze(&[0x50, 0x4B, 0x03]), Err(ApkError::BadZip)); // truncated header
    }

    #[test]
    fn conform_zip_without_apk_content_is_unsupported() {
        let not_an_apk = zip_store(&[("hello.txt", b"hello world")]);
        assert_eq!(analyze(&not_an_apk), Err(ApkError::Unsupported));
    }

    #[test]
    fn conform_corrupt_manifest_yields_bad_manifest() {
        // Binary garbage where the manifest should be: not valid AXML, not
        // plaintext XML. The analyzer would silently degrade this to defaults;
        // the adapter must surface it as a typed error instead.
        let garbage: Vec<u8> = vec![0xAB; 64];
        let apk = zip_store(&[
            ("AndroidManifest.xml", &garbage),
            ("lib/arm64-v8a/libunity.so", b"\x7fELF fake"),
        ]);
        match analyze(&apk) {
            Err(ApkError::BadManifest(reason)) => assert!(!reason.is_empty()),
            other => panic!("expected BadManifest, got {:?}", other),
        }
    }

    #[test]
    fn conform_plaintext_manifest_is_tolerated() {
        // Mirrors the real fixtures: plaintext XML manifest must not be
        // treated as corrupt (the existing apk_real test requires this).
        let apk = zip_store(&[
            ("AndroidManifest.xml", b"<?xml version=\"1.0\"?><manifest/>"),
            ("lib/arm64-v8a/libunity.so", b"\x7fELF fake"),
        ]);
        let meta = analyze(&apk).expect("plaintext manifest must be tolerated");
        assert_eq!(meta.engine, EngineKind::Unity);
    }

    #[test]
    fn conform_unreal_and_native_engines_map_correctly() {
        let unreal = zip_store(&[
            ("AndroidManifest.xml", b"<?xml version=\"1.0\"?><manifest/>"),
            ("lib/arm64-v8a/libUE4.so", b"\x7fELF fake"),
        ]);
        let meta = analyze(&unreal).expect("unreal apk must analyze");
        assert_eq!(meta.engine, EngineKind::Unreal);

        let native = zip_store(&[
            ("AndroidManifest.xml", b"<?xml version=\"1.0\"?><manifest/>"),
            ("lib/arm64-v8a/libnative-lib.so", b"\x7fELF fake"),
        ]);
        let meta = analyze(&native).expect("native apk must analyze");
        assert_eq!(meta.engine, EngineKind::Other);

        let unknown = zip_store(&[
            ("AndroidManifest.xml", b"<?xml version=\"1.0\"?><manifest/>"),
            ("assets/data.bin", b"data"),
        ]);
        let meta = analyze(&unknown).expect("unknown-engine apk must analyze");
        assert_eq!(meta.engine, EngineKind::Other);
    }

    #[test]
    fn conform_truncated_fake_header_cannot_panic() {
        let mut apk = zip_store(&[
            ("AndroidManifest.xml", b"<?xml version=\"1.0\"?><manifest/>"),
            ("lib/arm64-v8a/libunity.so", b"\x7fELF fake"),
        ]);
        // Forge an EOCD comment holding a fake local-header signature inside
        // the last bytes of the file: the header scanner must stop there,
        // never index out of bounds.
        let eocd = apk
            .windows(4)
            .rposition(|w| w == b"PK\x05\x06")
            .expect("eocd present");
        let comment = b"trailer PK\x03\x04";
        let comment_len_pos = eocd + 20;
        apk[comment_len_pos..comment_len_pos + 2]
            .copy_from_slice(&(comment.len() as u16).to_le_bytes());
        apk.extend_from_slice(comment);
        // Must not panic; the outcome is either Ok(meta) or a typed error.
        let _ = analyze(&apk);
    }

    // ------------------------------------------------------------------
    // Purity evidence: same bytes in -> same meta out, always.
    // ------------------------------------------------------------------

    #[test]
    fn determinism_analyze_same_bytes_same_meta() {
        let bytes = include_bytes!("../../../fixtures/unity_cube.apk");
        let first = analyze(bytes).expect("must analyze");
        for _ in 0..8 {
            assert_eq!(analyze(bytes), Ok(first.clone()));
        }
    }
}
