# P0 Implementation Summary

**Worktree:** `~/workspace/wt-emu-impl` (branch `feat/emu-missing-instr`)
**Date:** 2026-10-03
**Status:** Implementation complete, tests in progress.

## 1. HVC (PSCI) — `IrOp::Hvc`

**Problem:** DTB declares PSCI `method="hvc"`. The kernel uses HVC for PSCI calls during boot. Previously trapped with `R_HVC`.

**Implementation:** Honest PSCI stub across 6 files:

| File | Change |
|------|--------|
| `contracts/src/cpu.rs` | Added `IrOp::Hvc` variant with doc comment |
| `contracts/src/execution.rs` | Added `hvc(&mut self, func_id: i64) -> Result<i64, String>` to `HostOps` trait |
| `units/u2-ir-lift/src/lib.rs` | `lift_system`: HVC (`0b000 if ll == 0b10`) now emits `vec![IrOp::Hvc]` instead of `trap(R_HVC)` |
| `units/u3-wasm-jit/src/lib.rs` | Added `FUNC_HVC=5` (FUNC_RUN→6), type 5 `(i64)->(i64)`, `import_func("hvc", 5)`, `IrOp::Hvc` compilation (reg_get X0 → call → reg_set X0). Updated 2 structural tests for new import count. |
| `units/u15-exec-wasmi/src/lib.rs` | Registered `env.hvc` host function; added `hvc` to test `RamHost` |
| `units/u15-exec-wasmtime/src/lib.rs` | Registered `env.hvc` host function |
| `units/u12-orchestrator/src/lib.rs` | Implemented `hvc()`: PSCI_VERSION (0x84000000) → 0x00010000 (v1.0); all others → -1 (PSCI_NOT_SUPPORTED) |

**Semantics:** Unknown PSCI function IDs return -1 in X0 (honest failure, not silent success). The kernel will use fallback paths.

## 2. CONTEXTIDR_EL1 MSR

**Problem:** Kernel writes CONTEXTIDR_EL1 on every context switch. Previously trapped with `R_SYSTEM`.

**Implementation:** Simple stored u64, no behavior:

| File | Change |
|------|--------|
| `contracts/src/cpu.rs` | Added `SysReg::ContextidrEl1 = 21` |
| `contracts/src/machine.rs` | Added `pub contextidr_el1: u64` to `SysRegs`, Default=0, `read()`/`store()` arms, `from_index` 21 (also fixed missing 15-20 timer mappings) |
| `units/u2-ir-lift/src/lib.rs` | Added `(3, 0, 13, 0, 1) => SysReg::ContextidrEl1` to both MRS and MSR persistent matches |
| `units/u11-snapshot/src/lib.rs` | Added `contextidr_el1` to serialize/deserialize/test seeds (4 sites) |

## 3. SP Contract Fix

**Problem:** `load_image` set `cpu.sp = RAM_BASE + RAM_SIZE` (0x80000000 with 1GiB). Corrected AOSP boot contract requires SP=0 at kernel entry (per QEMU-observed state, verified by step0_gate.py).

**Changes** (`units/u12-orchestrator/src/lib.rs`):
- Line 869: `cpu.sp = RAM_BASE + RAM_SIZE` → `cpu.sp = 0`
- Test `load_image_sets_entry_contract`: `assert_eq!(cpu.sp, 0x4800_0000)` → `assert_eq!(cpu.sp, 0)`

## Test Results

- `u2-ir-lift`: 112 passed, 0 failed ✅
- `u3-wasm-jit`: 39 passed, 0 failed ✅ (after fixing type section + 2 structural tests)
- `u12-orchestrator`: 113 passed, 0 failed ✅ (single-threaded, 570s; includes updated SP contract test)
- `pathn-contracts`: builds ✅
- `u11-snapshot`: builds ✅ (fixed SysRegs initializer + serialization)

## Files Changed

```
contracts/src/cpu.rs                    (SysReg::ContextidrEl1, IrOp::Hvc)
contracts/src/execution.rs              (HostOps::hvc)
contracts/src/machine.rs                (SysRegs.contextidr_el1 + mappings)
units/u2-ir-lift/src/lib.rs             (HVC lift, CONTEXTIDR MSR/MRS, test update)
units/u3-wasm-jit/src/lib.rs            (FUNC_HVC, type 5, import, IrOp::Hvc codegen, 2 test updates)
units/u11-snapshot/src/lib.rs           (serialize/deserialize contextidr_el1)
units/u12-orchestrator/src/lib.rs       (hvc() PSCI dispatch, SP=0, test update)
units/u15-exec-wasmi/src/lib.rs         (hvc host registration + test RamHost)
units/u15-exec-wasmtime/src/lib.rs      (hvc host registration)
```
