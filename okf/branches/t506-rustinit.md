# T506 fresh Rust PID-1 boot

## Question

Does the T503-fixed Rust runner boot the exact T499 pinned Image, DTB, and initramfs from reset, with guest quirks disabled, until the guest `/init` emits its PID-1 UART marker?

## Branch relationship

This is a separate full-boot experiment family. T503 closed the pinned ORN oracle gate with ten QEMU/Rust state pairs; it did not establish a Rust PID-1 marker. T506 carries the fixed revision forward without changing the guest inputs or enabling legacy guest quirks.

## Pinned inputs and revision

- Source manifest: `/Users/Shared/wt-track499-matchedoracle/MANIFEST.json`.
- Image SHA-256: `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04`.
- DTB SHA-256: `f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0`.
- initramfs SHA-256: `49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38`.
- T506 worktree HEAD: `ec0a01706dc2e282ae33d47a2ad67d1c1ca8bae2`, with T503 fix `e29771b` in its ancestry.

## Acceptance rule

G506-1 records the release binary source revision and requires `guest-quirks enabled=false`. G506-2 requires the exact guest-emitted line `[PATHN-TRACK-B] ARM64 STATIC /INIT LAUNCHED (PID 1)` in Rust's guest console capture. G506-3 records the first divergence if the marker is absent, or requires ten consecutive fresh PID-1 marker runs before a reproducibility claim. The T499 QEMU marker is reference evidence only, not a Rust pass.

Raw run captures belong outside `okf/`. The gate ledger is [`GATES-T506.md`](../../GATES-T506.md); external findings are `/Users/Shared/codex-track506-rustinit.md`.

## T503 suite note

T503's full workspace suite was 349/350. The sole failure, `tests::vmemmap_l1_gap_step_vcpu_instruction_retry`, was an unrelated LDRB/MMU `TranslationFault` reproduced in isolation. Preserve that note in T506's final record.

## Status at checkpoint

Input hashes match the pinned manifest. The Rust release build and T506 boot have not yet been run; no PID-1 claim is made.
