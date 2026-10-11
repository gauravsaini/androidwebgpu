# T271: Rust virtio-blk MMIO device model

Status: PASS — the native Rust device completes a split-virtqueue read from the real `system.img`; the MMIO register sequence and values match the QEMU virtio-MMIO v2 contract used here.

## Claim

The Rust emulator now has a native, file-backed virtio-blk device at the guest MMIO dispatch layer. Its modern virtio-MMIO registers identify a block device and expose the read-only Android system image's 4,023,080 sectors. After feature negotiation, split-queue setup, and guest queue notification, an LBA0 read returns the exact 512 bytes from `/Users/Shared/android14-gsi/system.img`. A second read of nonzero LBA8 also matches byte-for-byte. The writable disk path supports `VIRTIO_BLK_T_OUT`; a fresh sparse 4 GiB image is available for `/data`.

## Scope

T271 device-model and MMIO-dispatch lane in `units/u12-orchestrator`. The probe uses a native unit-test guest-RAM buffer to program the QEMU-style MMIO sequence and submit descriptor chains through the actual Rust device. It also verifies the `WasmHost` guest-memory dispatcher can reach the MMIO registers. This is not a full Android guest boot or a guest Linux virtio-blk driver run.

## Device discovery

The destination device integration point is `units/u12-orchestrator/src/lib.rs`: `WasmHost::mem_load` and `mem_store` dispatch guest physical accesses; the existing console/UART path provides the MMIO-host pattern. Search found no Rust `simplefb` device model at `0x5f000000`; that address is a guest RAM reservation in the board memory map, while the current GPU model uses its own host path. The supplied virtio DTB nodes are at `0x0a000200`/IRQ 49 and `0x0a000400`/IRQ 50. The virtio dispatch precedes the overlapping GPU pseudo-MMIO window so the DTB transport windows resolve to block devices.

## Evidence

- `../../evidence/lba0-proof.json`: machine-readable expected and returned full sector hex for LBA0 and nonzero LBA8, SHA-256 pairs, used index, request status, and interrupt status. Both sector hashes match; LBA8 is nonzero.
- `../../evidence/virtio-probe.txt`: focused probe output; four virtio-named unit tests passed, including guest MMIO dispatch, GIC SPI IRQ 49 acknowledgment, the real-image read, and writable sparse-image write.
- `../../evidence/orchestrator-tests.txt`: full `u12-orchestrator` test-suite output.
- `../../evidence/run-kernel-build.txt` and `../../evidence/runner-attachment.txt`: runner binary build and a zero-step CLI launch that attaches the real readonly system image and the writable 4 GiB sparse disk at their configured MMIO addresses/IRQs.
- `../../evidence/boot35m-build.txt`: the external-asset `boot35m_extend` test target compiles with `--no-run`; the full package test invocation stopped before executing it because Cargo reported its executable missing. That test requires external `PATHN_TEST_KERNEL_IMAGE` and `PATHN_TEST_DTB` assets and is outside the T271 probe.
- `../../evidence/qemu-register-compare.txt`: exact local QEMU 11.1.2 source paths/line references for the register offsets, values, device ID, feature semantics, and default queue size.
- `../../evidence/device-discovery.txt`: device-model search and existing UART/MMIO dispatch references.
- `../../evidence/image-info.txt`: source image format/size, sparse `/data` logical size/allocation, and proof-file hash.
- `../../data-t271.img`: fresh sparse 4 GiB writable disk image; excluded from git.

## Compatibility notes

MMIO offsets and values match QEMU modern virtio-MMIO: magic `0x74726976`, version `2`, device ID `2`, vendor `0x554d4551`, queue size maximum `256`, capacity in 512-byte sectors, split-ring registers, status, interrupt, and config registers. The Rust device advertises the compatible subset it implements (`VERSION_1`, `BLK_SIZE`, and `RO` for the system image), rather than QEMU's optional block features it does not implement. The DTB base/IRQ mapping follows the prior T268/T269 guest artifacts.

## Limits

The evidence establishes register initialization, queue processing, LBA0 byte equality against the real input image, another nonzero-sector byte match, and write persistence to a fresh sparse disk. It does not establish a full Android boot on this Rust emulator.
