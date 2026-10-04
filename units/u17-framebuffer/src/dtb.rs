//! Device Tree (DTB) Simple-Framebuffer Node Model & Validator.
//!
//! Conforms to Linux simple-framebuffer bindings:
//! `Documentation/devicetree/bindings/display/simple-framebuffer.yaml`
//!
//! # Specification
//! Node name: `framebuffer@10000000`
//! - `compatible`: `"simple-framebuffer"`
//! - `reg`: `<0x0 0x10000000 0x0 0x0012c000>` (base: 0x1000_0000, size: 0x12_C000 / 1,228,800 bytes)
//! - `width`: `<640>` (u32)
//! - `height`: `<480>` (u32)
//! - `stride`: `<2560>` (u32)
//! - `format`: `"a8b8g8r8"` (string)
//! - `status`: `"okay"` (string)

use crate::model::{FB_BASE, FB_FORMAT, FB_HEIGHT, FB_SIZE, FB_STRIDE, FB_WIDTH};

/// Parsed properties of a simple-framebuffer DTB node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleFbProperties {
    pub node_name: String,
    pub compatible: String,
    pub base_addr: u64,
    pub size: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: String,
    pub status: String,
}

impl SimpleFbProperties {
    /// Return the canonical reference properties matching the frozen hardware contract.
    pub fn canonical() -> Self {
        Self {
            node_name: format!("framebuffer@{:x}", FB_BASE),
            compatible: "simple-framebuffer".to_string(),
            base_addr: FB_BASE,
            size: FB_SIZE,
            width: FB_WIDTH,
            height: FB_HEIGHT,
            stride: FB_STRIDE,
            format: FB_FORMAT.to_string(),
            status: "okay".to_string(),
        }
    }
}

/// Generate the Device Tree Source (DTS) fragment for the simple-framebuffer node.
pub fn generate_dts_node() -> String {
    format!(
        r#"	framebuffer0: framebuffer@{base:x} {{
		compatible = "simple-framebuffer";
		reg = <0x0 0x{base:08x} 0x0 0x{size:08x}>;
		width = <{width}>;
		height = <{height}>;
		stride = <{stride}>;
		format = "{format}";
		status = "okay";
	}};"#,
        base = FB_BASE,
        size = FB_SIZE,
        width = FB_WIDTH,
        height = FB_HEIGHT,
        stride = FB_STRIDE,
        format = FB_FORMAT
    )
}

/// FDT Token constants per Devicetree Specification v0.4
const FDT_MAGIC: u32 = 0xD00DFEED;
const FDT_BEGIN_NODE: u32 = 1;
const FDT_END_NODE: u32 = 2;
const FDT_PROP: u32 = 3;
const FDT_NOP: u32 = 4;
const FDT_END: u32 = 9;

/// Builder to construct an FDT binary node for simple-framebuffer.
pub struct SimpleFbFdtBuilder {
    struct_data: Vec<u8>,
    string_data: Vec<u8>,
}

impl Default for SimpleFbFdtBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleFbFdtBuilder {
    pub fn new() -> Self {
        Self {
            struct_data: Vec::new(),
            string_data: Vec::new(),
        }
    }

    fn add_string(&mut self, s: &str) -> u32 {
        if let Some(pos) = self
            .string_data
            .windows(s.len() + 1)
            .position(|window| window[..s.len()] == *s.as_bytes() && window[s.len()] == 0)
        {
            return pos as u32;
        }
        let offset = self.string_data.len() as u32;
        self.string_data.extend_from_slice(s.as_bytes());
        self.string_data.push(0);
        offset
    }

    fn align_struct(&mut self) {
        while self.struct_data.len() % 4 != 0 {
            self.struct_data.push(0);
        }
    }

    fn prop(&mut self, name: &str, value: &[u8]) {
        let name_offset = self.add_string(name);
        self.struct_data.extend_from_slice(&FDT_PROP.to_be_bytes());
        self.struct_data
            .extend_from_slice(&(value.len() as u32).to_be_bytes());
        self.struct_data.extend_from_slice(&name_offset.to_be_bytes());
        self.struct_data.extend_from_slice(value);
        self.align_struct();
    }

    fn prop_str(&mut self, name: &str, s: &str) {
        let mut v = Vec::from(s.as_bytes());
        v.push(0);
        self.prop(name, &v);
    }

    fn prop_u32(&mut self, name: &str, val: u32) {
        self.prop(name, &val.to_be_bytes());
    }

    fn prop_u64_pair(&mut self, name: &str, a: u64, b: u64) {
        let mut v = Vec::with_capacity(16);
        v.extend_from_slice(&a.to_be_bytes());
        v.extend_from_slice(&b.to_be_bytes());
        self.prop(name, &v);
    }

    /// Build the FDT structure bytes and strings table for the simple-framebuffer node.
    pub fn build_node_tokens(&mut self) -> (&[u8], &[u8]) {
        let node_name = format!("framebuffer@{:x}", FB_BASE);
        self.struct_data
            .extend_from_slice(&FDT_BEGIN_NODE.to_be_bytes());
        self.struct_data.extend_from_slice(node_name.as_bytes());
        self.struct_data.push(0);
        self.align_struct();

        self.prop_str("compatible", "simple-framebuffer");
        self.prop_u64_pair("reg", FB_BASE, FB_SIZE);
        self.prop_u32("width", FB_WIDTH);
        self.prop_u32("height", FB_HEIGHT);
        self.prop_u32("stride", FB_STRIDE);
        self.prop_str("format", FB_FORMAT);
        self.prop_str("status", "okay");

        self.struct_data
            .extend_from_slice(&FDT_END_NODE.to_be_bytes());
        (&self.struct_data, &self.string_data)
    }
}

/// Validate a compiled Device Tree Blob (DTB) to ensure it contains a valid simple-framebuffer node
/// with exact matching properties.
pub fn validate_dtb(dtb: &[u8]) -> Result<SimpleFbProperties, String> {
    if dtb.len() < 40 {
        return Err("DTB blob too small (< 40 bytes header)".to_string());
    }
    let magic = u32::from_be_bytes(dtb[0..4].try_into().unwrap());
    if magic != FDT_MAGIC {
        return Err(format!("Bad DTB magic: {magic:#010x}, expected {FDT_MAGIC:#010x}"));
    }

    let totalsize = u32::from_be_bytes(dtb[4..8].try_into().unwrap()) as usize;
    if dtb.len() < totalsize {
        return Err(format!("Truncated DTB: length {}, totalsize {totalsize}", dtb.len()));
    }

    let off_struct = u32::from_be_bytes(dtb[8..12].try_into().unwrap()) as usize;
    let off_strings = u32::from_be_bytes(dtb[12..16].try_into().unwrap()) as usize;
    let size_struct = u32::from_be_bytes(dtb[36..40].try_into().unwrap()) as usize;

    if off_struct + size_struct > dtb.len() {
        return Err("DTB struct block out of bounds".to_string());
    }

    // Parse FDT tokens
    let mut cursor = off_struct;
    let struct_end = off_struct + size_struct;
    let mut node_stack: Vec<String> = Vec::new();
    let mut current_node_name = String::new();
    let mut current_props: std::collections::HashMap<String, Vec<u8>> =
        std::collections::HashMap::new();

    let get_string = |name_offset: usize| -> Result<String, String> {
        let str_start = off_strings + name_offset;
        if str_start >= dtb.len() {
            return Err("String offset beyond DTB end".to_string());
        }
        let mut end = str_start;
        while end < dtb.len() && dtb[end] != 0 {
            end += 1;
        }
        String::from_utf8(dtb[str_start..end].to_vec())
            .map_err(|e| format!("Invalid UTF-8 in DTB string: {e}"))
    };

    while cursor < struct_end {
        let token = u32::from_be_bytes(dtb[cursor..cursor + 4].try_into().unwrap());
        cursor += 4;

        match token {
            FDT_BEGIN_NODE => {
                let name_start = cursor;
                while cursor < struct_end && dtb[cursor] != 0 {
                    cursor += 1;
                }
                let name = String::from_utf8_lossy(&dtb[name_start..cursor]).into_owned();
                cursor += 1; // skip null
                while cursor % 4 != 0 {
                    cursor += 1;
                }
                node_stack.push(current_node_name);
                current_node_name = name;
                current_props.clear();
            }
            FDT_END_NODE => {
                // Check if this node is simple-framebuffer
                let is_fb = current_props
                    .get("compatible")
                    .and_then(|v| std::str::from_utf8(v).ok())
                    .map(|s| s.trim_matches('\0').contains("simple-framebuffer"))
                    .unwrap_or(false)
                    || current_node_name.starts_with("framebuffer");

                if is_fb {
                    // Extract properties
                    let compatible = current_props
                        .get("compatible")
                        .and_then(|v| std::str::from_utf8(v).ok())
                        .map(|s| s.trim_matches('\0').to_string())
                        .unwrap_or_default();

                    let (base_addr, size) = if let Some(reg_bytes) = current_props.get("reg") {
                        if reg_bytes.len() >= 16 {
                            // Assume 64-bit base + 64-bit size (#address-cells = 2, #size-cells = 2)
                            let b = u64::from_be_bytes(reg_bytes[0..8].try_into().unwrap());
                            let s = u64::from_be_bytes(reg_bytes[8..16].try_into().unwrap());
                            (b, s)
                        } else if reg_bytes.len() >= 8 {
                            let b = u32::from_be_bytes(reg_bytes[0..4].try_into().unwrap()) as u64;
                            let s = u32::from_be_bytes(reg_bytes[4..8].try_into().unwrap()) as u64;
                            (b, s)
                        } else {
                            (0, 0)
                        }
                    } else {
                        (0, 0)
                    };

                    let width = current_props
                        .get("width")
                        .and_then(|v| {
                            if v.len() >= 4 {
                                Some(u32::from_be_bytes(v[0..4].try_into().unwrap()))
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);

                    let height = current_props
                        .get("height")
                        .and_then(|v| {
                            if v.len() >= 4 {
                                Some(u32::from_be_bytes(v[0..4].try_into().unwrap()))
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);

                    let stride = current_props
                        .get("stride")
                        .and_then(|v| {
                            if v.len() >= 4 {
                                Some(u32::from_be_bytes(v[0..4].try_into().unwrap()))
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);

                    let format = current_props
                        .get("format")
                        .and_then(|v| std::str::from_utf8(v).ok())
                        .map(|s| s.trim_matches('\0').to_string())
                        .unwrap_or_default();

                    let status = current_props
                        .get("status")
                        .and_then(|v| std::str::from_utf8(v).ok())
                        .map(|s| s.trim_matches('\0').to_string())
                        .unwrap_or_else(|| "okay".to_string());

                    return Ok(SimpleFbProperties {
                        node_name: current_node_name,
                        compatible,
                        base_addr,
                        size,
                        width,
                        height,
                        stride,
                        format,
                        status,
                    });
                }

                current_node_name = node_stack.pop().unwrap_or_default();
                current_props.clear();
            }
            FDT_PROP => {
                let prop_len =
                    u32::from_be_bytes(dtb[cursor..cursor + 4].try_into().unwrap()) as usize;
                let name_off =
                    u32::from_be_bytes(dtb[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
                cursor += 8;
                let val_bytes = dtb[cursor..cursor + prop_len].to_vec();
                cursor += prop_len;
                while cursor % 4 != 0 {
                    cursor += 1;
                }
                let prop_name = get_string(name_off)?;
                current_props.insert(prop_name, val_bytes);
            }
            FDT_NOP => {}
            FDT_END => break,
            other => return Err(format!("Unknown FDT token {other:#x} at offset {cursor}")),
        }
    }

    Err("No simple-framebuffer node found in DTB".to_string())
}

/// Splicing utility: inject the simple-framebuffer node into an existing DTB binary right before root node close.
pub fn inject_simple_framebuffer_into_dtb(orig_dtb: &[u8]) -> Result<Vec<u8>, String> {
    if orig_dtb.len() < 40 {
        return Err("DTB too small".to_string());
    }
    let magic = u32::from_be_bytes(orig_dtb[0..4].try_into().unwrap());
    if magic != FDT_MAGIC {
        return Err("Invalid DTB magic".to_string());
    }

    // Build the new node
    let mut builder = SimpleFbFdtBuilder::new();
    let (node_tokens, node_strings) = builder.build_node_tokens();

    let off_struct = u32::from_be_bytes(orig_dtb[8..12].try_into().unwrap()) as usize;
    let off_strings = u32::from_be_bytes(orig_dtb[12..16].try_into().unwrap()) as usize;
    let off_rsvmap = u32::from_be_bytes(orig_dtb[16..20].try_into().unwrap()) as usize;
    let size_strings = u32::from_be_bytes(orig_dtb[32..36].try_into().unwrap()) as usize;
    let size_struct = u32::from_be_bytes(orig_dtb[36..40].try_into().unwrap()) as usize;

    // Find insertion point: right before the last FDT_END_NODE (which closes the root node)
    let struct_slice = &orig_dtb[off_struct..off_struct + size_struct];
    let mut insert_offset_in_struct = 0;
    let mut cursor = 0;
    while cursor + 4 <= struct_slice.len() {
        let token = u32::from_be_bytes(struct_slice[cursor..cursor + 4].try_into().unwrap());
        if token == FDT_END_NODE {
            // Check if next token is FDT_END (meaning this is the root end node)
            if cursor + 8 <= struct_slice.len() {
                let next_token =
                    u32::from_be_bytes(struct_slice[cursor + 4..cursor + 8].try_into().unwrap());
                if next_token == FDT_END {
                    insert_offset_in_struct = cursor;
                    break;
                }
            }
        }
        cursor += 4;
        if token == FDT_PROP {
            if cursor + 8 <= struct_slice.len() {
                let prop_len =
                    u32::from_be_bytes(struct_slice[cursor..cursor + 4].try_into().unwrap())
                        as usize;
                cursor += 8 + prop_len;
                while cursor % 4 != 0 {
                    cursor += 1;
                }
            }
        } else if token == FDT_BEGIN_NODE {
            while cursor < struct_slice.len() && struct_slice[cursor] != 0 {
                cursor += 1;
            }
            cursor += 1;
            while cursor % 4 != 0 {
                cursor += 1;
            }
        }
    }

    if insert_offset_in_struct == 0 {
        return Err("Could not find root FDT_END_NODE insertion point".to_string());
    }

    // Assemble new DTB
    let before_insert = &orig_dtb[off_struct..off_struct + insert_offset_in_struct];
    let after_insert = &orig_dtb[off_struct + insert_offset_in_struct..off_struct + size_struct];

    // Adjust string offsets in the new node tokens
    let string_offset_base = size_strings as u32;
    let mut adjusted_node_tokens = node_tokens.to_vec();
    let mut tc = 0;
    while tc + 4 <= adjusted_node_tokens.len() {
        let tok = u32::from_be_bytes(adjusted_node_tokens[tc..tc + 4].try_into().unwrap());
        tc += 4;
        if tok == FDT_PROP {
            let plen =
                u32::from_be_bytes(adjusted_node_tokens[tc..tc + 4].try_into().unwrap()) as usize;
            let current_soff =
                u32::from_be_bytes(adjusted_node_tokens[tc + 4..tc + 8].try_into().unwrap());
            let new_soff = current_soff + string_offset_base;
            adjusted_node_tokens[tc + 4..tc + 8].copy_from_slice(&new_soff.to_be_bytes());
            tc += 8 + plen;
            while tc % 4 != 0 {
                tc += 1;
            }
        } else if tok == FDT_BEGIN_NODE {
            while tc < adjusted_node_tokens.len() && adjusted_node_tokens[tc] != 0 {
                tc += 1;
            }
            tc += 1;
            while tc % 4 != 0 {
                tc += 1;
            }
        }
    }

    let mut new_struct = Vec::new();
    new_struct.extend_from_slice(before_insert);
    new_struct.extend_from_slice(&adjusted_node_tokens);
    new_struct.extend_from_slice(after_insert);

    let mut new_strings = Vec::new();
    new_strings.extend_from_slice(&orig_dtb[off_strings..off_strings + size_strings]);
    new_strings.extend_from_slice(node_strings);

    // Reconstruct full DTB blob
    let rsvmap_len = off_struct - off_rsvmap;
    let new_off_rsvmap = 40;
    let new_off_struct = new_off_rsvmap + rsvmap_len;
    let new_off_strings = new_off_struct + new_struct.len();
    let new_totalsize = new_off_strings + new_strings.len();

    let mut out = Vec::with_capacity(new_totalsize);
    out.extend_from_slice(&FDT_MAGIC.to_be_bytes());
    out.extend_from_slice(&(new_totalsize as u32).to_be_bytes());
    out.extend_from_slice(&(new_off_struct as u32).to_be_bytes());
    out.extend_from_slice(&(new_off_strings as u32).to_be_bytes());
    out.extend_from_slice(&(new_off_rsvmap as u32).to_be_bytes());
    out.extend_from_slice(&17u32.to_be_bytes()); // version
    out.extend_from_slice(&16u32.to_be_bytes()); // last_comp_version
    out.extend_from_slice(&0u32.to_be_bytes()); // boot_cpuid_phys
    out.extend_from_slice(&(new_strings.len() as u32).to_be_bytes());
    out.extend_from_slice(&(new_struct.len() as u32).to_be_bytes());

    // rsvmap
    out.extend_from_slice(&orig_dtb[off_rsvmap..off_rsvmap + rsvmap_len]);
    // struct
    out.extend_from_slice(&new_struct);
    // strings
    out.extend_from_slice(&new_strings);

    Ok(out)
}
