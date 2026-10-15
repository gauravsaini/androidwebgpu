# T506 fresh Rust PID-1 boot acceptance gates

Status: Boot executed from reset on 2026-10-16. First divergence and halt point captured at step 44,738,735.

## Pinned inputs

Source manifest: `/Users/Shared/wt-track499-matchedoracle/MANIFEST.json` (`t499.matched-oracle.v1`). All paths below exist and their SHA-256 values match the manifest:

| Input | Path | SHA-256 | Verification |
|---|---|---|---|
| Image | `/Users/Shared/guest-inputs/Image` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | MATCH |
| DTB | `/Users/Shared/guest-inputs/minimal-virt-fixed.dtb` | `f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0` | MATCH |
| initramfs | `/Users/Shared/guest-inputs/initramfs.cpio` | `49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38` | MATCH |

The checkout is on branch `track506/rustinit`; `e29771b` (ORN fast-path fix) is an ancestor.

## Acceptance Results

- [x] **G506-1 — Release build & quirks disabled: MET**
  - Built `run_kernel` in release mode from revision `fcb45f8` (ancestor `e29771b`).
  - Runner confirmed `[guest-quirks] enabled=false` (quirks strictly disabled).
  - No survey mode, no snapshots loaded.
- [ ] **G506-2 — Fresh boot to exact guest PID-1 marker: NOT MET**
  - Guest stopped before reaching userspace `/init`.
  - Exact marker `[PATHN-TRACK-B] ARM64 STATIC /INIT LAUNCHED (PID 1)` was not observed on UART.
  - Guest console emitted 1,950 UART bytes (ending at `[ 0.000000] random: random: get_random_u64 called from __kmem_cache_create+0x8c/0x75c with crng_init=0`).
- [x] **G506-3 — First stopped point and divergence captured: MET**
  - Step count: `44,738,735`
  - PC: `0xffffff800946a438`
  - Instruction: `0xd4210000` (`BRK #0x800` / Linux `BUG()`)
  - Halt Reason: `Unsupported { addr: 18446743524109362232, reason: "System: BRK exception" }`
  - Divergence Mechanism: At `0xffffff800946a3d4`, `bl kasprintf`. At `0xffffff800946a3d8`, `cbz x0, 0xffffff800946a438`. In QEMU oracle, `kasprintf` succeeds returning allocated pointer `X0 = 0xffffffc03d803000`. In Rust emulator, `kasprintf` fails returning `X0 = 0x0` (NULL), causing `cbz` branch to `BUG()`.
  - Raw evidence: `/Users/Shared/agyx-logs/track506-agyx.log` and `/Users/Shared/agyx-logs/track506-guest-uart.log`.

## Prior T503 suite note

T503's full `cargo test --workspace` result was **349 passed, 1 failed of 350**. The single failure was `tests::vmemmap_l1_gap_step_vcpu_instruction_retry` on the LDRB/MMU translation path (`TranslationFault`), reproduced in isolation and recorded as unrelated to the ORN fix. Keep this exception visible in the final T506 record.
