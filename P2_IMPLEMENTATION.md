# P2 Implementation: Misc System Registers

**Worktree:** `~/workspace/wt-emu-p2` (branch `feat/emu-p2-misc`)
**Date:** 2026-10-03
**Status:** Implemented, tests green.

## What was implemented

5 new system registers, following the P0/P1 pattern:

| Register | Encoding (op0,op1,crn,crm,op2) | MRS | MSR | Behavior |
|---|---|---|---|---|
| PAR_EL1 | (3,0,7,4,0) | ✅ | — | Stored u64, default 0. Read after AT ops. |
| OSLAR_EL1 | (2,0,1,0,4) | — | ✅ | Write-only. Stored, no behavior. |
| PMCNTENSET_EL0 | (3,3,9,12,1) | ✅ | ✅ | Stored u64, default 0. PMU probe. |
| PMSELR_EL0 | (3,3,9,12,5) | ✅ | ✅ | Stored u64, default 0. PMU probe. |
| ACTLR_EL1 | (3,0,1,0,1) | ✅ | — | Returns 0 (honest: no aux features). |

## Files changed

1. **contracts/src/cpu.rs** — Added 5 SysReg variants (22-26): ParEl1, OslarEl1,
   PmcntensetEl0, PmselrEl0, ActlrEl1.

2. **contracts/src/machine.rs** — Added 5 u64 fields to SysRegs, Default impl,
   load()/store() arms, from_index() arms (22-26). Bumped SNAPSHOT_VERSION 10→11.

3. **units/u2-ir-lift/src/lib.rs** — Added MRS arms (PAR_EL1, ACTLR_EL1,
   PMCNTENSET_EL0, PMSELR_EL0) and MSR arms (OSLAR_EL1, PMCNTENSET_EL0,
   PMSELR_EL0). Updated test tables (mrs_cases, msr_cases).

4. **units/u11-snapshot/src/lib.rs** — Added 5 u64 fields to Writer::cpu() and
   Reader, updated CPU_ENCODED_BYTES 55*8 → 60*8, updated 2 test seeds.

## Design notes

- **OSLAR_EL1 is write-only**: intentionally NOT in the MRS match. An MRS
  OSLAR_EL1 encoding would be architecturally undefined and correctly traps
  with R_SYSTEM.
- **PAR_EL1 has no MSR arm**: the kernel never writes PAR_EL1 directly
  (it's written by AT instructions, which we NOP). MRS-only is correct.
- **ACTLR_EL1 has no MSR arm**: returning 0 on read is the honest answer;
  if the kernel writes it, that write will trap (acceptable — kernel doesn't
  write ACTLR on the boot path we care about).
- **Snapshot version bumped**: old snapshots (v10) will fail with
  VersionMismatch, which is the correct behavior for a format change.

## Test results

- `cargo test -p u2-ir-lift --lib`: 112 passed, 0 failed
- `cargo test -p u11-snapshot --lib`: 16 passed, 0 failed
- `cargo test -p u12-orchestrator --lib` (excluding 3 OOM tests): 110 passed, 0 failed
