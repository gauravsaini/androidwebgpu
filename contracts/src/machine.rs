//! Machine / snapshot / APK contract types — owned by Wave 0 leaf 1.3.
//!
//! Covers U11 (snapshot), U12 (orchestrator state threading), U10 (APK pipeline).

use crate::cpu::{IrqState, MmuState, SysReg};

/// Explicit per-vCPU register state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuState {
    pub regs: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    pub pstate: u64,
    pub sysregs: SysRegs,
}

/// Persistent system-register state (Track GB-sysreg2).
/// The writable system registers whose MSR writes GB-3 accepted without
/// state, now with real architectural values. ID registers stay modeled
/// constants in the lifter; NZCV stays in pstate (GB-2 live flag path).
/// Defaults match GB-3 MRS table values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SysRegs {
    pub daif: u64,
    pub tpidr_el1: u64,
    pub sctlr_el1: u64,
    pub sctlr_el2: u64,
    pub hcr_el2: u64,
    pub cnthctl_el2: u64,
    pub cntvoff_el2: u64,
    pub vbar_el1: u64,
    pub sp_el0: u64,
    pub cpacr_el1: u64,
    pub mdscr_el1: u64,
    pub mair_el1: u64,
    pub tcr_el1: u64,
    pub ttbr0_el1: u64,
    pub ttbr1_el1: u64,
    // Generic timer (devices track, 2026-10-02): physical counter is synced
    // from IrqState.timer_count on every tick; control/compare are guest-writable.
    pub cntpct_el0: u64,
    pub cntp_ctl_el0: u64,
    pub cntp_cval_el0: u64,
    pub cntv_ctl_el0: u64,
    pub cntv_cval_el0: u64,
    /// Context ID Register, EL1 (P0, 2026-10-03): written on every context
    /// switch, no behavior needed.
    pub contextidr_el1: u64,
}

impl Default for SysRegs {
    fn default() -> Self {
        Self {
            // Linux ARM64 boot protocol: all interrupts masked at entry.
            daif: 0x3c0,
            tpidr_el1: 0,
            sctlr_el1: 0,
            sctlr_el2: 0,
            hcr_el2: 0,
            cnthctl_el2: 0,
            cntvoff_el2: 0,
            vbar_el1: 0,
            sp_el0: 0,
            // Reset value is architecturally UNKNOWN; the kernel always
            // writes CPACR_EL1 before reading it, so 0 is a safe default.
            cpacr_el1: 0,
            // Reset value is architecturally UNKNOWN; the kernel always
            // writes MDSCR_EL1 before reading it, so 0 is a safe default.
            mdscr_el1: 0,
            // Reset value is architecturally UNKNOWN; the kernel always
            // writes MAIR_EL1 before reading it, so 0 is a safe default.
            mair_el1: 0,
            // Reset value is architecturally UNKNOWN; the kernel always
            // writes TCR_EL1 before reading it, so 0 is a safe default.
            tcr_el1: 0,
            // Reset value is architecturally UNKNOWN; the kernel always
            // writes TTBR0_EL1 before reading it, so 0 is a safe default.
            ttbr0_el1: 0,
            // Reset value is architecturally UNKNOWN; the kernel always
            // writes TTBR1_EL1 before reading it, so 0 is a safe default.
            ttbr1_el1: 0,
            // Generic timer: counter starts at 0, control disabled, compare max.
            cntpct_el0: 0,
            cntp_ctl_el0: 0,
            cntp_cval_el0: u64::MAX,
            cntv_ctl_el0: 0,
            cntv_cval_el0: u64::MAX,
            contextidr_el1: 0,
        }
    }
}

impl SysRegs {
    /// Read one persistent register by selector.
    pub fn load(&self, reg: SysReg) -> u64 {
        match reg {
            SysReg::Daif => self.daif,
            SysReg::TpidrEl1 => self.tpidr_el1,
            SysReg::SctlrEl1 => self.sctlr_el1,
            SysReg::SctlrEl2 => self.sctlr_el2,
            SysReg::HcrEl2 => self.hcr_el2,
            SysReg::CnthctlEl2 => self.cnthctl_el2,
            SysReg::CntvoffEl2 => self.cntvoff_el2,
            SysReg::VbarEl1 => self.vbar_el1,
            SysReg::SpEl0 => self.sp_el0,
            SysReg::CpacrEl1 => self.cpacr_el1,
            SysReg::MdscrEl1 => self.mdscr_el1,
            SysReg::MairEl1 => self.mair_el1,
            SysReg::TcrEl1 => self.tcr_el1,
            SysReg::Ttbr0El1 => self.ttbr0_el1,
            SysReg::Ttbr1El1 => self.ttbr1_el1,
            // Generic timer (devices track): real counter and control state.
            // ISTATUS (bit 2) is read-only, computed from counter >= compare.
            SysReg::CntpctEl0 => self.cntpct_el0,
            SysReg::CntvctEl0 => self.cntpct_el0.wrapping_sub(self.cntvoff_el2),
            SysReg::CntpCtlEl0 => {
                let istatus = if self.cntpct_el0 >= self.cntp_cval_el0 { 0x4 } else { 0 };
                (self.cntp_ctl_el0 & !0x4) | istatus
            }
            SysReg::CntpCvalEl0 => self.cntp_cval_el0,
            SysReg::CntvCtlEl0 => {
                let vct = self.cntpct_el0.wrapping_sub(self.cntvoff_el2);
                let istatus = if vct >= self.cntv_cval_el0 { 0x4 } else { 0 };
                (self.cntv_ctl_el0 & !0x4) | istatus
            }
            SysReg::CntvCvalEl0 => self.cntv_cval_el0,
            SysReg::ContextidrEl1 => self.contextidr_el1,
        }
    }

    /// Write one persistent register by selector.
    pub fn store(&mut self, reg: SysReg, val: u64) {
        match reg {
            SysReg::Daif => self.daif = val,
            SysReg::TpidrEl1 => self.tpidr_el1 = val,
            SysReg::SctlrEl1 => self.sctlr_el1 = val,
            SysReg::SctlrEl2 => self.sctlr_el2 = val,
            SysReg::HcrEl2 => self.hcr_el2 = val,
            SysReg::CnthctlEl2 => self.cnthctl_el2 = val,
            SysReg::CntvoffEl2 => self.cntvoff_el2 = val,
            SysReg::VbarEl1 => self.vbar_el1 = val,
            SysReg::SpEl0 => self.sp_el0 = val,
            SysReg::CpacrEl1 => self.cpacr_el1 = val,
            SysReg::MdscrEl1 => self.mdscr_el1 = val,
            SysReg::MairEl1 => self.mair_el1 = val,
            SysReg::TcrEl1 => self.tcr_el1 = val,
            SysReg::Ttbr0El1 => self.ttbr0_el1 = val,
            SysReg::Ttbr1El1 => self.ttbr1_el1 = val,
            // Generic timer: counter is read-only (writes ignored); ISTATUS
            // bit (2) of CTL is read-only, masked out on write.
            SysReg::CntpctEl0 | SysReg::CntvctEl0 => {}
            SysReg::CntpCtlEl0 => self.cntp_ctl_el0 = val & !0x4,
            SysReg::CntpCvalEl0 => self.cntp_cval_el0 = val,
            SysReg::CntvCtlEl0 => self.cntv_ctl_el0 = val & !0x4,
            SysReg::CntvCvalEl0 => self.cntv_cval_el0 = val,
            SysReg::ContextidrEl1 => self.contextidr_el1 = val,
        }
    }

    /// Selector from the u8 host-call index. Out-of-range indices are a
    /// contract violation: the lifter only emits valid discriminants.
    pub fn from_index(idx: u8) -> Option<SysReg> {
        match idx {
            0 => Some(SysReg::Daif),
            1 => Some(SysReg::TpidrEl1),
            2 => Some(SysReg::SctlrEl1),
            3 => Some(SysReg::SctlrEl2),
            4 => Some(SysReg::HcrEl2),
            5 => Some(SysReg::CnthctlEl2),
            6 => Some(SysReg::CntvoffEl2),
            7 => Some(SysReg::VbarEl1),
            8 => Some(SysReg::SpEl0),
            9 => Some(SysReg::CpacrEl1),
            10 => Some(SysReg::MdscrEl1),
            11 => Some(SysReg::MairEl1),
            12 => Some(SysReg::TcrEl1),
            13 => Some(SysReg::Ttbr0El1),
            14 => Some(SysReg::Ttbr1El1),
            15 => Some(SysReg::CntpctEl0),
            16 => Some(SysReg::CntvctEl0),
            17 => Some(SysReg::CntpCtlEl0),
            18 => Some(SysReg::CntpCvalEl0),
            19 => Some(SysReg::CntvCtlEl0),
            20 => Some(SysReg::CntvCvalEl0),
            21 => Some(SysReg::ContextidrEl1),
            _ => None,
        }
    }
}

/// Which device a [`DeviceState`] blob belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Block,
    Net,
    Gpu,
    Input,
    Console,
}

/// Per-device state container. The `blob` is owned and versioned by the device
/// unit itself; the machine layer treats it as opaque bytes. This keeps device
/// internals out of the machine schema while keeping state explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceState {
    pub kind: DeviceKind,
    pub blob: Vec<u8>,
}

/// The full explicit machine state. Everything the snapshot (U11) persists
/// and the orchestrator (U12) threads. No hidden state anywhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineState {
    pub cpu: Vec<CpuState>,
    pub mmu: MmuState,
    pub irq: IrqState,
    /// Guest physical RAM. Contract cap: 2 GiB (WASM 4 GiB ceiling, rest is JIT cache).
    pub ram: Vec<u8>,
    pub devices: Vec<DeviceState>,
}

/// Maximum guest RAM the contract allows: 2 GiB.
pub const MAX_GUEST_RAM_BYTES: usize = 2 * 1024 * 1024 * 1024;

/// Opaque, versioned snapshot blob (U11 output).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot(pub Vec<u8>);

/// Current snapshot format version. Bump on any format change.
pub const SNAPSHOT_VERSION: u32 = 9;

/// Snapshot restore failure. Data, not panic — corrupt input never crashes the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    VersionMismatch { found: u32 },
    Corrupt,
    TooLarge,
}

/// Detected APK engine (U10 analyzer output).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Unity,
    Unreal,
    Godot,
    Other,
}

/// Static APK analysis result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApkMeta {
    pub package: String,
    pub engine: EngineKind,
    pub gles_version: (u8, u8),
}

/// APK analysis failure. Data, not panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApkError {
    BadZip,
    BadManifest(String),
    Unsupported,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_state_is_fully_explicit() {
        let s = MachineState {
            cpu: vec![CpuState {
                regs: [0; 31],
                sp: 0,
                pc: 0x4000,
                pstate: 0,
                sysregs: SysRegs::default(),
            }],
            mmu: MmuState {
                ttbr0: 0x1000,
                ttbr1: 0,
                tcr: 0,
                sctlr: 1,
            },
            irq: IrqState {
                enabled: 0,
                pending: 0,
                timer_count: 0,
                timer_compare: 0,
            },
            ram: vec![0; 4096],
            devices: vec![],
        };
        assert_eq!(s.cpu[0].pc, 0x4000);
        assert!(s.ram.len() <= MAX_GUEST_RAM_BYTES);
    }

    #[test]
    fn snapshot_version_mismatch_is_typed() {
        let e = SnapshotError::VersionMismatch { found: 99 };
        assert_ne!(e, SnapshotError::Corrupt);
    }

    #[test]
    fn apk_meta_carries_engine_and_gles() {
        let m = ApkMeta {
            package: "com.example.game".to_string(),
            engine: EngineKind::Unity,
            gles_version: (3, 1),
        };
        assert_eq!(m.engine, EngineKind::Unity);
        assert_eq!(m.gles_version, (3, 1));
    }

    #[test]
    fn timer_sysregs_roundtrip_with_istatus() {
        let mut s = SysRegs::default();
        // Counter starts at 0, compare at MAX: ISTATUS clear.
        assert_eq!(s.load(SysReg::CntpctEl0), 0);
        assert_eq!(s.load(SysReg::CntpCtlEl0) & 0x4, 0);
        // Program compare below counter: ISTATUS sets on read.
        s.cntpct_el0 = 1000;
        s.store(SysReg::CntpCvalEl0, 500);
        s.store(SysReg::CntpCtlEl0, 0x1); // ENABLE=1
        assert_eq!(s.load(SysReg::CntpCtlEl0), 0x5); // ENABLE + ISTATUS
        assert_eq!(s.load(SysReg::CntpCvalEl0), 500);
        // ISTATUS is read-only: guest write of bit 2 is masked out.
        s.store(SysReg::CntpCtlEl0, 0x7);
        assert_eq!(s.cntp_ctl_el0, 0x3); // bit 2 cleared
        assert_eq!(s.load(SysReg::CntpCtlEl0), 0x7); // but ISTATUS still set
        // Counter writes are ignored (read-only).
        s.store(SysReg::CntpctEl0, 999);
        assert_eq!(s.load(SysReg::CntpctEl0), 1000);
        // Virtual counter subtracts the offset.
        s.cntvoff_el2 = 100;
        assert_eq!(s.load(SysReg::CntvctEl0), 900);
    }
}
