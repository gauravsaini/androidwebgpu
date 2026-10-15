# T499 matched-oracle

## Question

Does the kernel's CSINV at guest VA `0xffffff8008354018` produce the same result in native QEMU and the Rust emulator when both execute the pinned guest word with identical input state? If the live traces differ, does the divergence occur in instruction semantics or before the instruction?

## Pinned family

- Manifest: [`MANIFEST.json`](../../MANIFEST.json)
- Guest files: `/Users/Shared/guest-inputs/Image`, `minimal-virt-fixed.dtb`, and `initramfs.cpio`; each SHA-256 is computed from the supplied file and checked by `scripts/verify_manifest.py`.
- Native QEMU: 11.1.2, cortex-a53, 1024 MiB, `virt,gic-version=2,virtio-mmio-transports=1`, TCG single-thread.
- Rust source revision: `bebb568cb2862a49a82f1eb8933959929b2439b4`.

## Observations

- Native QEMU's serial stream reached `Run /init as init process`. PID 1 printed `INITRAMFS ROOTFS MOUNTED AT / OK` and its userspace PASS marker. See [`qemu-manifest-boot.log`](../../qemu-manifest-boot.log). This is guest serial output, not harness output.
- The Image bytes at offset `0x2d4018` are `58 80 81 da` (little-endian word `0xda818058`). QEMU's live guest and the Rust live runner both reached that word.
- On the live second hit, QEMU had PSTATE `0xa00001c5` and returned X24=`0x40000000`; Rust had PSTATE `0x60000000` and returned X24=`0xffffffffbfffffff`. X1 and X2 matched; X8 also differed.
- In the direct matched-state control, both engines received the same word, X1/X2, X8, X24, and PSTATE `0xa00001c5`; both advanced PC by four and returned X24=`0x40000000`.
- The live output difference is explained by the pre-instruction condition flags: QEMU has C=1,Z=0, while Rust has C=1,Z=1. The matched-state control does not identify the earlier source of that Rust PSTATE divergence.
- The memblock store rewrite and vmemmap fault population handler are disabled by default and require `--enable-guest-quirks`.

## Decision and limits

`CSINV verdict: EMULATOR-BUG`, scoped to the observed live Rust execution state at this site. The control shows the CSINV operation agrees when state is matched. One boot and one live capture are recorded; the TEN-PASSES threshold was not run, so this is not labeled reproducible. The first GICv3 setup attempt contradicted the pinned DTB's GICv2 node and is retained only as setup history; it is not treated as a pinned-manifest wall.

The approved DAFG acceptance ledger [`GATES-T499.md`](../../GATES-T499.md) reports G499-1, G499-2, and G499-3 as MET.

## Raw witnesses

- [`evidence/t499/qemu-csinv-live-hit2.log`](../../evidence/t499/qemu-csinv-live-hit2.log)
- [`evidence/t499/rust-csinv-live-hit2.log`](../../evidence/t499/rust-csinv-live-hit2.log)
- [`evidence/t499/qemu-csinv-micro.log`](../../evidence/t499/qemu-csinv-micro.log)
- [`evidence/t499/rust-csinv-micro.log`](../../evidence/t499/rust-csinv-micro.log)
- [`evidence/t499/qemu-gicv3-mismatch.log`](../../evidence/t499/qemu-gicv3-mismatch.log)
