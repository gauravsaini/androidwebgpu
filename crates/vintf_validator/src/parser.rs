//! XML Parser for VINTF Manifests.

use crate::error::VintfError;
use crate::manifest::{HalFormat, HalManifestEntry, VintfManifest};

/// Parse a VINTF manifest XML string into `VintfManifest`.
pub fn parse_manifest_xml(xml: &str) -> Result<VintfManifest, VintfError> {
    let stripped = strip_xml_comments(xml);
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        return Err(VintfError::XmlParse("Empty XML document".to_string()));
    }

    // Parse root <manifest ...> tag
    let (root_tag, body) = extract_tag_and_body(trimmed, "manifest")
        .ok_or_else(|| VintfError::XmlParse("Missing <manifest> root tag".to_string()))?;

    let version = extract_attribute(&root_tag, "version")
        .unwrap_or_else(|| "5.0".to_string());
    let manifest_type = extract_attribute(&root_tag, "type")
        .unwrap_or_else(|| "device".to_string());
    let target_level = extract_attribute(&root_tag, "target-level")
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| VintfError::MissingField("target-level attribute on <manifest>".to_string()))?;

    let mut hals = Vec::new();
    let mut cursor = body.as_str();

    while let Some((hal_open, hal_body, next_cursor)) = extract_next_tag_and_body(cursor, "hal") {
        let format_str = extract_attribute(&hal_open, "format")
            .unwrap_or_else(|| "aidl".to_string());
        let format = match format_str.to_lowercase().as_str() {
            "aidl" => HalFormat::Aidl,
            "hidl" => HalFormat::Hidl,
            other => return Err(VintfError::InvalidHalFormat(other.to_string())),
        };

        let name = extract_element_text(&hal_body, "name")
            .ok_or_else(|| VintfError::MissingField("<name> in <hal>".to_string()))?;

        let version = extract_element_text(&hal_body, "version")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);

        let mut fqnames = extract_all_element_texts(&hal_body, "fqname");

        // If interface & instance tags are used instead of fqname:
        if fqnames.is_empty() {
            if let Some(interface) = extract_element_text(&hal_body, "interface") {
                let instances = extract_all_element_texts(&hal_body, "instance");
                if instances.is_empty() {
                    fqnames.push(format!("{}/default", interface));
                } else {
                    for inst in instances {
                        fqnames.push(format!("{}/{}", interface, inst));
                    }
                }
            }
        }

        hals.push(HalManifestEntry {
            format,
            name,
            version,
            fqnames,
        });

        cursor = next_cursor;
    }

    Ok(VintfManifest {
        version,
        manifest_type,
        target_level,
        hals,
    })
}

fn strip_xml_comments(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("<!--") {
        output.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find("-->") {
            rest = &rest[start + end + 3..];
        } else {
            // Unclosed comment, terminate
            rest = "";
            break;
        }
    }
    output.push_str(rest);
    output
}

fn extract_attribute(tag_header: &str, attr_name: &str) -> Option<String> {
    let pattern = format!("{}=\"", attr_name);
    if let Some(idx) = tag_header.find(&pattern) {
        let after = &tag_header[idx + pattern.len()..];
        if let Some(end_quote) = after.find('"') {
            return Some(after[..end_quote].to_string());
        }
    }
    // Also try single quote
    let pattern_sq = format!("{}='", attr_name);
    if let Some(idx) = tag_header.find(&pattern_sq) {
        let after = &tag_header[idx + pattern_sq.len()..];
        if let Some(end_quote) = after.find('\'') {
            return Some(after[..end_quote].to_string());
        }
    }
    None
}

fn extract_tag_and_body(input: &str, tag_name: &str) -> Option<(String, String)> {
    let open_prefix = format!("<{}", tag_name);
    let close_tag = format!("</{}>", tag_name);

    let start_idx = input.find(&open_prefix)?;
    let tag_open_end = input[start_idx..].find('>')? + start_idx;
    let tag_header = input[start_idx..=tag_open_end].to_string();

    let close_idx = input[tag_open_end..].find(&close_tag)? + tag_open_end;
    let body = input[tag_open_end + 1..close_idx].to_string();

    Some((tag_header, body))
}

fn extract_next_tag_and_body<'a>(
    input: &'a str,
    tag_name: &str,
) -> Option<(String, String, &'a str)> {
    let open_prefix = format!("<{}", tag_name);
    let close_tag = format!("</{}>", tag_name);

    let start_idx = input.find(&open_prefix)?;
    let tag_open_end = input[start_idx..].find('>')? + start_idx;
    let tag_header = input[start_idx..=tag_open_end].to_string();

    let close_idx = input[tag_open_end..].find(&close_tag)? + tag_open_end;
    let body = input[tag_open_end + 1..close_idx].to_string();
    let next_cursor = &input[close_idx + close_tag.len()..];

    Some((tag_header, body, next_cursor))
}

fn extract_element_text(body: &str, elem_name: &str) -> Option<String> {
    let open_tag = format!("<{}>", elem_name);
    let close_tag = format!("</{}>", elem_name);

    let start = body.find(&open_tag)? + open_tag.len();
    let end = body[start..].find(&close_tag)? + start;
    Some(body[start..end].trim().to_string())
}

fn extract_all_element_texts(body: &str, elem_name: &str) -> Vec<String> {
    let open_tag = format!("<{}>", elem_name);
    let close_tag = format!("</{}>", elem_name);
    let mut results = Vec::new();
    let mut cursor = body;

    while let Some(start) = cursor.find(&open_tag) {
        let text_start = start + open_tag.len();
        if let Some(end) = cursor[text_start..].find(&close_tag) {
            let val = cursor[text_start..text_start + end].trim().to_string();
            results.push(val);
            cursor = &cursor[text_start + end + close_tag.len()..];
        } else {
            break;
        }
    }

    results
}
