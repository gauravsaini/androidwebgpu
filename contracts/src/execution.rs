//! Execution-backend contract: the quarantined seam between the orchestrator
//! (U12) and the WASM engine that runs U3-emitted blocks.
//!
//! wasmtime cannot target wasm32, so the browser needs a different engine
//! (wasmi). This trait keeps U12 engine-agnostic: native keeps wasmtime, the
//! browser uses wasmi, both behind the same interface. Additive amendment —
//! no existing contract type is changed.

use crate::cpu::WasmModule;

/// Host operations a U3 module imports: guest memory, console MMIO, WFI.
/// `String` errors become WASM traps, surfaced by U12 as
/// `HaltReason::WasmTrap` — the same mapping the wasmtime backend had.
pub trait HostOps {
    fn mem_load(&mut self, addr: i64, size: i64) -> Result<i64, String>;
    fn mem_store(&mut self, addr: i64, size: i64, val: i64) -> Result<(), String>;
    /// Notification only; backends track invocation themselves for the
    /// `wfi_seen` return value.
    fn wfi(&mut self);
    // Read persistent system-register state (GB-sysreg2). reg is the
    // SysReg discriminant (contracts cpu SysReg as u8). Unknown indices
    // are a contract violation and become WASM traps.
    fn sysreg_load(&mut self, reg: u8) -> Result<i64, String>;
    // Write persistent system-register state (GB-sysreg2).
    fn sysreg_store(&mut self, reg: u8, val: i64) -> Result<(), String>;
    /// PSCI hypervisor call (P0, 2026-10-03). func_id is X0 at HVC entry.
    /// Returns the PSCI result for X0: version for PSCI_VERSION,
    /// PSCI_NOT_SUPPORTED (-1) for all other function IDs.
    fn hvc(&mut self, func_id: i64) -> Result<i64, String>;
}

/// Executes one U3 block: checkpoint the 31 registers in, call the `run`
/// export, read the registers back. Returns `(exit_addr, wfi_seen)`.
///
/// Implementations own a compile cache keyed by module **bytes**: U3's
/// `compile` is pure and deterministic, so bytes are a valid key — strictly
/// more correct than pc-keying under self-modifying code (a changed block is
/// simply a new key).
pub trait BlockExecutor {
    fn run_block(
        &mut self,
        wasm: &WasmModule,
        regs: &mut [u64; 31],
        host: &mut dyn HostOps,
    ) -> Result<(i64, bool), String>;
    /// Number of distinct compiled blocks currently cached.
    fn cache_len(&self) -> usize;
    /// Drop all cached compilations (self-modifying code path).
    fn invalidate(&mut self);
}
