//! Full Binary Android XML (`AndroidManifest.xml` / AXML) Parser.
//!
//! Decodes `RES_XML_TYPE` chunks, `RES_STRING_POOL_TYPE`, `RES_XML_RESOURCE_MAP_TYPE`,
//! `RES_XML_START_ELEMENT_TYPE`, `RES_XML_END_ELEMENT_TYPE`, `RES_XML_START_NAMESPACE_TYPE`,
//! `RES_XML_END_NAMESPACE_TYPE`, and `RES_XML_CDATA_TYPE`.

use crate::types::{
    ActivityInfo, ApplicationInfo, IntentFilter, PackageInfo, ReceiverInfo, ServiceInfo,
};
use std::collections::HashMap;
use thiserror::Error;

// -----------------------------------------------------------------------------
// AXML Chunk Type Constants
// -----------------------------------------------------------------------------

pub const RES_NULL_TYPE: u16 = 0x0000;
pub const RES_STRING_POOL_TYPE: u16 = 0x0001;
pub const RES_TABLE_TYPE: u16 = 0x0002;
pub const RES_XML_TYPE: u16 = 0x0003;
pub const RES_XML_FIRST_CHUNK_TYPE: u16 = 0x0100;
pub const RES_XML_START_NAMESPACE_TYPE: u16 = 0x0100;
pub const RES_XML_END_NAMESPACE_TYPE: u16 = 0x0101;
pub const RES_XML_START_ELEMENT_TYPE: u16 = 0x0102;
pub const RES_XML_END_ELEMENT_TYPE: u16 = 0x0103;
pub const RES_XML_CDATA_TYPE: u16 = 0x0104;
pub const RES_XML_RESOURCE_MAP_TYPE: u16 = 0x0180;

// Standard Android Attribute Resource IDs
pub const ATTR_LABEL: u32 = 0x01010001;
pub const ATTR_ICON: u32 = 0x01010002;
pub const ATTR_NAME: u32 = 0x01010003;
pub const ATTR_PERMISSION: u32 = 0x01010006;
pub const ATTR_EXPORTED: u32 = 0x01010010;
pub const ATTR_LAUNCH_MODE: u32 = 0x01010011;
pub const ATTR_VERSION_CODE: u32 = 0x0101001b;
pub const ATTR_VERSION_NAME: u32 = 0x0101001c;
pub const ATTR_THEME: u32 = 0x01010020;
pub const ATTR_SCHEME: u32 = 0x01010027;
pub const ATTR_HOST: u32 = 0x01010028;
pub const ATTR_MIME_TYPE: u32 = 0x01010026;
pub const ATTR_PRIORITY: u32 = 0x0101001c;
pub const ATTR_HAS_CODE: u32 = 0x0101000c;
pub const ATTR_MIN_SDK_VERSION: u32 = 0x0101020c;
pub const ATTR_TARGET_SDK_VERSION: u32 = 0x0101021b;
pub const ATTR_GLES_VERSION: u32 = 0x01010281;
pub const ATTR_REQUIRED: u32 = 0x0101028e;

// Typed Value Data Types
pub const TYPE_NULL: u8 = 0;
pub const TYPE_REFERENCE: u8 = 1;
pub const TYPE_ATTRIBUTE: u8 = 2;
pub const TYPE_STRING: u8 = 3;
pub const TYPE_FLOAT: u8 = 4;
pub const TYPE_DIMENSION: u8 = 5;
pub const TYPE_FRACTION: u8 = 6;
pub const TYPE_DYNAMIC_REFERENCE: u8 = 7;
pub const TYPE_DYNAMIC_ATTRIBUTE: u8 = 8;
pub const TYPE_INT_DEC: u8 = 16;
pub const TYPE_INT_HEX: u8 = 17;
pub const TYPE_INT_BOOLEAN: u8 = 18;

// -----------------------------------------------------------------------------
// Errors
// -----------------------------------------------------------------------------

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AxmlError {
    #[error("Buffer too short: expected at least {expected} bytes, got {actual}")]
    BufferTooShort { expected: usize, actual: usize },
    #[error("Invalid AXML root magic: expected 0x{expected:04x}, got 0x{actual:04x}")]
    InvalidMagic { expected: u16, actual: u16 },
    #[error("Malformed chunk header at offset {offset}: chunk size {size} exceeds buffer")]
    MalformedChunk { offset: usize, size: usize },
    #[error("Malformed string pool chunk: {0}")]
    MalformedStringPool(String),
    #[error("Invalid element structure: {0}")]
    InvalidElement(String),
}

// -----------------------------------------------------------------------------
// Attribute and Node Models
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct XmlAttribute {
    pub namespace: Option<String>,
    pub name: String,
    pub resource_id: Option<u32>,
    pub raw_value: Option<String>,
    pub data_type: u8,
    pub data: u32,
}

impl XmlAttribute {
    pub fn as_string<'a>(&'a self, pool: &'a [String]) -> Option<&'a str> {
        if let Some(raw) = &self.raw_value {
            Some(raw.as_str())
        } else if self.data_type == TYPE_STRING {
            pool.get(self.data as usize).map(|s| s.as_str())
        } else {
            None
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self.data_type {
            TYPE_INT_DEC | TYPE_INT_HEX | TYPE_REFERENCE => Some(self.data),
            TYPE_INT_BOOLEAN => Some(if self.data != 0 { 1 } else { 0 }),
            _ => self
                .raw_value
                .as_ref()
                .and_then(|s| s.parse::<u32>().ok()),
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self.data_type {
            TYPE_INT_DEC | TYPE_INT_HEX => Some(self.data as i32),
            TYPE_INT_BOOLEAN => Some(if self.data != 0 { 1 } else { 0 }),
            TYPE_REFERENCE => Some(self.data as i32),
            _ => self
                .raw_value
                .as_ref()
                .and_then(|s| s.parse::<i32>().ok()),
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self.data_type {
            TYPE_INT_BOOLEAN => Some(self.data != 0),
            TYPE_INT_DEC | TYPE_INT_HEX => Some(self.data != 0),
            _ => self.raw_value.as_ref().and_then(|s| match s.as_str() {
                "true" | "1" => Some(true),
                "false" | "0" => Some(false),
                _ => None,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct UsesFeatureInfo {
    pub name: String,
    pub gl_es_version: u32,
    pub required: bool,
}

// -----------------------------------------------------------------------------
// Parsed Manifest Intermediate Representation
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParsedAxmlManifest {
    pub package_name: String,
    pub version_code: i32,
    pub version_name: Option<String>,
    pub min_sdk_version: i32,
    pub target_sdk_version: i32,
    pub application_name: Option<String>,
    pub application_label: Option<String>,
    pub application_icon: u32,
    pub application_theme: u32,
    pub has_code: bool,
    pub activities: Vec<ActivityInfo>,
    pub services: Vec<ServiceInfo>,
    pub receivers: Vec<ReceiverInfo>,
    pub uses_permissions: Vec<String>,
    pub uses_features: Vec<UsesFeatureInfo>,
    pub meta_data: HashMap<String, String>,
}

impl ParsedAxmlManifest {
    pub fn to_package_info(&self) -> PackageInfo {
        let app_info = ApplicationInfo {
            package_name: self.package_name.clone(),
            name: self.application_name.clone(),
            label: self.application_label.clone(),
            icon: self.application_icon,
            target_sdk_version: self.target_sdk_version,
            min_sdk_version: self.min_sdk_version,
            flags: if self.has_code { 1 << 2 } else { 0 },
            data_dir: format!("/data/user/0/{}", self.package_name),
            source_dir: format!("/data/app/{}/base.apk", self.package_name),
            public_source_dir: format!("/data/app/{}/base.apk", self.package_name),
            native_library_dir: format!("/data/app/{}/lib", self.package_name),
            uid: 10000,
            enabled: true,
        };

        let mut activities = self.activities.clone();
        for act in &mut activities {
            if act.package_name.is_empty() {
                act.package_name = self.package_name.clone();
            }
            if act.application_info.is_none() {
                act.application_info = Some(app_info.clone());
            }
        }

        PackageInfo {
            package_name: self.package_name.clone(),
            version_code: self.version_code,
            version_name: self.version_name.clone(),
            application_info: Some(app_info),
            activities,
            requested_permissions: self.uses_permissions.clone(),
            first_install_time: 0,
            last_update_time: 0,
        }
    }
}

// -----------------------------------------------------------------------------
// Binary AXML Parser
// -----------------------------------------------------------------------------

pub struct AxmlParser;

impl AxmlParser {
    /// Parse raw binary AXML buffer and extract `ParsedAxmlManifest`.
    pub fn parse(bytes: &[u8]) -> Result<ParsedAxmlManifest, AxmlError> {
        if bytes.len() < 8 {
            return Err(AxmlError::BufferTooShort {
                expected: 8,
                actual: bytes.len(),
            });
        }

        let magic = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
        if magic != RES_XML_TYPE {
            return Err(AxmlError::InvalidMagic {
                expected: RES_XML_TYPE,
                actual: magic,
            });
        }

        let total_size = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let limit = total_size.min(bytes.len());

        let mut string_pool: Vec<String> = Vec::new();
        let mut resource_map: Vec<u32> = Vec::new();

        let mut pos = 8;
        // First Pass: Extract String Pool and Resource Map
        while pos + 8 <= limit {
            let chunk_type = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
            let header_size = u16::from_le_bytes(bytes[pos + 2..pos + 4].try_into().unwrap()) as usize;
            let chunk_size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;

            if chunk_size < 8 || pos + chunk_size > bytes.len() {
                // If chunk_size is invalid, advance by 4
                pos += 4;
                continue;
            }

            match chunk_type {
                RES_STRING_POOL_TYPE => {
                    let pool_chunk = &bytes[pos..pos + chunk_size];
                    string_pool = Self::parse_string_pool(pool_chunk)?;
                }
                RES_XML_RESOURCE_MAP_TYPE => {
                    let map_chunk = &bytes[pos..pos + chunk_size];
                    resource_map = Self::parse_resource_map(map_chunk, header_size)?;
                }
                _ => {}
            }

            pos += chunk_size;
        }

        // Second Pass: Parse XML Element Hierarchy
        let mut manifest = ParsedAxmlManifest {
            has_code: true,
            min_sdk_version: 1,
            target_sdk_version: 33, // Default target SDK 33
            ..Default::default()
        };

        let mut tag_stack: Vec<String> = Vec::new();
        let mut current_activity: Option<ActivityInfo> = None;
        let mut current_receiver: Option<ReceiverInfo> = None;
        let mut current_service: Option<ServiceInfo> = None;
        let mut current_intent_filter: Option<IntentFilter> = None;

        pos = 8;
        while pos + 8 <= limit {
            let chunk_type = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
            let chunk_size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;

            if chunk_size < 8 || pos + chunk_size > bytes.len() {
                pos += 4;
                continue;
            }

            match chunk_type {
                RES_XML_START_ELEMENT_TYPE => {
                    if pos + 28 <= bytes.len() {
                        let tag_idx = u32::from_le_bytes(bytes[pos + 20..pos + 24].try_into().unwrap()) as usize;
                        let tag_name = string_pool
                            .get(tag_idx)
                            .cloned()
                            .unwrap_or_else(|| format!("tag_{}", tag_idx));

                        let attr_count = u16::from_le_bytes(bytes[pos + 28..pos + 30].try_into().unwrap()) as usize;
                        let mut attrs = Vec::with_capacity(attr_count);

                        let mut attr_offset = pos + 36;
                        for _ in 0..attr_count {
                            if attr_offset + 20 <= pos + chunk_size {
                                let ns_idx = u32::from_le_bytes(bytes[attr_offset..attr_offset + 4].try_into().unwrap());
                                let name_idx = u32::from_le_bytes(bytes[attr_offset + 4..attr_offset + 8].try_into().unwrap()) as usize;
                                let raw_idx = u32::from_le_bytes(bytes[attr_offset + 8..attr_offset + 12].try_into().unwrap());
                                let data_type = bytes[attr_offset + 15];
                                let data = u32::from_le_bytes(bytes[attr_offset + 16..attr_offset + 20].try_into().unwrap());

                                let attr_name = string_pool.get(name_idx).cloned().unwrap_or_default();
                                let namespace = if ns_idx != 0xFFFFFFFF {
                                    string_pool.get(ns_idx as usize).cloned()
                                } else {
                                    None
                                };
                                let raw_value = if raw_idx != 0xFFFFFFFF {
                                    string_pool.get(raw_idx as usize).cloned()
                                } else {
                                    None
                                };

                                let res_id = resource_map.get(name_idx).copied();

                                attrs.push(XmlAttribute {
                                    namespace,
                                    name: attr_name,
                                    resource_id: res_id,
                                    raw_value,
                                    data_type,
                                    data,
                                });

                                attr_offset += 20;
                            }
                        }

                        // Process Start Tag
                        Self::handle_start_element(
                            &tag_name,
                            &attrs,
                            &string_pool,
                            &mut manifest,
                            &mut current_activity,
                            &mut current_service,
                            &mut current_receiver,
                            &mut current_intent_filter,
                        );

                        tag_stack.push(tag_name);
                    }
                }
                RES_XML_END_ELEMENT_TYPE => {
                    if let Some(tag_name) = tag_stack.pop() {
                        Self::handle_end_element(
                            &tag_name,
                            &mut manifest,
                            &mut current_activity,
                            &mut current_service,
                            &mut current_receiver,
                            &mut current_intent_filter,
                        );
                    }
                }
                _ => {}
            }

            pos += chunk_size;
        }

        // Finalize any lingering components if end tag was missing
        if let Some(act) = current_activity.take() {
            manifest.activities.push(act);
        }
        if let Some(svc) = current_service.take() {
            manifest.services.push(svc);
        }
        if let Some(rcv) = current_receiver.take() {
            manifest.receivers.push(rcv);
        }

        Ok(manifest)
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_start_element(
        tag_name: &str,
        attrs: &[XmlAttribute],
        pool: &[String],
        manifest: &mut ParsedAxmlManifest,
        current_activity: &mut Option<ActivityInfo>,
        current_service: &mut Option<ServiceInfo>,
        current_receiver: &mut Option<ReceiverInfo>,
        current_intent_filter: &mut Option<IntentFilter>,
    ) {
        // Universal attribute scan for root/flat manifests
        for attr in attrs {
            if manifest.package_name.is_empty() && (attr.name == "package" || attr.name == "packageName") {
                if let Some(pkg) = attr.as_string(pool) {
                    manifest.package_name = pkg.to_string();
                }
            } else if attr.name == "glEsVersion" || attr.resource_id == Some(ATTR_GLES_VERSION) {
                if let Some(v) = attr.as_u32() {
                    manifest.uses_features.push(UsesFeatureInfo {
                        name: "android.hardware.opengles.version".to_string(),
                        gl_es_version: v,
                        required: true,
                    });
                }
            } else if attr.name == "name" || attr.resource_id == Some(ATTR_NAME) {
                if let Some(s) = attr.as_string(pool) {
                    if s.starts_with("android.permission.") && !manifest.uses_permissions.contains(&s.to_string()) {
                        manifest.uses_permissions.push(s.to_string());
                    } else if s.contains("vulkan") || s.contains("opengles") {
                        manifest.uses_features.push(UsesFeatureInfo {
                            name: s.to_string(),
                            gl_es_version: 0,
                            required: true,
                        });
                    }
                }
            }
        }

        match tag_name {
            "manifest" => {
                for attr in attrs {
                    if attr.name == "package" || attr.name == "packageName" {
                        if let Some(pkg) = attr.as_string(pool) {
                            manifest.package_name = pkg.to_string();
                        }
                    } else if attr.name == "versionCode" || attr.resource_id == Some(ATTR_VERSION_CODE) {
                        if let Some(vc) = attr.as_i32() {
                            manifest.version_code = vc;
                        }
                    } else if attr.name == "versionName" || attr.resource_id == Some(ATTR_VERSION_NAME) {
                        if let Some(vn) = attr.as_string(pool) {
                            manifest.version_name = Some(vn.to_string());
                        }
                    }
                }
            }
            "uses-sdk" => {
                for attr in attrs {
                    if attr.name == "minSdkVersion" || attr.resource_id == Some(ATTR_MIN_SDK_VERSION) {
                        if let Some(min) = attr.as_i32() {
                            manifest.min_sdk_version = min;
                        }
                    } else if attr.name == "targetSdkVersion" || attr.resource_id == Some(ATTR_TARGET_SDK_VERSION) {
                        if let Some(target) = attr.as_i32() {
                            manifest.target_sdk_version = target;
                        }
                    }
                }
            }
            "application" => {
                for attr in attrs {
                    if attr.name == "name" || attr.resource_id == Some(ATTR_NAME) {
                        if let Some(name) = attr.as_string(pool) {
                            manifest.application_name = Some(name.to_string());
                        }
                    } else if attr.name == "label" || attr.resource_id == Some(ATTR_LABEL) {
                        if let Some(label) = attr.as_string(pool) {
                            manifest.application_label = Some(label.to_string());
                        }
                    } else if attr.name == "icon" || attr.resource_id == Some(ATTR_ICON) {
                        if let Some(icon) = attr.as_u32() {
                            manifest.application_icon = icon;
                        }
                    } else if attr.name == "theme" || attr.resource_id == Some(ATTR_THEME) {
                        if let Some(theme) = attr.as_u32() {
                            manifest.application_theme = theme;
                        }
                    } else if attr.name == "hasCode" || attr.resource_id == Some(ATTR_HAS_CODE) {
                        if let Some(has_code) = attr.as_bool() {
                            manifest.has_code = has_code;
                        }
                    }
                }
            }
            "activity" | "activity-alias" => {
                let mut act = ActivityInfo {
                    package_name: manifest.package_name.clone(),
                    enabled: true,
                    exported: false,
                    ..Default::default()
                };
                for attr in attrs {
                    if attr.name == "name" || attr.resource_id == Some(ATTR_NAME) {
                        if let Some(name) = attr.as_string(pool) {
                            let mut full_name = name.to_string();
                            if full_name.starts_with('.') {
                                full_name = format!("{}{}", manifest.package_name, full_name);
                            } else if !full_name.contains('.') && !manifest.package_name.is_empty() {
                                full_name = format!("{}.{}", manifest.package_name, full_name);
                            }
                            act.name = full_name;
                        }
                    } else if attr.name == "label" || attr.resource_id == Some(ATTR_LABEL) {
                        if let Some(label) = attr.as_string(pool) {
                            act.label = Some(label.to_string());
                        }
                    } else if attr.name == "icon" || attr.resource_id == Some(ATTR_ICON) {
                        if let Some(icon) = attr.as_u32() {
                            act.icon = icon;
                        }
                    } else if attr.name == "theme" || attr.resource_id == Some(ATTR_THEME) {
                        if let Some(theme) = attr.as_u32() {
                            act.theme = theme;
                        }
                    } else if attr.name == "exported" || attr.resource_id == Some(ATTR_EXPORTED) {
                        if let Some(exp) = attr.as_bool() {
                            act.exported = exp;
                        }
                    } else if attr.name == "launchMode" || attr.resource_id == Some(ATTR_LAUNCH_MODE) {
                        if let Some(lm) = attr.as_i32() {
                            act.launch_mode = lm;
                        }
                    } else if attr.name == "permission" || attr.resource_id == Some(ATTR_PERMISSION) {
                        if let Some(perm) = attr.as_string(pool) {
                            act.permission = Some(perm.to_string());
                        }
                    }
                }
                *current_activity = Some(act);
            }
            "service" => {
                let mut svc = ServiceInfo {
                    package_name: manifest.package_name.clone(),
                    enabled: true,
                    exported: false,
                    ..Default::default()
                };
                for attr in attrs {
                    if attr.name == "name" || attr.resource_id == Some(ATTR_NAME) {
                        if let Some(name) = attr.as_string(pool) {
                            svc.name = name.to_string();
                        }
                    } else if attr.name == "permission" || attr.resource_id == Some(ATTR_PERMISSION) {
                        if let Some(perm) = attr.as_string(pool) {
                            svc.permission = Some(perm.to_string());
                        }
                    } else if attr.name == "exported" || attr.resource_id == Some(ATTR_EXPORTED) {
                        if let Some(exp) = attr.as_bool() {
                            svc.exported = exp;
                        }
                    }
                }
                *current_service = Some(svc);
            }
            "receiver" => {
                let mut rcv = ReceiverInfo {
                    package_name: manifest.package_name.clone(),
                    enabled: true,
                    exported: false,
                    ..Default::default()
                };
                for attr in attrs {
                    if attr.name == "name" || attr.resource_id == Some(ATTR_NAME) {
                        if let Some(name) = attr.as_string(pool) {
                            rcv.name = name.to_string();
                        }
                    } else if attr.name == "permission" || attr.resource_id == Some(ATTR_PERMISSION) {
                        if let Some(perm) = attr.as_string(pool) {
                            rcv.permission = Some(perm.to_string());
                        }
                    } else if attr.name == "exported" || attr.resource_id == Some(ATTR_EXPORTED) {
                        if let Some(exp) = attr.as_bool() {
                            rcv.exported = exp;
                        }
                    }
                }
                *current_receiver = Some(rcv);
            }
            "intent-filter" => {
                let mut filter = IntentFilter::default();
                for attr in attrs {
                    if attr.name == "priority" || attr.resource_id == Some(ATTR_PRIORITY) {
                        if let Some(pri) = attr.as_i32() {
                            filter.priority = pri;
                        }
                    }
                }
                *current_intent_filter = Some(filter);
            }
            "action" => {
                if let Some(filter) = current_intent_filter {
                    for attr in attrs {
                        if attr.name == "name" || attr.resource_id == Some(ATTR_NAME) {
                            if let Some(name) = attr.as_string(pool) {
                                filter.actions.push(name.to_string());
                            }
                        }
                    }
                }
            }
            "category" => {
                if let Some(filter) = current_intent_filter {
                    for attr in attrs {
                        if attr.name == "name" || attr.resource_id == Some(ATTR_NAME) {
                            if let Some(name) = attr.as_string(pool) {
                                filter.categories.push(name.to_string());
                            }
                        }
                    }
                }
            }
            "data" => {
                if let Some(filter) = current_intent_filter {
                    for attr in attrs {
                        if attr.name == "scheme" || attr.resource_id == Some(ATTR_SCHEME) {
                            if let Some(scheme) = attr.as_string(pool) {
                                filter.data_schemes.push(scheme.to_string());
                            }
                        }
                    }
                }
            }
            "uses-permission" => {
                for attr in attrs {
                    if attr.name == "name" || attr.resource_id == Some(ATTR_NAME) {
                        if let Some(name) = attr.as_string(pool) {
                            if !manifest.uses_permissions.contains(&name.to_string()) {
                                manifest.uses_permissions.push(name.to_string());
                            }
                        }
                    }
                }
            }
            "uses-feature" => {
                let mut feat = UsesFeatureInfo {
                    required: true,
                    ..Default::default()
                };
                for attr in attrs {
                    if attr.name == "name" || attr.resource_id == Some(ATTR_NAME) {
                        if let Some(name) = attr.as_string(pool) {
                            feat.name = name.to_string();
                        }
                    } else if attr.name == "glEsVersion" || attr.resource_id == Some(ATTR_GLES_VERSION) {
                        if let Some(v) = attr.as_u32() {
                            feat.gl_es_version = v;
                        }
                    } else if attr.name == "required" || attr.resource_id == Some(ATTR_REQUIRED) {
                        if let Some(req) = attr.as_bool() {
                            feat.required = req;
                        }
                    }
                }
                manifest.uses_features.push(feat);
            }
            "meta-data" => {
                let mut key = String::new();
                let mut val = String::new();
                for attr in attrs {
                    if attr.name == "name" || attr.resource_id == Some(ATTR_NAME) {
                        if let Some(k) = attr.as_string(pool) {
                            key = k.to_string();
                        }
                    } else if attr.name == "value" {
                        if let Some(v) = attr.as_string(pool) {
                            val = v.to_string();
                        }
                    }
                }
                if !key.is_empty() {
                    manifest.meta_data.insert(key, val);
                }
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_end_element(
        tag_name: &str,
        manifest: &mut ParsedAxmlManifest,
        current_activity: &mut Option<ActivityInfo>,
        current_service: &mut Option<ServiceInfo>,
        current_receiver: &mut Option<ReceiverInfo>,
        current_intent_filter: &mut Option<IntentFilter>,
    ) {
        match tag_name {
            "intent-filter" => {
                if let Some(filter) = current_intent_filter.take() {
                    if let Some(act) = current_activity.as_mut() {
                        act.intent_filters.push(filter);
                    } else if let Some(rcv) = current_receiver.as_mut() {
                        rcv.intent_filters.push(filter);
                    }
                }
            }
            "activity" | "activity-alias" => {
                if let Some(mut act) = current_activity.take() {
                    // In Android, an activity with intent-filters is exported by default unless explicitly exported=false
                    if !act.exported && !act.intent_filters.is_empty() {
                        act.exported = true;
                    }
                    manifest.activities.push(act);
                }
            }
            "service" => {
                if let Some(svc) = current_service.take() {
                    manifest.services.push(svc);
                }
            }
            "receiver" => {
                if let Some(rcv) = current_receiver.take() {
                    manifest.receivers.push(rcv);
                }
            }
            _ => {}
        }
    }

    /// Parse String Pool Chunk (supporting both UTF-8 and UTF-16 encodings).
    pub fn parse_string_pool(chunk: &[u8]) -> Result<Vec<String>, AxmlError> {
        if chunk.len() < 28 {
            return Err(AxmlError::MalformedStringPool(
                "Chunk header size too small (<28 bytes)".into(),
            ));
        }

        let string_count = u32::from_le_bytes(chunk[8..12].try_into().unwrap()) as usize;
        let flags = u32::from_le_bytes(chunk[16..20].try_into().unwrap());
        let strings_start = u32::from_le_bytes(chunk[20..24].try_into().unwrap()) as usize;
        let is_utf8 = (flags & (1 << 8)) != 0;

        let mut offsets = Vec::with_capacity(string_count);
        for i in 0..string_count {
            let off_pos = 28 + i * 4;
            if off_pos + 4 <= chunk.len() {
                let off = u32::from_le_bytes(chunk[off_pos..off_pos + 4].try_into().unwrap()) as usize;
                offsets.push(off);
            }
        }

        let mut pool = Vec::with_capacity(string_count);
        for off in offsets {
            let str_abs = strings_start + off;
            if str_abs >= chunk.len() {
                pool.push(String::new());
                continue;
            }

            if is_utf8 {
                let mut cursor = str_abs;
                // Skip UTF-16 length prefix (1 or 2 bytes)
                if cursor < chunk.len() {
                    if (chunk[cursor] & 0x80) != 0 {
                        cursor += 2;
                    } else {
                        cursor += 1;
                    }
                }
                // Read UTF-8 byte length prefix (1 or 2 bytes)
                let utf8_len = if cursor < chunk.len() {
                    let b = chunk[cursor];
                    if (b & 0x80) != 0 {
                        let next = if cursor + 1 < chunk.len() { chunk[cursor + 1] } else { 0 };
                        cursor += 2;
                        (((b & 0x7F) as usize) << 8) | (next as usize)
                    } else {
                        cursor += 1;
                        b as usize
                    }
                } else {
                    0
                };

                let end = (cursor + utf8_len).min(chunk.len());
                let s = String::from_utf8_lossy(&chunk[cursor..end])
                    .trim_end_matches('\0')
                    .to_string();
                pool.push(s);
            } else {
                let mut cursor = str_abs;
                // Read UTF-16 character length prefix
                let char_len = if cursor + 2 <= chunk.len() {
                    let len_prefix = u16::from_le_bytes(chunk[cursor..cursor + 2].try_into().unwrap());
                    if (len_prefix & 0x8000) != 0 {
                        cursor += 4;
                        let next = if cursor <= chunk.len() {
                            u16::from_le_bytes(chunk[cursor - 2..cursor].try_into().unwrap())
                        } else {
                            0
                        };
                        (((len_prefix & 0x7FFF) as usize) << 16) | (next as usize)
                    } else {
                        cursor += 2;
                        len_prefix as usize
                    }
                } else {
                    0
                };

                let mut u16_chars = Vec::with_capacity(char_len);
                for _ in 0..char_len {
                    if cursor + 2 <= chunk.len() {
                        let ch = u16::from_le_bytes(chunk[cursor..cursor + 2].try_into().unwrap());
                        if ch == 0 {
                            break;
                        }
                        u16_chars.push(ch);
                        cursor += 2;
                    }
                }
                let s = String::from_utf16_lossy(&u16_chars);
                pool.push(s);
            }
        }

        Ok(pool)
    }

    /// Parse Resource Map Chunk (`0x0180`).
    pub fn parse_resource_map(chunk: &[u8], header_size: usize) -> Result<Vec<u32>, AxmlError> {
        if chunk.len() < header_size {
            return Err(AxmlError::BufferTooShort {
                expected: header_size,
                actual: chunk.len(),
            });
        }

        let map_data = &chunk[header_size..];
        let count = map_data.len() / 4;
        let mut map = Vec::with_capacity(count);

        for i in 0..count {
            let offset = i * 4;
            let res_id = u32::from_le_bytes(map_data[offset..offset + 4].try_into().unwrap());
            map.push(res_id);
        }

        Ok(map)
    }
}
