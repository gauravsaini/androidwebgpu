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
