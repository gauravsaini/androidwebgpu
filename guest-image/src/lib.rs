//! Path N U9: reproducible ARM64 bare-metal guest-image build pipeline.
//!
//! This crate assembles a tiny bare-metal AArch64 shell guest (`pathn-sh`)
//! and packages it as a deterministic image with a JSON SBOM.
//!
//! - [`asm`]: mini-assembler — AArch64 instruction encoders with
//!   hand-computed golden words.
//! - [`guest`]: the `pathn-sh` shell guest, built by a two-pass label
//!   assembler over [`asm`].
//! - [`image`]: pure `build(manifest) -> (image_bytes, sbom_json)` pipeline,
//!   `PNIM` header layout, and the [`image::initial_cpu_state`] entry contract.
//! - [`platform`]: the physical memory map shared with `PLATFORM.md`.
//! - [`sha256`]: hand-written SHA-256 for SBOM hashing (no new dependency).
//!
//! Honest bounds: this is a bare-metal shell, not Linux and not AOSP. Nothing
//! here boots or executes the guest; actual boot execution belongs to leaf 5.1
//! (U12 orchestrator), which implements its device side against
//! `guest-image/PLATFORM.md`.

pub mod asm;
pub mod guest;
pub mod image;
pub mod platform;
pub mod rootfs;
pub mod sha256;

pub use image::{build, initial_cpu_state, parse_header, GuestManifest};
pub use rootfs::{FileEntry, Rootfs, RootfsError};
