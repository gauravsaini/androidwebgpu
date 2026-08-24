use crate::manifest_parser::BinaryXmlParser;
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};
use zip::ZipArchive;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EngineType {
    Unity,
    UnrealEngine,
    Godot,
    CustomNativeGles,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApkGpuProfile {
    pub package_name: String,
    pub min_gles_version: u32, // e.g. 0x00020000 (2.0) or 0x00030000 (3.0)
    pub requires_vulkan: bool,
    pub engine: EngineType,
    pub supported_texture_formats: Vec<String>,
    pub required_extensions: Vec<String>,
    pub native_libraries: Vec<String>,
}

pub struct ApkGpuAnalyzer;

impl ApkGpuAnalyzer {
    pub fn analyze_native_libs(lib_names: &[&str]) -> EngineType {
        for name in lib_names {
            if name.contains("libunity") || name.contains("libmain.so") {
                return EngineType::Unity;
            }
            if name.contains("libUE4") || name.contains("libUnreal") {
                return EngineType::UnrealEngine;
            }
            if name.contains("libgodot_android") || name.contains("libgodot") {
                return EngineType::Godot;
            }
            if name.contains("libGLES") || name.contains("libnative-lib") {
                return EngineType::CustomNativeGles;
            }
        }
        EngineType::Unknown
    }

    pub fn inspect_manifest(
        package_name: &str,
        gl_es_version: u32,
        uses_features: &[&str],
        lib_names: &[&str],
    ) -> ApkGpuProfile {
        let engine = Self::analyze_native_libs(lib_names);
        let requires_vulkan = uses_features.iter().any(|f| f.contains("vulkan"));
        let mut extensions = Vec::new();
        if gl_es_version >= 0x00030000 {
            extensions.push("GL_OES_texture_float".to_string());
            extensions.push("GL_OES_packed_depth_stencil".to_string());
        }

        ApkGpuProfile {
            package_name: package_name.to_string(),
            min_gles_version: gl_es_version,
            requires_vulkan,
            engine,
            supported_texture_formats: vec!["ETC2".to_string(), "ASTC".to_string(), "RGBA8".to_string()],
            required_extensions: extensions,
            native_libraries: lib_names.iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn analyze_apk_bytes(bytes: &[u8]) -> Result<ApkGpuProfile, String> {
        let reader = Cursor::new(bytes);
        let mut zip = ZipArchive::new(reader).map_err(|e| format!("Zip error: {:?}", e))?;

        let mut manifest_bytes = Vec::new();
        let mut native_libs = Vec::new();
        let mut has_astc = false;
        let mut has_etc2 = false;

        for i in 0..zip.len() {
            let mut file = zip.by_index(i).map_err(|e| format!("Zip entry error: {:?}", e))?;
            let name = file.name().to_string();

            if name == "AndroidManifest.xml" {
                let _ = file.read_to_end(&mut manifest_bytes);
            } else if name.starts_with("lib/") && name.ends_with(".so") {
                if let Some(filename) = name.split('/').last() {
                    native_libs.push(filename.to_string());
                }
            } else if name.ends_with(".astc") {
                has_astc = true;
            } else if name.ends_with(".pkm") || name.ends_with(".etc2") {
                has_etc2 = true;
            }
        }

        let mut manifest_info = if !manifest_bytes.is_empty() {
            BinaryXmlParser::parse_axml(&manifest_bytes).unwrap_or_default()
        } else {
            Default::default()
        };

        if manifest_info.package_name.is_empty() {
            manifest_info.package_name = "com.unknown.androidgpu".to_string();
        }

        let lib_refs: Vec<&str> = native_libs.iter().map(|s| s.as_str()).collect();
        let feature_refs: Vec<&str> = manifest_info.uses_features.iter().map(|s| s.as_str()).collect();

        let mut profile = Self::inspect_manifest(
            &manifest_info.package_name,
            manifest_info.min_gles_version,
            &feature_refs,
            &lib_refs,
        );

        if has_astc && !profile.supported_texture_formats.contains(&"ASTC".to_string()) {
            profile.supported_texture_formats.push("ASTC".to_string());
        }
        if has_etc2 && !profile.supported_texture_formats.contains(&"ETC2".to_string()) {
            profile.supported_texture_formats.push("ETC2".to_string());
        }

        Ok(profile)
    }
}
