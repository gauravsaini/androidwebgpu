//! CPU / JIT contract types — owned by Wave 0 leaf 1.1.
//!
//! Covers U1 (decode), U2 (IR lift), U3 (WASM JIT), U4 (MMU), U5 (GIC/timer).

/// Coarse AArch64 instruction class. The decoder refines into full semantics later;
/// the class is contract-stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsnKind {
    DataProc,
    LoadStore,
    Branch,
    System,
    /// Added 2026-09-30 (Phase-2 spike): SVC (supervisor call, immediate).
    /// bits[31:21] == 0b11010100000. Lifts to an honest unimplemented trap
    /// until the exception model lands; never silently executed.
    Svc,
    /// Added 2026-09-27 (Wave 4 amendment U1-G1): PC-relative address
    /// computation (ADR/ADRP). A distinct class because the lifter needs the
    /// instruction address to compute the target; DataProc semantics never do.
    PcRel,
    Unknown,
}

/// A decoded AArch64 instruction. Pure data — no execution semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction {
    pub addr: u64,
    pub word: u32,
    pub kind: InsnKind,
}

/// Decode outcome. Illegal words are data, not panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeResult {
    Ok(Instruction),
    Illegal { word: u32 },
}

/// System-register selector for IrOp::ReadSys / IrOp::WriteSys.
/// Added 2026-09-30 (Track GB-sysreg2): the writable system registers whose
/// MSR writes GB-3 accepted without state. Discriminants are the host-call
/// index, part of the U3 import contract: do not reorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SysReg {
    Daif = 0,
    TpidrEl1 = 1,
    SctlrEl1 = 2,
    SctlrEl2 = 3,
    HcrEl2 = 4,
    CnthctlEl2 = 5,
    CntvoffEl2 = 6,
    VbarEl1 = 7,
    SpEl0 = 8,
    /// Added 2026-09-30 (Track GB-9): Architectural Feature Access Control
    /// Register, EL1. The kernel writes CPACR_EL1 with FPEN=0b11 to enable
    /// FP/ASIMD during early CPU setup (MSR CPACR_EL1, X0).
    /// CORRECTION 2026-09-30: GB-8 labeled word 0xd5181040 as MSR TCR_EL1,
    /// but field extraction gives S3_0_C1_C0_2, which is CPACR_EL1
    /// (TCR_EL1 is S3_0_C2_C0_2 = 0xd5182040).
    CpacrEl1 = 9,
    /// Added 2026-09-30 (Track GB-10): Monitor Debug System Control
    /// Register, EL1. The kernel writes MDSCR_EL1 during early CPU setup
    /// (MSR MDSCR_EL1, X0 with X0=0x1000 from the preceding MOVZ X0, #0x1000;
    /// CORRECTION 2026-09-30: the GB-10 brief assumed X0=0, but the box's
    /// aarch64 objdump decodes step 7458 (0xd2820000) as MOVZ X0, #0x1000.)
    MdscrEl1 = 10,
    /// Added 2026-10-01 (Track GB-13): Memory Attribute Indirection
    /// Register, EL1. The kernel programs MAIR_EL1 during early MMU setup
    /// (MSR MAIR_EL1, X5). Field extraction on the measured halt word
    /// 0xd518a205 gives S3_0_C10_C2_0, which is MAIR_EL1 (verified against
    /// the ARM ARM; Rt = bits[4:0] = X5).
    MairEl1 = 11,
    /// Added 2026-10-01 (Track GB-17): Translation Control Register, EL1.
    /// The kernel programs TCR_EL1 during early MMU setup
    /// (MSR TCR_EL1, X10). Field extraction on the measured halt word
    /// 0xd518204a gives (op0,op1,crn,crm,op2) = (3,0,2,0,2) = S3_0_C2_C0_2,
    /// which is TCR_EL1 (verified against the ARM ARM; Rt = bits[4:0] = X10).
    /// This is the real TCR_EL1 that GB-8 mislabeled: word 0xd5181040 was
    /// S3_0_C1_C0_2 = CPACR_EL1, not TCR_EL1.
    TcrEl1 = 12,
    /// Added 2026-10-01 (Track GB-18): Translation Table Base Register 0,
    /// EL1. The kernel programs TTBR0_EL1 during early MMU setup
    /// (MSR TTBR0_EL1, X3), right after TCR_EL1 and MAIR_EL1. Field
    /// extraction on the measured halt word 0xd5182003 gives
    /// (op0,op1,crn,crm,op2) = (3,0,2,0,0) = S3_0_C2_C0_0, which is
    /// TTBR0_EL1 (verified against the ARM ARM; Rt = bits[4:0] = X3).
    Ttbr0El1 = 13,
    /// Added 2026-10-01 (Track GB-19): Translation Table Base Register 1,
    /// EL1. The kernel programs TTBR1_EL1 immediately after TTBR0_EL1
    /// during early MMU setup (MSR TTBR1_EL1, X4). Field extraction on
    /// the measured halt word 0xd5182024 gives
    /// (op0,op1,crn,crm,op2) = (3,0,2,0,1) = S3_0_C2_C0_1, which is
    /// TTBR1_EL1 (verified against the ARM ARM; Rt = bits[4:0] = X4).
    Ttbr1El1 = 14,
    /// Added 2026-10-02 (fam/integer-sandbox): Generic Timer registers.
    /// CNTPCT_EL0: Physical counter, CNTP_CTL_EL0/CNTP_CVAL_EL0: control/compare,
    /// CNTVCT_EL0/CNTV_CTL_EL0/CNTV_CVAL_EL0: virtual counter equivalents.
    /// Decode only; actual timer behavior is the devices track's work.
    CntpctEl0 = 15,
    CntvctEl0 = 16,
    CntpCtlEl0 = 17,
    CntpCvalEl0 = 18,
    CntvCtlEl0 = 19,
    CntvCvalEl0 = 20,
    /// Added 2026-10-03 (P0): Context ID Register, EL1. The kernel writes
    /// CONTEXTIDR_EL1 on every context switch (MSR CONTEXTIDR_EL1, Xt).
    /// S3_0_C13_C0_1. Stored as simple u64, no behavior needed.
    ContextidrEl1 = 21,
    /// Added 2026-10-03 (P2): Physical Address Register, EL1. The kernel
    /// reads PAR_EL1 after AT address-translate operations
    /// (MRS PAR_EL1, Xt). S3_0_C7_C4_0. Stored as simple u64, default 0.
    ParEl1 = 22,
    /// Added 2026-10-03 (P2): OS Lock Access Register, EL1. Write-only;
    /// the kernel writes OSLAR_EL1 during debug setup
    /// (MSR OSLAR_EL1, Xt). S2_0_C1_C0_4. Stored, no behavior needed.
    OslarEl1 = 23,
    /// Added 2026-10-03 (P2): PMU Counter Enable Set, EL0. The kernel
    /// probes the PMU during boot (MRS/MSR PMCNTENSET_EL0, Xt).
    /// S3_3_C9_C12_1. Stored as simple u64, default 0.
    PmcntensetEl0 = 24,
    /// Added 2026-10-03 (P2): PMU Event Counter Selection, EL0. The kernel
    /// probes the PMU during boot (MRS/MSR PMSELR_EL0, Xt).
    /// S3_3_C9_C12_5. Stored as simple u64, default 0.
    PmselrEl0 = 25,
    /// Added 2026-10-03 (P2): Auxiliary Control Register, EL1. The kernel
    /// may read ACTLR_EL1 during CPU setup (MRS ACTLR_EL1, Xt).
    /// S3_0_C1_C0_1. Returns 0: we don't implement auxiliary features,
    /// and 0 is the honest "nothing extra here" answer.
    ActlrEl1 = 26,
    // ===== P3 (2026-10-03, feat/emu-p3-impl) =====
    /// GICv3 CPU interface (slice A): Interrupt Controller System Register
    /// Enable, EL1. Kernel's gicv3 probe reads it to choose the sysreg
    /// access path. Default 0x1 (SRE=1): honest — our CPU interface is
    /// modeled as sysreg-accessible, not memory-mapped. Stored, RW.
    IccSreEl1 = 27,
    /// GICv3 CPU interface: Interrupt Controller Control Register, EL1.
    /// Stored u64, default 0.
    IccCtlrEl1 = 28,
    /// GICv3 CPU interface: Interrupt Controller Interrupt Group 1
    /// Enable Register, EL1. Stored u64, default 0.
    IccIgrpen1El1 = 29,
    /// GICv3 CPU interface: Interrupt Controller Interrupt Priority Mask
    /// Register, EL1. Stored u64, default 0 (everything masked: honest —
    /// we have no GIC model and deliver no interrupts, so the safest
    /// advertised state is "nothing can preempt").
    IccPmrEl1 = 30,
    /// GICv3 CPU interface: Interrupt Controller End Of Interrupt Register
    /// 1, EL1. WRITE-ONLY: no MRS arm (an MRS would be architecturally
    /// UNDEFINED and traps with R_SYSTEM). MSR stored, no behavior —
    /// there is no interrupt state to complete against yet.
    IccEoir1El1 = 31,
    /// GICv3 CPU interface: Interrupt Controller Deactivate Interrupt
    /// Register, EL1. WRITE-ONLY, same treatment as EOIR1.
    IccDirEl1 = 32,
    /// PMU (slice C): PMU Counter Enable Clear, EL0. Stored u64, default 0.
    PmcntEnClrEl0 = 33,
    /// PMU (slice C): PMU Overflow Flag Status Clear, EL0. Stored u64.
    PmovsclrEl0 = 34,
    /// PMU (slice C): PMU Event Type Select, EL0 (indirect event type
    /// register selected by PMSELR_EL0). Stored u64.
    PmxevtyperEl0 = 35,
    /// PMU (slice C): PMU Event Counter (indirect, selected by PMSELR_EL0).
    /// Stored u64.
    PmxevcntrEl0 = 36,
    /// PMU (slice C): PMU User Enable Register, EL0. P2 accepted MSR as a
    /// no-op; P3 stores it so MSR/MSR reads round-trip (kernel writes xzr,
    /// reads back 0 = EL0 PMU access disabled — consistent, we never let
    /// EL0 touch the PMU).
    PmuserenrEl0 = 37,
    /// Timers (slice D): Counter-timer Kernel Control, EL1. Stored u64.
    CntkctlEl1 = 38,
    /// Timers (slice D): Hypervisor Thread ID, EL2. P2 returned constant 0
    /// on MRS; P3 stores it so the (hyp, unreachable-at-EL1) MSR/MSR pair
    /// round-trips honestly instead of disagreeing.
    TpidrEl2 = 39,
    /// Timers (slice D): Physical Timer Value, EL0. DERIVED, no backing
    /// field: read = low 32 bits of (CNTP_CVAL_EL0 − CNTPCT_EL0); write
    /// sets CNTP_CVAL_EL0 = CNTPCT_EL0 + value[31:0]. This is the honest
    /// architectural alias, not a stored register.
    CntpTvalEl0 = 40,
    /// Timers (slice D): Virtual Timer Value, EL0. DERIVED like the
    /// physical one: CNTV_CVAL_EL0 − (CNTPCT_EL0 − CNTVOFF_EL2).
    CntvTvalEl0 = 41,
    /// FP/SIMD (slice E): Floating-point Control Register. Stored u64
    /// (architecturally 32-bit; full 64-bit store is fine for save/restore
    /// — the upper bits are the kernel's own, written back unchanged).
    Fpcr = 42,
    /// FP/SIMD (slice E): Floating-point Status Register. Stored u64,
    /// same treatment as FPCR.
    Fpsr = 43,
    /// Debug (slice F): OS Double Lock Register, EL1. WRITE-ONLY: no MRS
    /// arm (UNDEFINED on read → R_SYSTEM trap). MSR stored, no behavior.
    OsdlrEl1 = 44,
    // ===== P4 (2026-10-03, feat/emu-p4-impl) =====
    /// P4 (2026-10-03): DBGBVR0_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr0 = 45,
    /// P4 (2026-10-03): DBGBVR1_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr1 = 46,
    /// P4 (2026-10-03): DBGBVR2_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr2 = 47,
    /// P4 (2026-10-03): DBGBVR3_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr3 = 48,
    /// P4 (2026-10-03): DBGBVR4_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr4 = 49,
    /// P4 (2026-10-03): DBGBVR5_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr5 = 50,
    /// P4 (2026-10-03): DBGBVR6_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr6 = 51,
    /// P4 (2026-10-03): DBGBVR7_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr7 = 52,
    /// P4 (2026-10-03): DBGBVR8_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr8 = 53,
    /// P4 (2026-10-03): DBGBVR9_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr9 = 54,
    /// P4 (2026-10-03): DBGBVR10_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr10 = 55,
    /// P4 (2026-10-03): DBGBVR11_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr11 = 56,
    /// P4 (2026-10-03): DBGBVR12_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr12 = 57,
    /// P4 (2026-10-03): DBGBVR13_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr13 = 58,
    /// P4 (2026-10-03): DBGBVR14_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr14 = 59,
    /// P4 (2026-10-03): DBGBVR15_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbvr15 = 60,
    /// P4 (2026-10-03): DBGBCR0_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr0 = 61,
    /// P4 (2026-10-03): DBGBCR1_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr1 = 62,
    /// P4 (2026-10-03): DBGBCR2_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr2 = 63,
    /// P4 (2026-10-03): DBGBCR3_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr3 = 64,
    /// P4 (2026-10-03): DBGBCR4_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr4 = 65,
    /// P4 (2026-10-03): DBGBCR5_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr5 = 66,
    /// P4 (2026-10-03): DBGBCR6_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr6 = 67,
    /// P4 (2026-10-03): DBGBCR7_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr7 = 68,
    /// P4 (2026-10-03): DBGBCR8_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr8 = 69,
    /// P4 (2026-10-03): DBGBCR9_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr9 = 70,
    /// P4 (2026-10-03): DBGBCR10_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr10 = 71,
    /// P4 (2026-10-03): DBGBCR11_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr11 = 72,
    /// P4 (2026-10-03): DBGBCR12_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr12 = 73,
    /// P4 (2026-10-03): DBGBCR13_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr13 = 74,
    /// P4 (2026-10-03): DBGBCR14_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr14 = 75,
    /// P4 (2026-10-03): DBGBCR15_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgbcr15 = 76,
    /// P4 (2026-10-03): DBGWVR0_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr0 = 77,
    /// P4 (2026-10-03): DBGWVR1_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr1 = 78,
    /// P4 (2026-10-03): DBGWVR2_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr2 = 79,
    /// P4 (2026-10-03): DBGWVR3_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr3 = 80,
    /// P4 (2026-10-03): DBGWVR4_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr4 = 81,
    /// P4 (2026-10-03): DBGWVR5_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr5 = 82,
    /// P4 (2026-10-03): DBGWVR6_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr6 = 83,
    /// P4 (2026-10-03): DBGWVR7_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr7 = 84,
    /// P4 (2026-10-03): DBGWVR8_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr8 = 85,
    /// P4 (2026-10-03): DBGWVR9_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr9 = 86,
    /// P4 (2026-10-03): DBGWVR10_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr10 = 87,
    /// P4 (2026-10-03): DBGWVR11_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr11 = 88,
    /// P4 (2026-10-03): DBGWVR12_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr12 = 89,
    /// P4 (2026-10-03): DBGWVR13_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr13 = 90,
    /// P4 (2026-10-03): DBGWVR14_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr14 = 91,
    /// P4 (2026-10-03): DBGWVR15_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwvr15 = 92,
    /// P4 (2026-10-03): DBGWCR0_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr0 = 93,
    /// P4 (2026-10-03): DBGWCR1_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr1 = 94,
    /// P4 (2026-10-03): DBGWCR2_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr2 = 95,
    /// P4 (2026-10-03): DBGWCR3_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr3 = 96,
    /// P4 (2026-10-03): DBGWCR4_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr4 = 97,
    /// P4 (2026-10-03): DBGWCR5_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr5 = 98,
    /// P4 (2026-10-03): DBGWCR6_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr6 = 99,
    /// P4 (2026-10-03): DBGWCR7_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr7 = 100,
    /// P4 (2026-10-03): DBGWCR8_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr8 = 101,
    /// P4 (2026-10-03): DBGWCR9_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr9 = 102,
    /// P4 (2026-10-03): DBGWCR10_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr10 = 103,
    /// P4 (2026-10-03): DBGWCR11_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr11 = 104,
    /// P4 (2026-10-03): DBGWCR12_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr12 = 105,
    /// P4 (2026-10-03): DBGWCR13_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr13 = 106,
    /// P4 (2026-10-03): DBGWCR14_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr14 = 107,
    /// P4 (2026-10-03): DBGWCR15_EL1. Stored u64, no behavior (no debug hardware model).
    Dbgwcr15 = 108,
    /// P4 (2026-10-03): Virtualization Processor ID Register, EL2. MSR-only in the P4 scan; stored u64, no behavior.
    VpidrEl2 = 109,
    /// P4 (2026-10-03): Virtualization Multiprocessor ID Register, EL2. MSR-only in the P4 scan; stored u64, no behavior.
    VmpidrEl2 = 110,
    /// P4 (2026-10-03): Architectural Feature Trap Register, EL2. MSR-only in the P4 scan; stored u64, no behavior.
    CptrEl2 = 111,
    /// P4 (2026-10-03): Monitor Debug Configuration Register, EL2. MSR-only in the P4 scan; stored u64, no behavior.
    MdcrEl2 = 112,
    /// P4 (2026-10-03): Hypervisor System Trap Register, EL2. MSR-only in the P4 scan; stored u64, no behavior.
    HstrEl2 = 113,
    /// P4 (2026-10-03): SVE Control Register, EL2. MSR-only in the P4 scan; stored u64, no behavior.
    ZcrEl2 = 114,
    /// P4 (2026-10-03): Vector Base Address Register, EL2. MSR-only in the P4 scan; stored u64, no behavior.
    VbarEl2 = 115,
    /// P4 (2026-10-03): Interrupt Controller Hyp Control Register, EL2. MSR-only in the P4 scan; stored u64, no behavior.
    IchHcrEl2 = 116,
    /// P4 (2026-10-03): Virtualization Translation Table Base Register, EL2. MSR-only in the P4 scan; stored u64, no behavior.
    VttbrEl2 = 117,
    /// P4 (2026-10-03): Saved Program Status Register, EL2. MSR-only in the P4 scan; stored u64, no behavior.
    SpsrEl2 = 118,
    /// P4 (2026-10-03): Exception Link Register, EL2. MSR-only in the P4 scan; stored u64, no behavior.
    ElrEl2 = 119,
    /// P4 (2026-10-03): Statistical Profiling Control Register, EL2. MSR-only in the P4 scan; stored u64, no behavior.
    PmscrEl2 = 120,
    /// P4 (2026-10-03): ICC_SRE_EL2: Interrupt Controller System Register Enable, EL2. Default 0x1 (SRE=1), mirroring the EL1 default: our CPU interface is sysreg-accessible. Stored, RW.
    IccSreEl2 = 121,
    /// P4 (2026-10-03): ICC_BPR1_EL1: Interrupt Controller Binary Point Register 1, EL1. Stored u64.
    IccBpr1El1 = 122,
    /// P4 (2026-10-03): ICC_AP0R0_EL1: Interrupt Controller Active Priority Register 0.0, EL1. Written by gicv3_cpu_sys_reg_init to clear active priorities. Stored u64.
    IccAp0r0El1 = 123,
    /// P4 (2026-10-03): ICC_AP0R1_EL1: Interrupt Controller Active Priority Register 0.1, EL1. Written by gicv3_cpu_sys_reg_init to clear active priorities. Stored u64.
    IccAp0r1El1 = 124,
    /// P4 (2026-10-03): ICC_AP0R2_EL1: Interrupt Controller Active Priority Register 0.2, EL1. Written by gicv3_cpu_sys_reg_init to clear active priorities. Stored u64.
    IccAp0r2El1 = 125,
    /// P4 (2026-10-03): ICC_AP0R3_EL1: Interrupt Controller Active Priority Register 0.3, EL1. Written by gicv3_cpu_sys_reg_init to clear active priorities. Stored u64.
    IccAp0r3El1 = 126,
    /// P4 (2026-10-03): ICC_AP1R0_EL1: Interrupt Controller Active Priority Register 1.0, EL1. Stored u64.
    IccAp1r0El1 = 127,
    /// P4 (2026-10-03): ICC_AP1R1_EL1: Interrupt Controller Active Priority Register 1.1, EL1. Stored u64.
    IccAp1r1El1 = 128,
    /// P4 (2026-10-03): ICC_AP1R2_EL1: Interrupt Controller Active Priority Register 1.2, EL1. Stored u64.
    IccAp1r2El1 = 129,
    /// P4 (2026-10-03): ICC_AP1R3_EL1: Interrupt Controller Active Priority Register 1.3, EL1. Stored u64.
    IccAp1r3El1 = 130,
    /// P4 (2026-10-03): ICC_SGI1R_EL1: Interrupt Controller Software Generated Interrupt Register, EL1. WRITE-ONLY: no MRS arm (architecturally UNDEFINED on read -> R_SYSTEM trap). MSR stored, no behavior (no GIC model, no IPI delivery).
    IccSgi1rEl1 = 131,
    /// P4 (2026-10-03): DISR_EL1: Deferred Interrupt Status Register, EL1. Stored u64, no behavior.
    DisrEl1 = 132,
    /// P4 (2026-10-03): LORC_EL1: LORegion Control Register, EL1. Stored u64 (LORegions not modeled).
    LorcEl1 = 133,
    /// P4 (2026-10-03): PMCCNTR_EL0: Performance Monitors Cycle Count Register, EL0. Stored u64, default 0 (no PMU model: the counter never advances; honest flat zero).
    PmccntrEl0 = 134,
    /// P4 (2026-10-03): ZCR_EL1: SVE Control Register, EL1. Written/read by the kernel's SVE probe (mrs x3, zcr_el1 @ 0x4008474c). Stored u64, default 0. ID_AA64ZFR0_EL1 reads 0 (no SVE), so the probe must see a consistent stored value, not a trap.
    ZcrEl1 = 135,
}

/// Single IR operation (SSA-style). The lifter (U2) is the only producer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrOp {
    Add {
        dst: u8,
        a: u8,
        b: u8,
    },
    Sub {
        dst: u8,
        a: u8,
        b: u8,
    },
    Mov {
        dst: u8,
        imm: u64,
    },
    Load {
        dst: u8,
        addr: u64,
        size: u8,
    },
    Store {
        src: u8,
        addr: u64,
        size: u8,
    },
    Branch {
        target: u64,
    },
    /// Added 2026-09-27 (Wave 5 amendment, BL/RET scope): indirect branch by
    /// register index. `control goes to regs[reg]` (64-bit value); register
    /// 31 reads as 0 (XZR). The execution backend returns the register value
    /// as the block's next PC — `run() -> i64`'s result IS the exit address,
    /// so no new WASM imports are needed. Terminates the block like
    /// [`IrOp::Branch`]; the declared exit is [`BlockExit::Dynamic`] because
    /// no static address can be named ahead of time.
    BranchDyn {
        reg: u8,
    },
    /// Added 2026-09-27 (Wave 4 amendment U2-G1): dynamic-address access.
    /// `addr = regs[base] + off` computed at runtime; `size ∈ {1,2,4,8}`.
    /// Register 31 as `base` reads as 0 (XZR); as `dst` the loaded value is
    /// dropped but the access still happens (e.g. a console-RX read still
    /// consumes a byte). The execution backend intercepts MMIO ranges —
    /// they never alias RAM.
    LoadDyn {
        dst: u8,
        base: u8,
        off: u64,
        size: u8,
    },
    /// Added 2026-09-27 (Wave 4 amendment U2-G1): dynamic-address store.
    /// `mem[regs[base]+off] = low `size` bytes of regs[src]`. Register 31
    /// as `src` stores 0 (WZR).
    StoreDyn {
        src: u8,
        base: u8,
        off: u64,
        size: u8,
    },
    /// Added 2026-09-27 (Wave 4 amendment U2-G1): conditional branch
    /// (CBZ/CBNZ). If `(regs[reg] == 0) == when_zero`, control goes to
    /// `target`; otherwise it falls through to the next instruction.
    /// `when_zero = true` for CBZ, `false` for CBNZ. Terminates the block
    /// like [`IrOp::Branch`]; the fallthrough address is the block's
    /// `BlockExit::FallThrough`.
    CondBranch {
        reg: u8,
        target: u64,
        when_zero: bool,
    },
    /// Added 2026-09-27 (Wave 4 amendment U2-G1): OR with shifted register.
    /// `dst = regs[a] | shift(regs[b], shift, amount)`,
    /// `shift ∈ {0=LSL, 1=LSR, 2=ASR}` (0b11 is reserved in the encoding).
    /// 64-bit form only; the 32-bit form traps in the lifter (upper-bit
    /// zeroing is not expressible). Register 31 reads as 0 (XZR); writes
    /// to 31 are dropped.
    OrrShift {
        dst: u8,
        a: u8,
        b: u8,
        shift: u8,
        amount: u8,
    },
    /// Added 2026-09-30 (Track GB-4): AND with shifted register, optional invert (BIC), 32- or 64-bit.
    /// `dst = regs[a] & (invert ? ~shift(regs[b]) : shift(regs[b]))`.
    /// `shift ∈ {0=LSL, 1=LSR, 2=ASR, 3=ROR}`.
    /// When `is_32 = true`, operation is 32-bit and upper 32 bits of `dst` are zeroed.
    AndShift {
        dst: u8,
        a: u8,
        b: u8,
        shift: u8,
        amount: u8,
        invert: bool,
        is_32: bool,
    },
    /// Added 2026-09-30 (Track GB-4): OR with shifted register, optional invert (ORN), 32- or 64-bit.
    /// `dst = regs[a] | (invert ? ~shift(regs[b]) : shift(regs[b]))`.
    /// `shift ∈ {0=LSL, 1=LSR, 2=ASR, 3=ROR}`.
    /// When `is_32 = true`, operation is 32-bit and upper 32 bits of `dst` are zeroed.
    OrShift {
        dst: u8,
        a: u8,
        b: u8,
        shift: u8,
        amount: u8,
        invert: bool,
        is_32: bool,
    },
    /// Added 2026-09-30 (Track GB-4): EOR with shifted register, optional invert (EON), 32- or 64-bit.
    /// `dst = regs[a] ^ (invert ? ~shift(regs[b]) : shift(regs[b]))`.
    /// `shift ∈ {0=LSL, 1=LSR, 2=ASR, 3=ROR}`.
    /// When `is_32 = true`, operation is 32-bit and upper 32 bits of `dst` are zeroed.
    EorShift {
        dst: u8,
        a: u8,
        b: u8,
        shift: u8,
        amount: u8,
        invert: bool,
        is_32: bool,
    },
    /// Added 2026-09-30 (Track GB-4): Bitfield operation (SBFM=0, BFM=1, UBFM=2).
    /// Pure deterministic bitfield extract, insert, or shift.
    /// When `is_32 = true`, operation is 32-bit and upper 32 bits of `dst` are zeroed.
    Bitfield {
        dst: u8,
        src: u8,
        opc: u8,
        immr: u8,
        imms: u8,
        is_32: bool,
    },
    /// Added 2026-09-30 (Track GB-4): Variable shift by register value.
    /// `shift ∈ {0=ASRV, 1=LSRV, 2=LSLV, 3=RORV}`.
    /// When `is_32 = true`, shift amount is modulo 32 and upper 32 bits are zeroed.
    ShiftVar {
        dst: u8,
        a: u8,
        b: u8,
        shift: u8,
        is_32: bool,
    },
    /// Added 2026-09-30 (Track GB-4): Move wide with keep (MOVK).
    /// Replaces 16-bit halfword at `hw * 16` of `regs[dst]` with `imm`.
    /// When `is_32 = true`, upper 32 bits of `dst` are zeroed.
    Movk {
        dst: u8,
        imm: u16,
        hw: u8,
        is_32: bool,
    },
    /// Added 2026-09-30 (Track GB-sysreg2): read persistent system-register
    /// state into dst. Register 31 as dst drops the value (XZR semantics).
    ReadSys { dst: u8, reg: SysReg },
    /// Added 2026-09-30 (Track GB-sysreg2): write regs[src] into persistent
    /// system-register state. Register 31 as src reads as 0.
    WriteSys { src: u8, reg: SysReg },
    /// Added 2026-09-30 (Track GB-11): atomic read-modify-write of the
    /// persistent DAIF. `daif = (daif | set) & !clr`. Backs MSR DAIFSet /
    /// MSR DAIFClr, #imm. Masks are DAIF-positioned: imm bit n targets
    /// PSTATE bit n+6 (bit3=D, bit2=A, bit1=I, bit0=F -- verified against
    /// Linux's `msr daifclr, #2` == local_irq_enable, and the measured
    /// 0xd50348ff = daifclr #0x8 clearing D after the kernel programs
    /// MDSCR_EL1).
    DaifRmw { set: u64, clr: u64 },
    /// Added 2026-09-30 (Track GB-7): count leading zeros.
    /// `dst = clz(regs[src])`: number of zero bits above the highest set bit
    /// of the 64-bit value; 64 when `src` is 0. 64-bit form only; the 32-bit
    /// form traps in the lifter (upper-bit zeroing is not expressible).
    /// Register 31 reads as 0 (XZR), so CLZ Xd, XZR yields 64.
    Clz { dst: u8, src: u8 },
    /// Added 2026-09-30 (Track GB-8): multiply-add.
    /// `dst = a + n * m` (all 64-bit). 64-bit MADD only; the 32-bit form
    /// traps in the lifter (upper-bit zeroing is not expressible).
    /// Register 31 reads as 0 (XZR), so with `a = 31` this is a plain multiply.
    Madd { dst: u8, n: u8, m: u8, a: u8 },
    /// Added 2026-09-27 (Wave 4 amendment U2-G1): wait-for-interrupt marker.
    /// The execution backend yields the vCPU until an IRQ is pending;
    /// resumable, never an error and never a silent nop.
    Wfi,
    /// Added 2026-10-03 (P0): Hypervisor call. The execution backend
    /// dispatches PSCI via the DTB-declared `hvc` method: PSCI_VERSION
    /// (0x84000000) returns 0x00010000 (v1.0); all other function IDs
    /// return PSCI_NOT_SUPPORTED (-1) in X0. Honest stub, never silent.
    Hvc,
    /// Explicit trap for unimplemented/privileged semantics. Never a silent nop.
    Trap {
        reason: &'static str,
    },
}

/// Where an IR block can go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockExit {
    FallThrough(u64),
    Branch(u64),
    /// Added 2026-09-27 (Wave 5 amendment, BL/RET scope): control leaves the
    /// block to a register-held address ([`IrOp::BranchDyn`]). No static
    /// target is declared — the execution backend reports the runtime value.
    Dynamic,
    ExitVm,
}

/// One lifted basic block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrBlock {
    pub entry_addr: u64,
    pub ops: Vec<IrOp>,
    pub exits: Vec<BlockExit>,
}

/// Deterministic WASM build of one [`IrBlock`].
/// Same input bytes → byte-identical output (contract for U3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmModule {
    pub bytes: Vec<u8>,
}

/// Explicit MMU register state. Page tables live in guest RAM; nothing hidden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmuState {
    pub ttbr0: u64,
    pub ttbr1: u64,
    pub tcr: u64,
    pub sctlr: u64,
}

/// Memory access kind for translation checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
    Execute,
}

/// Translation failure. Data, not panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemFault {
    TranslationFault { va: u64 },
    PermissionFault { va: u64 },
}

/// Explicit interrupt-controller + timer state (values only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrqState {
    pub enabled: u32,
    pub pending: u64,
    pub timer_count: u64,
    pub timer_compare: u64,
}

/// A raised interrupt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Irq {
    pub num: u32,
}

/// Decode AArch64 bitmask immediate per ARM ARM pseudocode for `DecodeBitMasks`.
///
/// Returns the decoded 64-bit mask, or `None` if the encoding is reserved/unallocated.
pub fn decode_bitmasks(n: u8, imms: u8, immr: u8, sf: bool) -> Option<u64> {
    let val = ((n as u32 & 1) << 6) | ((!imms as u32) & 0x3F);
    let mut len_bit: i32 = -1;
    for bit in (0..=6).rev() {
        if (val >> bit) & 1 == 1 {
            len_bit = bit as i32;
            break;
        }
    }
    if len_bit < 1 {
        return None;
    }
    let esize = 1u64 << len_bit;
    if !sf && n != 0 {
        return None;
    }
    if !sf && esize == 64 {
        return None;
    }
    let levels = (1u32 << len_bit) - 1;
    if (imms as u32 & levels) == levels {
        return None;
    }
    let s = (imms as u32 & levels) as usize;
    let r = (immr as u32 & levels) as usize;
    let ones = if esize == 64 && s == 63 {
        !0u64
    } else {
        (1u64 << (s + 1)) - 1
    };
    let welem = if r == 0 {
        ones
    } else {
        ((ones >> r) | (ones << (esize - r as u64)))
            & if esize == 64 {
                !0u64
            } else {
                (1u64 << esize) - 1
            }
    };
    let mut mask64 = 0u64;
    let mut i = 0;
    while i < 64 {
        mask64 |= welem << i;
        i += esize;
    }
    if !sf {
        mask64 &= 0xFFFF_FFFF;
    }
    Some(mask64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn illegal_word_is_preserved_as_data() {
        let r = DecodeResult::Illegal { word: 0xFFFF_FFFF };
        assert_eq!(r, DecodeResult::Illegal { word: 0xFFFF_FFFF });
    }

    #[test]
    fn decoded_instruction_carries_addr_and_kind() {
        let i = Instruction {
            addr: 0x4000,
            word: 0xD280_0020,
            kind: InsnKind::DataProc,
        };
        assert_eq!(i.addr, 0x4000);
        assert_eq!(i.kind, InsnKind::DataProc);
    }

    #[test]
    fn ir_block_exits_are_explicit() {
        let b = IrBlock {
            entry_addr: 0x4000,
            ops: vec![IrOp::Trap {
                reason: "unimplemented",
            }],
            exits: vec![BlockExit::ExitVm],
        };
        assert_eq!(b.exits, vec![BlockExit::ExitVm]);
    }

    #[test]
    fn mem_faults_distinguish_translation_from_permission() {
        let t = MemFault::TranslationFault { va: 0x0 };
        let p = MemFault::PermissionFault { va: 0x0 };
        assert_ne!(t, p);
    }

    #[test]
    fn mmu_state_is_plain_values() {
        let s = MmuState {
            ttbr0: 0x1000,
            ttbr1: 0,
            tcr: 0,
            sctlr: 1,
        };
        assert_eq!(s.ttbr0, 0x1000);
    }
}
