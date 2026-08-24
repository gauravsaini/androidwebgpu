pub mod analyzer;
pub mod manifest_parser;

pub use analyzer::{ApkGpuAnalyzer, ApkGpuProfile, EngineType};
pub use manifest_parser::{BinaryXmlParser, ParsedManifestInfo};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unity_detection() {
        let engine = ApkGpuAnalyzer::analyze_native_libs(&["libunity.so", "libmain.so"]);
        assert_eq!(engine, EngineType::Unity);
    }

    #[test]
    fn test_unreal_detection() {
        let engine = ApkGpuAnalyzer::analyze_native_libs(&["libUE4.so"]);
        assert_eq!(engine, EngineType::UnrealEngine);
    }

    #[test]
    fn test_godot_detection() {
        let engine = ApkGpuAnalyzer::analyze_native_libs(&["libgodot_android.so"]);
        assert_eq!(engine, EngineType::Godot);
    }

    #[test]
    fn test_manifest_inspect() {
        let profile = ApkGpuAnalyzer::inspect_manifest(
            "com.example.game",
            0x00030000,
            &["android.hardware.vulkan.level"],
            &["libunity.so"],
        );
        assert_eq!(profile.package_name, "com.example.game");
        assert_eq!(profile.min_gles_version, 0x00030000);
        assert!(profile.requires_vulkan);
        assert_eq!(profile.engine, EngineType::Unity);
        assert!(profile.required_extensions.contains(&"GL_OES_texture_float".to_string()));
    }
}
