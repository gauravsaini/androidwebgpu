//! Pure deterministic guest-image pipeline.
//!
//! `build(manifest)` assembles the shell guest, lays out the image blob
//! (code at the load address, zero-padded up to the data page, then the data
//! section), prepends the `PNIM` header, and returns the image bytes plus a
//! JSON SBOM that hashes every input. No I/O, no randomness, no clock.

use crate::guest::{assemble_shell, ShellImage};
use crate::platform::{
    in_ram, DATA_BASE, DATA_OFFSET, GUEST_LOAD_ADDR, HEADER_LEN, IMAGE_MAGIC, IMAGE_VERSION,
    STACK_TOP,
};
use crate::sha256::{hex, sha256};
use pathn_contracts::machine::{CpuState, SysRegs};
use serde::Serialize;

/// Build manifest: the only input to [`build`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestManifest {
    /// Human-readable guest name, recorded in the SBOM.
    pub name: String,
    /// Image format version (must equal [`IMAGE_VERSION`]).
    pub version: u32,
    /// Guest load address == entry point (must equal [`GUEST_LOAD_ADDR`]).
    pub load_addr: u64,
}

impl GuestManifest {
    /// The canonical `pathn-sh` manifest.
    pub fn pathn_sh() -> Self {
        GuestManifest {
            name: "pathn-sh".to_string(),
            version: IMAGE_VERSION,
            load_addr: GUEST_LOAD_ADDR,
        }
    }
}

impl GuestManifest {
    /// Canonical JSON of the manifest itself (fixed field order, no whitespace).
    fn canonical_json(&self) -> String {
        format!(
            "{{\"name\":{:?},\"version\":{},\"load_addr\":{}}}",
            self.name, self.version, self.load_addr
        )
    }
}

#[derive(Serialize)]
struct SbomInput {
    name: &'static str,
    sha256: String,
    bytes: usize,
}

#[derive(Serialize)]
struct Sbom {
    format: &'static str,
    image: &'static str,
    format_version: u32,
    inputs: Vec<SbomInput>,
    image_sha256: String,
}

fn words_to_le_bytes(words: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(words.len() * 4);
    for w in words {
        out.extend_from_slice(&w.to_le_bytes());
    }
    out
}

/// Assemble the guest and return `(image_bytes, sbom_json)`.
///
/// Image layout: `[header 24B][guest blob]`, where the blob is
/// `[code][zero pad to DATA_OFFSET][data]` and the header is
/// `magic "PNIM" | u32 version | u64 entry | u64 blob size` (little-endian).
///
/// Panics if the manifest disagrees with the platform contract.
pub fn build(manifest: &GuestManifest) -> (Vec<u8>, String) {
    assert_eq!(manifest.version, IMAGE_VERSION, "manifest version mismatch");
    assert_eq!(
        manifest.load_addr, GUEST_LOAD_ADDR,
        "manifest load_addr mismatch"
    );
    assert!(in_ram(manifest.load_addr), "load address outside RAM");

    let ShellImage { code, data } = assemble_shell();
    let code_bytes = words_to_le_bytes(&code);

    let mut blob = Vec::with_capacity(DATA_OFFSET as usize + data.len());
    blob.extend_from_slice(&code_bytes);
    assert!(
        blob.len() <= DATA_OFFSET as usize,
        "guest code overflows the reserved page"
    );
    blob.resize(DATA_OFFSET as usize, 0);
    blob.extend_from_slice(&data);
    assert_eq!(&blob[DATA_OFFSET as usize..], &data[..]);

    let entry = manifest.load_addr;
    let mut image = Vec::with_capacity(HEADER_LEN + blob.len());
    image.extend_from_slice(&IMAGE_MAGIC);
    image.extend_from_slice(&IMAGE_VERSION.to_le_bytes());
    image.extend_from_slice(&entry.to_le_bytes());
    image.extend_from_slice(&(blob.len() as u64).to_le_bytes());
    image.extend_from_slice(&blob);

    let code_hash = hex(&sha256(&code_bytes));
    let data_hash = hex(&sha256(&data));
    let manifest_json = manifest.canonical_json();
    let manifest_hash = hex(&sha256(manifest_json.as_bytes()));
    let image_hash = hex(&sha256(&image));

    let sbom = Sbom {
        format: "pathn-sbom",
        image: "guest-image",
        format_version: 1,
        inputs: vec![
            SbomInput {
                name: "guest-code",
                sha256: code_hash,
                bytes: code_bytes.len(),
            },
            SbomInput {
                name: "guest-data",
                sha256: data_hash,
                bytes: data.len(),
            },
            SbomInput {
                name: "manifest",
                sha256: manifest_hash,
                bytes: manifest_json.len(),
            },
        ],
        image_sha256: image_hash,
    };
    let sbom_json = serde_json::to_string(&sbom).expect("SBOM serialization is infallible");

    (image, sbom_json)
}

/// Parse the 24-byte `PNIM` header. Returns `(version, entry, blob_size)`.
pub fn parse_header(image: &[u8]) -> Option<(u32, u64, u64)> {
    if image.len() < HEADER_LEN || image[0..4] != IMAGE_MAGIC {
        return None;
    }
    let version = u32::from_le_bytes(image[4..8].try_into().ok()?);
    let entry = u64::from_le_bytes(image[8..16].try_into().ok()?);
    let size = u64::from_le_bytes(image[16..24].try_into().ok()?);
    Some((version, entry, size))
}

/// The CPU state leaf 5.1 must install before jumping to the entry point:
/// `pc` = load address, `sp` = RAM top, MMU off, single vCPU (a host-side
/// concern documented in `PLATFORM.md`, not something this crate can enforce).
pub fn initial_cpu_state(manifest: &GuestManifest) -> CpuState {
    CpuState {
        regs: [0u64; 31],
        sp: STACK_TOP,
        sp_el1: 0,
        pc: manifest.load_addr,
        pstate: 0,
        sysregs: SysRegs::default(),
    }
}

/// Absolute address of the data section for a loaded image.
pub fn data_base() -> u64 {
    DATA_BASE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guest::{assemble_shell, D_PROMPT};
    use crate::platform::{ranges_overlap, CONSOLE_BASE, CONSOLE_SIZE, RAM_BASE, RAM_SIZE};

    #[test]
    fn image_build_deterministic() {
        let m = GuestManifest::pathn_sh();
        let (img1, sbom1) = build(&m);
        let (img2, sbom2) = build(&m);
        assert_eq!(img1, img2);
        assert_eq!(sbom1, sbom2);
        assert!(!img1.is_empty() && !sbom1.is_empty());
    }

    #[test]
    fn image_header_fields() {
        let (image, _) = build(&GuestManifest::pathn_sh());
        assert_eq!(&image[0..4], b"PNIM");
        let (version, entry, size) = parse_header(&image).expect("header must parse");
        assert_eq!(version, IMAGE_VERSION);
        assert_eq!(entry, GUEST_LOAD_ADDR);
        assert_eq!(size as usize, image.len() - HEADER_LEN);
        // First blob word = ADRP x10 (entry instruction).
        assert_eq!(
            u32::from_le_bytes(image[HEADER_LEN..HEADER_LEN + 4].try_into().unwrap()) >> 31,
            1
        );
        assert!(parse_header(b"short").is_none());
        assert!(parse_header(b"XXXX rest of header padding...").is_none());
    }

    #[test]
    fn image_sbom_valid() {
        let m = GuestManifest::pathn_sh();
        let (image, sbom_json) = build(&m);
        let sbom: serde_json::Value = serde_json::from_str(&sbom_json).expect("SBOM must be JSON");
        assert_eq!(sbom["format"], "pathn-sbom");
        assert_eq!(sbom["format_version"], 1);
        let inputs = sbom["inputs"].as_array().expect("inputs must be an array");
        assert_eq!(inputs.len(), 3);

        // Recompute every input hash from the image bytes and compare.
        let blob = &image[HEADER_LEN..];
        let shell = assemble_shell();
        let code_bytes = words_to_le_bytes(&shell.code);
        let data = &blob[DATA_OFFSET as usize..DATA_OFFSET as usize + shell.data.len()];
        let expected = [
            ("guest-code", hex(&sha256(&code_bytes))),
            ("guest-data", hex(&sha256(data))),
            ("manifest", hex(&sha256(m.canonical_json().as_bytes()))),
        ];
        for (i, (name, hash)) in expected.iter().enumerate() {
            assert_eq!(inputs[i]["name"], *name);
            assert_eq!(inputs[i]["sha256"], *hash, "SBOM hash mismatch for {name}");
        }
        assert_eq!(sbom["image_sha256"], hex(&sha256(&image)));
        // Prompt string really is inside the blob at the data page.
        let prompt_off = HEADER_LEN + DATA_OFFSET as usize + D_PROMPT as usize;
        assert_eq!(&image[prompt_off..prompt_off + 11], b"pathn-sh> \0");
    }

    #[test]
    fn image_platform_ranges() {
        // Entry inside RAM; console MMIO outside RAM and non-overlapping.
        assert!(in_ram(GUEST_LOAD_ADDR));
        assert!(!in_ram(CONSOLE_BASE));
        assert!(!ranges_overlap(
            CONSOLE_BASE,
            CONSOLE_BASE + CONSOLE_SIZE,
            RAM_BASE,
            RAM_BASE + RAM_SIZE
        ));
        let (_, _, size) = parse_header(&build(&GuestManifest::pathn_sh()).0).unwrap();
        assert!(
            GUEST_LOAD_ADDR + size <= RAM_BASE + RAM_SIZE,
            "image overflows RAM"
        );
    }

    #[test]
    fn image_initial_cpu_state_contract() {
        let st = initial_cpu_state(&GuestManifest::pathn_sh());
        assert_eq!(st.pc, GUEST_LOAD_ADDR);
        assert_eq!(st.sp, STACK_TOP);
        assert_eq!(st.pstate, 0);
        assert!(st.regs.iter().all(|r| *r == 0));
    }
}
