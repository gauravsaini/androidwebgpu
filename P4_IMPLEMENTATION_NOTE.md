# P4 Implementation: coverage-backlog system registers (186 rows)

**Worktree:** `~/workspace/wt-emu-p4impl` (branch `feat/emu-p4-impl`, based at `4a4b460`)
**Date:** 2026-10-03
**Status:** Implemented, tests green. NOT pushed, NOT merged (reviewer gate merges).
**Commits:** `c8cfc56` (contracts/state model) · `7d632cf` (decode arms) · `c0bc751` (fixtures/e2e)

## Scope

Static scan (`coverage_backlog.json`, 242 rows) left **191 P4 rows** after P3.
This run implements **186 of them** — every row with `access` in (MRS, MSR).
The 5 `exgen` rows (`brk`, `svc`, `smc`, `eret`, `hlt`) are intentionally
untouched: they are honest traps by design (exception-generation is not
modeled), same as in P0–P3.

Row groups implemented:

| Group | Rows | Registers |
|---|---|---|
| SVE control | 2 | ZCR_EL1 (MRS/MSR, stored u64, default 0) |
| Debug breakpoint/watchpoint | 128 | DBGBVR0–15, DBGBCR0–15, DBGWVR0–15, DBGWCR0–15 (MRS/MSR each; 64 new indexed SysReg variants, stored u64, no behavior — no debug hardware model) |
| EL2 (hyp-init writes) | 14 | VPIDR/VMPIDR/CPTR/MDCR/HSTR/ZCR/VBAR/ICH_HCR/VTTBR/SPSR/ELR/PMSCR_EL2 (MSR-only, stored); ICC_SRE_EL2 (MRS/MSR, stored, default 0x1 mirroring EL1) |
| GICv3 CPU MSRs | 9 | ICC_SGI1R_EL1 (WO accept, MRS traps), ICC_BPR1_EL1, ICC_AP0R0–3_EL1, ICC_AP1R0–3_EL1 (stored u64) |
| PMU cycle counter | 2 | PMCCNTR_EL0 (MRS/MSR, stored, default 0 — no PMU model, honest flat zero) |
| Misc | 2 | DISR_EL1, LORC_EL1 (MSR, stored u64) |
| Timer TVAL MSRs | 2 | CNTP_TVAL_EL0 / CNTV_TVAL_EL0 MSR rows map to the existing P3 derived-alias variants (write sets CVAL = counter + value[31:0]) |
| IMPDEF / reserved MRS | 16 | → constant 0 ("feature absent") |
| IMPDEF / reserved MSR | 10 | → honest no-op |

## Design decisions

- **Debug regs are persistent, not constant.** DBGBVRn_EL1 etc. are
  architecturally RW with real read semantics; the kernel's debug
  save/restore path (0x4019dxxx region) MSR-writes then MRS-reads them.
  Constant-0 MRS would break round-trip, so all 64 got indexed variants
  (discriminants 45–108) backed by 4×`[u64; 16]` arrays in `SysRegs`.
  No breakpoint behavior is modeled — there is no debug hardware.
- **ZCR_EL1 is stored, default 0.** The SVE probe at 0x4008474c
  (`mrs x3, zcr_el1` / `msr zcr_el1, x4`) must see a consistent value, not a
  trap. `ID_AA64ZFR0_EL1` already reads 0 (no SVE), so the kernel takes the
  no-SVE path after the probe.
- **EL2 regs are stored with no behavior.** All 13 come from KVM hyp-init
  code (0x40c83000 region) that runs at EL2 — unreachable in the guest, but
  the MSRs must not trap or the init sequence dies. MRS has no arm for the
  12 MSR-only EL2 regs (not in the static scan): an MRS traps `R_SYSTEM`
  loudly rather than inventing a value.
- **GIC CPU MSRs follow the P3 slice-A pattern.** `ICC_SGI1R_EL1` is
  write-only (generates an SGI); with no GIC model the MSR is accepted and
  stored, and an MRS is architecturally UNDEFINED → `R_SYSTEM` trap (tested).
  BPR1/AP0R/AP1R are stored u64 (the kernel's `gicv3_cpu_sys_reg_init`
  writes AP registers to clear active priorities).
- **IMPDEF/reserved encodings:** MRS → `Mov { imm: 0 }` ("no vendor
  feature / errata not present" — the honest answer for the c13_c5 errata-
  probe cluster at 0x41302344 and the op0=0/1/2 vendor space); MSR → `vec![]`
  (accepted no-op). These are the same shapes the lifter already used for
  TPIDRRO_EL0/CSSELR_EL1-style probes.
- **PMCCNTR_EL0:** stored u64, default 0. P3 stored the other PMU regs the
  same way; the counter never advances (no PMU model), which is the honest
  flat-zero a nonexistent PMU produces.
- **Snapshot v12 → v13.** 91 new u64s per vCPU (64 debug + 13 EL2 + 10 GIC
  + 4 misc); `CPU_ENCODED_BYTES` 76·8 → 167·8. Old v12 blobs fail with
  `VersionMismatch` (correct — the format changed).

## Honest-trap cases (deliberate, tested)

- `MRS ICC_SGI1R_EL1` (write-only encoding) → `R_SYSTEM` trap.
- `MRS` of any MSR-only P4 encoding (12 EL2 regs, BPR1, AP0R/AP1R, DISR,
  LORC) → `R_SYSTEM` trap (not in the static scan; loud, not silent).
- The 5 `exgen` rows (`brk`/`svc`/`smc`/`eret`/`hlt`) keep their P0 honest
  traps — deliberately not implemented.

## Files changed

1. **contracts/src/cpu.rs** — 91 new `SysReg` variants (45–135), documented.
2. **contracts/src/machine.rs** — `SysRegs` fields (4 debug arrays + 27
   u64s), defaults (`icc_sre_el2 = 0x1`), `load`/`store`/`from_index` arms,
   `SNAPSHOT_VERSION` 12 → 13.
3. **units/u2-ir-lift/src/lib.rs** — MRS/MSR persistent arms (incl. 64+64
   indexed debug arms), 16 constant-0 MRS arms, 10 MSR no-op arms.
4. **units/u11-snapshot/src/lib.rs** — writer/reader for the 91 fields
   (`u64x16`/`u64x4` helpers), `CPU_ENCODED_BYTES` → 167·8, seed tests.
5. **units/u15-exec-wasmi/src/lib.rs** — roundtrip test extended to all 92
   new WO/RW regs.
6. **units/u12-orchestrator/tests/sysreg_p4.rs** (new) + 6 fixture files
   (`fixtures/sysreg_p4_{zcr,debug,el2,gic,misc,impdef}.json`) — real
   decode→lift→compile→execute sequences through `Orchestrator::step_vcpu`.

## Test results

| Crate | Result |
|---|---|
| `cargo test -p pathn-contracts --lib` | 19 passed, 0 failed (incl. new `p4_defaults_and_discriminants`) |
| `cargo test -p u2-ir-lift --lib` | 117 passed, 0 failed (incl. 3 new P4 tests) |
| `cargo test -p u11-snapshot --lib` | 16 passed, 0 failed |
| `cargo test -p u15-exec-wasmi --lib` | 9 passed, 0 failed (roundtrip now covers 92 P4 regs) |
| `cargo test -p u12-orchestrator --test sysreg_p4` | 6 passed, 0 failed (9 sequences, 281 steps, all 186 encodings covered end-to-end) |

All run with `-- --test-threads=2` (u12 with `--test-threads=1`), no OOM.

## Coverage audit

A generator-side audit decodes every fixture word back to
(op0,op1,crn,crm,op2,dir) and diffs against the 186 backlog rows:
**MRS 83/83, MSR 103/103 — zero missing encodings.**

## What remains

- **5 `exgen` rows** — honest traps by design (`brk`/`svc`/`smc`/`eret`/`hlt`).
- **GIC model** — SGI1R/AP writes are accepted and stored; no distributor,
  no IRQ injection (same as P3).
- **PMU** — counters are stored, never advance (same as P3).
- **Full-boot validation** — static-semantics + unit/fixture level. The boot
  halt position may move once these registers stop trapping; the reviewer
  gate owns re-measuring.

## Gaps / risks (honest)

- The 64 debug variants inflate the `SysReg` enum to 136 discriminants and
  the snapshot by 91 u64s/vCPU. If snapshot size ever matters, the debug
  arrays are the obvious compression target (they are all zero in practice).
- `ICC_SRE_EL2 = 1` default mirrors EL1; if real EL2 code ever runs, the
  SRE value should come from the (unmodeled) reset state instead.
- IMPDEF MRS → 0 assumes "feature absent" is safe for every vendor
  encoding the kernel probes. If a future probe gates required behavior on
  a nonzero IMPDEF read, that specific encoding gets a measured value.
