# T157 QEMU wasm64 Bundle Artifact Manifest

This manifest records SHA-256 checksums, byte sizes, and local filesystem origins for all artifacts belonging to the T157 QEMU 11.1.2 wasm64 browser execution milestone (2026-10-09 16:30 AEDT).

Per project policy, **large binaries (>1 MiB, .wasm, .rom, large logs) remain OUT of Git** and reside locally on this Mac. Only clean source code, build recipes, launchers, manifests, and concise evidence excerpts are tracked in version control.

---

## 1. Binary Assets (Excluded from Git — Mac Local Storage)

All runtime binary assets are preserved in read-only format at:
`/Users/Shared/wt-track157-qemuwasm/browser-bundle/assets/`

| Filename | Size (bytes) | SHA-256 Checksum | Description |
|---|---|---|---|
| `qemu-system-aarch64.wasm` | 58,398,971 (55.7 MiB) | `1f958af8543bd6320c3c29b5be2b62780cc3be1d26dc02794cbc9505e98ba676` | QEMU 11.1.2 wasm64 TCI binary compiled with Emscripten `-sMEMORY64=2` |
| `qemu-system-aarch64.js` | 405,940 (396 KiB) | `d62a3abcf837d912a5e75500f5db3bae2934e4f0418884b6a4b0fb4e41c1c2b1` | Emscripten modular JavaScript glue code |
| `Image` | 23,073,280 (22.0 MiB) | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | ARM64 Linux v4 kernel image |
| `initramfs.cpio` | 6,656 (6.5 KiB) | `49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38` | Early userspace rootfs containing static `/init` binary |
| `minimal-virt-fixed.dtb` | 1,465 (1.4 KiB) | `f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0` | Flattened Device Tree for ARM `virt` platform |
| `roms/efi-e1000.rom` | 159,232 | `f37222fcdf0481bfbaf7a16d9aaf740aeb41d3d24c187801eb6e9a628cec0fc8` | QEMU pc-bios EFI ROM |
| `roms/efi-e1000e.rom` | 159,232 | `102c20348be2d0ebe45a77b076b31848182c2f7b4364775b6bc2ece6e97b4024` | QEMU pc-bios EFI ROM |
| `roms/efi-eepro100.rom` | 159,232 | `2b5b55ba8afae7b5ef7116606babf60e6ceaa531c345d54542857cc3fbe3912e` | QEMU pc-bios EFI ROM |
| `roms/efi-ne2k_pci.rom` | 157,696 | `9925e0a34547118110479d8fc4e01092a0a73e0a236416e9ff862655b46751f7` | QEMU pc-bios EFI ROM |
| `roms/efi-pcnet.rom` | 157,696 | `71c494fde94efa9871cf43868d7e3c8f4190e406d47a5102a2a21b36ded8a162` | QEMU pc-bios EFI ROM |
| `roms/efi-rtl8139.rom` | 160,768 | `4cdd69a7f5c6ce7aec3a3f20a034dc80e1d22aa37a739c364378952d997a32c7` | QEMU pc-bios EFI ROM |
| `roms/efi-virtio.rom` | 160,768 | `26be36901db7f8181c306cc62bd74891d8646528965a78e40cceadba5dd7c8e7` | QEMU pc-bios EFI ROM |
| `roms/efi-vmxnet3.rom` | 156,672 | `33e833123ad9831df94b2d623a17f91e4c779e3cb799baa97058f5972676a32a` | QEMU pc-bios EFI ROM |
| `roms/pxe-e1000.rom` | 67,072 | `981a728f95b7a01645645e9b753cfb764c9eb59266fa888ac58d7008fbb48c10` | QEMU pc-bios PXE ROM |
| `roms/pxe-eepro100.rom` | 61,440 | `07b89a742e0f6d0e92dafe57b0f28c493436d0e1e07b7b77e728490c1f7b45dd` | QEMU pc-bios PXE ROM |
| `roms/pxe-ne2k_pci.rom` | 61,440 | `bb9c0eaad36b9b4ce26c5f348bd05a832c8bf83ef38391db0ce9b51de8523ac7` | QEMU pc-bios PXE ROM |
| `roms/pxe-pcnet.rom` | 61,440 | `92358396c05b3346c6bfd52bbf2a47978d0a8c6ddea932ee091a24faefbd489e` | QEMU pc-bios PXE ROM |
| `roms/pxe-rtl8139.rom` | 61,440 | `d491351e37a50eb5a40e98a9f6b4d85010ee60fb9e64129d65ea10839287f138` | QEMU pc-bios PXE ROM |
| `roms/pxe-virtio.rom` | 60,416 | `6994337fe07783a6233072e33771dc4f58260edc3269debe677a5227b264bfdc` | QEMU pc-bios PXE ROM |
| `roms/qboot.rom` | 65,536 | `9b9dfc6c25740d6225625570d71cab6805cc9216e68c8932e343266daaeb8c4b` | QEMU pc-bios minimal x86/PCI bootloader ROM |

---

## 2. Source Deliverables (Committed to Git)

These files constitute the reproducible implementation and are committed in the repository under `browser-bundle/`:

| Path | Size (bytes) | SHA-256 Checksum | Description |
|---|---|---|---|
| `browser-bundle/index.html` | 926 | `ca63bec46b81a0e17e0aa6d577e9c20edb2196a1cc16b5566fd0373632272a9f` | Browser harness interface with serial output viewport |
| `browser-bundle/launch.js` | 3,579 | `d8bec5e07b3500a7de94dcf39a916f05e6b1b7b8460f5bbd177b3f4a765d7a06` | ES module harness: loads guest files, staging ROMs, passes `-L /qemu-data` |
| `browser-bundle/serve.py` | 1,077 | `7b0105e410a3472ee754649b48ec697c44cb99f688f8bf9031507f75015a2e8f` | Python 3 HTTP server with COOP/COEP headers for SharedArrayBuffer support |
| `browser-bundle/build/BUILD_RECIPE.md` | 6,594 | (version controlled) | Full technical recipe for cross-compiling QEMU 11.1.2 and dependencies |
| `browser-bundle/build/build-qemu-wasm.sh` | 3,511 | (version controlled) | Automated build script verifying environment and compiling QEMU wasm64 |
| `browser-bundle/build/pkg-config-wasm64` | 393 | (version controlled) | Target prefix pkg-config cross-wrapper |
| `browser-bundle/evidence/browser-qemuwasm-init-excerpt.log` | 1,936 | `a4e320f77d4da1c54b2fc59d95f877ca6704fa4b0cb9d7fa999ec6d62ff8bfae` | Concise 36-line log excerpt showing `Run /init as init process` milestone |
| `browser-bundle/README.md` | (version controlled) | (version controlled) | Step-by-step reproduction instructions |
| `browser-bundle/MANIFEST.md` | (this file) | (this file) | Artifact verification ledger |

---

## 3. Local Mac Storage Paths

To inspect or reproduce builds locally on this machine:

- **Original Bundle & Assets (Read-Only Source)**:
  `/Users/Shared/wt-track157-qemuwasm/browser-bundle/`
- **Full Serial Execution Log (13 KiB, 219 lines)**:
  `/Users/Shared/wt-track157-qemuwasm/browser-qemuwasm-init.log`
- **Compiled QEMU Build Directory**:
  `/Users/Shared/toolchain/qemu-11.1.2/build-wasm64-v7f/`
- **Wasm64 Dependency Prefix (zlib, pixman, pcre2, libffi, glib, libfdt)**:
  `/Users/Shared/toolchain/t157-v5-deps/target/`
- **Emscripten SDK (6.0.12)**:
  `/Users/Shared/toolchain/emsdk/`
- **DTC / libfdt Staged Source Archive**:
  `/Users/Shared/toolchain/wasm-deps/dtc.tar.gz`
- **T157 Full Feasibility Survey & Engineering Record**:
  `/Users/Shared/codex-track157-qemuwasm.md`
