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

/// Single IR operation (SSA-style). The lifter (U2) is the only producer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrOp {
    Add {
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
