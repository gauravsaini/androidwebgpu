//! U3 WASM module shape — the JIT/host ABI (Wave 4 amendment U3-G1).
//!
//! Added 2026-09-27 (Wave 4 amendment U3-G1): U3's Wave-1 output was a pure
//! "control-flow evaluator" — 256 zeroed `i64` locals, no imports, register
//! values died with the block. That model cannot honestly execute any guest
//! with dataflow (the 5.1 orchestrator docstring deferred exactly this to
//! Wave 4). This module documents the revised shape; it is ADDITIVE (a new
//! contract module — no existing type is touched).
//!
//! ## Imports (declaration order is contract — the host must provide them in
//! this order)
//!
//! All in the `"env"` module:
//!
//! 1. `mem_load`: `func (param i64 i64) (result i64)` — `(addr, size)` reads
//!    `size ∈ {1,2,4,8}` bytes (little-endian, zero-extended) from guest
//!    physical `addr`. Serves RAM reads AND console MMIO reads: the host
//!    dispatches `CONSOLE_RX` to the next input byte (0 when empty) and other
//!    console offsets to 0. Address faults trap via a host-side error.
//! 2. `mem_store`: `func (param i64 i64 i64)` — `(addr, size, value)` writes
//!    the low `size` bytes. Serves RAM writes AND console MMIO writes: the
//!    host dispatches `CONSOLE_TX` to the console sink. Faults trap.
//! 3. `wfi`: `func ()` — wait-for-interrupt marker. The host records that
//!    the current block executed WFI; the orchestrator then yields the vCPU
//!    until an IRQ is pending (resumable, never an error).
//! 4. `r0` … `r30`: 31 × `global (mut i64)` — the live register file
//!    X0–X30. The host writes them from the checkpointed register state
//!    before the call and reads them back after. There is NO `r31`:
//!    register 31 is XZR in the lifted forms — sources compile to
//!    `i64.const 0`, destinations are dropped (never a global).
//!
//! ## Export
//!
//! `run`: `func () (result i64)` — the block body. Returns the guest address
//! of the next block (a `BlockExit` target); `-1` is the ExitVm sentinel.
//! U3 emits the export itself.
//!
//! ## Codegen rules
//!
//! - Zero locals: the operand stack carries temporaries. Deterministic output
//!   (same block → byte-identical module), as before.
//! - Function indices: `0 = mem_load`, `1 = mem_store`, `2 = wfi`,
//!   `3 = run`. Global indices: `0..=30 = r0..r30`.
//! - `IrOp::Load`/`Store` (static) still compile to `unreachable`: the static
//!   forms are not in Wave-4 scope — loudly trapped, never silently served.
//! - SP-relative forms are out of scope: there is no SP global yet. The
//!   lifter traps them before they reach codegen.
//!
//! ## Rationale (why imports, not an imported linear memory)
//!
//! Guest RAM stays host-owned (`MachineState.ram` is a frozen contract and
//! the virtio/GPU device path reads it directly). Sharing it as an imported
//! WASM memory would force a RAM-ownership migration through the frozen
//! machine contract plus every device path. Host-function services keep one
//! RAM owner, reuse the existing `ram_offset`/MMIO dispatch, and keep the
//! console byte-exact (RX consumption order is preserved because loads run
//! inline, in program order).

/// The `"env"` module name all Wave-4 imports live under.
pub const ENV_MODULE: &str = "env";

/// Import names, in contract declaration order.
pub const IMPORT_MEM_LOAD: &str = "mem_load";
pub const IMPORT_MEM_STORE: &str = "mem_store";
pub const IMPORT_WFI: &str = "wfi";

/// Register-global import name for architectural register `i` (`0..=30`).
pub fn reg_global_name(i: u8) -> Option<&'static str> {
    const NAMES: [&str; 31] = [
        "r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "r13",
        "r14", "r15", "r16", "r17", "r18", "r19", "r20", "r21", "r22", "r23", "r24", "r25", "r26",
        "r27", "r28", "r29", "r30",
    ];
    NAMES.get(i as usize).copied()
}

/// Number of register globals in the import contract (X0–X30).
pub const REG_GLOBAL_COUNT: u8 = 31;

/// Export name of the block body function.
pub const EXPORT_RUN: &str = "run";

/// Function index of `run` in a U3-G1 module (3 imported funcs precede it).
pub const RUN_FUNC_INDEX: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reg_names_cover_x0_to_x30() {
        assert_eq!(reg_global_name(0), Some("r0"));
        assert_eq!(reg_global_name(10), Some("r10"));
        assert_eq!(reg_global_name(30), Some("r30"));
        assert_eq!(reg_global_name(31), None);
        assert_eq!(REG_GLOBAL_COUNT, 31);
    }

    #[test]
    fn import_order_constants_are_stable() {
        // Declaration order is contract; pin it.
        assert_eq!(ENV_MODULE, "env");
        assert_eq!(IMPORT_MEM_LOAD, "mem_load");
        assert_eq!(IMPORT_MEM_STORE, "mem_store");
        assert_eq!(IMPORT_WFI, "wfi");
        assert_eq!(EXPORT_RUN, "run");
        assert_eq!(RUN_FUNC_INDEX, 3);
    }
}
