//! U2 `arm-ir-lift`: decoded AArch64 instruction -> explicit IR ops.
//!
//! Purity: PURE. `lift` is a deterministic function of `&Instruction` only.
//! No I/O, no time, no threads, no hidden state. Imports only `pathn_contracts`.
//!
//! ## Lifting model
//!
//! The frozen contract (`pathn_contracts::cpu::IrOp`) is the whole truth about
//! what can be expressed:
//!
//! - `Add { dst, a, b }` / `Sub { dst, a, b }` / `Mov { dst, imm }` — operands are indices into the
//!   guest register file (`0..=30` = X0-X30/W0-W30, `31` = XZR/SP following the
//!   usual contextual AArch64 rule; the execution backend applies it).
//! - `Load { dst, addr, size }` / `Store { src, addr, size }` — `addr` is a
//!   **static** `u64` guest address and `size` is the access width in bytes
//!   (`4` or `8`). The lifter runs ahead of time and never knows register
//!   contents, so only statically-known addresses lift: PC-relative literal
//!   loads (`LDR (literal)` -> `addr = insn.addr + offset`).
//! - `LoadDyn { dst, base, off, size }` / `StoreDyn { src, base, off, size }`
//!   (Wave 4, U2-G1) — register-relative forms (`LDRB`/`STRB` unsigned
//!   immediate): the effective address `regs[base] + off` is computed at
//!   runtime by the execution backend. SP-relative (`Rn = 31`) forms trap:
//!   the Wave-4 register file has no SP.
//! - `Branch { target }` — statically computed `insn.addr + offset`; this is
//!   the explicit control-flow exit for the block. `BL` lowers to a link
//!   write (`Mov { dst: 30, imm: addr + 4 }`) followed by the same static
//!   `Branch` — no new op needed. `RET <Xn>` lowers to `BranchDyn { reg }`
//!   (Wave 5): the target is register-held, so only an indirect branch
//!   expresses it.
//! - `CondBranch { reg, target, when_zero }` (Wave 4, U2-G1) — `CBZ`/`CBNZ`
//!   (64-bit): runtime-tested register, static taken-target, fallthrough.
//! - `OrrShift { dst, a, b, shift, amount }` (Wave 4, U2-G1) — `ORR`
//!   (shifted register, 64-bit).
//! - `Wfi` (Wave 4, U2-G1) — the `WFI` hint word.
//! - `Mov { dst, imm }` also serves `ADR`/`ADRP` (`InsnKind::PcRel`, Wave 4,
//!   U1-G1): the lifter knows `insn.addr`, so the target is a static immediate.
//!
//! ## Scratch register
//!
//! `IrOp::Add` is register-register only, so immediate operands are
//! materialized first: `ADD Xd, Xn, #imm` lifts to
//! `Mov { dst: SCRATCH, imm }` followed by `Add { dst: rd, a: rn, b: SCRATCH }`.
//! `SCRATCH` is index `32`, outside the 32 architectural registers. The
//! execution backend (U3/orchestrator) must provide at least this one scratch
//! slot; it is never an architectural register and is dead after the
//! instruction's op sequence.
//!
//! ## Honesty rule
//!
//! Anything the contract cannot express — flag-setting ALU ops, 32-bit ALU
//! widths (upper-bit zeroing is not expressible in `IrOp::Add`/`OrrShift`),
//! halfword memory widths, SP-relative addresses, unrecognized words, and
//! the still-out-of-scope classes (B.cond/TBZ/TBNZ, other system
//! instructions) — lifts to `IrOp::Trap { reason }` naming exactly what is
//! unsupported. A trap is data, never a silent nop and never a panic.

use pathn_contracts::cpu::{decode_bitmasks, InsnKind, Instruction, IrOp, SysReg};

/// Private scratch register index used to materialize immediates.
/// See module docs. Outside the architectural `0..=31` range.
pub const SCRATCH: u8 = 32;

// ---- Trap reasons (exact strings; tests pin them) ----
const R_SYSTEM: &str = "System: system and privileged semantics are not lifted";
const R_UNKNOWN: &str = "Unknown: illegal or unrecognized instruction word";
const R_DP_UNSUPPORTED: &str = "DataProc: unsupported encoding";
const R_MOVZ_HW: &str = "DataProc: MOVZ hw field > 1 with sf = 0 is unallocated";
const R_ADD32_IMM: &str = "DataProc: 32-bit ADD immediate width is not expressible in IrOp::Add";
const R_SUB32_IMM: &str = "DataProc: 32-bit SUB immediate width is not expressible in IrOp::Sub";
const R_SUB32_REG: &str = "DataProc: 32-bit SUB register width is not expressible in IrOp::Sub";

/// DAIFSet/DAIFClr immediate -> DAIF-positioned mask (GB-11).
/// Per the ARM ARM, MSR DAIFSet/DAIFClr, #imm sets/clears PSTATE.{D,A,I,F}
/// from imm bits {3,2,1,0}; PSTATE D/A/I/F are bits 9/8/7/6. Verified:
/// Linux `msr daifclr, #2` unmasks IRQ (imm bit1 -> I), and the measured
/// 0xd50348ff (daifclr #0x8) clears D (imm bit3) on the kernel's
/// debug-exception unmask path after it programs MDSCR_EL1.
fn daif_imm_mask(imm: u32) -> u64 {
    ((imm & 0xF) as u64) << 6
}
const R_SUB_SHIFT: &str = "DataProc: shifted SUB register operand is not expressible in IrOp";
const R_CLZ32: &str = "DataProc: 32-bit CLZ width is not expressible in IrOp::Clz";
const R_MADD32: &str = "DataProc: 32-bit MADD width is not expressible in IrOp::Madd";
const R_MSUB: &str = "DataProc: MSUB is not implemented (GB-8 is MADD-64 only)";
const R_MADD_LONG: &str =
    "DataProc: long-multiply (SMADDL/UMADDL/SMSUBL/UMSUBL/SMULH/UMULH) is not implemented";
const R_ADD32_REG: &str = "DataProc: 32-bit ADD register width is not expressible in IrOp::Add";
const R_ADD_SHIFT: &str =
    "DataProc: shifted or extended ADD register operand is not expressible in IrOp";
const R_ADD_EXTEND_SIGNED: &str =
    "DataProc: signed-extend (SXTB/SXTH/SXTW/SXTX) ADD/SUB not yet implemented";
const R_ADC_CARRY: &str =
    "DataProc: ADC/SBC/ADCS/SBCS need NZCV carry flag, not expressible in IrOp";
const R_CSEL_COND: &str =
    "DataProc: CSEL/CSINC/CSINV/CSNEG need NZCV condition, not expressible in IrOp";
const R_LS_UNSUPPORTED: &str = "LoadStore: unsupported encoding";
const R_LS_SUBWORD: &str = "LoadStore: sub-word access width is not expressible in IrOp";
const R_BR_UNSUPPORTED: &str = "Branch: only B/BL/RET/CBZ/CBNZ are lifted";
const R_ORR32: &str = "DataProc: 32-bit ORR width is not expressible in IrOp::OrrShift";
const R_LS_SP: &str =
    "LoadStore: SP-relative address is not expressible (no SP in the Wave-4 register file)";
/// Phase-2 spike (2026-09-30): SVC recognized by U1 but the exception model
/// does not exist yet. Public so the orchestrator can map it to the distinct HaltReason::Svc instead of generic Unsupported.
pub const R_SVC_UNIMPL: &str = "Svc: exception model not yet implemented";
pub const R_BRK: &str = "System: BRK exception";
pub const R_HLT: &str = "System: HLT exception";
pub const R_HVC: &str = "System: HVC exception";
pub const R_SMC: &str = "System: SMC exception";
pub const R_ERET: &str = "System: ERET exception return not yet implemented";
pub const R_ATOMIC_CAS: &str =
    "LoadStore: atomic CAS requires memory arbitration (unsupported in IR)";
pub const R_EXCLUSIVE: &str =
    "LoadStore: exclusive monitor requires orchestrator state (unsupported in IR)";
pub const R_ATOMIC_LSE: &str = "LoadStore: atomic LSE operation (unsupported in IR)";
pub const R_FP_FMOV: &str = "FloatingPoint: FMOV scalar registers not yet expressible in IrOp";
pub const R_FP_FADD: &str = "FloatingPoint: FADD scalar arithmetic not yet expressible in IrOp";
pub const R_FP_FSUB: &str = "FloatingPoint: FSUB scalar arithmetic not yet expressible in IrOp";
pub const R_FP_FMUL: &str = "FloatingPoint: FMUL scalar arithmetic not yet expressible in IrOp";
pub const R_FP_FDIV: &str = "FloatingPoint: FDIV scalar arithmetic not yet expressible in IrOp";
pub const R_FP_FCMP: &str = "FloatingPoint: FCMP scalar compare not yet expressible in IrOp";
pub const R_FP_SCVTF: &str = "FloatingPoint: SCVTF conversion not yet expressible in IrOp";
pub const R_FP_UCVTF: &str = "FloatingPoint: UCVTF conversion not yet expressible in IrOp";
pub const R_FP_FCVTZS: &str = "FloatingPoint: FCVTZS conversion not yet expressible in IrOp";
pub const R_FP_FCVTZU: &str = "FloatingPoint: FCVTZU conversion not yet expressible in IrOp";
pub const R_FP_FRINTA: &str = "FloatingPoint: FRINTA rounding not yet expressible in IrOp";
pub const R_FP_UNSUPPORTED: &str = "FloatingPoint: FP/SIMD operation not yet expressible in IrOp";

fn trap(reason: &'static str) -> Vec<IrOp> {
    vec![IrOp::Trap { reason }]
}

/// Lift one decoded instruction to an explicit IR op sequence.
///
/// Every [`InsnKind`] is mapped: supported forms become guest-state
/// read/write ops, everything else becomes [`IrOp::Trap`] with a reason that
/// names the unsupported kind/form.
pub fn lift(insn: &Instruction) -> Vec<IrOp> {
    match insn.kind {
        InsnKind::DataProc => lift_data_proc(insn.word),
        InsnKind::LoadStore => lift_load_store(insn),
        InsnKind::Branch => lift_branch(insn),
        InsnKind::PcRel => lift_pc_rel(insn),
        InsnKind::System => lift_system(insn.word),
        InsnKind::Svc => trap(R_SVC_UNIMPL),
        InsnKind::Unknown => trap(R_UNKNOWN),
    }
}

/// PC-relative addressing (Wave 4, U1-G1): ADR/ADRP lower to a static `Mov` —
/// the lifter knows `insn.addr`, so the target is a link-time constant and no
/// dynamic PC semantics are needed.
fn lift_pc_rel(insn: &Instruction) -> Vec<IrOp> {
    let word = insn.word;
    let rd = (word & 0x1F) as u8;
    let immlo = (word >> 29) & 0x3;
    let immhi = (word >> 5) & 0x7FFFF;
    let imm21 = (immhi << 2) | immlo;
    // Sign-extend the 21-bit immediate.
    let offset = (((imm21 << 11) as i32) >> 11) as i64;
    let target = if (word >> 31) & 1 == 1 {
        // ADRP: (PC & !0xFFF) + (offset << 12).
        ((insn.addr & !0xFFF) as i64).wrapping_add(offset << 12) as u64
    } else {
        // ADR: PC + offset.
        (insn.addr as i64).wrapping_add(offset) as u64
    };
    vec![IrOp::Mov {
        dst: rd,
        imm: target,
    }]
}

/// System: WFI, barriers (DMB, DSB, ISB), HINTs (NOP, YIELD), system register
/// access (MSR, MRS), and system operations (DC, IC, TLBI).
fn lift_system(word: u32) -> Vec<IrOp> {
    // Kernel alternatives patches 0x7a441060 / 0x7a432040: patched over branch
    // loops by Linux alternatives mechanism (newer ARM extension, unallocated
    // in v8.0). Treated as NOP to allow progress. See u1-decode for details.
    if word == 0x7a44_1060 || word == 0x7a43_2040 {
        return vec![];
    }
    // Exception generation instructions: bits[31:24] == 0xD4
    if (word >> 24) == 0xD4 {
        let opc = (word >> 21) & 0x7;
        let ll = word & 0x3;
        return match opc {
            // HVC (P0, 2026-10-03): PSCI via DTB `method="hvc"`. Lifted to
            // IrOp::Hvc; the backend returns PSCI_VERSION or NOT_SUPPORTED.
            0b000 if ll == 0b10 => vec![IrOp::Hvc],
            0b000 if ll == 0b11 => trap(R_SMC),
            0b001 if (word & 0x1F) == 0 => trap(R_BRK),
            0b010 if (word & 0x1F) == 0 => trap(R_HLT),
            _ => trap(R_SYSTEM),
        };
    }
    // ERET: 1101 0110 100 11111 0000 00 11111 00000 (0xD69F03E0)
    if word == 0xD69F_03E0 {
        return trap(R_ERET);
    }
    // WFI is HINT #3 with CRm:op2 = 00100:01111: exact word match, no aliases.
    if word == 0xD503_207F {
        return vec![IrOp::Wfi];
    }
    // HINTs (including NOP = HINT #0): bits[31:12] == 0xD5032, Rt == 0x1F.
    // NOP and hints are honest NOPs on single-vCPU.
    if word >> 12 == 0xD5032 && word & 0x1F == 0x1F {
        return vec![];
    }
    // Barriers (DMB, DSB, ISB): bits[31:12] == 0xD5033, Rt == 0x1F.
    // On single-vCPU, memory and instruction barriers are honest NOPs.
    if word >> 12 == 0xD5033 && word & 0x1F == 0x1F {
        return vec![];
    }
    // System operations (e.g. DC CIVAC, DC IVAC, DC CVAC, IC IALLU, TLBI):
    // op0 == 1 (SYS). On single-vCPU emulator, cache and TLB maintenance are NOPs.
    if (word >> 22) & 0x3FF == 0x354 && ((word >> 21) & 1) == 0 && ((word >> 19) & 0x3) == 1 {
        return vec![];
    }
    // MRS Xt, <sysreg>: bit 21 == 1.
    if (word >> 22) & 0x3FF == 0x354 && ((word >> 21) & 1) == 1 {
        let op0 = (word >> 19) & 0x3;
        let op1 = (word >> 16) & 0x7;
        let crn = (word >> 12) & 0xF;
        let crm = (word >> 8) & 0xF;
        let op2 = (word >> 5) & 0x7;
        let rt = (word & 0x1F) as u8;

        // Persistent system registers: real state, read from the machine
        // (GB-sysreg2). These replaced GB-3 accepted-but-stateless values.
        let persistent = match (op0, op1, crn, crm, op2) {
            // DAIF: default all-masked (0x3c0) per Linux ARM64 boot protocol
            (3, 3, 4, 2, 1) => SysReg::Daif,
            // TPIDR_EL1: thread ID register
            (3, 0, 13, 0, 4) => SysReg::TpidrEl1,
            // SP_EL0 (GB-26): the kernel keeps the current task's
            // thread_info base here (read in preempt_disable/enable via
            // `mrs xN, sp_el0`; measured halt at step 1249003,
            // pc 0xffffff800839b164, word 0xd5384115). MSR SP_EL0 was
            // already persistent; the MRS read side was missing.
            (3, 0, 4, 1, 0) => SysReg::SpEl0,
            // CNTHCTL_EL2
            (3, 4, 14, 1, 0) => SysReg::CnthctlEl2,
            // CNTPCT_EL0 (fam/devices): physical counter, live value
            (3, 3, 14, 0, 1) => SysReg::CntpctEl0,
            // CNTVCT_EL0 (fam/devices): virtual counter, live value
            (3, 3, 14, 0, 2) => SysReg::CntvctEl0,
            // CNTP_CTL_EL0 (fam/devices): physical timer control
            (3, 3, 14, 2, 1) => SysReg::CntpCtlEl0,
            // CNTP_CVAL_EL0 (fam/devices): physical timer compare
            (3, 3, 14, 2, 2) => SysReg::CntpCvalEl0,
            // CNTV_CTL_EL0 (fam/devices): virtual timer control
            (3, 3, 14, 3, 1) => SysReg::CntvCtlEl0,
            // CNTV_CVAL_EL0 (fam/devices): virtual timer compare
            (3, 3, 14, 3, 2) => SysReg::CntvCvalEl0,
            // SCTLR_EL1, SCTLR_EL2
            (3, 0, 1, 0, 0) => SysReg::SctlrEl1,
            (3, 4, 1, 0, 0) => SysReg::SctlrEl2,
            // CPACR_EL1 (GB-9): kernel enables FP/ASIMD via MSR CPACR_EL1
            (3, 0, 1, 0, 2) => SysReg::CpacrEl1,
            // MDSCR_EL1 (GB-10): kernel zeroes debug control via MSR MDSCR_EL1
            (2, 0, 0, 2, 2) => SysReg::MdscrEl1,
            // MAIR_EL1 (GB-13): kernel programs memory attributes via MSR MAIR_EL1
            (3, 0, 10, 2, 0) => SysReg::MairEl1,
            // TCR_EL1 (GB-17): kernel programs translation control via MSR TCR_EL1
            (3, 0, 2, 0, 2) => SysReg::TcrEl1,
            // TTBR0_EL1 (GB-18): kernel programs translation table base via MSR TTBR0_EL1
            (3, 0, 2, 0, 0) => SysReg::Ttbr0El1,
            // TTBR1_EL1 (GB-19): kernel programs translation table base 1 via MSR TTBR1_EL1
            (3, 0, 2, 0, 1) => SysReg::Ttbr1El1,
            // CONTEXTIDR_EL1 (P0, 2026-10-03): kernel writes on every context
            // switch (MSR CONTEXTIDR_EL1, Xt). S3_0_C13_C0_1. Stored, no behavior.
            (3, 0, 13, 0, 1) => SysReg::ContextidrEl1,
            // VBAR_EL1 (P1, 2026-10-03): kernel reads back the vector base
            // it programmed via MSR VBAR_EL1 (e.g. to verify relocation).
            // S3_0_C12_C0_0. MSR already stored; MRS was trapping (R_SYSTEM).
            (3, 0, 12, 0, 0) => SysReg::VbarEl1,
            // HCR_EL2 (P1, 2026-10-03): kernel reads back hypervisor config
            // (e.g. to check RW/VM bits). S3_4_C1_C1_0. MSR already stored;
            // MRS was trapping (R_SYSTEM).
            (3, 4, 1, 1, 0) => SysReg::HcrEl2,
            // PAR_EL1 (P2, 2026-10-03): kernel reads the Physical Address
            // Register after AT address-translate operations.
            // S3_0_C7_C4_0. Stored as u64, default 0.
            (3, 0, 7, 4, 0) => SysReg::ParEl1,
            // ACTLR_EL1 (P2, 2026-10-03): kernel may read the Auxiliary
            // Control Register during CPU setup. S3_0_C1_C0_1. Returns 0:
            // honest "no auxiliary features implemented".
            (3, 0, 1, 0, 1) => SysReg::ActlrEl1,
            // PMCNTENSET_EL0 (P2, 2026-10-03): kernel probes the PMU
            // during boot. S3_3_C9_C12_1. Stored as u64, default 0.
            (3, 3, 9, 12, 1) => SysReg::PmcntensetEl0,
            // PMSELR_EL0 (P2, 2026-10-03): kernel probes the PMU during
            // boot. S3_3_C9_C12_5. Stored as u64, default 0.
            (3, 3, 9, 12, 5) => SysReg::PmselrEl0,
            // P3 slice A — GICv3 CPU interface (2026-10-03, feat/emu-p3-impl).
            // ICC_SRE_EL1 S3_0_C12_C12_5: returns SRE=1 (default), the
            // kernel then takes the sysreg GIC access path.
            (3, 0, 12, 12, 5) => SysReg::IccSreEl1,
            // ICC_CTLR_EL1 S3_0_C12_C12_4: stored u64.
            (3, 0, 12, 12, 4) => SysReg::IccCtlrEl1,
            // ICC_IGRPEN1_EL1 S3_0_C12_C12_7: stored u64.
            (3, 0, 12, 12, 7) => SysReg::IccIgrpen1El1,
            // ICC_PMR_EL1 S3_0_C4_C6_0: stored u64 (default all-masked).
            (3, 0, 4, 6, 0) => SysReg::IccPmrEl1,
            // P3 slice C — PMU remainder.
            // PMCNTENCLR_EL0 S3_3_C9_C12_2, PMOVSCLR_EL0 S3_3_C9_C12_3:
            // stored u64.
            (3, 3, 9, 12, 2) => SysReg::PmcntEnClrEl0,
            (3, 3, 9, 12, 3) => SysReg::PmovsclrEl0,
            // PMXEVCNTR_EL0 S3_3_C9_C13_2: stored u64.
            (3, 3, 9, 13, 2) => SysReg::PmxevcntrEl0,
            // PMXEVTYPER_EL0 S3_3_C9_C13_1: stored u64 (not in the P3
            // static list, but architecturally RW — MRS must agree with
            // the MSR).
            (3, 3, 9, 13, 1) => SysReg::PmxevtyperEl0,
            // PMUSERENR_EL0 S3_3_C9_C14_0: P2 returned constant 0 on MRS;
            // P3 stores it so MSR/MRS round-trip (kernel writes xzr).
            (3, 3, 9, 14, 0) => SysReg::PmuserenrEl0,
            // P3 slice D — timers.
            // CNTKCTL_EL1 S3_0_C14_C1_0: stored u64.
            (3, 0, 14, 1, 0) => SysReg::CntkctlEl1,
            // TPIDR_EL2 S3_4_C13_C0_2: P2 returned constant 0; P3 stores it
            // (hyp code is unreachable at EL1, but MSR/MRS now agree).
            (3, 4, 13, 0, 2) => SysReg::TpidrEl2,
            // CNTP_TVAL_EL0 S3_3_C14_C2_0: honest alias of CVAL
            // (read = low 32 bits of CVAL − counter).
            (3, 3, 14, 2, 0) => SysReg::CntpTvalEl0,
            // CNTV_TVAL_EL0 S3_3_C14_C3_0: virtual-counter alias.
            (3, 3, 14, 3, 0) => SysReg::CntvTvalEl0,
            // P3 slice E — FP/SIMD.
            // FPCR S3_3_C4_C4_0, FPSR S3_3_C4_C4_1: stored u64.
            (3, 3, 4, 4, 0) => SysReg::Fpcr,
            (3, 3, 4, 4, 1) => SysReg::Fpsr,
            _ => {
                return {
                    let val: u64 = match (op0, op1, crn, crm, op2) {
                        // CurrentEL: bits[3:2] = 0b01 (EL1) -> 0x4
                        (3, 0, 4, 2, 2) => 0x4,
                        // CTR_EL0: Cache Type Register (64B D-cache, 64B I-cache)
                        (3, 3, 0, 0, 1) => 0x8444_c004,
                        // NZCV: stays in GB-2 live pstate flag path
                        (3, 3, 4, 2, 0) => 0,
                        // ID_AA64PFR0_EL1: EL0/EL1 AArch64 supported
                        (3, 0, 0, 4, 0) => 0x11,
                        // ID_AA64PFR1_EL1 (2026-10-02): minimal Cortex-A53 =
                        // 0. No BT, no MTE, no RAS. If we advertised these,
                        // the alternatives framework would patch in code
                        // using unimplemented features.
                        (3, 0, 0, 4, 1) => 0,
                        // ID_AA64ISAR0_EL1 (2026-10-02): minimal Cortex-A53 =
                        // 0. No LSE atomics, no AES/SHA crypto, no CRC32.
                        // The kernel's alternatives framework reads this to
                        // decide patches; 0 takes the generic fallback path
                        // instead of patching in optimized sequences we can't
                        // execute (e.g. the 0x7a44_1060 / 0x7a43_2040
                        // unallocated-encoding patches).
                        (3, 0, 0, 6, 0) => 0,
                        // ID_AA64ISAR1_EL1 (2026-10-02): minimal Cortex-A53 =
                        // 0. No DPB, no APA, no JSCVT, no FCMA. Same
                        // alternatives-framework reasoning as ISAR0.
                        (3, 0, 0, 6, 1) => 0,
                        // ID_AA64MMFR2_EL1 (GB-16: was mislabeled ID_AA64MMFR1_EL1;
                        // (3,0,0,7,1) is the real ID_AA64MMFR1_EL1 -- see below)
                        (3, 0, 0, 7, 2) => 0,
                        // ID_AA64DFR0_EL1
                        (3, 0, 0, 5, 0) => 0,
                        // ID_AA64MMFR0_EL1 (GB-14): measured halt at step 7474
                        // (pc 0x40c03694, word 0xd5380705 = MRS X5,
                        // S3_0_C0_C7_0 -- fields extracted by hand from the
                        // word, not trusted from the first reading). Value 0
                        // = no memory-model features advertised, matching the
                        // sibling ID_AA64MMFR1_EL1 / ID_AA64DFR0_EL1 reads.
                        // The kernel only feature-probes this register
                        // (MRS -> bitfield extract -> compare -> conditional
                        // branch, same shape as the ID_AA64DFR0_EL1 probe at
                        // steps 7462-7465), so 0 takes the honest conservative
                        // fallback path.
                        (3, 0, 0, 7, 0) => 0,
                        // ID_AA64MMFR1_EL1 (GB-16): measured halt at step 7480
                        // (pc 0x40c036ac, word 0xd5380729 = MRS X9,
                        // S3_0_C0_C7_1 -- (op0,op1,crn,crm,op2) = (3,0,0,7,1)
                        // extracted by hand from the word and confirmed by
                        // aarch64-linux-gnu-objdump; not trusted from the
                        // first reading). Value 0 = no memory-model features
                        // advertised, matching the sibling ID_AA64MMFR0_EL1 /
                        // ID_AA64DFR0_EL1 / ID_AA64MMFR2_EL1 reads. The kernel
                        // only feature-probes this register (MRS -> AND #0xF
                        // -> CBZ), so 0 takes the honest conservative
                        // fallback path.
                        (3, 0, 0, 7, 1) => 0,
                        // DCZID_EL0 (GB-26): measured halt at step 1210749
                        // (pc 0xffffff8008209d80, word 0xd53b00e3 = MRS X3,
                        // S3_3_C0_C0_7 -- (op0,op1,crn,crm,op2) = (3,3,0,0,7)
                        // extracted by hand from the word and confirmed by
                        // capstone disassembly; not trusted from the first
                        // reading). Value 0x10: DZP=1 (DC ZVA prohibited).
                        // Our DC ops are honest NOPs, so advertising "allowed"
                        // would corrupt memory the kernel expects zeroed;
                        // DZP=1 takes the kernel's store-based fallback path,
                        // which we implement. Same conservative shape as the
                        // GB-14/GB-16 ID-register probes.
                        (3, 3, 0, 0, 7) => 0x10,
                        // MPIDR_EL1 (GB-26): measured halt at step 1248961
                        // (pc 0xffffff80095d2620, word 0xd53800a9 = MRS X9,
                        // S3_0_C0_C0_5 -- (op0,op1,crn,crm,op2) = (3,0,0,0,5)
                        // confirmed by capstone disassembly). Value
                        // 0x40000000: U (bit 30) = 1, uniprocessor system;
                        // Aff0 = 0, this is the single vCPU 0. The kernel
                        // derives its CPU number from MPIDR; this is the
                        // architecturally correct single-CPU value.
                        // (Fixed: old 0x80000000 set RES0 bit 31, not U.)
                        (3, 0, 0, 0, 5) => 0x4000_0000,
                        // MIDR_EL1 (GB-26): measured halt at step 1248969
                        // (pc 0xffffff80095d2640, word 0xd5380002 = MRS X2,
                        // S3_0_C0_C0_0 -- (op0,op1,crn,crm,op2) = (3,0,0,0,0)
                        // confirmed by capstone disassembly). Value 0: no
                        // implementer/part advertised, so the kernel's errata
                        // framework matches nothing and takes the generic
                        // path -- same conservative shape as the GB-14/GB-16
                        // ID-register probes. (Deliberately NOT a real
                        // Cortex-A57 MIDR: claiming real silicon would invite
                        // errata workarounds that poke IMPLEMENTATION DEFINED
                        // registers we don't model.)
                        (3, 0, 0, 0, 0) => 0,
                        // (P3, 2026-10-03) TPIDR_EL2 graduated to persistent
                        // (ReadSys); the constant-0 arm moved to the
                        // persistent match above.
                        // P3 slice A — GICv3 CPU interface.
                        // ICC_IAR1_EL1 S3_0_C12_C12_0 (read-only): 0x3ff =
                        // spurious, "no pending interrupt". Honest while we
                        // have no GIC model and deliver no interrupts: the
                        // kernel's entry.S el1_irq path reads this and must
                        // not see a phantom interrupt number.
                        (3, 0, 12, 12, 0) => 0x3ff,
                        // P3 slice B — ID register family completion. All 0:
                        // the conservative "no optional features advertised"
                        // answer (GB-14/GB-16 pattern). The kernel's cpuinfo
                        // block and alternatives framework take the generic
                        // fallback path instead of patching in optimized
                        // sequences we can't execute.
                        // AArch32 ID registers (read by /proc/cpuinfo block).
                        (3, 0, 0, 1, 0) => 0, // ID_PFR0_EL1
                        (3, 0, 0, 1, 1) => 0, // ID_PFR1_EL1
                        (3, 0, 0, 1, 2) => 0, // ID_DFR0_EL1
                        (3, 0, 0, 1, 4) => 0, // ID_MMFR0_EL1
                        (3, 0, 0, 1, 5) => 0, // ID_MMFR1_EL1
                        (3, 0, 0, 1, 6) => 0, // ID_MMFR2_EL1
                        (3, 0, 0, 1, 7) => 0, // ID_MMFR3_EL1
                        (3, 0, 0, 2, 0) => 0, // ID_ISAR0_EL1
                        (3, 0, 0, 2, 1) => 0, // ID_ISAR1_EL1
                        (3, 0, 0, 2, 2) => 0, // ID_ISAR2_EL1
                        (3, 0, 0, 2, 3) => 0, // ID_ISAR3_EL1
                        (3, 0, 0, 2, 4) => 0, // ID_ISAR4_EL1
                        (3, 0, 0, 2, 5) => 0, // ID_ISAR5_EL1
                        (3, 0, 0, 3, 0) => 0, // MVFR0_EL1
                        (3, 0, 0, 3, 1) => 0, // MVFR1_EL1
                        (3, 0, 0, 3, 2) => 0, // MVFR2_EL1
                        // AArch64: revision + debug + SVE.
                        (3, 0, 0, 0, 6) => 0, // REVIDR_EL1
                        (3, 0, 0, 5, 1) => 0, // ID_AA64DFR1_EL1
                        (3, 0, 0, 4, 4) => 0, // ID_AA64ZFR0_EL1: 0 = no SVE
                        // P3 slice C — PMU: 0 = no common events advertised /
                        // no SPE buffer.
                        (3, 3, 9, 12, 6) => 0, // PMCEID0_EL0
                        (3, 3, 9, 12, 7) => 0, // PMCEID1_EL0
                        (3, 0, 9, 10, 7) => 0, // PMBIDR_EL1
                        // P3 slice F — debug: OSLSR_EL1 = 0 (OSLK=0, lock
                        // not implemented). (OSDLR_EL1 is write-only: its
                        // MRS encoding is UNDEFINED and traps — see
                        // p3_write_only_sysregs_trap_on_mrs.)
                        (2, 0, 1, 1, 4) => 0, // OSLSR_EL1
                        // (P3, 2026-10-03) PMUSERENR_EL0 graduated to
                        // persistent (ReadSys); constant-0 arm moved above.
                        // CLIDR_EL1: Cache Level ID Register (10 static hits)
                        // L1 Harvard (separate I/D), L2 unified, LoUIS=1, LoUU=1, LoC=2
                        (3, 1, 0, 0, 1) => 0x0920_0023,
                        // CSSELR_EL1: Cache Size Selection Register
                        (3, 2, 0, 0, 0) => 0,
                        // CCSIDR_EL1: Cache Size ID Register (64B line, 4-way, 64KB)
                        (3, 1, 0, 0, 0) => 0x701F_E00A,
                        // CNTFRQ_EL0: Counter-timer frequency (62.5MHz)
                        (3, 3, 14, 0, 0) => 0x03B9_ACA0,
                        // (fam/devices) CNTVCT_EL0/CNTPCT_EL0 now persistent
                        // (ReadSys), not constant 0 -- kernel delay loops
                        // spin forever on a frozen counter.
                        // TPIDRRO_EL0: Thread read-only register (22 static hits)
                        (3, 3, 13, 0, 3) => 0,
                        // TPIDR_EL0: Thread ID register EL0 (21 static hits)
                        (3, 3, 13, 0, 2) => 0,
                        // Exception status / syndrome registers
                        (3, 0, 5, 2, 0) => 0, // ESR_EL1
                        (3, 0, 6, 0, 0) => 0, // FAR_EL1
                        (3, 0, 4, 0, 1) => 0, // ELR_EL1
                        (3, 0, 4, 0, 0) => 0, // SPSR_EL1
                        // Performance monitors
                        // PMCR_EL0 (P3: PMUSERENR_EL0 graduated to persistent
                        // above, so it no longer belongs in this list).
                        (3, 3, 9, 12, 0) => 0,
                        _ => return trap(R_SYSTEM),
                    };
                    vec![IrOp::Mov { dst: rt, imm: val }]
                };
            }
        };
        return vec![IrOp::ReadSys {
            dst: rt,
            reg: persistent,
        }];
    }
    // MSR <sysreg>, Xt or MSR <pstatefield>, #imm: bit 21 == 0.
    if (word >> 22) & 0x3FF == 0x354 && ((word >> 21) & 1) == 0 {
        let op0 = (word >> 19) & 0x3;
        let op1 = (word >> 16) & 0x7;
        let crn = (word >> 12) & 0xF;
        let crm = (word >> 8) & 0xF;
        let op2 = (word >> 5) & 0x7;
        let rt = (word & 0x1F) as u8;

        // Persistent system registers: MSR now stores real state
        // (GB-sysreg2). These replaced GB-3 accepted-but-stateless no-ops.
        let persistent = match (op0, op1, crn, crm, op2) {
            // DAIF
            (3, 3, 4, 2, 1) => SysReg::Daif,
            // TPIDR_EL1
            (3, 0, 13, 0, 4) => SysReg::TpidrEl1,
            // SCTLR_EL1, SCTLR_EL2
            (3, 0, 1, 0, 0) => SysReg::SctlrEl1,
            (3, 4, 1, 0, 0) => SysReg::SctlrEl2,
            // HCR_EL2
            (3, 4, 1, 1, 0) => SysReg::HcrEl2,
            // CNTHCTL_EL2
            (3, 4, 14, 1, 0) => SysReg::CnthctlEl2,
            // CNTP_CTL_EL0 (fam/devices)
            (3, 3, 14, 2, 1) => SysReg::CntpCtlEl0,
            // CNTP_CVAL_EL0 (fam/devices)
            (3, 3, 14, 2, 2) => SysReg::CntpCvalEl0,
            // CNTV_CTL_EL0 (fam/devices)
            (3, 3, 14, 3, 1) => SysReg::CntvCtlEl0,
            // CNTV_CVAL_EL0 (fam/devices)
            (3, 3, 14, 3, 2) => SysReg::CntvCvalEl0,
            // CNTVOFF_EL2
            (3, 4, 14, 0, 3) => SysReg::CntvoffEl2,
            // VBAR_EL1
            (3, 0, 12, 0, 0) => SysReg::VbarEl1,
            // SP_EL0
            (3, 0, 4, 1, 0) => SysReg::SpEl0,
            // CPACR_EL1 (GB-9): kernel enables FP/ASIMD via MSR CPACR_EL1
            (3, 0, 1, 0, 2) => SysReg::CpacrEl1,
            // MDSCR_EL1 (GB-10): kernel zeroes debug control via MSR MDSCR_EL1
            (2, 0, 0, 2, 2) => SysReg::MdscrEl1,
            // MAIR_EL1 (GB-13): kernel programs memory attributes via MSR MAIR_EL1
            (3, 0, 10, 2, 0) => SysReg::MairEl1,
            // TCR_EL1 (GB-17): kernel programs translation control via MSR TCR_EL1
            (3, 0, 2, 0, 2) => SysReg::TcrEl1,
            // TTBR0_EL1 (GB-18): kernel programs translation table base via MSR TTBR0_EL1
            (3, 0, 2, 0, 0) => SysReg::Ttbr0El1,
            // TTBR1_EL1 (GB-19): kernel programs translation table base 1 via MSR TTBR1_EL1
            (3, 0, 2, 0, 1) => SysReg::Ttbr1El1,
            // CONTEXTIDR_EL1 (P0, 2026-10-03): kernel writes on every context
            // switch (MSR CONTEXTIDR_EL1, Xt). S3_0_C13_C0_1. Stored, no behavior.
            (3, 0, 13, 0, 1) => SysReg::ContextidrEl1,
            // OSLAR_EL1 (P2, 2026-10-03): OS Lock Access Register, EL1.
            // Write-only; kernel writes during debug setup
            // (MSR OSLAR_EL1, Xt). S2_0_C1_C0_4. Stored, no behavior.
            (2, 0, 1, 0, 4) => SysReg::OslarEl1,
            // PMCNTENSET_EL0 (P2, 2026-10-03): kernel probes the PMU
            // during boot (MSR PMCNTENSET_EL0, Xt). S3_3_C9_C12_1.
            // Stored as u64, default 0.
            (3, 3, 9, 12, 1) => SysReg::PmcntensetEl0,
            // PMSELR_EL0 (P2, 2026-10-03): kernel probes the PMU during
            // boot (MSR PMSELR_EL0, Xt). S3_3_C9_C12_5. Stored as u64.
            (3, 3, 9, 12, 5) => SysReg::PmselrEl0,
            // P3 slice A — GICv3 CPU interface (2026-10-03).
            // ICC_EOIR1_EL1 S3_0_C12_C12_1: write-only accept (no interrupt
            // state to complete against yet).
            (3, 0, 12, 12, 1) => SysReg::IccEoir1El1,
            // ICC_DIR_EL1 S3_0_C12_C11_1: write-only accept.
            (3, 0, 12, 11, 1) => SysReg::IccDirEl1,
            // ICC_SRE_EL1 / ICC_CTLR_EL1 / ICC_IGRPEN1_EL1 / ICC_PMR_EL1:
            // stored u64.
            (3, 0, 12, 12, 5) => SysReg::IccSreEl1,
            (3, 0, 12, 12, 4) => SysReg::IccCtlrEl1,
            (3, 0, 12, 12, 7) => SysReg::IccIgrpen1El1,
            (3, 0, 4, 6, 0) => SysReg::IccPmrEl1,
            // P3 slice C — PMU remainder.
            // PMCNTENCLR_EL0 / PMOVSCLR_EL0 / PMXEVTYPER_EL0 / PMXEVCNTR_EL0 /
            // PMUSERENR_EL0: stored u64.
            (3, 3, 9, 12, 2) => SysReg::PmcntEnClrEl0,
            (3, 3, 9, 12, 3) => SysReg::PmovsclrEl0,
            (3, 3, 9, 13, 1) => SysReg::PmxevtyperEl0,
            (3, 3, 9, 13, 2) => SysReg::PmxevcntrEl0,
            (3, 3, 9, 14, 0) => SysReg::PmuserenrEl0,
            // P3 slice D — timers.
            // CNTKCTL_EL1 S3_0_C14_C1_0: stored u64.
            (3, 0, 14, 1, 0) => SysReg::CntkctlEl1,
            // TPIDR_EL2 S3_4_C13_C0_2: stored (hyp code unreachable at EL1).
            (3, 4, 13, 0, 2) => SysReg::TpidrEl2,
            // CNTP_TVAL_EL0 / CNTV_TVAL_EL0: write sets CVAL = counter +
            // value[31:0] (honest alias semantics).
            (3, 3, 14, 2, 0) => SysReg::CntpTvalEl0,
            (3, 3, 14, 3, 0) => SysReg::CntvTvalEl0,
            // P3 slice E — FP/SIMD: FPCR / FPSR stored u64.
            (3, 3, 4, 4, 0) => SysReg::Fpcr,
            (3, 3, 4, 4, 1) => SysReg::Fpsr,
            // P3 slice F — debug: OSDLR_EL1 write-only, stored.
            (2, 0, 1, 3, 4) => SysReg::OsdlrEl1,
            // MSR DAIFSet, #imm (op2=6) / MSR DAIFClr, #imm (op2=7): real
            // read-modify-write of the persistent DAIF (GB-11). Upgrades
            // the GB-3 accepted no-ops to honest state.
            (0, 3, 4, imm, 6) => {
                let set = daif_imm_mask(imm);
                return vec![IrOp::DaifRmw { set, clr: 0 }];
            }
            (0, 3, 4, imm, 7) => {
                let clr = daif_imm_mask(imm);
                return vec![IrOp::DaifRmw { set: 0, clr }];
            }
            // MSR PAN, #imm: Privileged Access Never (2,653 static hits)
            (0, 0, 4, _imm, 4) => return vec![],
            // MSR UAO, #imm: User Access Override (102 static hits)
            (0, 0, 4, _imm, 3) => return vec![],
            // MSR DIT / SSBS / TCO, #imm: Speculative & timing hints
            (0, 3, 4, _imm, 1) => return vec![],
            (0, 3, 4, _imm, 4) => return vec![],
            (0, 3, 4, _imm, 5) => return vec![],
            _ => {
                return {
                    match (op0, op1, crn, crm, op2) {
                        // NZCV: GB-2 live flag path (flags, not a stored register)
                        (3, 3, 4, 2, 0) => vec![],
                        // SPSel: accepted no-op (GB-3). DAIFSet/DAIFClr are real
                        // read-modify-write ops now (GB-11); see the arms above.
                        (0, 0, 4, 1, 5) => vec![],
                        // TPIDRRO_EL0 (29 static hits)
                        (3, 3, 13, 0, 3) => vec![],
                        // TPIDR_EL0 (19 static hits)
                        (3, 3, 13, 0, 2) => vec![],
                        // CSSELR_EL1: Cache Size Selection
                        (3, 2, 0, 0, 0) => vec![],
                        // Exception registers (ESR_EL1, FAR_EL1, ELR_EL1, SPSR_EL1)
                        (3, 0, 5, 2, 0) => vec![],
                        (3, 0, 6, 0, 0) => vec![],
                        (3, 0, 4, 0, 1) => vec![],
                        (3, 0, 4, 0, 0) => vec![],
                        // Performance monitors (PMCR_EL0, PMINTENSET_EL1, PMINTENCLR_EL1)
                        (3, 3, 9, 12, 0) => vec![],
                        (3, 0, 9, 14, 1) => vec![],
                        (3, 0, 9, 14, 2) => vec![],
                        _ => return trap(R_SYSTEM),
                    }
                };
            }
        };
        return vec![IrOp::WriteSys {
            src: rt,
            reg: persistent,
        }];
    }
    trap(R_SYSTEM)
}

/// Data-processing: MOVZ/MOVN/MOVK, ADD/SUB (immediate), ADD/SUB (shifted register, LSL #0),
/// CLZ (1-source, 64-bit),
/// Logical (immediate / shifted register), Bitfield (SBFM/BFM/UBFM),
/// Variable shifts (ASRV/LSRV/LSLV/RORV).
fn lift_data_proc(word: u32) -> Vec<IrOp> {
    // MOVZ: sf 10 100101 hw imm16 Rd  (bits 30:23 = 0xA5)
    if (word >> 23) & 0xFF == 0xA5 {
        let sf = word >> 31;
        let hw = (word >> 21) & 0x3;
        if sf == 0 && hw > 1 {
            return trap(R_MOVZ_HW);
        }
        let imm16 = (word >> 5) & 0xFFFF;
        let rd = (word & 0x1F) as u8;
        // 32-bit MOVZ zero-extends by definition, so the 64-bit slot value is
        // exactly imm16 << (hw * 16) with upper bits zero.
        let imm = (imm16 as u64) << (hw * 16);
        return vec![IrOp::Mov { dst: rd, imm }];
    }
    // MOVN: sf 00 100101 hw imm16 Rd  (bits 30:23 = 0x25)
    if (word >> 23) & 0xFF == 0x25 {
        let sf = word >> 31;
        let hw = (word >> 21) & 0x3;
        if sf == 0 && hw > 1 {
            return trap(R_MOVZ_HW);
        }
        let imm16 = (word >> 5) & 0xFFFF;
        let rd = (word & 0x1F) as u8;
        let val = (imm16 as u64) << (hw * 16);
        let imm = if sf == 1 { !val } else { (!val) & 0xFFFF_FFFF };
        return vec![IrOp::Mov { dst: rd, imm }];
    }
    // MOVK: sf 11 100101 hw imm16 Rd  (bits 30:23 = 0xE5) — Track GB-4
    if (word >> 23) & 0xFF == 0xE5 {
        let sf = word >> 31;
        let hw = ((word >> 21) & 0x3) as u8;
        if sf == 0 && hw > 1 {
            return trap(R_MOVZ_HW);
        }
        let imm16 = ((word >> 5) & 0xFFFF) as u16;
        let rd = (word & 0x1F) as u8;
        return vec![IrOp::Movk {
            dst: rd,
            imm: imm16,
            hw,
            is_32: sf == 0,
        }];
    }
    // ADD (immediate): sf 0 0 10001 sh imm12 Rn Rd  (bits 30:24 = 0x11;
    // bit 29 = 0 excludes ADDS, bit 30 = 0 excludes SUB(S)).
    // 32-bit form: result is masked to 32 bits via AndShift(is_32).
    // Addition mod 2^32 depends only on low 32 bits, so masking the
    // 64-bit result is exact (Rn=31/WSP high bits do not affect it).
    if (word >> 24) & 0x7F == 0x11 {
        let sf = word >> 31;
        let sh = (word >> 22) & 1;
        let imm12 = (word >> 10) & 0xFFF;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        let imm = (imm12 as u64) << (if sh == 1 { 12 } else { 0 });
        if sf == 0 {
            return vec![
                IrOp::Mov { dst: SCRATCH, imm },
                IrOp::Add {
                    dst: SCRATCH,
                    a: rn,
                    b: SCRATCH,
                },
                IrOp::AndShift {
                    dst: rd,
                    a: SCRATCH,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                    invert: false,
                    is_32: true,
                },
            ];
        }
        return vec![
            IrOp::Mov { dst: SCRATCH, imm },
            IrOp::Add {
                dst: rd,
                a: rn,
                b: SCRATCH,
            },
        ];
    }
    // SUB (immediate): sf 1 0 10001 sh imm12 Rn Rd  (bits 30:24 = 0x51;
    // bit 29 = 0 excludes SUBS, bit 30 = 1 specifies SUB).
    // 32-bit form: masked via AndShift(is_32), same reasoning as ADD.
    if (word >> 24) & 0x7F == 0x51 {
        let sf = word >> 31;
        let sh = (word >> 22) & 1;
        let imm12 = (word >> 10) & 0xFFF;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        let imm = (imm12 as u64) << (if sh == 1 { 12 } else { 0 });
        if sf == 0 {
            return vec![
                IrOp::Mov { dst: SCRATCH, imm },
                IrOp::Sub {
                    dst: SCRATCH,
                    a: rn,
                    b: SCRATCH,
                },
                IrOp::AndShift {
                    dst: rd,
                    a: SCRATCH,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                    invert: false,
                    is_32: true,
                },
            ];
        }
        return vec![
            IrOp::Mov { dst: SCRATCH, imm },
            IrOp::Sub {
                dst: rd,
                a: rn,
                b: SCRATCH,
            },
        ];
    }
    // Logical (immediate): sf opc 100100 N immr imms Rn Rd  (bits 28:23 = 0x24) — Track GB-4
    if (word >> 23) & 0x3F == 0x24 {
        let sf = (word >> 31) & 1 == 1;
        let opc = (word >> 29) & 0x3;
        let n = ((word >> 22) & 1) as u8;
        let immr = ((word >> 16) & 0x3F) as u8;
        let imms = ((word >> 10) & 0x3F) as u8;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        if let Some(imm) = decode_bitmasks(n, imms, immr, sf) {
            let is_32 = !sf;
            return match opc {
                0b00 | 0b11 => vec![
                    IrOp::Mov { dst: SCRATCH, imm },
                    IrOp::AndShift {
                        dst: rd,
                        a: rn,
                        b: SCRATCH,
                        shift: 0,
                        amount: 0,
                        invert: false,
                        is_32,
                    },
                ],
                0b01 => vec![
                    IrOp::Mov { dst: SCRATCH, imm },
                    IrOp::OrShift {
                        dst: rd,
                        a: rn,
                        b: SCRATCH,
                        shift: 0,
                        amount: 0,
                        invert: false,
                        is_32,
                    },
                ],
                0b10 => vec![
                    IrOp::Mov { dst: SCRATCH, imm },
                    IrOp::EorShift {
                        dst: rd,
                        a: rn,
                        b: SCRATCH,
                        shift: 0,
                        amount: 0,
                        invert: false,
                        is_32,
                    },
                ],
                _ => trap(R_DP_UNSUPPORTED),
            };
        }
    }
    // Bitfield (immediate): sf opc 100110 N immr imms Rn Rd  (bits 28:23 = 0x26) — Track GB-4
    if (word >> 23) & 0x3F == 0x26 {
        let sf = (word >> 31) & 1;
        let opc = ((word >> 29) & 0x3) as u8;
        let immr = ((word >> 16) & 0x3F) as u8;
        let imms = ((word >> 10) & 0x3F) as u8;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        if opc < 3 {
            return vec![IrOp::Bitfield {
                dst: rd,
                src: rn,
                opc,
                immr,
                imms,
                is_32: sf == 0,
            }];
        }
    }
    // ADD/SUB (extended register): sf op S 01011 opt 1 Rm option imm3 Rn Rd
    // (bit 21 = 1 distinguishes from shifted register form).
    // Unsigned extends (UXTB/UXTH/UXTW/UXTX) are implemented via AND mask + LSL.
    // Signed extends (SXTB/SXTH/SXTW/SXTX) trap honestly.
    {
        let top = (word >> 24) & 0xFF;
        let bit21 = (word >> 21) & 1;
        if (top == 0x0B || top == 0x8B || top == 0x4B || top == 0xCB) && bit21 == 1 {
            let sf = word >> 31;
            let op = (word >> 30) & 1; // 0=ADD, 1=SUB
            let rm = ((word >> 16) & 0x1F) as u8;
            let option = (word >> 13) & 0x7;
            let imm3 = ((word >> 10) & 0x7) as u8;
            let rn = ((word >> 5) & 0x1F) as u8;
            let rd = (word & 0x1F) as u8;
            // option: 0=UXTB, 1=UXTH, 2=UXTW, 3=UXTX, 4=SXTB, 5=SXTH, 6=SXTW, 7=SXTX
            let mask: u64 = match option {
                0 => 0xFF,       // UXTB
                1 => 0xFFFF,     // UXTH
                2 => 0xFFFFFFFF, // UXTW
                3 => u64::MAX,   // UXTX (no mask)
                _ => return trap(R_ADD_EXTEND_SIGNED), // SXTB/SXTH/SXTW/SXTX: honest trap
            };
            let mut ops = Vec::new();
            if option != 3 {
                // Load mask and AND to zero-extend.
                ops.push(IrOp::Mov {
                    dst: SCRATCH,
                    imm: mask,
                });
                ops.push(IrOp::AndShift {
                    dst: SCRATCH,
                    a: rm,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                    invert: false,
                    is_32: false,
                });
            } else {
                // UXTX: no mask needed, just use Rm directly.
                // Move to SCRATCH for the shift step.
                ops.push(IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31, // XZR
                    b: rm,
                    shift: 0, // LSL
                    amount: 0,
                });
            }
            // Shift left by imm3: SCRATCH = SCRATCH << imm3
            if imm3 != 0 {
                ops.push(IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31, // XZR
                    b: SCRATCH,
                    shift: 0, // LSL
                    amount: imm3,
                });
            }
            // ADD or SUB: Rd = Rn +/- SCRATCH
            if op == 0 {
                ops.push(IrOp::Add {
                    dst: rd,
                    a: rn,
                    b: SCRATCH,
                });
            } else {
                ops.push(IrOp::Sub {
                    dst: rd,
                    a: rn,
                    b: SCRATCH,
                });
            }
            // 32-bit form: mask result to 32 bits.
            if sf == 0 {
                ops.push(IrOp::AndShift {
                    dst: rd,
                    a: rd,
                    b: rd, // AND with itself = identity, is_32 does masking
                    shift: 0,
                    amount: 0,
                    invert: false,
                    is_32: true,
                });
            }
            return ops;
        }
    }
    // ADD (shifted register): sf 0 01011 00 0 Rm imm6 Rn Rd, LSL #0 only
    // (bits 31:24 = 0x0B/0x8B; S = 1 would be ADDS and never matches).
    // 32-bit form: masked via AndShift(is_32).
    let top = (word >> 24) & 0xFF;
    if top == 0x0B || top == 0x8B {
        let sf = word >> 31;
        let shift = (word >> 22) & 0x3;
        let imm6 = (word >> 10) & 0x3F;
        if shift != 0 || imm6 != 0 {
            return trap(R_ADD_SHIFT);
        }
        let rm = ((word >> 16) & 0x1F) as u8;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        if sf == 0 {
            return vec![
                IrOp::Add {
                    dst: SCRATCH,
                    a: rn,
                    b: rm,
                },
                IrOp::AndShift {
                    dst: rd,
                    a: SCRATCH,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                    invert: false,
                    is_32: true,
                },
            ];
        }
        return vec![IrOp::Add {
            dst: rd,
            a: rn,
            b: rm,
        }];
    }
    // SUB (shifted register): sf 1 01011 shift 0 Rm imm6 Rn Rd, LSL #0 only
    // (bits 31:24 = 0x4B/0xCB; S = 1 would be SUBS and never matches) — Track GB-6
    // 32-bit form: masked via AndShift(is_32).
    if top == 0x4B || top == 0xCB {
        let sf = word >> 31;
        let shift = (word >> 22) & 0x3;
        let imm6 = (word >> 10) & 0x3F;
        if shift != 0 || imm6 != 0 {
            return trap(R_SUB_SHIFT);
        }
        let rm = ((word >> 16) & 0x1F) as u8;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        if sf == 0 {
            return vec![
                IrOp::Sub {
                    dst: SCRATCH,
                    a: rn,
                    b: rm,
                },
                IrOp::AndShift {
                    dst: rd,
                    a: SCRATCH,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                    invert: false,
                    is_32: true,
                },
            ];
        }
        return vec![IrOp::Sub {
            dst: rd,
            a: rn,
            b: rm,
        }];
    }
    // ADC/SBC (with carry): sf op S 11010000 Rm 000000 Rn Rd
    // (bits[28:21] == 0xD0, bits[15:10] == 0). Carry flag (NZCV.C) is not
    // readable in IrOp, so these trap honestly. Rn=31 is XZR for S=1
    // (ADCS/SBCS), SP for S=0 (ADC/SBC).
    {
        let b28_21 = (word >> 21) & 0xFF;
        let b15_10 = (word >> 10) & 0x3F;
        if b28_21 == 0xD0 && b15_10 == 0 {
            return trap(R_ADC_CARRY);
        }
    }

    // CLZ (1 source): sf 1 S 11010 110 00000 000100 Rn Rd, S = 0
    // (bits 31:24 = 0xDA/0x5A) — Track GB-7. 64-bit only; the 32-bit form
    // traps: upper-bit zeroing is not expressible in IrOp::Clz.
    if top == 0xDA || top == 0x5A {
        if (word >> 21) & 0x7 != 0b110
            || (word >> 16) & 0x1F != 0
            || (word >> 10) & 0x3F != 0b000100
        {
            return trap(R_DP_UNSUPPORTED);
        }
        if word >> 31 == 0 {
            return trap(R_CLZ32);
        }
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        return vec![IrOp::Clz { dst: rd, src: rn }];
    }
    // MADD / MSUB / long-multiply (3 source): sf op54 11011 op31 Rm o0 Ra Rn Rd
    // (bits 28:24 = 0x1B) — Track GB-8. Only 64-bit MADD
    // (op54 = 00, op31 = 000, o0 = 0) lifts; 32-bit MADD, MSUB and the
    // long-multiply family trap with explicit reasons.
    if (word >> 24) & 0x1F == 0x1B {
        let op54 = (word >> 29) & 0x3;
        let op31 = (word >> 21) & 0x7;
        let o0 = (word >> 15) & 1;
        if op54 == 0 && op31 == 0 && o0 == 1 {
            return trap(R_MSUB);
        }
        if op54 == 0b01 || op54 == 0b10 {
            return trap(R_MADD_LONG);
        }
        if op54 != 0 || op31 != 0 || o0 != 0 {
            return trap(R_DP_UNSUPPORTED);
        }
        if word >> 31 == 0 {
            return trap(R_MADD32);
        }
        let rm = ((word >> 16) & 0x1F) as u8;
        let ra = ((word >> 10) & 0x1F) as u8;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        return vec![IrOp::Madd {
            dst: rd,
            n: rn,
            m: rm,
            a: ra,
        }];
    }
    // Logical (shifted register): sf opc 01010 shift N Rm imm6 Rn Rd  (bits 28:24 = 0x0A) — Track GB-4
    if (word >> 24) & 0x1F == 0x0A {
        let sf = (word >> 31) & 1;
        let opc = (word >> 29) & 0x3;
        let shift = ((word >> 22) & 0x3) as u8;
        let n = (word >> 21) & 1 == 1;
        let rm = ((word >> 16) & 0x1F) as u8;
        let amount = ((word >> 10) & 0x3F) as u8;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        let is_32 = sf == 0;
        return match opc {
            0b00 | 0b11 => vec![IrOp::AndShift {
                dst: rd,
                a: rn,
                b: rm,
                shift,
                amount,
                invert: n,
                is_32,
            }],
            0b01 => {
                if is_32 {
                    return trap(R_ORR32);
                }
                if !n {
                    // Exact backward compatibility with existing tests
                    vec![IrOp::OrrShift {
                        dst: rd,
                        a: rn,
                        b: rm,
                        shift,
                        amount,
                    }]
                } else {
                    vec![IrOp::OrShift {
                        dst: rd,
                        a: rn,
                        b: rm,
                        shift,
                        amount,
                        invert: n,
                        is_32: false,
                    }]
                }
            }
            0b10 => vec![IrOp::EorShift {
                dst: rd,
                a: rn,
                b: rm,
                shift,
                amount,
                invert: n,
                is_32,
            }],
            _ => trap(R_DP_UNSUPPORTED),
        };
    }
    // Data-processing (2 source): sf 0 0 11010 110 Rm 0010 op2 Rn Rd  (bits 28:21 = 0xD6) — Track GB-4
    if ((word >> 21) & 0xFF == 0xD6) && ((word >> 29) & 1 == 0) {
        let opcode2 = (word >> 10) & 0x3F;
        if (opcode2 >> 2) == 0b0010 {
            let sf = (word >> 31) & 1;
            let rm = ((word >> 16) & 0x1F) as u8;
            let rn = ((word >> 5) & 0x1F) as u8;
            let rd = (word & 0x1F) as u8;
            let shift = (opcode2 & 0x3) as u8;
            return vec![IrOp::ShiftVar {
                dst: rd,
                a: rn,
                b: rm,
                shift,
                is_32: sf == 0,
            }];
        }
    }
    // CSEL family (conditional select): sf op S 11010100 Rm cond op2 Rn Rd
    // (bits 30:21 = 0xD4 or 0x2D4). Condition codes AL (0b1110) and NV
    // (0b1111) are legal and always-true per ARM ARM — the decoder accepts
    // them; the lifter still traps because NZCV is not in IrOp.
    // CSET/CINC/CINV/CNEG are aliases (Rm=XZR or Rn=XZR with inverted cond).
    {
        let b30_21 = (word >> 21) & 0x3FF;
        if b30_21 == 0xD4 || b30_21 == 0x2D4 {
            return trap(R_CSEL_COND);
        }
    }
    if let Some(ops) = lift_fp(word) {
        return ops;
    }
    trap(R_DP_UNSUPPORTED)
}

/// Loads/stores (GB-1 scope):
/// - LDR (literal) keeps its static Load form; LDRSW (literal) adds sign extension.
/// - STP/LDP pairs (all variants: 32-bit, 64-bit, LDPSW, offset, pre/post-indexed, non-temporal)
///   lower to StoreDyn/LoadDyn sequences.
/// - LDR/STR (immediate unsigned offset, pre/post-indexed, register-offset)
///   lower to LoadDyn/StoreDyn.
/// - LDRSW variants load 4 bytes and sign-extend to 64-bit via OrrShift.
fn lift_signed_extend(ops: &mut Vec<IrOp>, rt: u8, size: u8, is_64: bool) {
    let shift = match size {
        1 => 56,
        2 => 48,
        4 => 32,
        _ => unreachable!(),
    };
    if is_64 {
        ops.push(IrOp::OrrShift {
            dst: SCRATCH,
            a: 31,
            b: SCRATCH,
            shift: 0,
            amount: shift,
        });
        ops.push(IrOp::OrrShift {
            dst: rt,
            a: 31,
            b: SCRATCH,
            shift: 2,
            amount: shift,
        });
    } else {
        ops.push(IrOp::OrrShift {
            dst: SCRATCH,
            a: 31,
            b: SCRATCH,
            shift: 0,
            amount: shift,
        });
        ops.push(IrOp::OrrShift {
            dst: SCRATCH,
            a: 31,
            b: SCRATCH,
            shift: 2,
            amount: shift,
        });
        ops.push(IrOp::AndShift {
            dst: rt,
            a: SCRATCH,
            b: SCRATCH,
            shift: 0,
            amount: 0,
            invert: false,
            is_32: true,
        });
    }
}

fn lift_load_store(insn: &Instruction) -> Vec<IrOp> {
    let word = insn.word;
    // 6. Atomic swap (SWP / SWPA / SWPL / SWPAL):
    // size 111 0 00 A R 1 Rs 1 00000 Rn Rt (bits 29:24 = 0b111000, bit 21 = 1, bit 15 = 1, bits 14:10 = 0)
    if (word >> 24) & 0x3F == 0b111000
        && (word >> 21) & 1 == 1
        && (word >> 15) & 1 == 1
        && (word >> 10) & 0x1F == 0b00000
    {
        let size_bits = (word >> 30) & 0x3;
        let rs = ((word >> 16) & 0x1F) as u8;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rt = (word & 0x1F) as u8;

        if rn == 31 {
            return trap(R_LS_SP);
        }
        if size_bits == 1 {
            return trap(R_LS_SUBWORD);
        }

        let size: u8 = 1 << size_bits;
        let mut ops = vec![
            IrOp::LoadDyn {
                dst: SCRATCH,
                base: rn,
                off: 0,
                size,
            },
            IrOp::StoreDyn {
                src: rs,
                base: rn,
                off: 0,
                size,
            },
        ];
        if rt != 31 {
            ops.push(IrOp::OrrShift {
                dst: rt,
                a: 31,
                b: SCRATCH,
                shift: 0,
                amount: 0,
            });
        }
        return ops;
    }

    // 7. Load-acquire / store-release (LDAR / STLR):
    // size 001000 1 L 0 11111 1 11111 Rn Rt
    if (word >> 24) & 0x3F == 0b001000
        && (word >> 23) & 1 == 1
        && (word >> 21) & 1 == 0
        && (word >> 16) & 0x1F == 0b11111
        && (word >> 15) & 1 == 1
        && (word >> 10) & 0x1F == 0b11111
    {
        let size_bits = (word >> 30) & 0x3;
        let is_load = (word >> 22) & 1 == 1;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rt = (word & 0x1F) as u8;

        if rn == 31 {
            return trap(R_LS_SP);
        }
        if size_bits == 1 {
            return trap(R_LS_SUBWORD);
        }

        let size: u8 = 1 << size_bits;
        if is_load {
            return vec![IrOp::LoadDyn {
                dst: rt,
                base: rn,
                off: 0,
                size,
            }];
        } else {
            return vec![IrOp::StoreDyn {
                src: rt,
                base: rn,
                off: 0,
                size,
            }];
        }
    }

    // 8. Load-acquire RCpc (LDAPR):
    // size 111 0 00 0 1 1 11111 110000 Rn Rt
    if (word >> 24) & 0x3F == 0b111000
        && (word >> 21) & 1 == 1
        && (word >> 16) & 0x1F == 0b11111
        && (word >> 10) & 0x3F == 0b110000
    {
        let size_bits = (word >> 30) & 0x3;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rt = (word & 0x1F) as u8;

        if rn == 31 {
            return trap(R_LS_SP);
        }
        if size_bits == 1 {
            return trap(R_LS_SUBWORD);
        }

        let size: u8 = 1 << size_bits;
        return vec![IrOp::LoadDyn {
            dst: rt,
            base: rn,
            off: 0,
            size,
        }];
    }

    // 9. Compare and swap (CAS / CASA / CASL / CASAL):
    // size 001000 1 R 1 Rs L 011111 Rn Rt
    if (word >> 24) & 0x3F == 0b001000
        && (word >> 23) & 1 == 1
        && (word >> 21) & 1 == 1
        && (word >> 10) & 0x1F == 0b11111
    {
        return trap(R_ATOMIC_CAS);
    }

    // 10. Load/store exclusive (LDXR / STXR / LDAXR / STLXR):
    // size 001000 0 L 0 Rs o0 11111 Rn Rt
    if (word >> 24) & 0x3F == 0b001000 && (word >> 23) & 1 == 0 && (word >> 10) & 0x1F == 0b11111 {
        return trap(R_EXCLUSIVE);
    }

    // 11. Other LSE atomic memory operations (LDADD, STADD, LDCLR, STCLR, etc.):
    if (word >> 24) & 0x3F == 0b111000 && (word >> 21) & 1 == 1 && (word >> 10) & 0x3 == 0b00 {
        return trap(R_ATOMIC_LSE);
    }

    let word = insn.word;

    // 1. LDR / LDRSW / PRFM (literal): sf 00 011000 imm19 Rt (bits 31:24 = 0x18 / 0x58 / 0x98 / 0xD8).
    // Address = PC + sign_extend(imm19 << 2): fully static.
    let top = (word >> 24) & 0xFF;
    if top == 0x18 || top == 0x58 || top == 0x98 || top == 0xD8 {
        if top == 0xD8 {
            // PRFM (literal): true no-op
            return vec![];
        }
        let size: u8 = if top == 0x58 { 8 } else { 4 };
        let imm19 = (word >> 5) & 0x7FFFF;
        let offset = (((imm19 as i32) << 13) >> 13) as i64 * 4;
        let addr = (insn.addr as i64).wrapping_add(offset) as u64;
        let rt = (word & 0x1F) as u8;
        if top == 0x98 {
            // LDRSW (literal): load 4 bytes and sign-extend to 64 bits.
            let mut ops = vec![IrOp::Load {
                dst: SCRATCH,
                addr,
                size: 4,
            }];
            lift_signed_extend(&mut ops, rt, 4, true);
            return ops;
        }
        return vec![IrOp::Load {
            dst: rt,
            addr,
            size,
        }];
    }

    // 2. Load/store pair (STP, LDP, LDPSW, STNP, LDNP):
    // bits[29:25] == 0b10100 (pins V=0, integer registers).
    if (word >> 25) & 0x1F == 0b10100 {
        let opc = (word >> 30) & 0x3;
        let idx_mode = (word >> 23) & 0x3;
        let is_load = (word >> 22) & 1 == 1;
        let imm7 = ((word >> 15) & 0x7F) as i32;
        let rt2 = ((word >> 10) & 0x1F) as u8;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rt = (word & 0x1F) as u8;

        if rn == 31 {
            return trap(R_LS_SP);
        }

        let simm7 = ((imm7 << 25) >> 25) as i64;
        let (size, scale, is_signed) = match opc {
            0b00 => (4u8, 4i64, false),           // 32-bit pair
            0b01 if is_load => (4u8, 4i64, true), // LDPSW
            0b10 => (8u8, 8i64, false),           // 64-bit pair
            _ => return trap(R_LS_UNSUPPORTED),
        };
        let offset = simm7 * scale;

        let (base_off, writeback) = match idx_mode {
            0b00 => (offset, false), // Non-temporal (STNP/LDNP)
            0b01 => (0, true),       // Post-index
            0b10 => (offset, false), // Signed offset
            0b11 => (offset, true),  // Pre-index
            _ => unreachable!(),
        };

        let mut ops = Vec::new();
        if is_load {
            if is_signed {
                // LDPSW: load signed words into 64-bit registers
                ops.push(IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: rn,
                    off: base_off as u64,
                    size: 4,
                });
                ops.push(IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 32,
                });
                ops.push(IrOp::OrrShift {
                    dst: rt,
                    a: 31,
                    b: SCRATCH,
                    shift: 2,
                    amount: 32,
                });

                ops.push(IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: rn,
                    off: (base_off + 4) as u64,
                    size: 4,
                });
                ops.push(IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 32,
                });
                ops.push(IrOp::OrrShift {
                    dst: rt2,
                    a: 31,
                    b: SCRATCH,
                    shift: 2,
                    amount: 32,
                });
            } else {
                ops.push(IrOp::LoadDyn {
                    dst: rt,
                    base: rn,
                    off: base_off as u64,
                    size,
                });
                ops.push(IrOp::LoadDyn {
                    dst: rt2,
                    base: rn,
                    off: (base_off + size as i64) as u64,
                    size,
                });
            }
        } else {
            ops.push(IrOp::StoreDyn {
                src: rt,
                base: rn,
                off: base_off as u64,
                size,
            });
            ops.push(IrOp::StoreDyn {
                src: rt2,
                base: rn,
                off: (base_off + size as i64) as u64,
                size,
            });
        }

        if writeback && offset != 0 {
            ops.push(IrOp::Mov {
                dst: SCRATCH,
                imm: offset as u64,
            });
            ops.push(IrOp::Add {
                dst: rn,
                a: rn,
                b: SCRATCH,
            });
        }
        return ops;
    }

    // 3. Load/store register (immediate, unsigned offset):
    // size 11 111001 opc imm12 Rn Rt (bits 29:24 = 0x39).
    if (word >> 24) & 0x3F == 0x39 {
        let size_bits = (word >> 30) & 0x3;
        let opc = (word >> 22) & 0x3;
        let imm12 = (word >> 10) & 0xFFF;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rt = (word & 0x1F) as u8;

        if rn == 31 {
            return trap(R_LS_SP);
        }

        if opc == 0b10 && size_bits == 3 {
            // PRFM (immediate): true no-op
            return vec![];
        }

        let size: u8 = 1 << size_bits;
        let off = (imm12 as u64) << size_bits;

        if opc == 0b00 {
            return vec![IrOp::StoreDyn {
                src: rt,
                base: rn,
                off,
                size,
            }];
        } else if opc == 0b01 {
            return vec![IrOp::LoadDyn {
                dst: rt,
                base: rn,
                off,
                size,
            }];
        } else if opc == 0b10 {
            // Signed load into 64-bit register: LDRSB (size=1), LDRSH (size=2), LDRSW (size=4)
            let mut ops = vec![IrOp::LoadDyn {
                dst: SCRATCH,
                base: rn,
                off,
                size,
            }];
            lift_signed_extend(&mut ops, rt, size, true);
            return ops;
        } else if opc == 0b11 && size_bits <= 1 {
            // Signed load into 32-bit register: LDRSB Wt (size=1), LDRSH Wt (size=2)
            let mut ops = vec![IrOp::LoadDyn {
                dst: SCRATCH,
                base: rn,
                off,
                size,
            }];
            lift_signed_extend(&mut ops, rt, size, false);
            return ops;
        }
        return trap(R_LS_UNSUPPORTED);
    }

    // 4. Load/store register (immediate pre/post-indexed, unscaled, and unprivileged):
    // size 111 V 00 opc 0 imm9 type Rn Rt with bits[29:24] == 0b111000, bit 21 == 0.
    if (word >> 24) & 0x3F == 0b111000 && (word >> 21) & 1 == 0 {
        let idx_type = (word >> 10) & 0x3;
        let size_bits = (word >> 30) & 0x3;
        let opc = (word >> 22) & 0x3;
        let imm9 = ((word >> 12) & 0x1FF) as i32;
        let simm9 = ((imm9 << 23) >> 23) as i64;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rt = (word & 0x1F) as u8;

        if rn == 31 {
            return trap(R_LS_SP);
        }

        if opc == 0b10 && size_bits == 3 && idx_type == 0b00 {
            // PRFUM: true no-op
            return vec![];
        }

        let size: u8 = 1 << size_bits;
        let base_off = if idx_type == 0b01 { 0i64 } else { simm9 };

        let mut ops = Vec::new();
        if opc == 0b00 {
            ops.push(IrOp::StoreDyn {
                src: rt,
                base: rn,
                off: base_off as u64,
                size,
            });
        } else if opc == 0b01 {
            ops.push(IrOp::LoadDyn {
                dst: rt,
                base: rn,
                off: base_off as u64,
                size,
            });
        } else if opc == 0b10 && size_bits <= 2 {
            // LDRSB/H/W Xt (64-bit sign-extended)
            ops.push(IrOp::LoadDyn {
                dst: SCRATCH,
                base: rn,
                off: base_off as u64,
                size,
            });
            lift_signed_extend(&mut ops, rt, size, true);
        } else if opc == 0b11 && size_bits <= 1 {
            // LDRSB/H Wt (32-bit sign-extended)
            ops.push(IrOp::LoadDyn {
                dst: SCRATCH,
                base: rn,
                off: base_off as u64,
                size,
            });
            lift_signed_extend(&mut ops, rt, size, false);
        } else {
            return trap(R_LS_UNSUPPORTED);
        }

        if simm9 != 0 && (idx_type == 0b01 || idx_type == 0b11) {
            // Writeback for pre/post-index only
            ops.push(IrOp::Mov {
                dst: SCRATCH,
                imm: simm9 as u64,
            });
            ops.push(IrOp::Add {
                dst: rn,
                a: rn,
                b: SCRATCH,
            });
        }
        return ops;
    }

    // 5. Load/store register (register offset):
    // size 111 V 00 opc 1 Rm option S 10 Rn Rt
    if (word >> 24) & 0x3F == 0b111000 && (word >> 21) & 1 == 1 {
        let size_bits = (word >> 30) & 0x3;
        let opc = (word >> 22) & 0x3;
        let rm = ((word >> 16) & 0x1F) as u8;
        let option = (word >> 13) & 0x7;
        let s = (word >> 12) & 1;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rt = (word & 0x1F) as u8;

        if rn == 31 {
            return trap(R_LS_SP);
        }

        if opc == 0b10 && size_bits == 3 {
            // PRFM (register): true no-op
            return vec![];
        }

        let size: u8 = 1 << size_bits;
        let shift_amt: u8 = if s == 1 { size_bits as u8 } else { 0 };

        // option 011 = 64-bit register offset (LSL), 010 = 32-bit UXTW (zero-extended)
        if option == 0b011 || option == 0b010 {
            let mut ops = Vec::new();
            if option == 0b010 {
                // UXTW: zero-extend Wm to 64-bit into SCRATCH
                ops.push(IrOp::AndShift {
                    dst: SCRATCH,
                    a: rm,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                    invert: false,
                    is_32: true,
                });
                if shift_amt > 0 {
                    ops.push(IrOp::OrrShift {
                        dst: SCRATCH,
                        a: 31,
                        b: SCRATCH,
                        shift: 0,
                        amount: shift_amt,
                    });
                }
                ops.push(IrOp::Add {
                    dst: SCRATCH,
                    a: rn,
                    b: SCRATCH,
                });
            } else if shift_amt > 0 {
                ops.push(IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: rm,
                    shift: 0,
                    amount: shift_amt,
                });
                ops.push(IrOp::Add {
                    dst: SCRATCH,
                    a: rn,
                    b: SCRATCH,
                });
            } else {
                ops.push(IrOp::Add {
                    dst: SCRATCH,
                    a: rn,
                    b: rm,
                });
            }

            if opc == 0b00 {
                ops.push(IrOp::StoreDyn {
                    src: rt,
                    base: SCRATCH,
                    off: 0,
                    size,
                });
            } else if opc == 0b01 {
                ops.push(IrOp::LoadDyn {
                    dst: rt,
                    base: SCRATCH,
                    off: 0,
                    size,
                });
            } else if opc == 0b10 && size_bits <= 2 {
                // LDRSB/H/W Xt (64-bit sign-extended)
                ops.push(IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: SCRATCH,
                    off: 0,
                    size,
                });
                lift_signed_extend(&mut ops, rt, size, true);
            } else if opc == 0b11 && size_bits <= 1 {
                // LDRSB/H Wt (32-bit sign-extended)
                ops.push(IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: SCRATCH,
                    off: 0,
                    size,
                });
                lift_signed_extend(&mut ops, rt, size, false);
            } else {
                return trap(R_LS_UNSUPPORTED);
            }
            return ops;
        }
    }

    trap(R_LS_UNSUPPORTED)
}

/// Branches: unconditional immediate B, BL (link + branch), RET (indirect),
/// plus CBZ/CBNZ (64-bit, Wave 4).
fn lift_branch(insn: &Instruction) -> Vec<IrOp> {
    let word = insn.word;
    // B: 000101 imm26 — target = addr + sign_extend(imm26 << 2).
    if (word >> 26) & 0x3F == 0x05 {
        let imm26 = word & 0x3FFF_FFFF;
        let offset = (((imm26 as i32) << 6) >> 6) as i64 * 4;
        let target = (insn.addr as i64).wrapping_add(offset) as u64;
        return vec![IrOp::Branch { target }];
    }
    // BL: 100101 imm26 — X30 = addr + 4 (the link), then branch to target.
    // No new IrOp needed: the link is a plain Mov to register 30 and the
    // transfer is the existing static Branch.
    if (word >> 26) & 0x3F == 0x25 {
        let imm26 = word & 0x3FFF_FFFF;
        let offset = (((imm26 as i32) << 6) >> 6) as i64 * 4;
        let target = (insn.addr as i64).wrapping_add(offset) as u64;
        return vec![
            IrOp::Mov {
                dst: 30,
                imm: insn.addr.wrapping_add(4),
            },
            IrOp::Branch { target },
        ];
    }
    // RET: 1101011 0 010 11111 000000 Rn 00000 — indirect branch to regs[Rn].
    // The mask clears only the Rn field, so any RET <Xn> matches.
    if word & 0xFFFF_FC1F == 0xD65F_0000 {
        let rn = ((word >> 5) & 0x1F) as u8;
        return vec![IrOp::BranchDyn { reg: rn }];
    }
    // BR: 1101011 0 000 11111 000000 Rn 00000 — indirect branch to regs[Rn],
    // no link. Same lowering as RET.
    if word & 0xFFFF_FC1F == 0xD61F_0000 {
        let rn = ((word >> 5) & 0x1F) as u8;
        return vec![IrOp::BranchDyn { reg: rn }];
    }
    // BLR: 1101011 0 001 11111 000000 Rn 00000 — X30 = addr + 4 (the link),
    // then indirect branch to regs[Rn]. The link is written first; when
    // rn == 30 the old X30 is preserved via SCRATCH first (MOV (register)
    // is ADD with XZR, the architectural alias — the target must be the
    // pre-link value).
    if word & 0xFFFF_FC1F == 0xD63F_0000 {
        let rn = ((word >> 5) & 0x1F) as u8;
        let link = IrOp::Mov {
            dst: 30,
            imm: insn.addr.wrapping_add(4),
        };
        if rn == 30 {
            return vec![
                IrOp::Add {
                    dst: SCRATCH,
                    a: 30,
                    b: 31,
                },
                link,
                IrOp::BranchDyn { reg: SCRATCH },
            ];
        }
        return vec![link, IrOp::BranchDyn { reg: rn }];
    }
    // CBZ/CBNZ (64-bit): sf 011010 op imm19 Rt (bits 31:24 = 0xB4/0xB5).
    let top8 = (word >> 24) & 0xFF;
    if top8 == 0xB4 || top8 == 0xB5 {
        let imm19 = (word >> 5) & 0x7FFFF;
        let offset = (((imm19 as i32) << 13) >> 13) as i64 * 4;
        let target = (insn.addr as i64).wrapping_add(offset) as u64;
        let rt = (word & 0x1F) as u8;
        return vec![IrOp::CondBranch {
            reg: rt,
            target,
            when_zero: top8 == 0xB4,
        }];
    }
    // CBZ/CBNZ (32-bit): sf 001010 op imm19 Rt (bits 31:24 = 0x34/0x35).
    // Tests only the LOW 32 bits of Rt. Not directly expressible in
    // CondBranch (which tests the full 64-bit slot), so zero-extend
    // into SCRATCH first: (Rt << 32) >> 32 logical = Rt & 0xFFFFFFFF.
    // (GB-26: was a deliberate trap until the kernel hit 32-bit CBZ at
    // step 1249078, pc 0xffffff800839b390, word 0x35000056.)
    if top8 == 0x34 || top8 == 0x35 {
        let imm19 = (word >> 5) & 0x7FFFF;
        let offset = (((imm19 as i32) << 13) >> 13) as i64 * 4;
        let target = (insn.addr as i64).wrapping_add(offset) as u64;
        let rt = (word & 0x1F) as u8;
        return vec![
            IrOp::OrrShift {
                dst: SCRATCH,
                a: 31,
                b: rt,
                shift: 0,
                amount: 32,
            },
            IrOp::OrrShift {
                dst: SCRATCH,
                a: 31,
                b: SCRATCH,
                shift: 1,
                amount: 32,
            },
            IrOp::CondBranch {
                reg: SCRATCH,
                target,
                when_zero: top8 == 0x34,
            },
        ];
    }
    trap(R_BR_UNSUPPORTED)
}

fn lift_fp(word: u32) -> Option<Vec<IrOp>> {
    // 1. Conversion between floating-point and integer
    if (word >> 24) & 0x7F == 0b0011110 && ((word >> 21) & 1) == 1 {
        let type_bits = (word >> 22) & 0x3;
        let scale = (word >> 10) & 0x3F;
        if type_bits <= 1 && scale == 0 {
            let rmode = (word >> 19) & 0x3;
            let opcode = (word >> 16) & 0x7;
            let sf = (word >> 31) & 1;
            match rmode {
                0b00 => match opcode {
                    0b010 => return Some(trap(R_FP_SCVTF)),
                    0b011 => return Some(trap(R_FP_UCVTF)),
                    0b110 | 0b111 => {
                        if sf == type_bits {
                            return Some(trap(R_FP_FMOV));
                        }
                    }
                    _ => return Some(trap(R_FP_UNSUPPORTED)),
                },
                0b11 => match opcode {
                    0b000 => return Some(trap(R_FP_FCVTZS)),
                    0b001 => return Some(trap(R_FP_FCVTZU)),
                    _ => return Some(trap(R_FP_UNSUPPORTED)),
                },
                _ => return Some(trap(R_FP_UNSUPPORTED)),
            }
        }
    }

    // 2. Floating-point immediate: FMOV
    if (word >> 24) == 0x1E
        && ((word >> 21) & 1) == 1
        && ((word >> 22) & 0x3) <= 1
        && ((word >> 10) & 0x7) == 0b100
        && ((word >> 5) & 0x1F) == 0
    {
        return Some(trap(R_FP_FMOV));
    }

    // 3. Floating-point compare & conditional compare: FCMP / FCCMP
    if (word >> 24) == 0x1E && ((word >> 21) & 1) == 1 && ((word >> 22) & 0x3) <= 1 {
        if ((word >> 10) & 0x3F) == 0b001000 && (word & 0x7) == 0 {
            let is_zero = ((word >> 3) & 1) == 1;
            let rm = (word >> 16) & 0x1F;
            if !is_zero || rm == 0 {
                return Some(trap(R_FP_FCMP));
            }
        }
        if ((word >> 10) & 0x3) == 0b01 && ((word >> 4) & 1) == 0 {
            return Some(trap(R_FP_FCMP));
        }
    }

    // 4. Floating-point conditional select: FCSEL
    if (word >> 24) == 0x1E
        && ((word >> 21) & 1) == 1
        && ((word >> 22) & 0x3) <= 1
        && ((word >> 10) & 0x3) == 0b11
    {
        return Some(trap(R_FP_UNSUPPORTED));
    }

    // 5. Floating-point data-processing (1 source): FMOV, FRINTA, etc.
    if (word >> 24) == 0x1E
        && ((word >> 21) & 1) == 1
        && ((word >> 22) & 0x3) <= 1
        && ((word >> 10) & 0x1F) == 0b10000
    {
        let opcode = (word >> 15) & 0x3F;
        return match opcode {
            0b000000 => Some(trap(R_FP_FMOV)),
            0b001100 => Some(trap(R_FP_FRINTA)),
            _ => Some(trap(R_FP_UNSUPPORTED)),
        };
    }

    // 6. Floating-point data-processing (2 sources): FADD, FSUB, FMUL, FDIV, etc.
    if (word >> 24) == 0x1E
        && ((word >> 21) & 1) == 1
        && ((word >> 22) & 0x3) <= 1
        && ((word >> 10) & 0x3) == 0b10
    {
        let opcode = (word >> 12) & 0xF;
        return match opcode {
            0b0000 => Some(trap(R_FP_FMUL)),
            0b0001 => Some(trap(R_FP_FDIV)),
            0b0010 => Some(trap(R_FP_FADD)),
            0b0011 => Some(trap(R_FP_FSUB)),
            _ => Some(trap(R_FP_UNSUPPORTED)),
        };
    }

    // 7. Floating-point data-processing (3 sources): FMADD, FMSUB, etc.
    if (word >> 24) == 0x1F && ((word >> 22) & 0x3) <= 1 {
        return Some(trap(R_FP_UNSUPPORTED));
    }

    None
}

#[cfg(test)]

mod tests {
    use super::*;

    fn insn(addr: u64, word: u32, kind: InsnKind) -> Instruction {
        Instruction { addr, word, kind }
    }

    // ---------- goldens: exact op sequences ----------

    #[test]
    fn golden_movz_64_lsl16() {
        // MOVZ X0, #0x1234, LSL #16
        let ops = lift(&insn(0x4000, 0xD2A2_4680, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Mov {
                dst: 0,
                imm: 0x1234_0000
            }]
        );
    }

    #[test]
    fn golden_movz_32_zero_extends() {
        // MOVZ W1, #0xAB  -> X1 = 0xAB exactly (upper 32 zeroed by definition)
        let ops = lift(&insn(0x4000, 0x5280_1561, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Mov { dst: 1, imm: 0xAB }]);
    }

    #[test]
    fn golden_add_imm() {
        // ADD X0, X1, #0x10
        let ops = lift(&insn(0x4000, 0x9100_4020, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![
                IrOp::Mov {
                    dst: SCRATCH,
                    imm: 0x10
                },
                IrOp::Add {
                    dst: 0,
                    a: 1,
                    b: SCRATCH
                },
            ]
        );
    }

    #[test]
    fn golden_add_imm_shifted12() {
        // ADD X2, X3, #1, LSL #12  -> imm = 0x1000
        let ops = lift(&insn(0x4000, 0x9140_0462, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![
                IrOp::Mov {
                    dst: SCRATCH,
                    imm: 0x1000
                },
                IrOp::Add {
                    dst: 2,
                    a: 3,
                    b: SCRATCH
                },
            ]
        );
    }

    #[test]
    fn golden_sub_imm() {
        // SUB X3, X2, #1 (real kernel step 17 word: 0xd1000443)
        let ops = lift(&insn(0x4000_4d74, 0xd100_0443, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![
                IrOp::Mov {
                    dst: SCRATCH,
                    imm: 1
                },
                IrOp::Sub {
                    dst: 3,
                    a: 2,
                    b: SCRATCH
                },
            ]
        );
    }

    #[test]
    fn golden_sub_imm_shifted12() {
        // SUB X0, X1, #1, LSL #12 -> imm = 0x1000
        let ops = lift(&insn(0x4000, 0xd140_0420, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![
                IrOp::Mov {
                    dst: SCRATCH,
                    imm: 0x1000
                },
                IrOp::Sub {
                    dst: 0,
                    a: 1,
                    b: SCRATCH
                },
            ]
        );
    }

    #[test]
    fn golden_sub_reg() {
        // SUB X1, X1, X0, LSL #0 (real kernel step 57 word: 0xcb000021)
        let ops = lift(&insn(0x413c_004c, 0xcb00_0021, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Sub { dst: 1, a: 1, b: 0 },]);
    }

    #[test]
    fn trap_sub_reg_32bit() {
        // SUB W1, W1, W0: 32-bit sub lifts to Sub + AndShift(is_32) mask.
        // 0x4b00_0021 = SUB W1, W1, W0 (sf=0, Rm=0, Rn=1, Rd=1).
        let ops = lift(&insn(0x4000, 0x4b00_0021, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![
                IrOp::Sub {
                    dst: SCRATCH,
                    a: 1,
                    b: 0,
                },
                IrOp::AndShift {
                    dst: 1,
                    a: SCRATCH,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                    invert: false,
                    is_32: true,
                },
            ]
        );
    }

    #[test]
    fn golden_clz() {
        // CLZ X5, X5 (real kernel step 5201 word: 0xdac010a5)
        let ops = lift(&insn(0x413c_0088, 0xdac0_10a5, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Clz { dst: 5, src: 5 },]);
    }

    #[test]
    fn trap_clz_32bit() {
        // CLZ W5, W5: 32-bit width not expressible
        let ops = lift(&insn(0x4000, 0x5ac0_10a5, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_CLZ32 }]);
    }

    #[test]
    fn trap_clz_sibling_stays_unsupported() {
        // RBIT X5, X5 (1-source sibling, out of scope) -> honest trap
        let ops = lift(&insn(0x4000, 0xdac0_00a5, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_DP_UNSUPPORTED
            }]
        );
    }

    #[test]
    fn golden_madd() {
        // MADD X10, X10, X13, XZR (real kernel step 5217 word: 0x9b0d7d4a)
        let ops = lift(&insn(0x413c_0100, 0x9b0d_7d4a, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Madd {
                dst: 10,
                n: 10,
                m: 13,
                a: 31
            },]
        );
    }

    #[test]
    fn trap_madd_32bit() {
        // MADD W10, W10, W13, WZR: 32-bit width not expressible
        let ops = lift(&insn(0x4000, 0x1b0d_7d4a, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_MADD32 }]);
    }

    #[test]
    fn trap_msub_stays_unsupported() {
        // MSUB X10, X10, X13, XZR (o0 = 1): out of scope -> honest trap
        let ops = lift(&insn(0x4000, 0x9b0d_fd4a, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_MSUB }]);
    }

    #[test]
    fn trap_madd_long_stays_unsupported() {
        // SMADDL X10, W10, W13, XZR (op54 = 01): out of scope -> honest trap
        let ops = lift(&insn(0x4000, 0xdb0d_7d4a, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_MADD_LONG
            }]
        );
    }

    #[test]
    fn trap_sub_reg_shifted() {
        // SUB X1, X1, X0, LSL #1: shifter not expressible
        let ops = lift(&insn(0x4000, 0xcb00_0421, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_SUB_SHIFT
            }]
        );
    }

    #[test]
    fn golden_add_reg() {
        // ADD X0, X1, X2
        let ops = lift(&insn(0x4000, 0x8B02_0020, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Add { dst: 0, a: 1, b: 2 }]);
    }

    #[test]
    fn golden_ldr_literal_64() {
        // LDR X0, [PC, #8] at 0x4000 -> static address 0x4008
        let ops = lift(&insn(0x4000, 0x5800_0040, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::Load {
                dst: 0,
                addr: 0x4008,
                size: 8
            }]
        );
    }

    #[test]
    fn golden_ldr_literal_32_negative() {
        // LDR W3, [PC, #-4] at 0x4000 -> static address 0x3FFC, 4-byte width
        let ops = lift(&insn(0x4000, 0x18FF_FFE3, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::Load {
                dst: 3,
                addr: 0x3FFC,
                size: 4
            }]
        );
    }

    #[test]
    fn golden_str_register_base_lifts_dynamic() {
        // STR X5, [X6, #0x20]
        let ops = lift(&insn(0x4000, 0xF900_10C5, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::StoreDyn {
                src: 5,
                base: 6,
                off: 0x20,
                size: 8
            }]
        );
    }

    #[test]
    fn golden_ldr_register_base_lifts_dynamic() {
        // LDR X0, [X1, #8]
        let ops = lift(&insn(0x4000, 0xF940_0420, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::LoadDyn {
                dst: 0,
                base: 1,
                off: 8,
                size: 8
            }]
        );
    }

    // ---- GB-1: STP/LDP pairs, indexed LDR/STR, LDRSW ----

    #[test]
    fn gb1_stp_x21_x1_x0_kernel_step6() {
        // Kernel instruction 6: stp x21, x1, [x0]
        let ops = lift(&insn(0x413C_002C, 0xA900_0415, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![
                IrOp::StoreDyn {
                    src: 21,
                    base: 0,
                    off: 0,
                    size: 8
                },
                IrOp::StoreDyn {
                    src: 1,
                    base: 0,
                    off: 8,
                    size: 8
                },
            ]
        );
    }

    #[test]
    fn gb1_stp_x2_x3_offset_kernel_step7() {
        // Kernel instruction 7: stp x2, x3, [x0, #16]
        let ops = lift(&insn(0x413C_0030, 0xA901_0C02, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![
                IrOp::StoreDyn {
                    src: 2,
                    base: 0,
                    off: 16,
                    size: 8
                },
                IrOp::StoreDyn {
                    src: 3,
                    base: 0,
                    off: 24,
                    size: 8
                },
            ]
        );
    }

    #[test]
    fn gb1_stp_pre_and_post_indexed() {
        // Pre-indexed: stp x2, x3, [x0, #16]!
        let ops_pre = lift(&insn(0x4000, 0xA981_0C02, InsnKind::LoadStore));
        assert_eq!(
            ops_pre,
            vec![
                IrOp::StoreDyn {
                    src: 2,
                    base: 0,
                    off: 16,
                    size: 8
                },
                IrOp::StoreDyn {
                    src: 3,
                    base: 0,
                    off: 24,
                    size: 8
                },
                IrOp::Mov {
                    dst: SCRATCH,
                    imm: 16
                },
                IrOp::Add {
                    dst: 0,
                    a: 0,
                    b: SCRATCH
                },
            ]
        );

        // Post-indexed: stp x2, x3, [x0], #16
        let ops_post = lift(&insn(0x4000, 0xA881_0C02, InsnKind::LoadStore));
        assert_eq!(
            ops_post,
            vec![
                IrOp::StoreDyn {
                    src: 2,
                    base: 0,
                    off: 0,
                    size: 8
                },
                IrOp::StoreDyn {
                    src: 3,
                    base: 0,
                    off: 8,
                    size: 8
                },
                IrOp::Mov {
                    dst: SCRATCH,
                    imm: 16
                },
                IrOp::Add {
                    dst: 0,
                    a: 0,
                    b: SCRATCH
                },
            ]
        );
    }

    #[test]
    fn gb1_ldp_64_and_32() {
        // 64-bit: ldp x2, x3, [x0, #16]
        let ops_64 = lift(&insn(0x4000, 0xA941_0C02, InsnKind::LoadStore));
        assert_eq!(
            ops_64,
            vec![
                IrOp::LoadDyn {
                    dst: 2,
                    base: 0,
                    off: 16,
                    size: 8
                },
                IrOp::LoadDyn {
                    dst: 3,
                    base: 0,
                    off: 24,
                    size: 8
                },
            ]
        );

        // 32-bit: ldp w2, w3, [x0, #8]
        let ops_32 = lift(&insn(0x4000, 0x2941_0C02, InsnKind::LoadStore));
        assert_eq!(
            ops_32,
            vec![
                IrOp::LoadDyn {
                    dst: 2,
                    base: 0,
                    off: 8,
                    size: 4
                },
                IrOp::LoadDyn {
                    dst: 3,
                    base: 0,
                    off: 12,
                    size: 4
                },
            ]
        );
    }

    #[test]
    fn gb1_ldpsw_sign_extends() {
        // ldpsw x2, x3, [x0, #8]
        let ops = lift(&insn(0x4000, 0x6941_0C02, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![
                IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: 0,
                    off: 8,
                    size: 4
                },
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 32
                },
                IrOp::OrrShift {
                    dst: 2,
                    a: 31,
                    b: SCRATCH,
                    shift: 2,
                    amount: 32
                },
                IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: 0,
                    off: 12,
                    size: 4
                },
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 32
                },
                IrOp::OrrShift {
                    dst: 3,
                    a: 31,
                    b: SCRATCH,
                    shift: 2,
                    amount: 32
                },
            ]
        );
    }

    #[test]
    fn gb1_ldr_pre_and_post_indexed() {
        // Pre-indexed: ldr x0, [x1, #8]!
        let ops_pre = lift(&insn(0x4000, 0xF840_8C20, InsnKind::LoadStore));
        assert_eq!(
            ops_pre,
            vec![
                IrOp::LoadDyn {
                    dst: 0,
                    base: 1,
                    off: 8,
                    size: 8
                },
                IrOp::Mov {
                    dst: SCRATCH,
                    imm: 8
                },
                IrOp::Add {
                    dst: 1,
                    a: 1,
                    b: SCRATCH
                },
            ]
        );

        // Post-indexed: ldr x0, [x1], #8
        let ops_post = lift(&insn(0x4000, 0xF840_8420, InsnKind::LoadStore));
        assert_eq!(
            ops_post,
            vec![
                IrOp::LoadDyn {
                    dst: 0,
                    base: 1,
                    off: 0,
                    size: 8
                },
                IrOp::Mov {
                    dst: SCRATCH,
                    imm: 8
                },
                IrOp::Add {
                    dst: 1,
                    a: 1,
                    b: SCRATCH
                },
            ]
        );
    }

    #[test]
    fn gb26_stur_ldur_unscaled_no_writeback() {
        // GB-26: STUR X8, [X29, #-8] (word 0xF81F83A8, measured kernel
        // halt at step 1248988). Unscaled: address = base + simm9, and
        // -- unlike pre/post-index -- there is NO writeback.
        let ops = lift(&insn(0x4000, 0xF81F_83A8, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::StoreDyn {
                src: 8,
                base: 29,
                off: (-8i64) as u64,
                size: 8
            }]
        );

        // LDUR X0, [X1] (word 0xF8400020): zero offset, still no writeback.
        let ops = lift(&insn(0x4000, 0xF840_0020, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::LoadDyn {
                dst: 0,
                base: 1,
                off: 0,
                size: 8
            }]
        );
    }

    #[test]
    fn gb1_ldrsw_variants() {
        // ldrsw x0, [x1, #4] (unsigned offset)
        let ops_off = lift(&insn(0x4000, 0xB980_0420, InsnKind::LoadStore));
        assert_eq!(
            ops_off,
            vec![
                IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: 1,
                    off: 4,
                    size: 4
                },
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 32
                },
                IrOp::OrrShift {
                    dst: 0,
                    a: 31,
                    b: SCRATCH,
                    shift: 2,
                    amount: 32
                },
            ]
        );

        // ldrsw x0, [pc, #8] (literal at 0x4000 -> addr 0x4008)
        let ops_lit = lift(&insn(0x4000, 0x9800_0040, InsnKind::LoadStore));
        assert_eq!(
            ops_lit,
            vec![
                IrOp::Load {
                    dst: SCRATCH,
                    addr: 0x4008,
                    size: 4
                },
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 32
                },
                IrOp::OrrShift {
                    dst: 0,
                    a: 31,
                    b: SCRATCH,
                    shift: 2,
                    amount: 32
                },
            ]
        );
    }

    #[test]
    fn gb1_ldr_reg_offset() {
        // ldr x0, [x1, x2, lsl #3]
        let ops = lift(&insn(0x4000, 0xF862_7820, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: 2,
                    shift: 0,
                    amount: 3
                },
                IrOp::Add {
                    dst: SCRATCH,
                    a: 1,
                    b: SCRATCH
                },
                IrOp::LoadDyn {
                    dst: 0,
                    base: SCRATCH,
                    off: 0,
                    size: 8
                },
            ]
        );
    }

    #[test]
    fn golden_b_forward() {
        // B +0x100 at 0x4000 -> explicit exit to 0x4100
        let ops = lift(&insn(0x4000, 0x1400_0040, InsnKind::Branch));
        assert_eq!(ops, vec![IrOp::Branch { target: 0x4100 }]);
    }

    #[test]
    fn golden_b_backward_sign_extended() {
        // B -4 at 0x4000 -> 0x3FFC (imm26 sign extension exercised)
        let ops = lift(&insn(0x4000, 0x17FF_FFFF, InsnKind::Branch));
        assert_eq!(ops, vec![IrOp::Branch { target: 0x3FFC }]);
    }

    // ---------- traps: unsupported kinds/forms are data, never silent ----------

    #[test]
    fn trap_system_kind_names_kind() {
        // 0xD538_1040 was MRS CPACR_EL1 before GB-9 made it persistent;
        // use S3_0_C1_C0_3 (no such register) as the unrecognized example.
        let ops = lift(&insn(0x4000, 0xD538_1060, InsnKind::System));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_SYSTEM }]);
        assert!(matches!(&ops[0], IrOp::Trap { reason } if reason.contains("System")));
    }

    #[test]
    fn trap_unknown_kind_names_kind() {
        let ops = lift(&insn(0x4000, 0xFFFF_FFFF, InsnKind::Unknown));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_UNKNOWN }]);
    }

    #[test]
    fn trap_svc_kind_is_honest_unimplemented() {
        let ops = lift(&insn(0x4000, 0xD400_0001, InsnKind::Svc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_SVC_UNIMPL
            }]
        );
        assert!(matches!(&ops[0], IrOp::Trap { reason } if reason.contains("Svc")));
    }

    #[test]
    fn trap_adds_flag_update() {
        // ADDS X0, X1, #1: flag semantics not expressible -> trap
        let ops = lift(&insn(0x4000, 0xB100_0420, InsnKind::DataProc));
        assert_eq!(ops.len(), 1);
        assert!(matches!(&ops[0], IrOp::Trap { .. }));
    }


    #[test]
    fn csel_cond_trap() {
        // CSEL X0, X1, X2, EQ (0x9A820020): NZCV not in IrOp -> honest trap.
        let ops = lift(&insn(0x4000, 0x9A82_0020, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_CSEL_COND
            }]
        );
        // CSEL X0, X1, X2, AL (0x9A82E020): AL is legal always-true per ARM ARM,
        // decoder accepts it, but lifter still traps (no NZCV in IrOp).
        let ops = lift(&insn(0x4000, 0x9A82_E020, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_CSEL_COND
            }]
        );
    }

    #[test]
    fn trap_add_imm_32bit_width() {
        // ADD W0, W1, #1: 32-bit add lifts to Mov+Add+AndShift(is_32) mask.
        // 0x1100_0420 = ADD W0, W1, #1 (sf=0, sh=0, imm12=1, Rn=1, Rd=0).
        let ops = lift(&insn(0x4000, 0x1100_0420, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![
                IrOp::Mov {
                    dst: SCRATCH,
                    imm: 1
                },
                IrOp::Add {
                    dst: SCRATCH,
                    a: 1,
                    b: SCRATCH,
                },
                IrOp::AndShift {
                    dst: 0,
                    a: SCRATCH,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                    invert: false,
                    is_32: true,
                },
            ]
        );
    }

    #[test]
    fn add_extended_uxtb() {
        // ADD X13, X8, UXTB X14, #2 (0x8b2e090d): unsigned extend byte + LSL #2 + ADD.
        // Lifts to: Mov(0xFF) + AndShift(mask) + OrrShift(LSL #2) + Add.
        let ops = lift(&insn(0x4000, 0x8B2E_090D, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![
                IrOp::Mov {
                    dst: SCRATCH,
                    imm: 0xFF,
                },
                IrOp::AndShift {
                    dst: SCRATCH,
                    a: 14,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                    invert: false,
                    is_32: false,
                },
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 2,
                },
                IrOp::Add {
                    dst: 13,
                    a: 8,
                    b: SCRATCH,
                },
            ]
        );
    }

    #[test]
    fn trap_sub_imm_32bit_width() {
        // SUB W0, W1, #1: 32-bit sub lifts to Mov+Sub+AndShift(is_32) mask.
        // 0x5100_0420 = SUB W0, W1, #1 (sf=0, sh=0, imm12=1, Rn=1, Rd=0).
        let ops = lift(&insn(0x4000, 0x5100_0420, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![
                IrOp::Mov {
                    dst: SCRATCH,
                    imm: 1
                },
                IrOp::Sub {
                    dst: SCRATCH,
                    a: 1,
                    b: SCRATCH,
                },
                IrOp::AndShift {
                    dst: 0,
                    a: SCRATCH,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                    invert: false,
                    is_32: true,
                },
            ]
        );
    }

    #[test]
    fn trap_add_reg_shifted() {
        // ADD X0, X1, X2, LSL #1: shifter not expressible
        let ops = lift(&insn(0x4000, 0x8B02_0420, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_ADD_SHIFT
            }]
        );
    }

    #[test]
    fn trap_movz_bad_hw() {
        // sf = 0, hw = 2: unallocated encoding
        let ops = lift(&insn(0x4000, 0x52C0_0000, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_MOVZ_HW }]);
    }

    #[test]
    fn ldrh_16bit_supported() {
        // LDRH W0, [X1, #2]: 16-bit halfword load lifts to LoadDyn with size: 2.
        let ops = lift(&insn(0x4000, 0x7940_0420, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::LoadDyn {
                dst: 0,
                base: 1,
                off: 2,
                size: 2
            }]
        );

        // STRH W0, [X1, #2]: 16-bit halfword store lifts to StoreDyn with size: 2.
        let ops_strh = lift(&insn(0x4000, 0x7900_0420, InsnKind::LoadStore));
        assert_eq!(
            ops_strh,
            vec![IrOp::StoreDyn {
                src: 0,
                base: 1,
                off: 2,
                size: 2
            }]
        );
    }

    #[test]
    fn prfm_prefetch_is_true_noop() {
        // PRFM PLDL1KEEP, [X0, #8] (unsigned imm)
        let ops_imm = lift(&insn(0x4000, 0xF980_0400, InsnKind::LoadStore));
        assert_eq!(ops_imm, Vec::<IrOp>::new());

        // PRFM PLDL1KEEP, [PC, #16] (literal)
        let ops_lit = lift(&insn(0x4000, 0xD800_0080, InsnKind::LoadStore));
        assert_eq!(ops_lit, Vec::<IrOp>::new());

        // PRFUM PLDL1KEEP, [X0, #-8] (unscaled)
        let ops_unscaled = lift(&insn(0x4000, 0xF89F_8000, InsnKind::LoadStore));
        assert_eq!(ops_unscaled, Vec::<IrOp>::new());

        // PRFM PLDL1KEEP, [X0, X1, LSL #3] (register offset)
        let ops_reg = lift(&insn(0x4000, 0xF8A1_7800, InsnKind::LoadStore));
        assert_eq!(ops_reg, Vec::<IrOp>::new());
    }

    #[test]
    fn ldrsb_ldrsh_signed_loads() {
        // LDRSB X0, [X1, #2]: signed byte -> 64-bit (LSL 56, ASR 56)
        let ops = lift(&insn(0x4000, 0x3980_0820, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![
                IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: 1,
                    off: 2,
                    size: 1
                },
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 56
                },
                IrOp::OrrShift {
                    dst: 0,
                    a: 31,
                    b: SCRATCH,
                    shift: 2,
                    amount: 56
                }
            ]
        );

        // LDRSB W0, [X1, #2]: signed byte -> 32-bit (LSL 56, ASR 56, AndShift is_32: true)
        let ops = lift(&insn(0x4000, 0x39C0_0820, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![
                IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: 1,
                    off: 2,
                    size: 1
                },
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 56
                },
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 2,
                    amount: 56
                },
                IrOp::AndShift {
                    dst: 0,
                    a: SCRATCH,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                    invert: false,
                    is_32: true
                }
            ]
        );

        // LDRSH X0, [X1, #4]: signed halfword -> 64-bit (LSL 48, ASR 48)
        let ops = lift(&insn(0x4000, 0x7980_0820, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![
                IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: 1,
                    off: 4,
                    size: 2
                },
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 48
                },
                IrOp::OrrShift {
                    dst: 0,
                    a: 31,
                    b: SCRATCH,
                    shift: 2,
                    amount: 48
                }
            ]
        );
    }

    #[test]
    fn trap_unrecognized_branch_word() {
        // A Branch-kind word matching no known encoding still traps with the
        // exact reason (U1 would have called this Illegal; the lifter is a
        // pure function of &Instruction and must not guess).
        let ops = lift(&insn(0x4000, 0x0000_0000, InsnKind::Branch));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_BR_UNSUPPORTED
            }]
        );
    }

    // ---------- Wave 5 (BL/RET scope) ----------

    #[test]
    fn wave5_bl_forward_sets_link_then_branches() {
        // BL +0x100 at 0x4000: X30 = 0x4004, then Branch to 0x4100.
        let ops = lift(&insn(0x4000, 0x9400_0040, InsnKind::Branch));
        assert_eq!(
            ops,
            vec![
                IrOp::Mov {
                    dst: 30,
                    imm: 0x4004
                },
                IrOp::Branch { target: 0x4100 },
            ]
        );
    }

    #[test]
    fn wave5_bl_backward_sign_extended() {
        // BL -4 at 0x4000 -> target 0x3FFC, link still addr+4.
        let ops = lift(&insn(0x4000, 0x97FF_FFFF, InsnKind::Branch));
        assert_eq!(
            ops,
            vec![
                IrOp::Mov {
                    dst: 30,
                    imm: 0x4004
                },
                IrOp::Branch { target: 0x3FFC },
            ]
        );
    }

    #[test]
    fn wave5_bl_link_is_addr_plus_4_not_target() {
        // The link register holds the RETURN address, not the target:
        // BL at 0x4000_004C (the real guest's first BL) links 0x4000_0050.
        let ops = lift(&insn(0x4000_004C, 0x97FF_FF00, InsnKind::Branch));
        assert_eq!(
            ops[0],
            IrOp::Mov {
                dst: 30,
                imm: 0x4000_0050
            }
        );
        assert!(matches!(ops[1], IrOp::Branch { .. }));
    }

    #[test]
    fn wave5_ret_x30_is_indirect_branch() {
        // RET X30 (0xD65F03C0): control goes to regs[30].
        let ops = lift(&insn(0x4000, 0xD65F_03C0, InsnKind::Branch));
        assert_eq!(ops, vec![IrOp::BranchDyn { reg: 30 }]);
    }

    #[test]
    fn wave5_ret_other_register() {
        // RET X9 (0xD65F0120): indirect through regs[9], not just X30.
        let ops = lift(&insn(0x4000, 0xD65F_0120, InsnKind::Branch));
        assert_eq!(ops, vec![IrOp::BranchDyn { reg: 9 }]);
    }

    #[test]
    fn gb21_br_x3() {
        // BR X3 (0xD61F0060): indirect branch, no link — same as RET.
        let ops = lift(&insn(0x4000, 0xD61F_0060, InsnKind::Branch));
        assert_eq!(ops, vec![IrOp::BranchDyn { reg: 3 }]);
    }

    #[test]
    fn gb21_blr_x3() {
        // BLR X3 (0xD63F0060): X30 = addr + 4, then indirect branch to
        // regs[3]. (The real GB-21 halt word was BLR X8, 0xD63F0100 —
        // same shape, Rn = 8.)
        let ops = lift(&insn(0x4000, 0xD63F_0060, InsnKind::Branch));
        assert_eq!(
            ops,
            vec![
                IrOp::Mov {
                    dst: 30,
                    imm: 0x4004
                },
                IrOp::BranchDyn { reg: 3 },
            ]
        );
    }

    #[test]
    fn gb21_blr_x30_preserves_old_link() {
        // BLR X30 (0xD63F03C0): the target must be the PRE-link X30, so
        // the old value is spilled to SCRATCH before X30 is overwritten.
        let ops = lift(&insn(0x4000, 0xD63F_03C0, InsnKind::Branch));
        assert_eq!(
            ops,
            vec![
                IrOp::Add {
                    dst: SCRATCH,
                    a: 30,
                    b: 31
                },
                IrOp::Mov {
                    dst: 30,
                    imm: 0x4004
                },
                IrOp::BranchDyn { reg: SCRATCH },
            ]
        );
    }

    #[test]
    fn gb21_blr_xzr_targets_zero() {
        // BLR XZR (0xD63F03E0): regs[31] reads as 0; the backend
        // fetch-faults honestly on target 0. The link is still written.
        let ops = lift(&insn(0x4000, 0xD63F_03E0, InsnKind::Branch));
        assert_eq!(
            ops,
            vec![
                IrOp::Mov {
                    dst: 30,
                    imm: 0x4004
                },
                IrOp::BranchDyn { reg: 31 },
            ]
        );
    }

    // ---------- structural properties ----------

    #[test]
    fn every_kind_maps_to_nonempty_ops() {
        let kinds = [
            InsnKind::DataProc,
            InsnKind::LoadStore,
            InsnKind::Branch,
            InsnKind::System,
            InsnKind::Unknown,
        ];
        for kind in kinds {
            let ops = lift(&insn(0x4000, 0xFFFF_FFFF, kind));
            assert!(!ops.is_empty(), "kind {kind:?} produced no ops");
        }
    }

    #[test]
    fn determinism_lift_twice_equal() {
        let cases = [
            insn(0x4000, 0xD2A2_4680, InsnKind::DataProc),
            insn(0x4000, 0x9100_4020, InsnKind::DataProc),
            insn(0x4000, 0x5800_0040, InsnKind::LoadStore),
            insn(0x4000, 0x1400_0040, InsnKind::Branch),
            insn(0x4000, 0xFFFF_FFFF, InsnKind::Unknown),
        ];
        for c in cases {
            assert_eq!(lift(&c), lift(&c));
        }
    }

    #[test]
    fn scratch_does_not_collide_with_arch_regs() {
        const { assert!(SCRATCH > 31) } // scratch must sit outside X0-X30/XZR-SP
    }

    // ---------- Wave 4 (U2-G1): ADR/ADRP, byte lifts, CBZ/CBNZ, ORR-shift, WFI ----------

    #[test]
    fn wave4_adrp_entry_word_lifts_to_static_mov() {
        // Real guest entry: ADRP X10, #0x1000 at 0x4000_0000 -> X10 = 0x4000_1000.
        let ops = lift(&insn(0x4000_0000, 0xB000_000A, InsnKind::PcRel));
        assert_eq!(
            ops,
            vec![IrOp::Mov {
                dst: 10,
                imm: 0x4000_1000
            }]
        );
    }

    #[test]
    fn wave4_adrp_negative_page() {
        // ADRP X0, page-1 at 0x4000_1000 -> X0 = 0x4000_0000.
        let ops = lift(&insn(0x4000_1000, 0xF0FF_FFE0, InsnKind::PcRel));
        assert_eq!(
            ops,
            vec![IrOp::Mov {
                dst: 0,
                imm: 0x4000_0000
            }]
        );
    }

    #[test]
    fn wave4_adr_plain() {
        // ADR X0, #0 at 0x4000 -> X0 = 0x4000.
        let ops = lift(&insn(0x4000, 0x1000_0000, InsnKind::PcRel));
        assert_eq!(
            ops,
            vec![IrOp::Mov {
                dst: 0,
                imm: 0x4000
            }]
        );
    }

    #[test]
    fn wave4_ldrb_register_relative() {
        // LDRB W2, [X0] (guest read_char): dynamic byte load.
        let ops = lift(&insn(0x4000, 0x3940_0002, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::LoadDyn {
                dst: 2,
                base: 0,
                off: 0,
                size: 1
            }]
        );
    }

    #[test]
    fn wave4_ldrb_with_offset() {
        // LDRB W1, [X10, #0x100] (guest word-buffer peek).
        let word = 0x3940_0000 | (0x100 << 10) | (10 << 5) | 1;
        let ops = lift(&insn(0x4000, word, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::LoadDyn {
                dst: 1,
                base: 10,
                off: 0x100,
                size: 1
            }]
        );
    }

    #[test]
    fn wave4_strb_wzr_source() {
        // STRB WZR, [X11] (guest NUL terminator): stores zero, NOT dropped.
        let word = 0x3900_0000 | (11 << 5) | 31;
        let ops = lift(&insn(0x4000, word, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::StoreDyn {
                src: 31,
                base: 11,
                off: 0,
                size: 1
            }]
        );
    }

    #[test]
    fn wave4_strb_sp_base_traps() {
        // STRB W0, [SP, #8]: SP-relative is not expressible in Wave 4.
        let word = 0x3900_0000 | (8 << 10) | (31 << 5);
        let ops = lift(&insn(0x4000, word, InsnKind::LoadStore));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_LS_SP }]);
    }

    #[test]
    fn wave4_cbz_forward() {
        // CBZ X2, +0x10 at 0x4000 -> taken to 0x4010 when X2 == 0.
        let word = 0xB400_0000 | (4 << 5) | 2;
        let ops = lift(&insn(0x4000, word, InsnKind::Branch));
        assert_eq!(
            ops,
            vec![IrOp::CondBranch {
                reg: 2,
                target: 0x4010,
                when_zero: true
            }]
        );
    }

    #[test]
    fn wave4_cbnz_backward() {
        // CBNZ X0, -8 at 0x4008 -> taken to 0x4000 when X0 != 0.
        let word = 0xB500_0000 | (0x7FFFE << 5);
        let ops = lift(&insn(0x4008, word, InsnKind::Branch));
        assert_eq!(
            ops,
            vec![IrOp::CondBranch {
                reg: 0,
                target: 0x4000,
                when_zero: false
            }]
        );
    }

    #[test]
    fn wave4_cbz_32bit_zero_extends() {
        // CBZ W0, #0 (0x34000000): tests only the low 32 bits, so the
        // lifter zero-extends W0 into SCRATCH, then CondBranches on it.
        let ops = lift(&insn(0x4000, 0x3400_0000, InsnKind::Branch));
        assert_eq!(
            ops,
            vec![
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: 0,
                    shift: 0,
                    amount: 32
                },
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 1,
                    amount: 32
                },
                IrOp::CondBranch {
                    reg: SCRATCH,
                    target: 0x4000,
                    when_zero: true
                },
            ]
        );
    }

    #[test]
    fn gb26_cbnz_32bit_kernel_word() {
        // CBNZ W22, #+8 (word 0x35000056, measured kernel halt at step
        // 1249078, pc 0xffffff800839b390). imm19 = 2 -> offset +8.
        let ops = lift(&insn(0xffffff800839b390, 0x3500_0056, InsnKind::Branch));
        assert_eq!(
            ops,
            vec![
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: 22,
                    shift: 0,
                    amount: 32
                },
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 1,
                    amount: 32
                },
                IrOp::CondBranch {
                    reg: SCRATCH,
                    target: 0xffffff800839b398,
                    when_zero: false,
                },
            ]
        );
    }

    #[test]
    fn f2_cbz_cbnz_xzr_lifting() {
        // CBZ XZR, +16: 0xB400_009F at 0x4000
        let ops_cbz64 = lift(&insn(0x4000, 0xB400_009F, InsnKind::Branch));
        assert_eq!(
            ops_cbz64,
            vec![IrOp::CondBranch {
                reg: 31,
                target: 0x4010,
                when_zero: true,
            }]
        );

        // CBNZ XZR, +16: 0xB500_009F at 0x4000
        let ops_cbnz64 = lift(&insn(0x4000, 0xB500_009F, InsnKind::Branch));
        assert_eq!(
            ops_cbnz64,
            vec![IrOp::CondBranch {
                reg: 31,
                target: 0x4010,
                when_zero: false,
            }]
        );

        // CBZ WZR, +16: 0x3400_009F at 0x4000
        let ops_cbz32 = lift(&insn(0x4000, 0x3400_009F, InsnKind::Branch));
        assert_eq!(
            ops_cbz32,
            vec![
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: 31,
                    shift: 0,
                    amount: 32,
                },
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 1,
                    amount: 32,
                },
                IrOp::CondBranch {
                    reg: SCRATCH,
                    target: 0x4010,
                    when_zero: true,
                },
            ]
        );
    }

    #[test]
    fn wave4_orr_shift_copy() {
        // ORR X11, XZR, X10 (guest register copy).
        let word = 0xAA00_0000 | (10 << 16) | (31 << 5) | 11;
        let ops = lift(&insn(0x4000, word, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::OrrShift {
                dst: 11,
                a: 31,
                b: 10,
                shift: 0,
                amount: 0
            }]
        );
    }

    #[test]
    fn wave4_orr_shift_lsl56() {
        // ORR X2, XZR, X1, LSL #56 (guest eq_byte trick).
        let word = 0xAA00_0000 | (1 << 16) | (56 << 10) | (31 << 5) | 2;
        let ops = lift(&insn(0x4000, word, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::OrrShift {
                dst: 2,
                a: 31,
                b: 1,
                shift: 0,
                amount: 56
            }]
        );
    }

    #[test]
    fn wave4_orr_32bit_traps() {
        // ORR W0, WZR, W1: 32-bit zeroing is not expressible.
        let word = 0x2A00_0000 | (1 << 16) | (31 << 5);
        let ops = lift(&insn(0x4000, word, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_ORR32 }]);
    }

    #[test]
    fn wave4_wfi_lifts() {
        let ops = lift(&insn(0x4000, 0xD503_207F, InsnKind::System));
        assert_eq!(ops, vec![IrOp::Wfi]);
    }

    #[test]
    fn gb3_nop_and_barriers_lift_to_empty() {
        // NOP (HINT #0)
        assert_eq!(lift(&insn(0x4000, 0xD503_201F, InsnKind::System)), vec![]);
        // DMB sy
        assert_eq!(lift(&insn(0x4000, 0xD503_3FBF, InsnKind::System)), vec![]);
        // DMB ish
        assert_eq!(lift(&insn(0x4000, 0xD503_3BBF, InsnKind::System)), vec![]);
        // DSB sy
        assert_eq!(lift(&insn(0x4000, 0xD503_3F9F, InsnKind::System)), vec![]);
        // DSB ishst
        assert_eq!(lift(&insn(0x4000, 0xD503_3A9F, InsnKind::System)), vec![]);
        // ISB
        assert_eq!(lift(&insn(0x4000, 0xD503_3FDF, InsnKind::System)), vec![]);
    }

    #[test]
    fn gb3_mrs_sysregs_lift_to_mov() {
        // CurrentEL -> 4 (EL1)
        assert_eq!(
            lift(&insn(0x4000, 0xD538_4240, InsnKind::System)),
            vec![IrOp::Mov { dst: 0, imm: 4 }]
        );
        // CTR_EL0 -> 0x8444_c004
        assert_eq!(
            lift(&insn(0x4000, 0xD53B_0023, InsnKind::System)),
            vec![IrOp::Mov {
                dst: 3,
                imm: 0x8444_C004
            }]
        );
        // DAIF -> persistent (GB-sysreg2)
        assert_eq!(
            lift(&insn(0x4000, 0xD53B_4220, InsnKind::System)),
            vec![IrOp::ReadSys {
                dst: 0,
                reg: SysReg::Daif
            }]
        );
        // SP_EL0 -> persistent (GB-26: kernel reads thread_info base via
        // `mrs x21, sp_el0`; word 0xD5384115, halt at step 1249003)
        assert_eq!(
            lift(&insn(0x4000, 0xD538_4115, InsnKind::System)),
            vec![IrOp::ReadSys {
                dst: 21,
                reg: SysReg::SpEl0
            }]
        );
        // NZCV -> 0 (stays in GB-2 live pstate flag path)
        assert_eq!(
            lift(&insn(0x4000, 0xD53B_4200, InsnKind::System)),
            vec![IrOp::Mov { dst: 0, imm: 0 }]
        );
        // TPIDR_EL1 -> persistent (GB-sysreg2)
        assert_eq!(
            lift(&insn(0x4000, 0xD538_D080, InsnKind::System)),
            vec![IrOp::ReadSys {
                dst: 0,
                reg: SysReg::TpidrEl1
            }]
        );
    }

    #[test]
    fn gb26_mrs_dczid_el0_lifts_to_mov_prohibited() {
        // DCZID_EL0 = MRS X3, S3_3_C0_C0_7 (word 0xD53B00E3, measured
        // kernel halt at step 1210749, pc 0xffffff8008209d80).
        // DZP (bit 4) is set: our DC ZVA is an honest NOP, so the
        // kernel must take its store-based zeroing fallback.
        assert_eq!(
            lift(&insn(0x4000, 0xD53B_00E3, InsnKind::System)),
            vec![IrOp::Mov { dst: 3, imm: 0x10 }]
        );
    }

    #[test]
    fn gb26_mrs_mpidr_el1_lifts_to_mov_uniprocessor() {
        // MPIDR_EL1 = MRS X9, S3_0_C0_C0_5 (word 0xD53800A9, measured
        // kernel halt at step 1248961, pc 0xffffff80095d2620).
        // U (bit 30) = 1: uniprocessor; Aff0 = 0: this is vCPU 0.
        assert_eq!(
            lift(&insn(0x4000, 0xD538_00A9, InsnKind::System)),
            vec![IrOp::Mov {
                dst: 9,
                imm: 0x4000_0000
            }]
        );
    }

    #[test]
    fn gb26_mrs_midr_el1_lifts_to_mov_zero() {
        // MIDR_EL1 = MRS X2, S3_0_C0_C0_0 (word 0xD5380002, measured
        // kernel halt at step 1248969, pc 0xffffff80095d2640).
        // 0 = no implementer/part advertised; the kernel's errata
        // framework matches nothing and takes the generic path.
        assert_eq!(
            lift(&insn(0x4000, 0xD538_0002, InsnKind::System)),
            vec![IrOp::Mov { dst: 2, imm: 0 }]
        );
    }

    // Build an MRS (mrs=true) or MSR (mrs=false) system-register word.
    fn sys_word(op0: u32, op1: u32, crn: u32, crm: u32, op2: u32, rt: u32, mrs: bool) -> u32 {
        (0x354 << 22)
            | ((mrs as u32) << 21)
            | (op0 << 19)
            | (op1 << 16)
            | (crn << 12)
            | (crm << 8)
            | (op2 << 5)
            | rt
    }

    #[test]
    fn gbsysreg2_mrs_msr_lift_to_persistent_ops() {
        // Every persistent MRS lifts to ReadSys with the right selector.
        let mrs_cases = [
            ((3, 3, 4, 2, 1), SysReg::Daif),
            ((3, 0, 13, 0, 4), SysReg::TpidrEl1),
            ((3, 0, 1, 0, 0), SysReg::SctlrEl1),
            ((3, 4, 1, 0, 0), SysReg::SctlrEl2),
            ((3, 4, 14, 1, 0), SysReg::CnthctlEl2),
            ((3, 0, 1, 0, 2), SysReg::CpacrEl1),
            ((2, 0, 0, 2, 2), SysReg::MdscrEl1),
            ((3, 0, 10, 2, 0), SysReg::MairEl1),
            ((3, 0, 2, 0, 2), SysReg::TcrEl1),
            ((3, 0, 2, 0, 0), SysReg::Ttbr0El1),
            ((3, 0, 2, 0, 1), SysReg::Ttbr1El1),
            ((3, 0, 12, 0, 0), SysReg::VbarEl1),
            ((3, 4, 1, 1, 0), SysReg::HcrEl2),
            ((3, 0, 7, 4, 0), SysReg::ParEl1),
            ((3, 0, 1, 0, 1), SysReg::ActlrEl1),
            ((3, 3, 9, 12, 1), SysReg::PmcntensetEl0),
            ((3, 3, 9, 12, 5), SysReg::PmselrEl0),
            // P3 slice A (GICv3): SRE/CTLR/IGRPEN1/PMR are persistent.
            // (IAR1/EOIR1/DIR are WO/RO-accept, not persistent reads.)
            ((3, 0, 12, 12, 5), SysReg::IccSreEl1),
            ((3, 0, 12, 12, 4), SysReg::IccCtlrEl1),
            ((3, 0, 12, 12, 7), SysReg::IccIgrpen1El1),
            ((3, 0, 4, 6, 0), SysReg::IccPmrEl1),
            // P3 slice C (PMU remainder).
            ((3, 3, 9, 12, 2), SysReg::PmcntEnClrEl0),
            ((3, 3, 9, 12, 3), SysReg::PmovsclrEl0),
            ((3, 3, 9, 13, 2), SysReg::PmxevcntrEl0),
            ((3, 3, 9, 13, 1), SysReg::PmxevtyperEl0),
            ((3, 3, 9, 14, 0), SysReg::PmuserenrEl0),
            // P3 slice D (timers): CNTKCTL, TPIDR_EL2, TVAL aliases.
            ((3, 0, 14, 1, 0), SysReg::CntkctlEl1),
            ((3, 4, 13, 0, 2), SysReg::TpidrEl2),
            ((3, 3, 14, 2, 0), SysReg::CntpTvalEl0),
            ((3, 3, 14, 3, 0), SysReg::CntvTvalEl0),
            // P3 slice E (FP/SIMD): FPCR/FPSR.
            ((3, 3, 4, 4, 0), SysReg::Fpcr),
            ((3, 3, 4, 4, 1), SysReg::Fpsr),
        ];
        for ((op0, op1, crn, crm, op2), reg) in mrs_cases {
            let word = sys_word(op0, op1, crn, crm, op2, 7, true);
            assert_eq!(
                lift(&insn(0x4000, word, InsnKind::System)),
                vec![IrOp::ReadSys { dst: 7, reg }],
                "mrs {op0} {op1} {crn} {crm} {op2}"
            );
        }
        // Every persistent MSR lifts to WriteSys with src = Rt.
        let msr_cases = [
            ((3, 3, 4, 2, 1), SysReg::Daif),
            ((3, 0, 13, 0, 4), SysReg::TpidrEl1),
            ((3, 0, 1, 0, 0), SysReg::SctlrEl1),
            ((3, 4, 1, 0, 0), SysReg::SctlrEl2),
            ((3, 4, 1, 1, 0), SysReg::HcrEl2),
            ((3, 4, 14, 1, 0), SysReg::CnthctlEl2),
            ((3, 4, 14, 0, 3), SysReg::CntvoffEl2),
            ((3, 0, 12, 0, 0), SysReg::VbarEl1),
            ((3, 0, 4, 1, 0), SysReg::SpEl0),
            ((3, 0, 1, 0, 2), SysReg::CpacrEl1),
            ((2, 0, 0, 2, 2), SysReg::MdscrEl1),
            ((3, 0, 10, 2, 0), SysReg::MairEl1),
            ((3, 0, 2, 0, 2), SysReg::TcrEl1),
            ((3, 0, 2, 0, 0), SysReg::Ttbr0El1),
            ((3, 0, 2, 0, 1), SysReg::Ttbr1El1),
            ((2, 0, 1, 0, 4), SysReg::OslarEl1),
            ((3, 3, 9, 12, 1), SysReg::PmcntensetEl0),
            ((3, 3, 9, 12, 5), SysReg::PmselrEl0),
            // P3 slice A (GICv3): EOIR1/DIR WO-accept; SRE/CTLR/IGRPEN1/PMR.
            ((3, 0, 12, 12, 1), SysReg::IccEoir1El1),
            ((3, 0, 12, 11, 1), SysReg::IccDirEl1),
            ((3, 0, 12, 12, 5), SysReg::IccSreEl1),
            ((3, 0, 12, 12, 4), SysReg::IccCtlrEl1),
            ((3, 0, 12, 12, 7), SysReg::IccIgrpen1El1),
            ((3, 0, 4, 6, 0), SysReg::IccPmrEl1),
            // P3 slice C (PMU remainder).
            ((3, 3, 9, 12, 2), SysReg::PmcntEnClrEl0),
            ((3, 3, 9, 12, 3), SysReg::PmovsclrEl0),
            ((3, 3, 9, 13, 1), SysReg::PmxevtyperEl0),
            ((3, 3, 9, 13, 2), SysReg::PmxevcntrEl0),
            ((3, 3, 9, 14, 0), SysReg::PmuserenrEl0),
            // P3 slice D (timers).
            ((3, 0, 14, 1, 0), SysReg::CntkctlEl1),
            ((3, 4, 13, 0, 2), SysReg::TpidrEl2),
            ((3, 3, 14, 2, 0), SysReg::CntpTvalEl0),
            ((3, 3, 14, 3, 0), SysReg::CntvTvalEl0),
            // P3 slice E (FP/SIMD).
            ((3, 3, 4, 4, 0), SysReg::Fpcr),
            ((3, 3, 4, 4, 1), SysReg::Fpsr),
            // P3 slice F (debug): OSDLR_EL1 write-only.
            ((2, 0, 1, 3, 4), SysReg::OsdlrEl1),
        ];
        for ((op0, op1, crn, crm, op2), reg) in msr_cases {
            let word = sys_word(op0, op1, crn, crm, op2, 5, false);
            assert_eq!(
                lift(&insn(0x4000, word, InsnKind::System)),
                vec![IrOp::WriteSys { src: 5, reg }],
                "msr {op0} {op1} {crn} {crm} {op2}"
            );
        }
        // Carve-outs keep GB-3 behavior: NZCV/MSR no-op, ID constants
        // still Mov. (DAIFSet/DAIFClr graduated to real RMW ops in GB-11.)
        let w = sys_word(3, 3, 4, 2, 0, 5, false);
        assert_eq!(lift(&insn(0x4000, w, InsnKind::System)), vec![]);
        let w = sys_word(3, 3, 4, 2, 0, 0, true);
        assert_eq!(
            lift(&insn(0x4000, w, InsnKind::System)),
            vec![IrOp::Mov { dst: 0, imm: 0 }]
        );
        let w = sys_word(3, 0, 0, 4, 0, 2, true);
        assert_eq!(
            lift(&insn(0x4000, w, InsnKind::System)),
            vec![IrOp::Mov { dst: 2, imm: 0x11 }]
        );
    }

    #[test]
    fn p3_mrs_constants_lift_to_mov() {
        // P3 slice A: MRS ICC_IAR1_EL1 (S3_0_C12_C12_0, Rt=X22 in the
        // kernel's entry.S) -> 0x3ff = spurious, "no pending interrupt".
        let w = sys_word(3, 0, 12, 12, 0, 22, true);
        assert_eq!(
            lift(&insn(0x4000, w, InsnKind::System)),
            vec![IrOp::Mov { dst: 22, imm: 0x3ff }]
        );
        // P3 slice B: ID family completion -> all 0 (conservative).
        let id_cases = [
            (3, 0, 0, 1, 0), // ID_PFR0_EL1
            (3, 0, 0, 1, 1), // ID_PFR1_EL1
            (3, 0, 0, 1, 2), // ID_DFR0_EL1
            (3, 0, 0, 1, 4), // ID_MMFR0_EL1
            (3, 0, 0, 1, 5), // ID_MMFR1_EL1
            (3, 0, 0, 1, 6), // ID_MMFR2_EL1
            (3, 0, 0, 1, 7), // ID_MMFR3_EL1
            (3, 0, 0, 2, 0), // ID_ISAR0_EL1
            (3, 0, 0, 2, 1), // ID_ISAR1_EL1
            (3, 0, 0, 2, 2), // ID_ISAR2_EL1
            (3, 0, 0, 2, 3), // ID_ISAR3_EL1
            (3, 0, 0, 2, 4), // ID_ISAR4_EL1
            (3, 0, 0, 2, 5), // ID_ISAR5_EL1
            (3, 0, 0, 3, 0), // MVFR0_EL1
            (3, 0, 0, 3, 1), // MVFR1_EL1
            (3, 0, 0, 3, 2), // MVFR2_EL1
            (3, 0, 0, 0, 6), // REVIDR_EL1
            (3, 0, 0, 5, 1), // ID_AA64DFR1_EL1
            (3, 0, 0, 4, 4), // ID_AA64ZFR0_EL1
        ];
        for (op0, op1, crn, crm, op2) in id_cases {
            let w = sys_word(op0, op1, crn, crm, op2, 8, true);
            assert_eq!(
                lift(&insn(0x4000, w, InsnKind::System)),
                vec![IrOp::Mov { dst: 8, imm: 0 }],
                "mrs {op0} {op1} {crn} {crm} {op2}"
            );
        }
        // P3 slice C: PMCEID0/1 and PMBIDR -> 0 (no events / no SPE).
        for (op0, op1, crn, crm, op2) in [
            (3, 3, 9, 12, 6), // PMCEID0_EL0
            (3, 3, 9, 12, 7), // PMCEID1_EL0
            (3, 0, 9, 10, 7), // PMBIDR_EL1
        ] {
            let w = sys_word(op0, op1, crn, crm, op2, 4, true);
            assert_eq!(
                lift(&insn(0x4000, w, InsnKind::System)),
                vec![IrOp::Mov { dst: 4, imm: 0 }],
                "mrs {op0} {op1} {crn} {crm} {op2}"
            );
        }
        // P3 slice F: OSLSR_EL1 -> 0 (OSLK=0).
        let w = sys_word(2, 0, 1, 1, 4, 10, true);
        assert_eq!(
            lift(&insn(0x4000, w, InsnKind::System)),
            vec![IrOp::Mov { dst: 10, imm: 0 }]
        );
    }

    #[test]
    fn p3_write_only_sysregs_trap_on_mrs() {
        // ICC_EOIR1_EL1, ICC_DIR_EL1, OSDLR_EL1 are write-only: an MRS
        // encoding is architecturally UNDEFINED and must trap, not
        // silently return a value.
        for (op0, op1, crn, crm, op2) in [
            (3, 0, 12, 12, 1), // ICC_EOIR1_EL1
            (3, 0, 12, 11, 1), // ICC_DIR_EL1
            (2, 0, 1, 3, 4),   // OSDLR_EL1
        ] {
            let w = sys_word(op0, op1, crn, crm, op2, 8, true);
            let ops = lift(&insn(0x4000, w, InsnKind::System));
            assert!(
                matches!(ops.as_slice(), [IrOp::Trap { .. }]),
                "mrs {op0} {op1} {crn} {crm} {op2} must trap, got {ops:?}"
            );
        }
    }

    #[test]
    fn gb9_cpacr_el1_msr_mrs_lift_to_persistent_ops() {
        // Measured halt word: MSR CPACR_EL1, X0 (step 7457, pc 0x40c0364c).
        // (GB-8 mislabeled this TCR_EL1; S3_0_C1_C0_2 is CPACR_EL1.)
        assert_eq!(
            lift(&insn(0x4000, 0xD518_1040, InsnKind::System)),
            vec![IrOp::WriteSys {
                src: 0,
                reg: SysReg::CpacrEl1
            }]
        );
        // MRS CPACR_EL1, X5: same system encoding with bit 21 set.
        assert_eq!(
            lift(&insn(0x4000, 0xD538_1045, InsnKind::System)),
            vec![IrOp::ReadSys {
                dst: 5,
                reg: SysReg::CpacrEl1
            }]
        );
    }

    #[test]
    fn gb10_mdscr_el1_msr_mrs_lift_to_persistent_ops() {
        // Measured halt word: MSR MDSCR_EL1, X0 (step 7459, pc 0x40c03654).
        assert_eq!(
            lift(&insn(0x4000, 0xD510_0240, InsnKind::System)),
            vec![IrOp::WriteSys {
                src: 0,
                reg: SysReg::MdscrEl1
            }]
        );
        // MRS MDSCR_EL1, X5: same system encoding with bit 21 set.
        assert_eq!(
            lift(&insn(0x4000, 0xD530_0245, InsnKind::System)),
            vec![IrOp::ReadSys {
                dst: 5,
                reg: SysReg::MdscrEl1
            }]
        );
    }

    #[test]
    fn gb13_mair_el1_msr_mrs_lift_to_persistent_ops() {
        // Measured halt word: MSR MAIR_EL1, X5 (step 7467, pc 0x40c03678).
        // Field extraction: (op0,op1,crn,crm,op2) = (3,0,10,2,0) = S3_0_C10_C2_0
        // = MAIR_EL1; Rt = bits[4:0] = X5.
        assert_eq!(
            lift(&insn(0x4000, 0xD518_A205, InsnKind::System)),
            vec![IrOp::WriteSys {
                src: 5,
                reg: SysReg::MairEl1
            }]
        );
        // MRS MAIR_EL1, X5: same system encoding with bit 21 set.
        assert_eq!(
            lift(&insn(0x4000, 0xD538_A205, InsnKind::System)),
            vec![IrOp::ReadSys {
                dst: 5,
                reg: SysReg::MairEl1
            }]
        );
    }

    #[test]
    fn gb17_tcr_el1_msr_mrs_lift_to_persistent_ops() {
        // Measured halt word: MSR TCR_EL1, X10 (step 7483, pc 0x40c036bc).
        // Field extraction: (op0,op1,crn,crm,op2) = (3,0,2,0,2) = S3_0_C2_C0_2
        // = TCR_EL1 (verified against the ARM ARM; Rt = bits[4:0] = X10).
        // This is the real TCR_EL1 that GB-8 mislabeled (word 0xd5181040 was
        // S3_0_C1_C0_2 = CPACR_EL1).
        assert_eq!(
            lift(&insn(0x4000, 0xD518_204A, InsnKind::System)),
            vec![IrOp::WriteSys {
                src: 10,
                reg: SysReg::TcrEl1
            }]
        );
        // MRS TCR_EL1, X10: same system encoding with bit 21 set.
        assert_eq!(
            lift(&insn(0x4000, 0xD538_204A, InsnKind::System)),
            vec![IrOp::ReadSys {
                dst: 10,
                reg: SysReg::TcrEl1
            }]
        );
    }

    #[test]
    fn gb18_ttbr0_el1_msr_mrs_lift_to_persistent_ops() {
        // Measured halt word: MSR TTBR0_EL1, X3 (step 7503, pc 0x40c03274).
        // Field extraction: (op0,op1,crn,crm,op2) = (3,0,2,0,0) = S3_0_C2_C0_0
        // = TTBR0_EL1 (verified against the ARM ARM; Rt = bits[4:0] = X3).
        assert_eq!(
            lift(&insn(0x4000, 0xD518_2003, InsnKind::System)),
            vec![IrOp::WriteSys {
                src: 3,
                reg: SysReg::Ttbr0El1
            }]
        );
        // MRS TTBR0_EL1, X3: same system encoding with bit 21 set.
        assert_eq!(
            lift(&insn(0x4000, 0xD538_2003, InsnKind::System)),
            vec![IrOp::ReadSys {
                dst: 3,
                reg: SysReg::Ttbr0El1
            }]
        );
    }

    #[test]
    fn gb19_ttbr1_el1_msr_mrs_lift_to_persistent_ops() {
        // Measured halt word: MSR TTBR1_EL1, X4 (step 7504, pc 0x40c03278).
        // Field extraction: (op0,op1,crn,crm,op2) = (3,0,2,0,1) = S3_0_C2_C0_1
        // = TTBR1_EL1 (verified against the ARM ARM; Rt = bits[4:0] = X4).
        assert_eq!(
            lift(&insn(0x4000, 0xD518_2024, InsnKind::System)),
            vec![IrOp::WriteSys {
                src: 4,
                reg: SysReg::Ttbr1El1
            }]
        );
        // MRS TTBR1_EL1, X4: same system encoding with bit 21 set.
        assert_eq!(
            lift(&insn(0x4000, 0xD538_2024, InsnKind::System)),
            vec![IrOp::ReadSys {
                dst: 4,
                reg: SysReg::Ttbr1El1
            }]
        );
    }

    #[test]
    fn gb14_id_aa64mmfr0_el1_mrs_lifts_to_mov() {
        // Measured halt word: MRS X5, ID_AA64MMFR0_EL1 (step 7474,
        // pc 0x40c03694). Field extraction: (op0,op1,crn,crm,op2) =
        // (3,0,0,7,0) = S3_0_C0_C7_0 = ID_AA64MMFR0_EL1; Rt = X5.
        // Value 0 = no memory-model features advertised (matches the
        // sibling ID_AA64MMFR1_EL1 / ID_AA64DFR0_EL1 reads).
        assert_eq!(
            lift(&insn(0x40c0_3694, 0xD538_0705, InsnKind::System)),
            vec![IrOp::Mov { dst: 5, imm: 0 }]
        );
    }

    #[test]
    fn gb16_id_aa64mmfr1_el1_mrs_lifts_to_mov() {
        // Measured halt word: MRS X9, ID_AA64MMFR1_EL1 (step 7480,
        // pc 0x40c036ac). Field extraction: (op0,op1,crn,crm,op2) =
        // (3,0,0,7,1) = S3_0_C0_C7_1 = ID_AA64MMFR1_EL1; Rt = X9.
        // objdump-confirmed on the box. Value 0 = no memory-model
        // features advertised (matches the sibling ID_AA64MMFR0_EL1 /
        // ID_AA64DFR0_EL1 reads).
        assert_eq!(
            lift(&insn(0x40c0_36ac, 0xD538_0729, InsnKind::System)),
            vec![IrOp::Mov { dst: 9, imm: 0 }]
        );
    }

    #[test]
    fn gb11_daifclr_daifset_lift_to_rmw() {
        // Measured halt word: MSR DAIFClr, #0x8 (step 7461, pc 0x40c0365c).
        // imm=0x8 -> bit3 -> PSTATE.D (bit 9): clears the debug mask.
        let w = sys_word(0, 3, 4, 8, 7, 31, false);
        assert_eq!(w, 0xD503_48FF);
        assert_eq!(
            lift(&insn(0x4000, w, InsnKind::System)),
            vec![IrOp::DaifRmw { set: 0, clr: 0x200 }]
        );
        // MSR DAIFSet, #0x2 (Linux local_irq_disable shape): imm bit1 -> I.
        let w = sys_word(0, 3, 4, 2, 6, 31, false);
        assert_eq!(
            lift(&insn(0x4000, w, InsnKind::System)),
            vec![IrOp::DaifRmw { set: 0x80, clr: 0 }]
        );
        // MSR DAIFClr, #0x2 (Linux local_irq_enable shape): clears I.
        let w = sys_word(0, 3, 4, 2, 7, 31, false);
        assert_eq!(
            lift(&insn(0x4000, w, InsnKind::System)),
            vec![IrOp::DaifRmw { set: 0, clr: 0x80 }]
        );
        // MSR DAIFSet, #0xF: all four masks (D/A/I/F -> bits 9/8/7/6).
        let w = sys_word(0, 3, 4, 0xF, 6, 31, false);
        assert_eq!(
            lift(&insn(0x4000, w, InsnKind::System)),
            vec![IrOp::DaifRmw { set: 0x3C0, clr: 0 }]
        );
    }

    #[test]
    fn gb3_msr_and_cache_ops_lift_to_empty() {
        // MSR DAIF, X0 -> persistent store (GB-sysreg2)
        assert_eq!(
            lift(&insn(0x4000, 0xD51B_4220, InsnKind::System)),
            vec![IrOp::WriteSys {
                src: 0,
                reg: SysReg::Daif
            }]
        );
        // MSR NZCV, X0 -> still accepted no-op (GB-2 live flag path)
        assert_eq!(lift(&insn(0x4000, 0xD51B_4200, InsnKind::System)), vec![]);
        // MSR TPIDR_EL1, X0 -> persistent store (GB-sysreg2)
        assert_eq!(
            lift(&insn(0x4000, 0xD518_D080, InsnKind::System)),
            vec![IrOp::WriteSys {
                src: 0,
                reg: SysReg::TpidrEl1
            }]
        );
        // MSR SPSel, #1
        assert_eq!(lift(&insn(0x4000, 0xD500_41BF, InsnKind::System)), vec![]);
        // DC CIVAC, X1
        assert_eq!(lift(&insn(0x4000, 0xD50B_7E21, InsnKind::System)), vec![]);
    }

    #[test]
    fn gb3_unsupported_system_traps() {
        // Non-system instruction passed as System
        let ops = lift(&insn(0x4000, 0x0000_0000, InsnKind::System));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_SYSTEM }]);
    }

    // ---- Track GB-4 unit tests ----

    #[test]
    fn gb4_movk_lifts() {
        // MOVK X0, #0x1234, LSL #16 (sf=1, hw=1, imm=0x1234, rd=0)
        let word = 0xF2A2_4680;
        let ops = lift(&insn(0x4000, word, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Movk {
                dst: 0,
                imm: 0x1234,
                hw: 1,
                is_32: false,
            }]
        );

        // MOVK W1, #0x5678, LSL #0 (sf=0, hw=0, imm=0x5678, rd=1)
        let word32 = 0x728A_CF01;
        let ops32 = lift(&insn(0x4000, word32, InsnKind::DataProc));
        assert_eq!(
            ops32,
            vec![IrOp::Movk {
                dst: 1,
                imm: 0x5678,
                hw: 0,
                is_32: true,
            }]
        );
    }

    #[test]
    fn gb4_and_imm_lifts() {
        // AND X23, X23, #0x1fffff (kernel stext+0c: 0x924052f7)
        let word = 0x9240_52F7;
        let ops = lift(&insn(0x4000, word, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![
                IrOp::Mov {
                    dst: SCRATCH,
                    imm: 0x0000_0000_001F_FFFF,
                },
                IrOp::AndShift {
                    dst: 23,
                    a: 23,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                    invert: false,
                    is_32: false,
                },
            ]
        );
    }

    #[test]
    fn gb4_bitfield_ubfm_lifts() {
        // UBFX X3, X3, #16, #4 (kernel 0x40004d68: 0xd3504c63)
        // sf=1, opc=2, immr=16, imms=19, rn=3, rd=3
        let word = 0xD350_4C63;
        let ops = lift(&insn(0x4000, word, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Bitfield {
                dst: 3,
                src: 3,
                opc: 2,
                immr: 16,
                imms: 19,
                is_32: false,
            }]
        );
    }

    #[test]
    fn gb4_bitfield_sbfm_lifts() {
        // SBFX X0, X1, #0, #4 (sf=1, opc=0, immr=0, imms=3, rn=1, rd=0)
        let word = 0x9340_0C20;
        let ops = lift(&insn(0x4000, word, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Bitfield {
                dst: 0,
                src: 1,
                opc: 0,
                immr: 0,
                imms: 3,
                is_32: false,
            }]
        );
    }

    #[test]
    fn gb4_logical_shifted_reg_bic_lifts() {
        // BIC X1, X1, X3 (kernel 0x40004d78: 0x8a230021)
        // sf=1, opc=0, n=1, shift=0, amount=0, rm=3, rn=1, rd=1
        let word = 0x8A23_0021;
        let ops = lift(&insn(0x4000, word, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::AndShift {
                dst: 1,
                a: 1,
                b: 3,
                shift: 0,
                amount: 0,
                invert: true,
                is_32: false,
            }]
        );
    }

    #[test]
    fn gb4_dp_2source_lslv_lifts() {
        // LSL X2, X2, X3 (kernel 0x40004d70: 0x9ac32042)
        // sf=1, rm=3, rn=2, rd=2, shift=0 (LSLV: 00=LSL, 01=LSR, 10=ASR, 11=ROR)
        let word = 0x9AC3_2042;
        let ops = lift(&insn(0x4000, word, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::ShiftVar {
                dst: 2,
                a: 2,
                b: 3,
                shift: 0,
                is_32: false,
            }]
        );
    }

    // ---------- Floating-point & SIMD tests (fam/fpsimd) ----------

    #[test]
    fn fp_fmov_imm_lifts_to_trap() {
        // FMOV S0, #1.0 (0x1E2E1000)
        let ops = lift(&insn(0x4000, 0x1E2E_1000, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FMOV }]);
        // FMOV D0, #1.0 (0x1E6E1000)
        let ops = lift(&insn(0x4000, 0x1E6E_1000, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FMOV }]);
    }

    #[test]
    fn fp_fmov_reg_lifts_to_trap() {
        // FMOV S0, S1 (0x1E204020)
        let ops = lift(&insn(0x4000, 0x1E20_4020, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FMOV }]);
        // FMOV D0, D1 (0x1E604020)
        let ops = lift(&insn(0x4000, 0x1E60_4020, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FMOV }]);
        // FMOV W0, S1 (0x1E260020)
        let ops = lift(&insn(0x4000, 0x1E26_0020, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FMOV }]);
        // FMOV X0, D1 (0x9E660020)
        let ops = lift(&insn(0x4000, 0x9E66_0020, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FMOV }]);
        // FMOV S0, W1 (0x1E270020)
        let ops = lift(&insn(0x4000, 0x1E27_0020, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FMOV }]);
        // FMOV D0, X1 (0x9E670020)
        let ops = lift(&insn(0x4000, 0x9E67_0020, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FMOV }]);
    }

    #[test]
    fn fp_fadd_fsub_lifts_to_trap() {
        // FADD S0, S1, S2 (0x1E222820)
        let ops = lift(&insn(0x4000, 0x1E22_2820, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FADD }]);
        // FADD D0, D1, D2 (0x1E622820)
        let ops = lift(&insn(0x4000, 0x1E62_2820, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FADD }]);
        // FSUB S0, S1, S2 (0x1E223820)
        let ops = lift(&insn(0x4000, 0x1E22_3820, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FSUB }]);
        // FSUB D0, D1, D2 (0x1E623820)
        let ops = lift(&insn(0x4000, 0x1E62_3820, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FSUB }]);
    }

    #[test]
    fn fp_fmul_fdiv_lifts_to_trap() {
        // FMUL S0, S1, S2 (0x1E220820)
        let ops = lift(&insn(0x4000, 0x1E22_0820, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FMUL }]);
        // FMUL D0, D1, D2 (0x1E620820)
        let ops = lift(&insn(0x4000, 0x1E62_0820, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FMUL }]);
        // FDIV S0, S1, S2 (0x1E221820)
        let ops = lift(&insn(0x4000, 0x1E22_1820, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FDIV }]);
        // FDIV D0, D1, D2 (0x1E621820)
        let ops = lift(&insn(0x4000, 0x1E62_1820, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FDIV }]);
    }

    #[test]
    fn fp_fcmp_fccmp_lifts_to_trap() {
        // FCMP S0, S1 (0x1E212000)
        let ops = lift(&insn(0x4000, 0x1E21_2000, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FCMP }]);
        // FCMP D0, D1 (0x1E612000)
        let ops = lift(&insn(0x4000, 0x1E61_2000, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FCMP }]);
        // FCMP S0, #0.0 (0x1E202008)
        let ops = lift(&insn(0x4000, 0x1E20_2008, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FCMP }]);
        // FCMP D0, #0.0 (0x1E602008)
        let ops = lift(&insn(0x4000, 0x1E60_2008, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FCMP }]);
        // FCCMP S0, S1, #0, EQ (0x1E210400)
        let ops = lift(&insn(0x4000, 0x1E21_0400, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_FCMP }]);
    }

    #[test]
    fn fp_scvtf_ucvtf_lifts_to_trap() {
        // SCVTF S0, W1 (0x1E220020)
        let ops = lift(&insn(0x4000, 0x1E22_0020, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_SCVTF }]);
        // SCVTF D0, X1 (0x9E620020)
        let ops = lift(&insn(0x4000, 0x9E62_0020, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_SCVTF }]);
        // UCVTF S0, W1 (0x1E230020)
        let ops = lift(&insn(0x4000, 0x1E23_0020, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_UCVTF }]);
        // UCVTF D0, X1 (0x9E630020)
        let ops = lift(&insn(0x4000, 0x9E63_0020, InsnKind::DataProc));
        assert_eq!(ops, vec![IrOp::Trap { reason: R_FP_UCVTF }]);
    }

    #[test]
    fn fp_fcvtzs_fcvtzu_lifts_to_trap() {
        // FCVTZS W0, S1 (0x1E380020)
        let ops = lift(&insn(0x4000, 0x1E38_0020, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_FP_FCVTZS
            }]
        );
        // FCVTZS X0, D1 (0x9E780020)
        let ops = lift(&insn(0x4000, 0x9E78_0020, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_FP_FCVTZS
            }]
        );
        // FCVTZU W0, S1 (0x1E390020)
        let ops = lift(&insn(0x4000, 0x1E39_0020, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_FP_FCVTZU
            }]
        );
        // FCVTZU X0, D1 (0x9E790020)
        let ops = lift(&insn(0x4000, 0x9E79_0020, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_FP_FCVTZU
            }]
        );
    }

    #[test]
    fn fp_frinta_lifts_to_trap() {
        // FRINTA S0, S1 (0x1E264020)
        let ops = lift(&insn(0x4000, 0x1E26_4020, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_FP_FRINTA
            }]
        );
        // FRINTA D0, D1 (0x1E664020)
        let ops = lift(&insn(0x4000, 0x1E66_4020, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_FP_FRINTA
            }]
        );
    }

    #[test]
    fn exceptions_lift_to_honest_traps() {
        assert_eq!(
            lift(&insn(0x4000, 0xD420_0000, InsnKind::System)),
            vec![IrOp::Trap { reason: R_BRK }]
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD440_0000, InsnKind::System)),
            vec![IrOp::Trap { reason: R_HLT }]
        );
        // HVC (P0, 2026-10-03): now lifts to IrOp::Hvc (PSCI stub),
        // not a trap. The host returns version or NOT_SUPPORTED in X0.
        assert_eq!(
            lift(&insn(0x4000, 0xD400_0002, InsnKind::System)),
            vec![IrOp::Hvc]
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD400_0003, InsnKind::System)),
            vec![IrOp::Trap { reason: R_SMC }]
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD69F_03E0, InsnKind::System)),
            vec![IrOp::Trap { reason: R_ERET }]
        );
    }

    #[test]
    fn pan_and_uao_lift_to_empty() {
        assert_eq!(lift(&insn(0x4000, 0xD500_419F, InsnKind::System)), vec![]); // MSR PAN, #1
        assert_eq!(lift(&insn(0x4000, 0xD500_409F, InsnKind::System)), vec![]); // MSR PAN, #0
        assert_eq!(lift(&insn(0x4000, 0xD500_417F, InsnKind::System)), vec![]); // MSR UAO, #1
        assert_eq!(lift(&insn(0x4000, 0xD500_407F, InsnKind::System)), vec![]); // MSR UAO, #0
    }

    #[test]
    fn high_frequency_sysregs_lift_correctly() {
        // P3 (2026-10-03): MRS X0, TPIDR_EL2 is a persistent read now
        // (P2's constant-0 arm was replaced so MSR/MRS round-trip).
        assert_eq!(
            lift(&insn(0x4000, 0xD53C_D040, InsnKind::System)),
            vec![IrOp::ReadSys {
                dst: 0,
                reg: SysReg::TpidrEl2
            }] // MRS X0, TPIDR_EL2
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD539_0020, InsnKind::System)),
            vec![IrOp::Mov {
                dst: 0,
                imm: 0x0920_0023
            }] // MRS X0, CLIDR_EL1
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD53A_0000, InsnKind::System)),
            vec![IrOp::Mov { dst: 0, imm: 0 }] // MRS X0, CSSELR_EL1
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD51A_0000, InsnKind::System)),
            vec![] // MSR CSSELR_EL1, X0
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD539_0000, InsnKind::System)),
            vec![IrOp::Mov {
                dst: 0,
                imm: 0x701F_E00A
            }] // MRS X0, CCSIDR_EL1
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD53B_E000, InsnKind::System)),
            vec![IrOp::Mov {
                dst: 0,
                imm: 0x03B9_ACA0
            }] // MRS X0, CNTFRQ_EL0
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD53B_E040, InsnKind::System)),
            vec![IrOp::ReadSys {
                dst: 0,
                reg: SysReg::CntvctEl0
            }] // MRS X0, CNTVCT_EL0 (fam/devices: live counter)
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD53B_E020, InsnKind::System)),
            vec![IrOp::ReadSys {
                dst: 0,
                reg: SysReg::CntpctEl0
            }] // MRS X0, CNTPCT_EL0 (fam/devices: live counter)
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD538_5200, InsnKind::System)),
            vec![IrOp::Mov { dst: 0, imm: 0 }] // MRS X0, ESR_EL1
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD538_6000, InsnKind::System)),
            vec![IrOp::Mov { dst: 0, imm: 0 }] // MRS X0, FAR_EL1
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD538_4020, InsnKind::System)),
            vec![IrOp::Mov { dst: 0, imm: 0 }] // MRS X0, ELR_EL1
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD538_4000, InsnKind::System)),
            vec![IrOp::Mov { dst: 0, imm: 0 }] // MRS X0, SPSR_EL1
        );
    }

    #[test]
    fn cache_and_tlb_maintenance_lift_to_empty() {
        assert_eq!(lift(&insn(0x4000, 0xD508_871F, InsnKind::System)), vec![]); // TLBI VMALLE1
        assert_eq!(lift(&insn(0x4000, 0xD508_831F, InsnKind::System)), vec![]); // TLBI VMALLE1IS
        assert_eq!(lift(&insn(0x4000, 0xD508_837F, InsnKind::System)), vec![]); // TLBI VAAE1IS
        assert_eq!(lift(&insn(0x4000, 0xD508_7620, InsnKind::System)), vec![]); // DC IVAC, X0
        assert_eq!(lift(&insn(0x4000, 0xD50B_7E20, InsnKind::System)), vec![]); // DC CIVAC, X0
        assert_eq!(lift(&insn(0x4000, 0xD508_751F, InsnKind::System)), vec![]); // IC IALLU
        assert_eq!(lift(&insn(0x4000, 0xD508_711F, InsnKind::System)), vec![]); // IC IALLUIS
    }

    #[test]
    fn swp_lifts_to_load_store_and_transfer() {
        // SWP X0, X1, [X2]
        assert_eq!(
            lift(&insn(0x4000, 0xF820_8041, InsnKind::LoadStore)),
            vec![
                IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: 2,
                    off: 0,
                    size: 8,
                },
                IrOp::StoreDyn {
                    src: 0,
                    base: 2,
                    off: 0,
                    size: 8,
                },
                IrOp::OrrShift {
                    dst: 1,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                },
            ]
        );
        // SWPA X0, X1, [X2]
        assert_eq!(
            lift(&insn(0x4000, 0xF8A0_8041, InsnKind::LoadStore)),
            vec![
                IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: 2,
                    off: 0,
                    size: 8,
                },
                IrOp::StoreDyn {
                    src: 0,
                    base: 2,
                    off: 0,
                    size: 8,
                },
                IrOp::OrrShift {
                    dst: 1,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                },
            ]
        );
        // SWPL X0, X1, [X2]
        assert_eq!(
            lift(&insn(0x4000, 0xF860_8041, InsnKind::LoadStore)),
            vec![
                IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: 2,
                    off: 0,
                    size: 8,
                },
                IrOp::StoreDyn {
                    src: 0,
                    base: 2,
                    off: 0,
                    size: 8,
                },
                IrOp::OrrShift {
                    dst: 1,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                },
            ]
        );
        // SWPAL X0, X1, [X2]
        assert_eq!(
            lift(&insn(0x4000, 0xF8E0_8041, InsnKind::LoadStore)),
            vec![
                IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: 2,
                    off: 0,
                    size: 8,
                },
                IrOp::StoreDyn {
                    src: 0,
                    base: 2,
                    off: 0,
                    size: 8,
                },
                IrOp::OrrShift {
                    dst: 1,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                },
            ]
        );
        // SWP W0, W1, [X2]
        assert_eq!(
            lift(&insn(0x4000, 0xB820_8041, InsnKind::LoadStore)),
            vec![
                IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: 2,
                    off: 0,
                    size: 4,
                },
                IrOp::StoreDyn {
                    src: 0,
                    base: 2,
                    off: 0,
                    size: 4,
                },
                IrOp::OrrShift {
                    dst: 1,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                },
            ]
        );
        // SWPB W0, W1, [X2]
        assert_eq!(
            lift(&insn(0x4000, 0x3820_8041, InsnKind::LoadStore)),
            vec![
                IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: 2,
                    off: 0,
                    size: 1,
                },
                IrOp::StoreDyn {
                    src: 0,
                    base: 2,
                    off: 0,
                    size: 1,
                },
                IrOp::OrrShift {
                    dst: 1,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 0,
                },
            ]
        );
    }

    #[test]
    fn ldar_stlr_ldapr_lift() {
        // LDAR X0, [X1]
        assert_eq!(
            lift(&insn(0x4000, 0xC8DF_FC20, InsnKind::LoadStore)),
            vec![IrOp::LoadDyn {
                dst: 0,
                base: 1,
                off: 0,
                size: 8,
            }]
        );
        // STLR X0, [X1]
        assert_eq!(
            lift(&insn(0x4000, 0xC89F_FC20, InsnKind::LoadStore)),
            vec![IrOp::StoreDyn {
                src: 0,
                base: 1,
                off: 0,
                size: 8,
            }]
        );
        // LDAR W0, [X1]
        assert_eq!(
            lift(&insn(0x4000, 0x88DF_FC20, InsnKind::LoadStore)),
            vec![IrOp::LoadDyn {
                dst: 0,
                base: 1,
                off: 0,
                size: 4,
            }]
        );
        // STLR W0, [X1]
        assert_eq!(
            lift(&insn(0x4000, 0x889F_FC20, InsnKind::LoadStore)),
            vec![IrOp::StoreDyn {
                src: 0,
                base: 1,
                off: 0,
                size: 4,
            }]
        );
        // LDAPR X0, [X1]
        assert_eq!(
            lift(&insn(0x4000, 0xF8BF_C020, InsnKind::LoadStore)),
            vec![IrOp::LoadDyn {
                dst: 0,
                base: 1,
                off: 0,
                size: 8,
            }]
        );
        // LDAPR W0, [X1]
        assert_eq!(
            lift(&insn(0x4000, 0xB8BF_C020, InsnKind::LoadStore)),
            vec![IrOp::LoadDyn {
                dst: 0,
                base: 1,
                off: 0,
                size: 4,
            }]
        );
    }

    #[test]
    fn cas_and_exclusives_lift_to_honest_traps() {
        assert_eq!(
            lift(&insn(0x4000, 0x88A0_7C41, InsnKind::LoadStore)),
            vec![IrOp::Trap {
                reason: R_ATOMIC_CAS,
            }]
        ); // CAS W0, W1, [X2]
        assert_eq!(
            lift(&insn(0x4000, 0x88E0_7C41, InsnKind::LoadStore)),
            vec![IrOp::Trap {
                reason: R_ATOMIC_CAS,
            }]
        ); // CASA W0, W1, [X2]
        assert_eq!(
            lift(&insn(0x4000, 0x88A0_FC41, InsnKind::LoadStore)),
            vec![IrOp::Trap {
                reason: R_ATOMIC_CAS,
            }]
        ); // CASL W0, W1, [X2]
        assert_eq!(
            lift(&insn(0x4000, 0x88E0_FC41, InsnKind::LoadStore)),
            vec![IrOp::Trap {
                reason: R_ATOMIC_CAS,
            }]
        ); // CASAL W0, W1, [X2]
        assert_eq!(
            lift(&insn(0x4000, 0xC8A0_7C41, InsnKind::LoadStore)),
            vec![IrOp::Trap {
                reason: R_ATOMIC_CAS,
            }]
        ); // CAS X0, X1, [X2]
        assert_eq!(
            lift(&insn(0x4000, 0x885F_7C20, InsnKind::LoadStore)),
            vec![IrOp::Trap {
                reason: R_EXCLUSIVE,
            }]
        ); // LDXR W0, [X1]
        assert_eq!(
            lift(&insn(0x4000, 0x8802_7C20, InsnKind::LoadStore)),
            vec![IrOp::Trap {
                reason: R_EXCLUSIVE,
            }]
        ); // STXR W2, W0, [X1]
    }
}
