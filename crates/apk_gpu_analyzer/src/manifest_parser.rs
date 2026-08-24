pub const RES_XML_TYPE: u16 = 0x0003;
pub const RES_STRING_POOL_TYPE: u16 = 0x0001;
pub const RES_XML_START_ELEMENT_TYPE: u16 = 0x0102;
pub const RES_XML_END_ELEMENT_TYPE: u16 = 0x0103;

#[derive(Debug, Default, Clone)]
pub struct ParsedManifestInfo {
    pub package_name: String,
    pub min_gles_version: u32,
    pub uses_features: Vec<String>,
    pub permissions: Vec<String>,
}

pub struct BinaryXmlParser;

impl BinaryXmlParser {
    pub fn parse_axml(bytes: &[u8]) -> Result<ParsedManifestInfo, String> {
        if bytes.len() < 8 {
            return Err("AXML buffer too short".to_string());
        }

        let magic = u16::from_le_bytes(match bytes[0..2].try_into() {
            Ok(b) => b,
            Err(_) => return Err("Failed to read magic".to_string()),
        });

        if magic != RES_XML_TYPE {
            return Err(format!("Invalid AXML magic: 0x{:04x}", magic));
        }

        let mut string_pool: Vec<String> = Vec::new();
        let mut info = ParsedManifestInfo::default();

        // First pass: locate and parse string pool
        let mut pos = 8;
        while pos + 8 <= bytes.len() {
            let chunk_type = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap_or([0, 0]));
            let chunk_size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap_or([0, 0, 0, 0])) as usize;
            if chunk_type == RES_STRING_POOL_TYPE {
                let pool_end = if pos + chunk_size <= bytes.len() && chunk_size > 0 {
                    pos + chunk_size
                } else {
                    bytes.len()
                };
                if let Ok(pool) = Self::parse_string_pool(&bytes[pos..pool_end]) {
                    string_pool = pool;
                    break;
                }
            }
            pos += 4;
        }

        // Second pass: scan for all START_ELEMENT chunks
        let mut i = 8;
        while i + 36 <= bytes.len() {
            let chunk_type = u16::from_le_bytes(bytes[i..i + 2].try_into().unwrap_or([0, 0]));
            if chunk_type == RES_XML_START_ELEMENT_TYPE {
                let chunk_size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().unwrap_or([0, 0, 0, 0])) as usize;
                let attr_count_offset = i + 28;
                let attr_count = u16::from_le_bytes(
                    bytes[attr_count_offset..attr_count_offset + 2]
                        .try_into()
                        .unwrap_or([0, 0]),
                ) as usize;

                let mut attr_cursor = i + 36;
                let max_end = if chunk_size > 0 && i + chunk_size <= bytes.len() {
                    i + chunk_size
                } else {
                    bytes.len()
                };

                for _ in 0..attr_count {
                    if attr_cursor + 20 <= max_end {
                        let name_idx = u32::from_le_bytes(
                            bytes[attr_cursor + 4..attr_cursor + 8]
                                .try_into()
                                .unwrap_or([0, 0, 0, 0]),
                        ) as usize;
                        let val_idx = u32::from_le_bytes(
                            bytes[attr_cursor + 8..attr_cursor + 12]
                                .try_into()
                                .unwrap_or([0, 0, 0, 0]),
                        ) as usize;
                        let raw_data = u32::from_le_bytes(
                            bytes[attr_cursor + 16..attr_cursor + 20]
                                .try_into()
                                .unwrap_or([0, 0, 0, 0]),
                        );

                        let attr_name = string_pool.get(name_idx).cloned().unwrap_or_default();
                        let str_val = string_pool.get(val_idx).cloned().unwrap_or_default();

                        if attr_name == "package" {
                            info.package_name = str_val;
                        } else if attr_name == "glEsVersion" {
                            info.min_gles_version = raw_data;
                        } else if attr_name == "name" {
                            if str_val.contains("vulkan") || str_val.contains("opengles") {
                                info.uses_features.push(str_val);
                            } else if str_val.starts_with("android.permission") {
                                info.permissions.push(str_val);
                            }
                        }

                        attr_cursor += 20;
                    }
                }
                if chunk_size >= 36 {
                    i += chunk_size;
                    continue;
                }
            }
            i += 4;
        }

        Ok(info)
    }

    fn parse_string_pool(chunk: &[u8]) -> Result<Vec<String>, String> {
        if chunk.len() < 28 {
            return Err("String pool header too short".to_string());
        }

        let string_count = u32::from_le_bytes(chunk[8..12].try_into().unwrap_or([0, 0, 0, 0])) as usize;
        let flags = u32::from_le_bytes(chunk[16..20].try_into().unwrap_or([0, 0, 0, 0]));
        let strings_start = u32::from_le_bytes(chunk[20..24].try_into().unwrap_or([0, 0, 0, 0])) as usize;
        let is_utf8 = (flags & (1 << 8)) != 0;

        let mut offsets = Vec::with_capacity(string_count);
        for i in 0..string_count {
            let pos = 28 + i * 4;
            if pos + 4 <= chunk.len() {
                let off = u32::from_le_bytes(chunk[pos..pos + 4].try_into().unwrap_or([0, 0, 0, 0])) as usize;
                offsets.push(off);
            }
        }

        let mut pool = Vec::with_capacity(string_count);
        for off in offsets {
            let str_abs = strings_start + off;
            if str_abs < chunk.len() {
                if is_utf8 {
                    let mut cursor = str_abs;
                    // Skip 1 or 2 bytes utf-16 len and 1 or 2 bytes utf-8 len if high bit set
                    if cursor < chunk.len() {
                        if chunk[cursor] & 0x80 != 0 { cursor += 2; } else { cursor += 1; }
                    }
                    if cursor < chunk.len() {
                        if chunk[cursor] & 0x80 != 0 { cursor += 2; } else { cursor += 1; }
                    }
                    let mut end = cursor;
                    while end < chunk.len() && chunk[end] != 0 {
                        end += 1;
                    }
                    let s = String::from_utf8_lossy(&chunk[cursor..end]).to_string();
                    pool.push(s);
                } else {
                    let mut curr = str_abs;
                    // Standard UTF-16 strings have 2-byte (or 4-byte if high bit set) char length prefix
                    if curr + 2 <= chunk.len() {
                        let len_prefix = u16::from_le_bytes(chunk[curr..curr + 2].try_into().unwrap_or([0, 0]));
                        if len_prefix & 0x8000 != 0 {
                            curr += 4;
                        } else {
                            curr += 2;
                        }
                    }
                    let mut u16_chars = Vec::new();
                    while curr + 2 <= chunk.len() {
                        let c = u16::from_le_bytes(chunk[curr..curr + 2].try_into().unwrap_or([0, 0]));
                        if c == 0 {
                            break;
                        }
                        u16_chars.push(c);
                        curr += 2;
                    }
                    let s = String::from_utf16_lossy(&u16_chars);
                    pool.push(s);
                }
            }
        }

        Ok(pool)
    }
}
