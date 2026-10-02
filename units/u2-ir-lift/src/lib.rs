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
    // Exception generation instructions: bits[31:24] == 0xD4
    if (word >> 24) == 0xD4 {
        let opc = (word >> 21) & 0x7;
        let ll = word & 0x3;
        return match opc {
            0b000 if ll == 0b10 => trap(R_HVC),
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
                        // TPIDR_EL2: Hypervisor Thread ID (4,998 static hits)
                        (3, 4, 13, 0, 2) => 0,
                        // CLIDR_EL1: Cache Level ID Register (10 static hits)
                        // L1 Harvard (separate I/D), L2 unified, LoUIS=1, LoUU=1, LoC=2
                        (3, 1, 0, 0, 1) => 0x0920_0023,
                        // CSSELR_EL1: Cache Size Selection Register
                        (3, 2, 0, 0, 0) => 0,
                        // CCSIDR_EL1: Cache Size ID Register (64B line, 4-way, 64KB)
                        (3, 1, 0, 0, 0) => 0x701F_E00A,
                        // CNTFRQ_EL0: Counter-timer frequency (62.5MHz)
                        (3, 3, 14, 0, 0) => 0x03B9_ACA0,
                        // CNTVCT_EL0: Virtual counter count
                        (3, 3, 14, 0, 2) => 0,
                        // CNTPCT_EL0: Physical counter count
                        (3, 3, 14, 0, 1) => 0,
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
                        (3, 3, 9, 12, 0) => 0, // PMCR_EL0
                        (3, 3, 9, 14, 0) => 0, // PMUSERENR_EL0
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
    if (word >> 24) & 0x7F == 0x11 {
        if word >> 31 == 0 {
            return trap(R_ADD32_IMM);
        }
        let sh = (word >> 22) & 1;
        let imm12 = (word >> 10) & 0xFFF;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        let imm = (imm12 as u64) << (if sh == 1 { 12 } else { 0 });
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
    if (word >> 24) & 0x7F == 0x51 {
        if word >> 31 == 0 {
            return trap(R_SUB32_IMM);
        }
        let sh = (word >> 22) & 1;
        let imm12 = (word >> 10) & 0xFFF;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        let imm = (imm12 as u64) << (if sh == 1 { 12 } else { 0 });
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
    // ADD (shifted register): sf 0 01011 00 0 Rm imm6 Rn Rd, LSL #0 only
    // (bits 31:24 = 0x0B/0x8B; S = 1 would be ADDS and never matches).
    let top = (word >> 24) & 0xFF;
    if top == 0x0B || top == 0x8B {
        if word >> 31 == 0 {
            return trap(R_ADD32_REG);
        }
        let shift = (word >> 22) & 0x3;
        let imm6 = (word >> 10) & 0x3F;
        if shift != 0 || imm6 != 0 {
            return trap(R_ADD_SHIFT);
        }
        let rm = ((word >> 16) & 0x1F) as u8;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        return vec![IrOp::Add {
            dst: rd,
            a: rn,
            b: rm,
        }];
    }
    // SUB (shifted register): sf 1 01011 shift 0 Rm imm6 Rn Rd, LSL #0 only
    // (bits 31:24 = 0x4B/0xCB; S = 1 would be SUBS and never matches) — Track GB-6
    if top == 0x4B || top == 0xCB {
        if word >> 31 == 0 {
            return trap(R_SUB32_REG);
        }
        let shift = (word >> 22) & 0x3;
        let imm6 = (word >> 10) & 0x3F;
        if shift != 0 || imm6 != 0 {
            return trap(R_SUB_SHIFT);
        }
        let rm = ((word >> 16) & 0x1F) as u8;
        let rn = ((word >> 5) & 0x1F) as u8;
        let rd = (word & 0x1F) as u8;
        return vec![IrOp::Sub {
            dst: rd,
            a: rn,
            b: rm,
        }];
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
    trap(R_DP_UNSUPPORTED)
}

/// Loads/stores (GB-1 scope):
/// - LDR (literal) keeps its static Load form; LDRSW (literal) adds sign extension.
/// - STP/LDP pairs (all variants: 32-bit, 64-bit, LDPSW, offset, pre/post-indexed, non-temporal)
///   lower to StoreDyn/LoadDyn sequences.
/// - LDR/STR (immediate unsigned offset, pre/post-indexed, register-offset)
///   lower to LoadDyn/StoreDyn.
/// - LDRSW variants load 4 bytes and sign-extend to 64-bit via OrrShift.
fn lift_load_store(insn: &Instruction) -> Vec<IrOp> {
    let word = insn.word;

    // 1. LDR / LDRSW (literal): sf 00 011000 imm19 Rt (bits 31:24 = 0x18 / 0x58 / 0x98).
    // Address = PC + sign_extend(imm19 << 2): fully static.
    let top = (word >> 24) & 0xFF;
    if top == 0x18 || top == 0x58 || top == 0x98 {
        let size: u8 = if top == 0x58 { 8 } else { 4 };
        let imm19 = (word >> 5) & 0x7FFFF;
        let offset = (((imm19 as i32) << 13) >> 13) as i64 * 4;
        let addr = (insn.addr as i64).wrapping_add(offset) as u64;
        let rt = (word & 0x1F) as u8;
        if top == 0x98 {
            // LDRSW (literal): load 4 bytes and sign-extend to 64 bits.
            return vec![
                IrOp::Load {
                    dst: SCRATCH,
                    addr,
                    size: 4,
                },
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 32,
                },
                IrOp::OrrShift {
                    dst: rt,
                    a: 31,
                    b: SCRATCH,
                    shift: 2,
                    amount: 32,
                },
            ];
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

        // Halfword is still sub-word and kept as trap per existing tests.
        if size_bits == 1 {
            return trap(R_LS_SUBWORD);
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
        } else if opc == 0b10 && size_bits == 2 {
            // LDRSW (unsigned offset)
            return vec![
                IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: rn,
                    off,
                    size: 4,
                },
                IrOp::OrrShift {
                    dst: SCRATCH,
                    a: 31,
                    b: SCRATCH,
                    shift: 0,
                    amount: 32,
                },
                IrOp::OrrShift {
                    dst: rt,
                    a: 31,
                    b: SCRATCH,
                    shift: 2,
                    amount: 32,
                },
            ];
        }
        return trap(R_LS_UNSUPPORTED);
    }

    // 4. Load/store register (immediate pre/post-indexed, and unscaled):
    // size 111 V 00 opc 0 imm9 type Rn Rt with bits[29:24] == 0b111000, bit 21 == 0.
    // type 0b01 = post-index, 0b11 = pre-index, 0b00 = unscaled (LDUR/STUR:
    // address = base + simm9, no writeback). GB-26: unscaled was
    // deliberately out of scope until the kernel hit STUR at step 1248988
    // (pc 0xffffff80083969c0, word 0xf81f83a8 = STUR X8, [X29, #-8]).
    if (word >> 24) & 0x3F == 0b111000 && (word >> 21) & 1 == 0 {
        let idx_type = (word >> 10) & 0x3;
        if idx_type == 0b01 || idx_type == 0b11 || idx_type == 0b00 {
            let size_bits = (word >> 30) & 0x3;
            let opc = (word >> 22) & 0x3;
            let imm9 = ((word >> 12) & 0x1FF) as i32;
            let simm9 = ((imm9 << 23) >> 23) as i64;
            let rn = ((word >> 5) & 0x1F) as u8;
            let rt = (word & 0x1F) as u8;

            if rn == 31 {
                return trap(R_LS_SP);
            }

            if size_bits == 1 {
                return trap(R_LS_SUBWORD);
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
            } else if opc == 0b10 && size_bits == 2 {
                // LDRSW (pre/post-indexed)
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
            } else {
                return trap(R_LS_UNSUPPORTED);
            }

            if simm9 != 0 && idx_type != 0b00 {
                // Writeback for pre/post-index only. Unscaled (LDUR/STUR)
                // has no writeback: the address IS base + simm9.
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

        if size_bits == 1 {
            return trap(R_LS_SUBWORD);
        }

        // option 011 = 64-bit register offset (LSL #0 or LSL #size_bits)
        if option == 0b011 {
            let size: u8 = 1 << size_bits;
            let shift_amt: u8 = if s == 1 { size_bits as u8 } else { 0 };

            let mut ops = Vec::new();
            if shift_amt > 0 {
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
            } else if opc == 0b10 && size_bits == 2 {
                // LDRSW (reg offset)
                ops.push(IrOp::LoadDyn {
                    dst: SCRATCH,
                    base: SCRATCH,
                    off: 0,
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
        // SUB W1, W1, W0: 32-bit width not expressible
        let ops = lift(&insn(0x4000, 0x4b00_0021, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_SUB32_REG
            }]
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
    fn trap_add_imm_32bit_width() {
        // ADD W0, W1, #1: 32-bit zeroing of Xd not expressible in IrOp::Add
        let ops = lift(&insn(0x4000, 0x1100_0420, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_ADD32_IMM
            }]
        );
    }

    #[test]
    fn trap_sub_imm_32bit_width() {
        // SUB W0, W1, #1: 32-bit zeroing of Xd not expressible in IrOp::Sub
        let ops = lift(&insn(0x4000, 0x5100_0420, InsnKind::DataProc));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_SUB32_IMM
            }]
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
    fn trap_ldrh_subword() {
        // LDRH W0, [X1]: 16-bit width is still not expressible (Wave 4
        // lifted only the byte forms).
        let ops = lift(&insn(0x4000, 0x7940_0420, InsnKind::LoadStore));
        assert_eq!(
            ops,
            vec![IrOp::Trap {
                reason: R_LS_SUBWORD
            }]
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
                    when_zero: false
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
    fn exceptions_lift_to_honest_traps() {
        assert_eq!(
            lift(&insn(0x4000, 0xD420_0000, InsnKind::System)),
            vec![IrOp::Trap { reason: R_BRK }]
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD440_0000, InsnKind::System)),
            vec![IrOp::Trap { reason: R_HLT }]
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD400_0002, InsnKind::System)),
            vec![IrOp::Trap { reason: R_HVC }]
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
        assert_eq!(
            lift(&insn(0x4000, 0xD53C_D040, InsnKind::System)),
            vec![IrOp::Mov { dst: 0, imm: 0 }] // MRS X0, TPIDR_EL2
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
            vec![IrOp::Mov { dst: 0, imm: 0 }] // MRS X0, CNTVCT_EL0
        );
        assert_eq!(
            lift(&insn(0x4000, 0xD53B_E020, InsnKind::System)),
            vec![IrOp::Mov { dst: 0, imm: 0 }] // MRS X0, CNTPCT_EL0
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
}
