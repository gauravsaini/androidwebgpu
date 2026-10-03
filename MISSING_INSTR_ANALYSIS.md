# Missing System Instructions Analysis

**Worktree:** `~/workspace/wt-emu-impl` (branch `feat/emu-missing-instr`, tip `304ad91`)
**Date:** 2026-10-03
**Status:** Analysis only — no implementation code written.

---

## 1. Trap mechanism: how "unsupported System instruction" halts happen

The honest-trap pipeline for system instructions has three stages:

```
U1 decode (units/u1-decode/src/system.rs)
  → bits[31:22]==0x354  →  InsnKind::System
  → 0xD4xxxxxx          →  Svc / System(HVC,SMC,BRK,HLT)
  → 0xD69F03E0          →  System(ERET)

U2 lift (units/u2-ir-lift/src/lib.rs :: lift_system, line 175)
  → handled encodings   →  IrOp::{ReadSys,WriteSys,Mov,Wfi} or vec![] (honest NOP)
  → unhandled MRS/MSR   →  trap(R_SYSTEM)      // "System: system and privileged semantics are not lifted"
  → HVC / SMC / BRK / HLT → trap(R_HVC / R_SMC / R_BRK / R_HLT)
  → ERET                →  trap(R_ERET)         // "System: ERET exception return not yet implemented"

U12 orchestrate (units/u12-orchestrator/src/lib.rs, lines ~2595-2610)
  → first_trap_reason(&ops) checked BEFORE compiling
  → HaltReason::Unsupported { addr: pc, reason }
  → vCPU halts; PC + reason recorded (honest trap, never silent no-op)
```

Key property: **any MRS/MSR whose `(op0,op1,crn,crm,op2)` tuple is not in the
match arms of `lift_system` halts the emulator** with `R_SYSTEM`. There is no
fallback. This is deliberate (honest trap > silent no-op), but it means every
new system register the kernel touches must be explicitly added.

---

## 2. The old halt at 0xffffff8008209d80 — already fixed

The task cites "last known halt at PC 0xffffff8008209d80 (unsupported System
instruction)". Decoding the trapped word (from the GB-26 code comment):

```
word 0xd53b00e3:
  bits[31:22] = 0x354          → system instruction
  bit 21 (L)  = 1              → MRS (read)
  (op0,op1,crn,crm,op2) = (3,3,0,0,7), Rt = X3
  → S3_3_C0_C0_7 = DCZID_EL0, MRS X3, DCZID_EL0
```

**This halt is already fixed.** GB-26 (merged as `8f34efe`, present in this
worktree) implements `MRS DCZID_EL0 → 0x10` (DZP=1, DC ZVA prohibited, so the
kernel takes its store-based cache-zero fallback instead of the `DC ZVA`
instruction our NOP cache ops cannot honor). The reviewer gate confirms: "Old
1.2M halt (unsupported System instr at 0xffffff8008209d80) eliminated."

Current walls are **not** system-instruction traps:
- Corrected boot path: 70,582 steps → PC `0xffffff80094925ac`, WasmTrap
  translation fault at VA 0 (MMU/page-table issue).
- Old boot path: 10M steps, PC stable in loop at `0xffffff80095d2a54`
  (infinite/panic-style loop, no trap).

The gaps below are the **predicted next traps** as boot progresses past the
current walls, prioritized by Linux ARM64 boot order.

---

## 3. What's implemented (complete inventory from `lift_system`)

### 3a. MRS → persistent SysReg (real state via host calls)

| Tuple (op0,op1,crn,crm,op2) | Register | Notes |
|---|---|---|
| (3,3,4,2,1) | DAIF | default 0x3c0 per boot protocol |
| (3,0,13,0,4) | TPIDR_EL1 | thread ID |
| (3,0,4,1,0) | SP_EL0 | thread_info base |
| (3,4,14,1,0) | CNTHCTL_EL2 | |
| (3,3,14,0,1) | CNTPCT_EL0 | live physical counter |
| (3,3,14,0,2) | CNTVCT_EL0 | live virtual counter |
| (3,3,14,2,1) | CNTP_CTL_EL0 | |
| (3,3,14,2,2) | CNTP_CVAL_EL0 | |
| (3,3,14,3,1) | CNTV_CTL_EL0 | |
| (3,3,14,3,2) | CNTV_CVAL_EL0 | |
| (3,0,1,0,0) | SCTLR_EL1 | |
| (3,4,1,0,0) | SCTLR_EL2 | |
| (3,0,1,0,2) | CPACR_EL1 | FP/ASIMD enable |
| (2,0,0,2,2) | MDSCR_EL1 | debug control |
| (3,0,10,2,0) | MAIR_EL1 | memory attributes |
| (3,0,2,0,2) | TCR_EL1 | translation control |
| (3,0,2,0,0) | TTBR0_EL1 | table base 0 |
| (3,0,2,0,1) | TTBR1_EL1 | table base 1 |

### 3b. MRS → constant value (feature-probe / ID registers)

| Tuple | Register | Value | Rationale |
|---|---|---|---|
| (3,0,4,2,2) | CurrentEL | 0x4 | EL1 → kernel skips el2_setup |
| (3,3,0,0,1) | CTR_EL0 | 0x8444c004 | 64B I/D cache lines |
| (3,3,4,2,0) | NZCV | 0 | live flag path |
| (3,0,0,4,0) | ID_AA64PFR0_EL1 | 0x11 | AArch64 EL0+EL1 only |
| (3,0,0,4,1) | ID_AA64PFR1_EL1 | 0 | no BT/MTE/RAS |
| (3,0,0,6,0) | ID_AA64ISAR0_EL1 | 0 | no LSE/AES/SHA/CRC/RNDR |
| (3,0,0,6,1) | ID_AA64ISAR1_EL1 | 0 | no DPB/APA/JSCVT/FCMA |
| (3,0,0,7,2) | ID_AA64MMFR2_EL1 | 0 | |
| (3,0,0,5,0) | ID_AA64DFR0_EL1 | 0 | no debug features |
| (3,0,0,7,0) | ID_AA64MMFR0_EL1 | 0 | conservative fallback |
| (3,0,0,7,1) | ID_AA64MMFR1_EL1 | 0 | conservative fallback |
| (3,3,0,0,7) | DCZID_EL0 | 0x10 | DZP=1, DC ZVA prohibited |
| (3,0,0,0,5) | MPIDR_EL1 | 0x40000000 | U=1, Aff0=0, single vCPU |
| (3,0,0,0,0) | MIDR_EL1 | 0 | no errata workarounds |
| (3,4,13,0,2) | TPIDR_EL2 | 0 | |
| (3,1,0,0,1) | CLIDR_EL1 | 0x09200023 | L1 Harvard, L2 unified |
| (3,2,0,0,0) | CSSELR_EL1 | 0 | |
| (3,1,0,0,0) | CCSIDR_EL1 | 0x701FE00A | 64B line, 64KB |
| (3,3,14,0,0) | CNTFRQ_EL0 | 0x03B9ACA0 | 62.5 MHz |
| (3,3,13,0,3) | TPIDRRO_EL0 | 0 | |
| (3,3,13,0,2) | TPIDR_EL0 | 0 | |
| (3,0,5,2,0) | ESR_EL1 | 0 | |
| (3,0,6,0,0) | FAR_EL1 | 0 | |
| (3,0,4,0,1) | ELR_EL1 | 0 | |
| (3,0,4,0,0) | SPSR_EL1 | 0 | |
| (3,3,9,12,0) | PMCR_EL0 | 0 | |
| (3,3,9,14,0) | PMUSERENR_EL0 | 0 | |

### 3c. MSR → persistent SysReg (19 registers)

DAIF, TPIDR_EL1, SCTLR_EL1, SCTLR_EL2, HCR_EL2, CNTHCTL_EL2, CNTVOFF_EL2,
VBAR_EL1, SP_EL0, CPACR_EL1, MDSCR_EL1, MAIR_EL1, TCR_EL1, TTBR0_EL1,
TTBR1_EL1, CNTP_CTL_EL0, CNTP_CVAL_EL0, CNTV_CTL_EL0, CNTV_CVAL_EL0,
plus DAIFSet/DAIFClr immediate (real RMW).

### 3d. MSR → honest no-op

PAN, UAO, DIT, SSBS, TCO (immediate forms); NZCV; SPSel; TPIDRRO_EL0;
TPIDR_EL0; CSSELR_EL1; ESR_EL1, FAR_EL1, ELR_EL1, SPSR_EL1; PMCR_EL0,
PMINTENSET_EL1, PMINTENCLR_EL1.

### 3e. Always NOP (single-vCPU honest)

- HINTs (NOP/YIELD/SEV/WFE/WFI is `IrOp::Wfi` → host yield)
- DMB/DSB/ISB barriers
- SYS cache/TLB ops: DC IVAC/CVAC/CIVAC, IC IVAU/IALLU, TLBI VMALLE1/VAE1/VMALL
  (all treated as NOP; DC ZVA guarded by DCZID_EL0.DZP=1)

### 3f. Always trap

- HVC → R_HVC, SMC → R_SMC, BRK → R_BRK, HLT → R_HLT
- ERET → R_ERET ("not yet implemented")
- Any other MRS/MSR → R_SYSTEM

---

## 4. Prioritized gaps (predicted next traps)

### P0 — will trap on the normal boot path

**1. HVC (PSCI calls) — `hvc #0` → R_HVC**
- ARM ARM: HVC exception, §D1. "Hypervisor Call".
- Why: DTB declares `psci, method = "hvc"`. `setup_arch()` →
  `psci_dt_init()` → `psci_0_2_init()` → `invoke_psci_fn(PSCI_VERSION)` uses
  inline-asm `hvc #0`. Also `psci_cpu_on`, `psci_system_reset`, CPU idle
  (`psci_cpu_suspend`) — all HVC.
- Note: the emulator deliberately skips EL3 firmware; PSCI cannot be honored
  by executing firmware. Options: (a) trap-and-emulate minimal PSCI
  (VERSION/SYSTEM_RESET/SYSTEM_OFF as no-ops or halts), (b) keep honest trap
  and treat as boot blocker. The kernel cannot complete `setup_arch`
...[truncated 7816 chars]