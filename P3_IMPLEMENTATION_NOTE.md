# P3 Implementation: system-register semantics (slices A–F)

**Worktree:** `~/workspace/wt-emu-p3impl` (branch `feat/emu-p3-impl`, based at `7d8f4c2`)
**Date:** 2026-10-03
**Status:** Implemented, tests green. NOT pushed, NOT merged (reviewer gate merges).

## Scope

Static scan (`P3_COVERAGE_BACKLOG.md` / `coverage_backlog.json`, 242 rows) found
**51 P3 rows in 7 slices** the emulator didn't implement. P0 (HVC/PSCI,
CONTEXTIDR_EL1, SP=0) and P1/P2 (VBAR/HCR/MAIR/TCR/TTBR0 MRS, PAR_EL1,
OSLAR_EL1, PMCNTENSET/PMSELR, ACTLR_EL1) were verified present in the
integration tip (`git log` shows `1034d04 feat(emu): P2 misc system
registers` etc.) — not redone.

This run implements **all 51 P3 rows**, grouped into 6 slices (the backlog's
7th "slice" was the P4 remainder, 191 rows, explicitly out of scope):

| Slice | Rows | Registers implemented |
|---|---|---|
| A — GICv3 CPU interface | 10 | ICC_SRE_EL1 (MRS/MSR), ICC_CTLR_EL1 (MRS/MSR), ICC_IGRPEN1_EL1 (MRS/MSR), ICC_PMR_EL1 (MRS/MSR), ICC_IAR1_EL1 (MRS→0x3ff), ICC_EOIR1_EL1 (MSR, WO), ICC_DIR_EL1 (MSR, WO) |
| B — ID family completion | 19 | ID_PFR0/1, ID_DFR0, ID_MMFR0–3, ID_ISAR0–5, MVFR0–2, REVIDR, ID_AA64DFR1, ID_AA64ZFR0 — all MRS→0 |
| C — PMU remainder | 9 | PMCNTENCLR_EL0, PMOVSCLR_EL0, PMXEVTYPER_EL0, PMXEVCNTR_EL0, PMUSERENR_EL0 (MRS/MSR), PMCEID0/1_EL0 (MRS→0), PMBIDR_EL1 (MRS→0) |
| D — timers | 6 | CNTKCTL_EL1 (MRS/MSR), TPIDR_EL2 (MRS/MSR — P2's constant-0 arm replaced), CNTP_TVAL_EL0 (MRS/MSR, honest CVAL alias), CNTV_TVAL_EL0 (MRS/MSR, alias) |
| E — FP/SIMD | 4 | FPCR, FPSR (MRS/MSR each) |
| F — debug | 3 | OSDLR_EL1 (MSR, WO), OSLSR_EL1 (MRS→0) |

51/51 P3 rows land. 191 P4 rows remain untouched.

## Design decisions (per-slice)

- **GICv3:** no GIC model (out of scope) — register-level semantics only.
  `ICC_SRE_EL1` defaults to 1 (SRE=1) so the kernel's gicv3 probe takes the
  sysreg path; `ICC_PMR_EL1` defaults to 0 (all masked — honest, we deliver
  no interrupts); `ICC_IAR1_EL1` reads `0x3ff` (spurious/no-pending, so the
  entry.S `el1_irq` path never sees a phantom interrupt); EOIR1/DIR are
  write-only accepts (stored, no behavior).
- **ID registers:** all 0 — the GB-14/GB-16 conservative pattern. Kernel
  cpuinfo + alternatives framework take the generic fallback path instead of
  patching in optimized sequences we can't execute. Deliberately not real
  silicon IDs (same reasoning as the MIDR_EL1=0 decision in GB-26).
- **PMU:** stored u64s, no counter behavior. PMUSERENR_EL0 graduated from
  P2's MSR no-op to stored so MSR/MRS round-trip (kernel writes xzr → reads
  0 = EL0 PMU access disabled, which matches our enforcement).
- **Timers:** `CNTP_TVAL_EL0`/`CNTV_TVAL_EL0` are *derived aliases*, not
  stored registers: read = low 32 bits of (CVAL − counter); write sets
  CVAL = counter + value[31:0]. This is the honest architectural definition,
  and delay loops terminate because the counter actually advances.
  TPIDR_EL2 moved from P2's constant-0 MRS to a stored register so the
  (hyp, unreachable-at-EL1) MSR/MRS pair agrees.
- **FPCR/FPSR:** stored u64 (32-bit registers in 64-bit slots; upper bits
  round-trip the kernel's own value).
- **Debug:** OSDLR_EL1 write-only stored; OSLSR_EL1 → 0 (OSLK=0).

## Honest-trap cases (deliberate, tested)

- `MRS ICC_EOIR1_EL1`, `MRS ICC_DIR_EL1`, `MRS OSDLR_EL1` (write-only
  encodings) → `R_SYSTEM` trap, never a silent value. Covered by
  `p3_write_only_sysregs_trap_on_mrs`.

## Files changed

1. **contracts/src/cpu.rs** — 18 new `SysReg` variants (27–44), documented.
2. **contracts/src/machine.rs** — 16 new `SysRegs` u64 fields (TVALs are
   derived, no backing field), defaults, `load()`/`store()` arms,
   `from_index()` arms (27–44). `SNAPSHOT_VERSION` 11 → 12.
3. **units/u2-ir-lift/src/lib.rs** — MRS/MSR decode arms; 23 new ID/PMU/debug
   constant arms; `high_frequency_sysregs_lift_correctly` updated for the
   TPIDR_EL2 graduation.
4. **units/u11-snapshot/src/lib.rs** — 16 fields in writer + reader;
   `CPU_ENCODED_BYTES` 60·8 → 76·8; both seed tests updated.
5. **units/u15-exec-wasmi/src/lib.rs** — roundtrip test extended to all 16
   stored P3 regs; new `backend_p3_tval_derives_from_cval`,
   `backend_p3_gic_defaults` tests.
6. **units/u12-orchestrator/tests/sysreg_p3.rs** (new) + 5 fixture files
   (`fixtures/sysreg_p3_{gic,id,pmu,timer,fp}.json`) — real
   decode→lift→compile→execute sequences through `Orchestrator::step_vcpu`.

## Test results

| Crate | Result |
|---|---|
| `cargo test -p pathn-contracts --lib` | 18 passed, 0 failed (incl. 2 new: TVAL alias, GIC defaults) |
| `cargo test -p u2-ir-lift --lib` | 114 passed, 0 failed (incl. 3 new: P3 constants, WO-trap, extended case tables) |
| `cargo test -p u11-snapshot --lib` | 16 passed, 0 failed |
| `cargo test -p u15-exec-wasmi --lib` | 9 passed, 0 failed (incl. 2 new: TVAL derive, GIC defaults) |
| `cargo test -p u12-orchestrator --test sysreg_p3` | 5 passed, 0 failed (5 fixture sequences, 51 P3 encodings covered end-to-end) |
| `cargo test -p u12-orchestrator --lib` (3 known OOM snapshot/persist tests skipped) | 110 passed, 0 failed — matches the P2 baseline |

All run with `-- --test-threads=2` (u12 with `--test-threads=1`), builds before
tests, no OOM.

## What remains

- **191 P4 rows** — untouched, all still trap with `R_SYSTEM` (loud, not
  silent). Nothing new was guessed at.
- **GIC model** — interrupts are never delivered; IAR1 always reads 0x3ff.
  A real distributor/redistributor + IRQ injection is the next big slice when
  the box returns (needs QEMU traces to validate against).
- **Full-boot validation** — this work is static-semantics + unit/fixture
  level. The boot halt position may move once these registers stop trapping;
  the reviewer gate owns re-measuring.
- Snapshot format bumped to v12: old v11 snapshots fail with
  `VersionMismatch` (correct — the format changed).

## Gaps / risks (honest)

- TVAL-as-alias is exact only because we control the counter sync; if the
  devices track later changes counter ticking, `store`/`load` still agree
  (both derive from the same counter), so no drift is possible.
- PMU registers are stored, not counting: a kernel PMU driver will read
  back its own writes and see zero counters. That's honest (no PMU exists),
  but `perf` on the guest will report flat zeros.
- ID registers all read 0 including `ID_AA64ZFR0` (no SVE) and
  `ID_AA64DFR1`. If the kernel later gates a needed feature on one of
  these, the specific bit can be raised with a measured justification.
