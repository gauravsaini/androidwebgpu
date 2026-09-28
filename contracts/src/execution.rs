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
