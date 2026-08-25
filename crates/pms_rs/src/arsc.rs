//! Binary `resources.arsc` Table Parser and Resource Resolver.
//!
//! Decodes `RES_TABLE_TYPE` (0x0002), `RES_STRING_POOL_TYPE` (0x0001),
//! `RES_TABLE_PACKAGE_TYPE` (0x0200), `RES_TABLE_TYPE_SPEC_TYPE` (0x0202),
//! and `RES_TABLE_TYPE_TYPE` (0x0201) chunks.

use crate::axml::AxmlParser;
use std::collections::HashMap;
use thiserror::Error;

// -----------------------------------------------------------------------------
// ARSC Chunk Types & Entry Flags
// -----------------------------------------------------------------------------

pub const RES_NULL_TYPE: u16 = 0x0000;
pub const RES_STRING_POOL_TYPE: u16 = 0x0001;
pub const RES_TABLE_TYPE: u16 = 0x0002;
pub const RES_TABLE_PACKAGE_TYPE: u16 = 0x0200;
pub const RES_TABLE_TYPE_TYPE: u16 = 0x0201;
pub const RES_TABLE_TYPE_SPEC_TYPE: u16 = 0x0202;
pub const RES_TABLE_LIBRARY_TYPE: u16 = 0x0203;

pub const ENTRY_FLAG_COMPLEX: u16 = 0x0001;
pub const ENTRY_FLAG_PUBLIC: u16 = 0x0002;
pub const NO_ENTRY: u32 = 0xFFFFFFFF;

// -----------------------------------------------------------------------------
// Errors
// -----------------------------------------------------------------------------

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ArscError {
    #[error("Buffer too short: expected at least {expected} bytes, got {actual}")]
    BufferTooShort { expected: usize, actual: usize },
    #[error("Invalid ARSC root table magic: expected 0x{expected:04x}, got 0x{actual:04x}")]
    InvalidMagic { expected: u16, actual: u16 },
    #[error("Malformed chunk header at offset {offset}: chunk size {size} exceeds buffer")]
    MalformedChunk { offset: usize, size: usize },
    #[error("Malformed string pool in ARSC table: {0}")]
    MalformedStringPool(String),
    #[error("Malformed package header: {0}")]
    MalformedPackage(String),
}

// -----------------------------------------------------------------------------
// Resource Value Types
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub enum ResourceValue {
    #[default]
    Null,
    String(String),
    Integer(i32),
    Boolean(bool),
    Color(u32),
    Reference(u32),
    Raw { data_type: u8, data: u32 },
    Bag { parent: u32, items: Vec<(u32, ResourceValue)> },
}

impl ResourceValue {
    pub fn as_string(&self) -> Option<&str> {
        match self {
            ResourceValue::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self {
            ResourceValue::Integer(i) => Some(*i),
            ResourceValue::Boolean(b) => Some(if *b { 1 } else { 0 }),
            ResourceValue::Color(c) => Some(*c as i32),
            ResourceValue::Reference(r) => Some(*r as i32),
            ResourceValue::Raw { data, .. } => Some(*data as i32),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ResourceValue::Boolean(b) => Some(*b),
            ResourceValue::Integer(i) => Some(*i != 0),
            ResourceValue::Raw { data, .. } => Some(*data != 0),
            _ => None,
        }
    }
}

// -----------------------------------------------------------------------------
// Internal ARSC Table Representation
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ArscEntry {
    pub key: String,
    pub value: ResourceValue,
}

#[derive(Debug, Clone, Default)]
pub struct ArscType {
    pub id: u8,
    pub name: String,
    pub entries: HashMap<u16, ArscEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct ArscPackage {
    pub id: u32,
    pub name: String,
    pub type_strings: Vec<String>,
    pub key_strings: Vec<String>,
    pub types: HashMap<u8, ArscType>,
}

#[derive(Debug, Clone, Default)]
pub struct ArscTable {
    pub global_strings: Vec<String>,
    pub packages: HashMap<u32, ArscPackage>,
    pub package_names: HashMap<String, u32>,
}

impl ArscTable {
    /// Resolve a resource by its 32-bit ID (`0xPPTTEEEE`).
    pub fn resolve_resource(&self, res_id: u32) -> Option<ResourceValue> {
        let pkg_id = (res_id >> 24) & 0xFF;
        let type_id = ((res_id >> 16) & 0xFF) as u8;
        let entry_idx = (res_id & 0xFFFF) as u16;

        let pkg = self.packages.get(&pkg_id).or_else(|| {
            // If pkg_id is 0x7f and only one package exists, fall back to that package
            if self.packages.len() == 1 {
                self.packages.values().next()
            } else {
                None
            }
        })?;

        let arsc_type = pkg.types.get(&type_id)?;
        let entry = arsc_type.entries.get(&entry_idx)?;
        Some(entry.value.clone())
    }

    /// Resolve a string resource by ID.
    pub fn resolve_string(&self, res_id: u32) -> Option<String> {
        match self.resolve_resource(res_id) {
            Some(ResourceValue::String(s)) => Some(s),
            Some(ResourceValue::Reference(ref_id)) => self.resolve_string(ref_id),
            _ => None,
        }
    }

    /// Resolve a resource by package, type name, and entry key.
    pub fn resolve_by_name(
        &self,
        package_name: &str,
        type_name: &str,
        entry_key: &str,
    ) -> Option<ResourceValue> {
        let pkg = self
            .package_names
            .get(package_name)
            .and_then(|id| self.packages.get(id))
            .or_else(|| {
                // If package name is empty or not found, try the primary package
                if self.packages.len() == 1 {
                    self.packages.values().next()
                } else {
                    None
                }
            })?;

        for arsc_type in pkg.types.values() {
            if arsc_type.name == type_name {
                for entry in arsc_type.entries.values() {
                    if entry.key == entry_key {
                        return Some(entry.value.clone());
                    }
                }
            }
        }
        None
    }

    /// Resolve a string resource by package and entry key (e.g. `@string/app_name`).
    pub fn resolve_string_by_name(&self, package_name: &str, entry_key: &str) -> Option<String> {
        match self.resolve_by_name(package_name, "string", entry_key) {
            Some(ResourceValue::String(s)) => Some(s),
            Some(ResourceValue::Reference(ref_id)) => self.resolve_string(ref_id),
            _ => None,
        }
    }

    /// Resolve string reference or raw value (e.g. "@string/app_name" or "@0x7f040001" or direct string).
    pub fn resolve_string_ref(&self, text: &str, default_package: &str) -> String {
        if let Some(stripped) = text.strip_prefix('@') {
            if let Some(hex_str) = stripped.strip_prefix("0x") {
                if let Ok(id) = u32::from_str_radix(hex_str, 16) {
                    if let Some(val) = self.resolve_string(id) {
                        return val;
                    }
                }
            } else if let Some((type_name, entry_name)) = stripped.split_once('/') {
                let pkg = if let Some((p, _t)) = type_name.split_once(':') {
                    p
                } else {
                    default_package
                };
                let actual_type = if type_name.contains(':') {
                    type_name.split(':').nth(1).unwrap_or(type_name)
                } else {
                    type_name
                };
                if let Some(val) = self.resolve_by_name(pkg, actual_type, entry_name) {
                    if let Some(s) = val.as_string() {
                        return s.to_string();
                    }
                }
            }
        }
        text.to_string()
    }
}

// -----------------------------------------------------------------------------
// ARSC Parser Implementation
// -----------------------------------------------------------------------------

pub struct ArscParser;

impl ArscParser {
    /// Parse raw binary `resources.arsc` data.
    pub fn parse(bytes: &[u8]) -> Result<ArscTable, ArscError> {
        if bytes.len() < 12 {
            return Err(ArscError::BufferTooShort {
                expected: 12,
                actual: bytes.len(),
            });
        }

        let magic = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
        if magic != RES_TABLE_TYPE {
            return Err(ArscError::InvalidMagic {
                expected: RES_TABLE_TYPE,
                actual: magic,
            });
        }

        let header_size = u16::from_le_bytes(bytes[2..4].try_into().unwrap()) as usize;
        let total_size = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let limit = total_size.min(bytes.len());

        let mut table = ArscTable::default();
        let mut pos = header_size;

        while pos + 8 <= limit {
            let chunk_type = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap());
            let chunk_size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;

            if chunk_size < 8 || pos + chunk_size > bytes.len() {
                pos += 4;
                continue;
            }

            match chunk_type {
                RES_STRING_POOL_TYPE => {
                    let pool_chunk = &bytes[pos..pos + chunk_size];
                    if let Ok(pool) = AxmlParser::parse_string_pool(pool_chunk) {
                        table.global_strings = pool;
                    }
                }
                RES_TABLE_PACKAGE_TYPE => {
                    let pkg_chunk = &bytes[pos..pos + chunk_size];
                    if let Ok(pkg) = Self::parse_package(pkg_chunk, &table.global_strings) {
                        table.package_names.insert(pkg.name.clone(), pkg.id);
                        table.packages.insert(pkg.id, pkg);
                    }
                }
                _ => {}
            }

            pos += chunk_size;
        }

        Ok(table)
    }

    fn parse_package(chunk: &[u8], global_strings: &[String]) -> Result<ArscPackage, ArscError> {
        if chunk.len() < 288 {
            return Err(ArscError::MalformedPackage(
                "Package chunk header smaller than 288 bytes".into(),
            ));
        }

        let pkg_id = u32::from_le_bytes(chunk[8..12].try_into().unwrap());

        // Parse package name (128 UTF-16 chars)
        let name_bytes = &chunk[12..268];
        let mut u16_name = Vec::with_capacity(128);
        for i in 0..128 {
            let ch = u16::from_le_bytes(name_bytes[i * 2..i * 2 + 2].try_into().unwrap());
            if ch == 0 {
                break;
            }
            u16_name.push(ch);
        }
        let pkg_name = String::from_utf16_lossy(&u16_name);

        let type_strings_offset = u32::from_le_bytes(chunk[268..272].try_into().unwrap()) as usize;
        let key_strings_offset = u32::from_le_bytes(chunk[276..280].try_into().unwrap()) as usize;

        let mut type_strings = Vec::new();
        if type_strings_offset > 0 && type_strings_offset < chunk.len() {
            let pool_slice = &chunk[type_strings_offset..];
            if let Ok(pool) = AxmlParser::parse_string_pool(pool_slice) {
                type_strings = pool;
            }
        }

        let mut key_strings = Vec::new();
        if key_strings_offset > 0 && key_strings_offset < chunk.len() {
            let pool_slice = &chunk[key_strings_offset..];
            if let Ok(pool) = AxmlParser::parse_string_pool(pool_slice) {
                key_strings = pool;
            }
        }

        let mut package = ArscPackage {
            id: pkg_id,
            name: pkg_name,
            type_strings,
            key_strings,
            types: HashMap::new(),
        };

        // Scan subsequent chunks inside package (TypeSpec and Type)
        let mut pos = 288.max(key_strings_offset);
        if let Some(last_offset) = [type_strings_offset, key_strings_offset].iter().max() {
            if *last_offset + 8 <= chunk.len() {
                let pool_sz = u32::from_le_bytes(chunk[*last_offset + 4..*last_offset + 8].try_into().unwrap()) as usize;
                pos = *last_offset + pool_sz;
            }
        }

        while pos + 8 <= chunk.len() {
            let chunk_type = u16::from_le_bytes(chunk[pos..pos + 2].try_into().unwrap());
            let header_size = u16::from_le_bytes(chunk[pos + 2..pos + 4].try_into().unwrap()) as usize;
            let chunk_size = u32::from_le_bytes(chunk[pos + 4..pos + 8].try_into().unwrap()) as usize;

            if chunk_size < 8 || pos + chunk_size > chunk.len() {
                pos += 4;
                continue;
            }

            match chunk_type {
                RES_TABLE_TYPE_SPEC_TYPE => {
                    let type_id = chunk[pos + 8];
                    let type_name = package
                        .type_strings
                        .get(type_id.saturating_sub(1) as usize)
                        .cloned()
                        .unwrap_or_else(|| format!("type_{}", type_id));

                    package.types.entry(type_id).or_insert_with(|| ArscType {
                        id: type_id,
                        name: type_name,
                        entries: HashMap::new(),
                    });
                }
                RES_TABLE_TYPE_TYPE => {
                    let type_id = chunk[pos + 8];
                    let entry_count = u32::from_le_bytes(chunk[pos + 12..pos + 16].try_into().unwrap()) as usize;
                    let entries_start = u32::from_le_bytes(chunk[pos + 16..pos + 20].try_into().unwrap()) as usize;

                    let type_name = package
                        .type_strings
                        .get(type_id.saturating_sub(1) as usize)
                        .cloned()
                        .unwrap_or_else(|| format!("type_{}", type_id));

                    let arsc_type = package.types.entry(type_id).or_insert_with(|| ArscType {
                        id: type_id,
                        name: type_name,
                        entries: HashMap::new(),
                    });

                    // Offsets table starts after header_size
                    let offsets_pos = pos + header_size;
                    for entry_idx in 0..entry_count {
                        let off_cursor = offsets_pos + entry_idx * 4;
                        if off_cursor + 4 <= pos + chunk_size {
                            let entry_rel_offset = u32::from_le_bytes(
                                chunk[off_cursor..off_cursor + 4].try_into().unwrap(),
                            );
                            if entry_rel_offset != NO_ENTRY {
                                let entry_abs = pos + entries_start + (entry_rel_offset as usize);
                                if entry_abs + 8 <= pos + chunk_size {
                                    let flags = u16::from_le_bytes(
                                        chunk[entry_abs + 2..entry_abs + 4].try_into().unwrap(),
                                    );
                                    let key_idx = u32::from_le_bytes(
                                        chunk[entry_abs + 4..entry_abs + 8].try_into().unwrap(),
                                    ) as usize;
                                    let key_str = package
                                        .key_strings
                                        .get(key_idx)
                                        .cloned()
                                        .unwrap_or_else(|| format!("key_{}", key_idx));

                                    let is_complex = (flags & ENTRY_FLAG_COMPLEX) != 0;
                                    if !is_complex {
                                        // Simple Res_value (8 bytes at entry_abs + 8)
                                        let val_abs = entry_abs + 8;
                                        if val_abs + 8 <= pos + chunk_size {
                                            let data_type = chunk[val_abs + 3];
                                            let data = u32::from_le_bytes(
                                                chunk[val_abs + 4..val_abs + 8].try_into().unwrap(),
                                            );

                                            let val = match data_type {
                                                3 => {
                                                    // String in global pool
                                                    let s = global_strings
                                                        .get(data as usize)
                                                        .cloned()
                                                        .unwrap_or_default();
                                                    ResourceValue::String(s)
                                                }
                                                16 | 17 => ResourceValue::Integer(data as i32),
                                                18 => ResourceValue::Boolean(data != 0),
                                                28..=31 => ResourceValue::Color(data),
                                                1 => ResourceValue::Reference(data),
                                                _ => ResourceValue::Raw { data_type, data },
                                            };

                                            arsc_type.entries.insert(
                                                entry_idx as u16,
                                                ArscEntry {
                                                    key: key_str,
                                                    value: val,
                                                },
                                            );
                                        }
                                    } else {
                                        // Complex entry (bag)
                                        if entry_abs + 16 <= pos + chunk_size {
                                            let parent = u32::from_le_bytes(
                                                chunk[entry_abs + 8..entry_abs + 12]
                                                    .try_into()
                                                    .unwrap(),
                                            );
                                            let count = u32::from_le_bytes(
                                                chunk[entry_abs + 12..entry_abs + 16]
                                                    .try_into()
                                                    .unwrap(),
                                            ) as usize;

                                            let mut items = Vec::with_capacity(count);
                                            let mut item_cursor = entry_abs + 16;
                                            for _ in 0..count {
                                                if item_cursor + 12 <= pos + chunk_size {
                                                    let item_name = u32::from_le_bytes(
                                                        chunk[item_cursor..item_cursor + 4]
                                                            .try_into()
                                                            .unwrap(),
                                                    );
                                                    let data_type = chunk[item_cursor + 7];
                                                    let data = u32::from_le_bytes(
                                                        chunk[item_cursor + 8..item_cursor + 12]
                                                            .try_into()
                                                            .unwrap(),
                                                    );
                                                    let val = match data_type {
                                                        3 => {
                                                            let s = global_strings
                                                                .get(data as usize)
                                                                .cloned()
                                                                .unwrap_or_default();
                                                            ResourceValue::String(s)
                                                        }
                                                        16 | 17 => ResourceValue::Integer(data as i32),
                                                        18 => ResourceValue::Boolean(data != 0),
                                                        28..=31 => ResourceValue::Color(data),
                                                        1 => ResourceValue::Reference(data),
                                                        _ => ResourceValue::Raw { data_type, data },
                                                    };
                                                    items.push((item_name, val));
                                                    item_cursor += 12;
                                                }
                                            }

                                            arsc_type.entries.insert(
                                                entry_idx as u16,
                                                ArscEntry {
                                                    key: key_str,
                                                    value: ResourceValue::Bag { parent, items },
                                                },
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }

            pos += chunk_size;
        }

        Ok(package)
    }
}
