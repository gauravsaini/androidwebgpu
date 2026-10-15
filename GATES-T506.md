# T506 fresh Rust PID-1 boot acceptance gates

Status at checkpoint commit: boot not yet launched; no boot outcome is claimed.

## Pinned inputs

Source manifest: `/Users/Shared/wt-track499-matchedoracle/MANIFEST.json` (`t499.matched-oracle.v1`). All paths below exist and their SHA-256 values match the manifest:

| Input | Path | SHA-256 |
|---|---|---|
| Image | `/Users/Shared/guest-inputs/Image` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` |
| DTB | `/Users/Shared/guest-inputs/minimal-virt-fixed.dtb` | `f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0` |
| initramfs | `/Users/Shared/guest-inputs/initramfs.cpio` | `49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38` |

The checkout is `ec0a01706dc2e282ae33d47a2ad67d1c1ca8bae2`; `e29771b` is an ancestor. The manifest's historical `rust_binary_rev` is not used as the T506 build revision.

## Acceptance

- **G506-1 — pending build:** build `run_kernel` from this worktree/revision. Do not pass `--enable-guest-quirks`; the runner must report `enabled=false`. Record the full binary source revision.
- **G506-2 — pending fresh boot:** launch the Rust runner from reset with only the pinned Image, DTB, and initramfs. Accept PID-1 only when the exact guest-emitted UART marker `[PATHN-TRACK-B] ARM64 STATIC /INIT LAUNCHED (PID 1)` is present in the emulator's guest console bytes. Harness output is not a guest witness.
- **G506-3 — pending:** if PID-1 is not reached, capture the first stopped point with step count, PC, expected behavior, and observed behavior, then compare with the pinned QEMU oracle where possible. If PID-1 is reached, require ten consecutive fresh accepted runs before claiming reproducibility.

The marker text above is visible in the existing pinned QEMU guest UART capture and in the supplied initramfs `/init`; that prior evidence does not satisfy G506-2 for a Rust run.

## Prior T503 suite note

T503's full `cargo test --workspace` result was **349 passed, 1 failed of 350**. The single failure was `tests::vmemmap_l1_gap_step_vcpu_instruction_retry` on the LDRB/MMU translation path (`TranslationFault`), reproduced in isolation and recorded as unrelated to the ORN fix. Keep this exception visible in the final T506 record.
