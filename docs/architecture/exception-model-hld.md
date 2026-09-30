# Exception Model HLD — SVC/ERET for Guest Boot (Phase 2)

Status: design doc, no code. Target: booting the real AOSP ARM64 kernel
(`/mnt/sdb1/aosp/Image`) under the Path N emulator. Doc only — the
implementer turns §6 into code.

Acronyms on first use: HLD (High-Level Design), EL (Exception Level),
SVC (Supervisor Call), ERET (Exception Return), VBAR_EL1 (Vector Base
Address Register, EL1), SPSR_EL1 (Saved Program Status Register, EL1),
ELR_EL1 (Exception Link Register, EL1), ESR_EL1 (Exception Syndrome
Register, EL1), DAIF (Debug/SError/IRQ/FIQ mask bits), PSTATE (Processor
State), MMU (Memory Management Unit), GIC (Generic Interrupt Controller).

## 1. Where we boot: EL1 — finding, with evidence

**Finding: the emulator presents EL1 to the guest. The AOSP kernel detects
this via its standard `head.S` check and takes the EL1 entry path. No EL2
emulation is needed for boot.**

Evidence (all measured, 2026-09-30):

1. `units/u2-ir-lift/src/lib.rs:157` — `lift_system` answers
   `MRS Xt, CurrentEL` with the constant `0x4` (bits[3:2] = `0b01` = EL1).
   The guest can never observe any other EL.
2. Disassembly of the real kernel image
   (`aarch64-linux-gnu-objdump -b binary -m aarch64`, entry at
   `0x40000000`): `el2_setup` at `0x40c03008`, called from `stext` at
   `0x413c0004` (`97e10c01  bl 0x40c03008`):
   ```
   40c03008: d50041bf  msr spsel, #0x1
   40c0300c: d5384240  mrs x0, currentel
   40c03010: f100201f  cmp x0, #0x8        ; CurrentEL_EL2
   40c03014: 540000e0  b.eq 0x40c03030     ; EL2 setup block
   40c03018: d2a60a00  mov  x0, #0x30500000 ; EL1 path: SCTLR_EL1 value
   40c0301c: f2810000  movk x0, #0x800
   40c03020: d5181000  msr sctlr_el1, x0
   40c03024: 5281c220  mov  w0, #0xe11
   40c03028: d5033fdf  isb
   40c0302c: d65f03c0  ret
   40c03030: d51c1000  msr sctlr_el2, x0   ; EL2 block follows (msr hcr_el2…)
   ```
3. Measured execution trace (GB-5 gate, real kernel): steps 38–41 are
   exactly `0x40c03020 → 0x40c03024 → 0x40c03028 → 0x40c0302c` — the EL1
   fall-through (`msr sctlr_el1`, `mov`, `isb`, `ret`). The `b.eq` was not
   taken; the EL2 block at `0x40c03030` never executed.
4. Linux supports EL1 entry by design (`Documentation/arm64/booting.rst`:
   entry at EL1 or EL2; `init_kernel_el` re-checks `CurrentEL` and takes the
   EL1 branch). Entering at EL1 is the same path as UEFI/kexec boot.

UNKNOWN / assumptions (marking per brief):

- QEMU `virt` behavior is NOT verified in this work. Believed but
  unverified: QEMU/TCG enters the kernel at EL2 and the kernel drops to
  EL1 itself via `el2_setup`'s ERET path. We bypass that entirely by
  entering at EL1 — simpler, and sufficient because the kernel's EL1
  path is self-contained.
- UNKNOWN whether any later AOSP kernel path requires EL2 (e.g. KVM).
  Out of scope for boot-to-init. If EL2 `MRS`/`MSR` appear in a later
  trace, GB-3 already accepts the common ones silently
  (`units/u2-ir-lift/src/lib.rs:194`).

Consequence: the exception model needs only **EL1 synchronous exceptions**
(SVC executed at EL1) and **ERET to EL1**. No EL2/EL3 state, no SCR_EL3,
no HCR_EL2 routing.

## 2. Exception entry mechanics

### 2.1 Vector selection

- New state: `VBAR_EL1` must be **persisted**. Today
  `MSR VBAR_EL1, Xt` is silently dropped
  (`units/u2-ir-lift/src/lib.rs:194`).
- `VBAR_EL1[10:0]` are RES0; vector base = `VBAR_EL1 & !0x7FF`.
- The kernel runs with `SP_EL1` (`el2_setup`'s first instruction is
  `msr spsel, #0x1`). An SVC executed at EL1 therefore takes the
  "current EL, SP_ELx" synchronous vector: **offset `0x200`**.
- Full AArch64 `VBAR_EL1` layout, for reference (v1 implements only the
  `0x200` entry): `+0x000` sync/SP_EL0, `+0x080` IRQ/SP_EL0,
  `+0x100` FIQ/SP_EL0, `+0x180` SError/SP_EL0, **`+0x200`
  sync/current-EL/SP_ELx**, `+0x280` IRQ/current-EL/SP_ELx,
  `+0x300` FIQ/…, `+0x380` SError/…, `+0x400` sync/lower-EL AArch64,
  `+0x480`/`+0x500`/`+0x580` IRQ/FIQ/SError lower-EL AArch64,
  `+0x600`–`+0x780` lower-EL AArch32 variants.

### 2.2 State saved on entry (SVC executed at EL1)

Performed by the orchestrator — not in wasm (see §5) — atomically before
the first vector instruction executes:

- `SPSR_EL1 ← PSTATE`: NZCV → bits[31:28]; D/A/I/F → bits[9:6];
  M[4:0] = `0b00101` (EL1h, "EL1 with SP_ELx"); all other fields 0 in v1.
- `ELR_EL1 ← pc + 4` (address of the instruction after the SVC).
- `ESR_EL1 ← (0x15 << 26) | (1 << 25) | imm16` = `0x56000000 | imm16`:
  EC = `0x15` (`0b010101`, "SVC instruction execution in AArch64 state"),
  IL = 1 (32-bit instruction), ISS[15:0] = `imm16 = (word >> 5) & 0xFFFF`
  (e.g. `0xD4000001` = `SVC #0` → `ESR_EL1 = 0x56000000`).
- `PSTATE.{D,A,I,F} ← 1` (mask debug + interrupts on entry; Linux's
  vector entry code assumes IRQs are masked on arrival).
- `pc ← (VBAR_EL1 & !0x7FF) + 0x200`.

UNKNOWN: the exact architectural conditional for `PSTATE.D` on entry —
verify against ARM ARM DDI0487 `AArch64.TakeException` during
implementation. A/I/F ← 1 is the load-bearing part for Linux; D is
currently unobservable in our model.

### 2.3 What we model vs. ignore

- MODEL: DAIF bits in `pstate[9:6]` (architectural positions). Boot value
  `0x3c0` (all masked) — consistent with GB-3's `MRS DAIF → 0x3c0`
  constant (`units/u2-ir-lift/src/lib.rs`) and the Linux ARM64 boot
  protocol. Today DAIF is NOT stored (`MSR DAIF` is dropped,
  `units/u2-ir-lift/src/lib.rs:191`); the exception model is the forcing
  function to persist it.
- IGNORE in v1: `SPSel` (kernel sets it once to SP_EL1, never changes in
  early boot); `PAN`/`UAO`/`DIT`/`SSBS`/`TCO`/`SS` PSTATE fields (zero);
  nested exceptions (a second exception before ERET → honest halt,
  not handled); `SP_EL0` (the single `sp` field models `SP_EL1`; Linux
  uses `SP_EL0` only for EL0 user tasks, which do not exist before the
  first SVC — UNKNOWN, verify if the trace ever executes
  `msr spsel, #0`).

## 3. ERET mechanics

- Encoding `0xD69F03E0`. Note: U1 does NOT classify it today — it is not
  the RET pattern (`word & 0xFFFF_FC1F == 0xD65F_0000`,
  `units/u1-decode/src/lib.rs:302`) and not the System pattern
  (`bits[31:22] == 0x354`, `:306`) — so it currently falls through to
  `DecodeResult::Illegal`. The implementer must classify it (as
  `InsnKind::System`) and lift it.
- Semantics: `pc ← ELR_EL1`; `PSTATE ← SPSR_EL1` (restores NZCV + DAIF +
  M). Exclusive monitors are cleared — irrelevant (no LDXR/STXR yet).
- If `SPSR_EL1.M` is not EL1h (`0b00101`) / EL1t (`0b00100`) on ERET →
  honest halt. Linux always returns to EL1h here.

## 4. Minimal viable "handling" for boot-loop v1

**v1 = log + halt with a distinct `HaltReason`, NOT vector dispatch.**
(The SVC spike agent, working in parallel, is already building this as
`HaltReason::Svc`.)

Why, in order:

1. The loop's metric is `halt_step`. A distinct reason separates
   "kernel reached its first SVC" — a milestone meaning early boot,
   page-table setup and `init_kernel_el` all survived — from "died on an
   unimplemented instruction". Without the distinction the metric lies.
2. It matches the established honest-trap pattern: `first_trap_reason`
   → `HaltReason::Unsupported` checked BEFORE compiling
   (`units/u12-orchestrator/src/lib.rs:868-872`).
3. Vector dispatch without ERET strands the guest: vector code runs, then
   has no way back. A half-model is worse than an honest halt because the
   trace becomes misleading. Dispatch and ERET must land in the same
   phase (§6, phases 3–4).
4. Each phase stays small enough for one agent iteration with its own
   gate.

## 5. Current code flow (what the implementer touches)

- `contracts/src/machine.rs:9-14` — `CpuState { regs: [u64; 31],
  sp: u64, pc: u64, pstate: u64 }`. **Gap: no
  `vbar_el1`/`spsr_el1`/`elr_el1`/`esr_el1`.** Recommendation: add a
  per-vCPU `ExcState` struct here and bump `SNAPSHOT_VERSION` (currently
  1) — snapshot format change, additive.
- `contracts/src/cpu.rs:194` — `IrOp::Trap { reason }`: "Explicit trap
  for unimplemented/privileged semantics. Never a silent nop."
- `units/u1-decode/src/lib.rs:306-310` — System class is
  `bits[31:22] == 0x354`. **SVC (`0xD4…`, e.g. `0xD4000001`: `>> 22` =
  `0x350`) does NOT match → `DecodeResult::Illegal` →
  `HaltReason::IllegalInstruction` today.** The spike must classify it
  first. **ERET (`0xD69F03E0`) likewise → Illegal today** (§3).
- `units/u2-ir-lift/src/lib.rs:127-209` — `lift_system`: WFI →
  `IrOp::Wfi`; HINTs/barriers/cache-ops → `vec![]` (honest NOPs on
  single-vCPU); MRS → `IrOp::Mov` with constants (`:157` CurrentEL →
  `0x4`, CTR_EL0, DAIF → `0x3c0`, NZCV → 0, TPIDR_EL1 → 0, ID regs);
  MSR → `vec![]` dropped (`:191-194`, including `VBAR_EL1`,
  `SCTLR_EL1/2`, `HCR_EL2`, `DAIF`, `NZCV`, `TPIDR_EL1`). **Gaps:
  VBAR_EL1 not persisted; DAIF/NZCV writes not persisted** (NZCV has a
  partial path via `pstate` in the orchestrator,
  `units/u12-orchestrator/src/lib.rs:668`).
- `units/u12-orchestrator/src/lib.rs:820` — `step_vcpu`: fetch → U1 →
  `execute_arm64` (GB-2 direct branch/flag execution) → U2 lift →
  `first_trap_reason` (`:1251`) → `HaltReason::Unsupported` before
  compiling → U3 compile → `executor.run_block` → `pc = exit_addr`.
  **SVC/ERET entry/return should be intercepted here, next to the trap
  check**, because VBAR/SPSR/ELR/ESR live in orchestrator-side state,
  not in the wasm `regs` array.
- `units/u12-orchestrator/src/lib.rs:137-163` — `HaltReason` (spike adds
  `Svc`; later phases keep it — a different milestone from `Unsupported`).
- `units/u15-exec-wasmi/src/lib.rs:81` — `run_block(&mut self, wasm,
  regs: &mut [u64; 31], host) -> Result<(i64, bool), String>` returns the
  exit address. **No exception state crosses this boundary — by design,
  exceptions stay above it.**
- `docs/architecture/guest-boot-roadmap.md:44-47` — Phase 2 gate:
  "kernel survives its first SVC".

Design decision (recommended): add `IrOp::Svc { imm: u16 }` and
`IrOp::Eret`; handle both in `step_vcpu` next to the trap check.
**U3 needs no changes** — no new wasm lowering; the exception never
enters compiled code as data flow.

## 6. Phased task list

Each phase is one agent iteration with its own gate. No phase starts
until the previous gate is green.

- **Phase 0 — SVC spike (in flight, separate agent):** U1 classifies
  `0xD4000001` (SVC generally) as `System`; U2 lifts to an honest trap;
  new `HaltReason::Svc { addr, imm }`. Zero vector/ERET work. Gate:
  golden U1/U2 tests; kernel test still halts at step 57 (no
  regression).
- **Phase 1 — exception state:** add `ExcState { vbar_el1, spsr_el1,
  elr_el1, esr_el1: u64 }` to `CpuState` (`contracts/src/machine.rs`),
  bump `SNAPSHOT_VERSION` 1 → 2, snapshot round-trip test. Persist DAIF
  in `pstate[9:6]`, boot value `0x3c0`. Gate: unit + snapshot tests;
  kernel halt unchanged.
- **Phase 2 — VBAR_EL1 persistence:** `MSR VBAR_EL1, Xt` stores (mask off
  low 11 bits); `MRS Xt, VBAR_EL1` reads back. Change the
  `units/u2-ir-lift/src/lib.rs:194` arm from drop to a new `IrOp`. Gate:
  MSR/MRS round-trip test; kernel halt unchanged.
- **Phase 3 — vector dispatch:** on `IrOp::Svc`: save SPSR/ELR/ESR per
  §2.2, mask DAIF, `pc ← (VBAR_EL1 & !0x7FF) + 0x200`. Synthetic test
  with a known `VBAR_EL1` and a planted vector. Gate: synthetic dispatch
  test asserts `SPSR_EL1`/`ELR_EL1`/`ESR_EL1`/`pc`; kernel test is
  expected to change halt — document the new halt, never guess it
  beforehand.
- **Phase 4 — ERET:** U1 classifies `0xD69F03E0` as `System`; U2 →
  `IrOp::Eret`; `step_vcpu` restores `pc ← ELR_EL1`,
  `PSTATE ← SPSR_EL1`; non-EL1 `SPSR_EL1.M` → honest halt. Gate:
  round-trip test (SVC → vector → ERET lands on the instruction after
  the SVC with PSTATE restored); roadmap gate: **kernel survives its
  first SVC**.
- **Phase 5 — hardening (only if the kernel asks):** nested-exception
  halt reason, `SP_EL0` if ever observed, ESR for data aborts (Phase 3
  MMU will need EC `0x24`/`0x25`). Not started until the trace demands
  it.

Out of scope for Phase 2: IRQs/FIQs/SErrors (Phase 4 GIC), EL2/EL3,
virtual timers, `WFI`-as-exception.

## Appendix: evidence log

- EL1 entry path: `el2_setup` disassembly above, from
  `/mnt/sdb1/aosp/Image` (`-rwxrwxrwx`, 23,073,280 bytes, observed
  2026-09-30), via `aarch64-linux-gnu-objdump -b binary -m aarch64`.
- GB-5 measured trace steps 38–41 (`0x40c03020: d5181000`,
  `0x40c03024: 5281c220`, `0x40c03028: d5033fdf`,
  `0x40c0302c: d65f03c0`) == EL1 fall-through of `el2_setup`.
- ESR/SVC/ERET encodings: ARM ARM DDI0487 (from knowledge, not
  re-verified against the manual in this work — the unit tests in
  Phase 1/3 are the verification).
