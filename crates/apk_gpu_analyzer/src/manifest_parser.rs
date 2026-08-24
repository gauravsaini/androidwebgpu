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

        let mut offset = 8;
        let mut string_pool: Vec<String> = Vec::new();
        let mut info = ParsedManifestInfo::default();

        while offset + 8 <= bytes.len() {
            let chunk_type = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap_or([0, 0]));
            let header_size = u16::from_le_bytes(bytes[offset + 2..offset + 4].try_into().unwrap_or([0, 0])) as usize;
            let chunk_size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap_or([0, 0, 0, 0])) as usize;

            if chunk_size == 0 || offset + chunk_size > bytes.len() {
                break;
            }

            match chunk_type {
                RES_STRING_POOL_TYPE => {
                    if let Ok(pool) = Self::parse_string_pool(&bytes[offset..offset + chunk_size]) {
                        string_pool = pool;
                    }
                }
                RES_XML_START_ELEMENT_TYPE => {
                    if offset + header_size + 20 <= offset + chunk_size {
                        let attr_count_offset = offset + 28;
                        if attr_count_offset + 2 <= bytes.len() {
                            let attr_count = u16::from_le_bytes(
                                bytes[attr_count_offset..attr_count_offset + 2]
                                    .try_into()
                                    .unwrap_or([0, 0]),
                            ) as usize;

                            let mut attr_cursor = offset + 36;
                            for _ in 0..attr_count {
                                if attr_cursor + 20 <= offset + chunk_size {
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
                        }
                    }
                }
                _ => {}
            }

            offset += chunk_size;
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
                    let mut end = str_abs;
                    while end < chunk.len() && chunk[end] != 0 {
                        end += 1;
                    }
                    let s = String::from_utf8_lossy(&chunk[str_abs..end]).to_string();
                    pool.push(s);
                } else {
                    let mut u16_chars = Vec::new();
                    let mut curr = str_abs;
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
