//! ARM64 Linux / AOSP kernel image packaging and boot protocol types.
//!
//! Provides pure parsers for the Linux ARM64 Image header (`Documentation/arm64/booting.rst`),
//! deterministic PNIM packaging for kernel + initramfs + DTB payloads, and the
//! entry contract generator for Linux vCPU boot.

use crate::platform::{
    in_ram, HEADER_LEN, IMAGE_MAGIC, IMAGE_VERSION, RAM_BASE, STACK_TOP,
};
use crate::sha256::{hex, sha256};
use pathn_contracts::machine::{CpuState, SysRegs};
use serde::Serialize;

/// 64-bit ARM Linux kernel Image header magic: "ARM\x64" (LE u32 0x644d5241).
pub const ARM64_KERNEL_MAGIC: u32 = 0x644d_5241;

/// Standard Linux ARM64 header length (64 bytes).
pub const ARM64_HEADER_LEN: usize = 64;

/// Parsed ARM64 Linux kernel header fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arm64Header {
    pub code0: u32,
    pub code1: u32,
    pub text_offset: u64,
    pub image_size: u64,
    pub flags: u64,
}

/// Errors parsing an ARM64 Linux kernel image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelHeaderError {
    TooShort { have: usize, need: usize },
    BadMagic { found: u32 },
}

/// Parse an ARM64 Linux kernel image header per `Documentation/arm64/booting.rst`.
///
/// Header format:
/// - 0x00: `code0` (u32, LE)
/// - 0x04: `code1` (u32, LE)
/// - 0x08: `text_offset` (u64, LE)
/// - 0x10: `image_size` (u64, LE)
/// - 0x18: `flags` (u64, LE)
/// - 0x20..0x38: reserved
/// - 0x38: `magic` (u32, LE, must equal `0x644d5241`)
pub fn parse_kernel_header(bytes: &[u8]) -> Result<Arm64Header, KernelHeaderError> {
    if bytes.len() < ARM64_HEADER_LEN {
        return Err(KernelHeaderError::TooShort {
            have: bytes.len(),
            need: ARM64_HEADER_LEN,
        });
    }
    let magic = u32::from_le_bytes(bytes[0x38..0x3c].try_into().unwrap());
    if magic != ARM64_KERNEL_MAGIC {
        return Err(KernelHeaderError::BadMagic { found: magic });
    }
    let code0 = u32::from_le_bytes(bytes[0x00..0x04].try_into().unwrap());
    let code1 = u32::from_le_bytes(bytes[0x04..0x08].try_into().unwrap());
    let text_offset = u64::from_le_bytes(bytes[0x08..0x10].try_into().unwrap());
    let image_size = u64::from_le_bytes(bytes[0x10..0x18].try_into().unwrap());
    let flags = u64::from_le_bytes(bytes[0x18..0x20].try_into().unwrap());

    Ok(Arm64Header {
        code0,
        code1,
        text_offset,
        image_size,
        flags,
    })
}

/// Manifest for a kernel-based guest image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelManifest {
    pub name: String,
    pub version: u32,
    pub load_addr: u64,
    pub ramdisk_offset: Option<u64>,
    pub dtb_offset: Option<u64>,
}

impl KernelManifest {
    pub fn new(name: impl Into<String>, load_addr: u64) -> Self {
        Self {
            name: name.into(),
            version: IMAGE_VERSION,
            load_addr,
            ramdisk_offset: None,
            dtb_offset: None,
        }
    }

    fn canonical_json(&self) -> String {
        format!(
            "{{\"name\":{:?},\"version\":{},\"load_addr\":{},\"ramdisk_offset\":{:?},\"dtb_offset\":{:?}}}",
            self.name, self.version, self.load_addr, self.ramdisk_offset, self.dtb_offset
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
struct KernelSbom {
    format: &'static str,
    image: &'static str,
    format_version: u32,
    inputs: Vec<SbomInput>,
    image_sha256: String,
}

/// Deterministically package a kernel, optional initramfs, and optional DTB into
/// a PNIM-format image and generate a complete SBOM.
pub fn build_kernel_image(
    manifest: &KernelManifest,
    kernel_bytes: &[u8],
    ramdisk_bytes: Option<&[u8]>,
    dtb_bytes: Option<&[u8]>,
) -> (Vec<u8>, String) {
    assert_eq!(manifest.version, IMAGE_VERSION, "manifest version mismatch");
    assert!(in_ram(manifest.load_addr), "load address outside RAM");

    // Layout blob:
    // [kernel] [zero pad to ramdisk_offset] [ramdisk] [zero pad to dtb_offset] [dtb]
    let mut blob = Vec::new();
    blob.extend_from_slice(kernel_bytes);

    let ramdisk_len = if let (Some(offset), Some(rd)) = (manifest.ramdisk_offset, ramdisk_bytes) {
        let off = offset as usize;
        if blob.len() < off {
            blob.resize(off, 0);
        }
        blob.extend_from_slice(rd);
        rd.len()
    } else {
        0
    };

    let dtb_len = if let (Some(offset), Some(dtb)) = (manifest.dtb_offset, dtb_bytes) {
        let off = offset as usize;
        if blob.len() < off {
            blob.resize(off, 0);
        }
        blob.extend_from_slice(dtb);
        dtb.len()
    } else {
        0
    };

    let entry = manifest.load_addr;
    let mut image = Vec::with_capacity(HEADER_LEN + blob.len());
    image.extend_from_slice(&IMAGE_MAGIC);
    image.extend_from_slice(&IMAGE_VERSION.to_le_bytes());
    image.extend_from_slice(&entry.to_le_bytes());
    image.extend_from_slice(&(blob.len() as u64).to_le_bytes());
    image.extend_from_slice(&blob);

    let kernel_hash = hex(&sha256(kernel_bytes));
    let manifest_json = manifest.canonical_json();
    let manifest_hash = hex(&sha256(manifest_json.as_bytes()));
    let image_hash = hex(&sha256(&image));

    let mut inputs = vec![
        SbomInput {
            name: "kernel-image",
            sha256: kernel_hash,
            bytes: kernel_bytes.len(),
        },
        SbomInput {
            name: "manifest",
            sha256: manifest_hash,
            bytes: manifest_json.len(),
        },
    ];

    if let Some(rd) = ramdisk_bytes {
        inputs.push(SbomInput {
            name: "ramdisk",
            sha256: hex(&sha256(rd)),
            bytes: ramdisk_len,
        });
    }

    if let Some(dtb) = dtb_bytes {
        inputs.push(SbomInput {
            name: "dtb",
            sha256: hex(&sha256(dtb)),
            bytes: dtb_len,
        });
    }

    let sbom = KernelSbom {
        format: "pathn-sbom",
        image: "kernel-guest-image",
        format_version: 1,
        inputs,
        image_sha256: image_hash,
    };
    let sbom_json = serde_json::to_string(&sbom).expect("SBOM serialization is infallible");

    (image, sbom_json)
}

/// Establish the Linux ARM64 initial CPU state per the kernel boot protocol:
/// - `x0` = physical address of DTB in system RAM
/// - `x1..x3` = 0
/// - `pc` = kernel entry point (`manifest.load_addr`)
/// - `sp` = RAM top
pub fn initial_kernel_cpu_state(manifest: &KernelManifest) -> CpuState {
    let mut regs = [0u64; 31];
    if let Some(dtb_off) = manifest.dtb_offset {
        regs[0] = RAM_BASE + dtb_off;
    }
    CpuState {
        regs,
        sp: STACK_TOP,
        pc: manifest.load_addr,
        pstate: 0,
        sysregs: SysRegs::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_arm64_header() {
        let mut header = [0u8; 64];
        // code0, code1
        header[0..4].copy_from_slice(&0x91005a4du32.to_le_bytes());
        header[4..8].copy_from_slice(&0x144effffu32.to_le_bytes());
        // text_offset = 0x80000
        header[8..16].copy_from_slice(&0x00080000u64.to_le_bytes());
        // image_size = 0x166d000
        header[16..24].copy_from_slice(&0x0166d000u64.to_le_bytes());
        // flags = 0xa
        header[24..32].copy_from_slice(&0x0au64.to_le_bytes());
        // magic at 0x38 = 0x644d5241 ("ARM\x64")
        header[0x38..0x3c].copy_from_slice(&ARM64_KERNEL_MAGIC.to_le_bytes());

        let parsed = parse_kernel_header(&header).expect("must parse");
        assert_eq!(parsed.code0, 0x91005a4d);
        assert_eq!(parsed.code1, 0x144effff);
        assert_eq!(parsed.text_offset, 0x80000);
        assert_eq!(parsed.image_size, 0x166d000);
        assert_eq!(parsed.flags, 0xa);
    }

    #[test]
    fn parse_bad_magic_fails() {
        let mut header = [0u8; 64];
        header[0x38..0x3c].copy_from_slice(&0x12345678u32.to_le_bytes());
        let res = parse_kernel_header(&header);
        assert_eq!(res, Err(KernelHeaderError::BadMagic { found: 0x12345678 }));
    }

    #[test]
    fn parse_short_buffer_fails() {
        let res = parse_kernel_header(&[0u8; 32]);
        assert_eq!(
            res,
            Err(KernelHeaderError::TooShort {
                have: 32,
                need: ARM64_HEADER_LEN
            })
        );
    }

    #[test]
    fn build_kernel_image_deterministic() {
        let manifest = KernelManifest {
            name: "test-kernel".to_string(),
            version: IMAGE_VERSION,
            load_addr: RAM_BASE + 0x80000,
            ramdisk_offset: Some(0x2000_000),
            dtb_offset: Some(0x1000_000),
        };
        let kernel = b"FAKE_ARM64_KERNEL_BLOB";
        let ramdisk = b"FAKE_RAMDISK";
        let dtb = b"FAKE_DTB";

        let (img1, sbom1) =
            build_kernel_image(&manifest, kernel, Some(ramdisk), Some(dtb));
        let (img2, sbom2) =
            build_kernel_image(&manifest, kernel, Some(ramdisk), Some(dtb));

        assert_eq!(img1, img2);
        assert_eq!(sbom1, sbom2);
        assert!(!img1.is_empty());

        let cpu = initial_kernel_cpu_state(&manifest);
        assert_eq!(cpu.pc, RAM_BASE + 0x80000);
        assert_eq!(cpu.regs[0], RAM_BASE + 0x1000_000); // DTB at x0
        assert_eq!(cpu.regs[1], 0);
        assert_eq!(cpu.sp, STACK_TOP);
    }
}
