//! U15-exec-wasmi: the portable [`BlockExecutor`] backend (wasmi 2).
//!
//! Pure-Rust interpreter — the only engine here that runs on
//! `wasm32-unknown-unknown`, which is why the browser host uses it. Same
//! module shape (U3-G1), same import contract, same trap semantics as the
//! wasmtime backend; only the engine differs.
//!
//! Import contract (U3-G1), mirrored from the wasmtime backend:
//! - `env.mem_load(i64, i64) -> i64`, `env.mem_store(i64, i64, i64) -> ()`,
//!   `env.wfi() -> ()`
//! - 31 mutable `i64` globals `env.r0`..`env.r30` (checkpoint regs in, read
//!   them back out after the call)
//! - exported `run() -> i64`; its return value IS the exit address.
//!
//! Host-side [`HostOps`] errors are wrapped in [`HostMsg`] and mapped to
//! `wasmi::Error` at the import boundary so traps keep their messages; the
//! backend returns `Err("trap: ...")` on any trap, exactly like the
//! wasmtime backend.

use pathn_contracts::cpu::WasmModule;
use pathn_contracts::execution::{BlockExecutor, HostOps};
use std::collections::HashMap;
use wasmi::errors::HostError;
use wasmi::{Caller, Engine, Error, Global, Linker, Module, Mutability, Store, Val};

/// Portable wasmi-backed executor (native + wasm32). Owns the engine and
/// the bytes-keyed compile cache.
pub struct WasmiExecutor {
    engine: Engine,
    cache: HashMap<Vec<u8>, Module>,
}

impl WasmiExecutor {
    pub fn new() -> Self {
        Self {
            engine: Engine::default(),
            cache: HashMap::new(),
        }
    }

    fn compile(&mut self, wasm: &WasmModule) -> Result<Module, String> {
        if let Some(m) = self.cache.get(&wasm.bytes) {
            return Ok(m.clone());
        }
        let module =
            Module::new(&self.engine, &wasm.bytes[..]).map_err(|e| format!("module: {e}"))?;
        self.cache.insert(wasm.bytes.clone(), module.clone());
        Ok(module)
    }
}

impl Default for WasmiExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Wraps a host-side [`HostOps`] `String` error as a wasmi host error so the
/// trap message survives the import boundary.
#[derive(Debug)]
struct HostMsg(String);

impl std::fmt::Display for HostMsg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for HostMsg {}

impl HostError for HostMsg {}

/// The store data: borrows the orchestrator's [`HostOps`] for exactly one
/// block execution and records whether the `wfi` import fired.
struct TrackingHost<'a> {
    ops: &'a mut dyn HostOps,
    wfi_seen: bool,
}

impl BlockExecutor for WasmiExecutor {
    fn run_block(
        &mut self,
        wasm: &WasmModule,
        regs: &mut [u64; 31],
        host: &mut dyn HostOps,
    ) -> Result<(i64, bool), String> {
        let module = self.compile(wasm)?;
        let mut store = Store::new(
            &self.engine,
            TrackingHost {
                ops: host,
                wfi_seen: false,
            },
        );
        let mut linker: Linker<TrackingHost> = Linker::new(&self.engine);
        linker
            .func_wrap(
                "env",
                "mem_load",
                |mut caller: Caller<'_, TrackingHost>, addr: i64, size: i64| {
                    caller
                        .data_mut()
                        .ops
                        .mem_load(addr, size)
                        .map_err(|e| Error::host(HostMsg(e)))
                },
            )
            .map_err(|e| format!("link mem_load: {e}"))?;
        linker
            .func_wrap(
                "env",
                "mem_store",
                |mut caller: Caller<'_, TrackingHost>, addr: i64, size: i64, val: i64| {
                    caller
                        .data_mut()
                        .ops
                        .mem_store(addr, size, val)
                        .map_err(|e| Error::host(HostMsg(e)))
                },
            )
            .map_err(|e| format!("link mem_store: {e}"))?;
        linker
            .func_wrap("env", "wfi", |mut caller: Caller<'_, TrackingHost>| {
                caller.data_mut().wfi_seen = true;
                caller.data_mut().ops.wfi();
                Ok(())
            })
            .map_err(|e| format!("link wfi: {e}"))?;
        linker
            .func_wrap(
                "env",
                "sysreg_load",
                |mut caller: Caller<'_, TrackingHost>, reg: i64, _pad: i64| {
                    let idx = u8::try_from(reg)
                        .map_err(|_| Error::host(HostMsg(format!("sysreg_load: bad index"))))?;
                    caller
                        .data_mut()
                        .ops
                        .sysreg_load(idx)
                        .map_err(|e| Error::host(HostMsg(e)))
                },
            )
            .map_err(|e| format!("link sysreg_load: {e}"))?;
        linker
            .func_wrap(
                "env",
                "sysreg_store",
                |mut caller: Caller<'_, TrackingHost>, reg: i64, val: i64| {
                    let idx = u8::try_from(reg)
                        .map_err(|_| Error::host(HostMsg(format!("sysreg_store: bad index"))))?;
                    caller
                        .data_mut()
                        .ops
                        .sysreg_store(idx, val)
                        .map_err(|e| Error::host(HostMsg(e)))
                },
            )
            .map_err(|e| format!("link sysreg_store: {e}"))?;
        linker
            .func_wrap(
                "env",
                "hvc",
                |mut caller: Caller<'_, TrackingHost>, func_id: i64| {
                    caller
                        .data_mut()
                        .ops
                        .hvc(func_id)
                        .map_err(|e| Error::host(HostMsg(e)))
                },
            )
            .map_err(|e| format!("link hvc: {e}"))?;
        // Register file: checkpointed X0-X30 in, mutated X0-X30 out.
        let mut globals = Vec::with_capacity(31);
        for (i, reg) in regs.iter().enumerate() {
            let g = Global::new(&mut store, Val::I64(*reg as i64), Mutability::Var);
            linker
                .define("env", &format!("r{i}"), g)
                .map_err(|e| format!("define r{i}: {e}"))?;
            globals.push(g);
        }

        // wasmi 2 folds plain `instantiate` into `instantiate_and_start`;
        // U3 modules declare no start function, so the run phase is a no-op.
        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .map_err(|e| format!("instantiate: {e}"))?;
        let run = instance
            .get_typed_func::<(), i64>(&store, "run")
            .map_err(|e| format!("export 'run': {e}"))?;
        let exit_addr = run.call(&mut store, ()).map_err(|e| format!("trap: {e}"))?;
        for (i, g) in globals.iter().enumerate() {
            if let Val::I64(v) = g.get(&store) {
                regs[i] = v as u64;
            }
        }
        let wfi_seen = store.data().wfi_seen;
        Ok((exit_addr, wfi_seen))
    }

    fn cache_len(&self) -> usize {
        self.cache.len()
    }

    fn invalidate(&mut self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pathn_contracts::cpu::{BlockExit, IrBlock, IrOp, SysReg};
    use pathn_contracts::machine::SysRegs;

    /// Minimal host: a flat RAM with bounds checking. Panics are fine —
    /// these tests never trigger unexpected calls by construction.
    struct RamHost {
        ram: Vec<u8>,
        wfi_calls: u32,
        sysregs: SysRegs,
    }

    impl RamHost {
        fn new(size: usize) -> Self {
            Self {
                ram: vec![0; size],
                wfi_calls: 0,
                sysregs: SysRegs::default(),
            }
        }
    }

    impl HostOps for RamHost {
        fn mem_load(&mut self, addr: i64, size: i64) -> Result<i64, String> {
            let (a, s) = (addr as usize, size as usize);
            let bytes = self
                .ram
                .get(a..a + s)
                .ok_or_else(|| format!("load OOB {addr}#{size}"))?;
            let mut val = 0i64;
            for (i, b) in bytes.iter().enumerate() {
                val |= (*b as i64) << (8 * i);
            }
            Ok(val)
        }

        fn mem_store(&mut self, addr: i64, size: i64, val: i64) -> Result<(), String> {
            let (a, s) = (addr as usize, size as usize);
            let slot = self
                .ram
                .get_mut(a..a + s)
                .ok_or_else(|| format!("store OOB {addr}#{size}"))?;
            for (i, b) in slot.iter_mut().enumerate() {
                *b = ((val >> (8 * i)) & 0xff) as u8;
            }
            Ok(())
        }

        fn wfi(&mut self) {
            self.wfi_calls += 1;
        }

        fn sysreg_load(&mut self, reg: u8) -> Result<i64, String> {
            let sel = SysRegs::from_index(reg)
                .ok_or_else(|| format!("sysreg_load: bad index {reg}"))?;
            Ok(self.sysregs.load(sel) as i64)
        }

        fn sysreg_store(&mut self, reg: u8, val: i64) -> Result<(), String> {
            let sel = SysRegs::from_index(reg)
                .ok_or_else(|| format!("sysreg_store: bad index {reg}"))?;
            self.sysregs.store(sel, val as u64);
            Ok(())
        }

        fn hvc(&mut self, func_id: i64) -> Result<i64, String> {
            // Test stub mirrors the orchestrator's PSCI dispatch.
            const PSCI_VERSION: u64 = 0x8400_0000;
            if func_id as u64 == PSCI_VERSION {
                Ok(0x0001_0000)
            } else {
                Ok(-1)
            }
        }
    }

    fn compile(ops: Vec<IrOp>, exits: Vec<BlockExit>) -> WasmModule {
        u3_wasm_jit::compile(&IrBlock {
            entry_addr: 0,
            ops,
            exits,
        })
    }

    #[test]
    fn backend_runs_u3_block() {
        let wasm = compile(
            vec![
                IrOp::Mov { dst: 0, imm: 42 },
                IrOp::Branch { target: 0x100 },
            ],
            vec![BlockExit::Branch(0x100)],
        );
        let mut exe = WasmiExecutor::new();
        let mut regs = [0u64; 31];
        let mut host = RamHost::new(0x1000);

        let (exit, wfi) = exe.run_block(&wasm, &mut regs, &mut host).unwrap();
        assert_eq!(exit, 0x100);
        assert!(!wfi);
        assert_eq!(regs[0], 42);
        assert_eq!(exe.cache_len(), 1);

        // Same bytes again: compile cache hit, no second compilation.
        let (exit2, wfi2) = exe.run_block(&wasm, &mut regs, &mut host).unwrap();
        assert_eq!(exit2, 0x100);
        assert!(!wfi2);
        assert_eq!(exe.cache_len(), 1);
    }

    #[test]
    fn backend_umulh_values() {
        // UMULH: dst = ((n as u128 * m as u128) >> 64) as u64.
        // Verified against Rust u128 across edge cases.
        let cases: &[(u64, u64)] = &[
            (0, 0),
            (1, 1),
            (u64::MAX, 1),
            (u64::MAX, u64::MAX),
            (0xFFFFFFFFFFFFFFFF, 0xFFFFFFFFFFFFFFFF),
            (0x123456789ABCDEF0, 0xFEDCBA9876543210),
            (1 << 63, 2),
            (1 << 32, 1 << 32),
        ];
        for &(a, b) in cases {
            let wasm = compile(
                vec![
                    IrOp::Mov { dst: 0, imm: a },
                    IrOp::Mov { dst: 1, imm: b },
                    IrOp::Umulh { dst: 2, n: 0, m: 1 },
                    IrOp::Branch { target: 0x100 },
                ],
                vec![BlockExit::Branch(0x100)],
            );
            let mut exe = WasmiExecutor::new();
            let mut regs = [0u64; 31];
            let mut host = RamHost::new(0x1000);
            exe.run_block(&wasm, &mut regs, &mut host).unwrap();
            let expected = ((a as u128 * b as u128) >> 64) as u64;
            assert_eq!(regs[2], expected, "umulh({a:#x}, {b:#x})");
        }
    }

    #[test]
    fn backend_smulh_values() {
        // SMULH: dst = ((n as i128 * m as i128) >> 64) as u64 (arithmetic).
        let cases: &[(u64, u64)] = &[
            (0, 0),
            (1, 1),
            (u64::MAX, 1), // -1 * 1 = -1 -> high = all 1s
            (u64::MAX, u64::MAX), // (-1)*(-1) = 1 -> high = 0
            (1 << 63, 1 << 63), // (-2^63)*(-2^63) = 2^126 -> high = 2^62
            (0x8000000000000000, 2), // (-2^63)*2 = -2^64 -> high = -1
        ];
        for &(a, b) in cases {
            let wasm = compile(
                vec![
                    IrOp::Mov { dst: 0, imm: a },
                    IrOp::Mov { dst: 1, imm: b },
                    IrOp::Smulh { dst: 2, n: 0, m: 1 },
                    IrOp::Branch { target: 0x100 },
                ],
                vec![BlockExit::Branch(0x100)],
            );
            let mut exe = WasmiExecutor::new();
            let mut regs = [0u64; 31];
            let mut host = RamHost::new(0x1000);
            exe.run_block(&wasm, &mut regs, &mut host).unwrap();
            let expected = ((a as i64 as i128 * b as i64 as i128) >> 64) as u64;
            assert_eq!(regs[2], expected, "smulh({a:#x}, {b:#x})");
        }
    }

    #[test]
    fn backend_umull_values() {
        // UMULL: dst = (n as u32 as u64) * (m as u32 as u64).
        let cases: &[(u64, u64)] = &[
            (0, 0),
            (1, 1),
            (u32::MAX as u64, 1),
            (u32::MAX as u64, u32::MAX as u64), // max 32-bit product
            (0x1_0000_0001, 0x1_0000_0001), // upper bits ignored, low 32 bits = 1
            (0xFFFF_FFFF_FFFF_FFFF, 0xFFFF_FFFF_FFFF_FFFF), // upper bits ignored
        ];
        for &(a, b) in cases {
            let wasm = compile(
                vec![
                    IrOp::Mov { dst: 0, imm: a },
                    IrOp::Mov { dst: 1, imm: b },
                    IrOp::Umull { dst: 2, n: 0, m: 1 },
                    IrOp::Branch { target: 0x100 },
                ],
                vec![BlockExit::Branch(0x100)],
            );
            let mut exe = WasmiExecutor::new();
            let mut regs = [0u64; 31];
            let mut host = RamHost::new(0x1000);
            exe.run_block(&wasm, &mut regs, &mut host).unwrap();
            let expected = (a as u32 as u64).wrapping_mul(b as u32 as u64);
            assert_eq!(regs[2], expected, "umull({a:#x}, {b:#x})");
        }
    }

    #[test]
    fn backend_smull_values() {
        // SMULL: dst = (n as i32 as i64) * (m as i32 as i64).
        let cases: &[(u64, u64)] = &[
            (0, 0),
            (1, 1),
            (0xFFFF_FFFF, 1), // -1 * 1 = -1 (as u64)
            (0xFFFF_FFFF, 0xFFFF_FFFF), // (-1)*(-1) = 1
            (0x8000_0000, 2), // (-2^31)*2 = -2^32 (as u64)
            (0x1_FFFF_FFFF, 0x1_FFFF_FFFF), // upper bits ignored, low = -1
        ];
        for &(a, b) in cases {
            let wasm = compile(
                vec![
                    IrOp::Mov { dst: 0, imm: a },
                    IrOp::Mov { dst: 1, imm: b },
                    IrOp::Smull { dst: 2, n: 0, m: 1 },
                    IrOp::Branch { target: 0x100 },
                ],
                vec![BlockExit::Branch(0x100)],
            );
            let mut exe = WasmiExecutor::new();
            let mut regs = [0u64; 31];
            let mut host = RamHost::new(0x1000);
            exe.run_block(&wasm, &mut regs, &mut host).unwrap();
            let expected = (a as u32 as i32 as i64).wrapping_mul(b as u32 as i32 as i64) as u64;
            assert_eq!(regs[2], expected, "smull({a:#x}, {b:#x})");
        }
    }

    #[test]
    fn backend_smaddl_values() {
        // SMADDL: dst = a + (n as i32 as i64) * (m as i32 as i64).
        let cases: &[(u64, u64, u64)] = &[
            (0, 0, 0),
            (10, 3, 4),       // 10 + 3*4 = 22
            (0, 0xFFFF_FFFF, 0xFFFF_FFFF), // 0 + (-1)*(-1) = 1
            (5, 0x8000_0000, 2), // 5 + (-2^31)*2 = 5 - 2^32
            (0xFFFF_FFFF_FFFF_FFFF, 1, 1), // -1 + 1 = 0 (wrapping)
        ];
        for &(acc, n, m) in cases {
            let wasm = compile(
                vec![
                    IrOp::Mov { dst: 0, imm: n },
                    IrOp::Mov { dst: 1, imm: m },
                    IrOp::Mov { dst: 3, imm: acc },
                    IrOp::Smaddl { dst: 2, n: 0, m: 1, a: 3 },
                    IrOp::Branch { target: 0x100 },
                ],
                vec![BlockExit::Branch(0x100)],
            );
            let mut exe = WasmiExecutor::new();
            let mut regs = [0u64; 31];
            let mut host = RamHost::new(0x1000);
            exe.run_block(&wasm, &mut regs, &mut host).unwrap();
            let expected = acc.wrapping_add(
                (n as u32 as i32 as i64).wrapping_mul(m as u32 as i32 as i64) as u64
            );
            assert_eq!(regs[2], expected, "smaddl({acc:#x}, {n:#x}, {m:#x})");
        }
    }

    #[test]
    fn backend_smsubl_values() {
        // SMSUBL: dst = a - (n as i32 as i64) * (m as i32 as i64).
        let cases: &[(u64, u64, u64)] = &[
            (0, 0, 0),
            (10, 3, 4),       // 10 - 3*4 = -2 (wrapping)
            (0, 0xFFFF_FFFF, 0xFFFF_FFFF), // 0 - (-1)*(-1) = -1 (wrapping)
        ];
        for &(acc, n, m) in cases {
            let wasm = compile(
                vec![
                    IrOp::Mov { dst: 0, imm: n },
                    IrOp::Mov { dst: 1, imm: m },
                    IrOp::Mov { dst: 3, imm: acc },
                    IrOp::Smsubl { dst: 2, n: 0, m: 1, a: 3 },
                    IrOp::Branch { target: 0x100 },
                ],
                vec![BlockExit::Branch(0x100)],
            );
            let mut exe = WasmiExecutor::new();
            let mut regs = [0u64; 31];
            let mut host = RamHost::new(0x1000);
            exe.run_block(&wasm, &mut regs, &mut host).unwrap();
            let expected = acc.wrapping_sub(
                (n as u32 as i32 as i64).wrapping_mul(m as u32 as i32 as i64) as u64
            );
            assert_eq!(regs[2], expected, "smsubl({acc:#x}, {n:#x}, {m:#x})");
        }
    }

    #[test]
    fn backend_umaddl_values() {
        // UMADDL: dst = a + (n as u32 as u64) * (m as u32 as u64).
        let cases: &[(u64, u64, u64)] = &[
            (0, 0, 0),
            (10, 3, 4),       // 10 + 12 = 22
            (0, 0xFFFF_FFFF, 0xFFFF_FFFF), // (2^32-1)^2
            (1, 0xFFFF_FFFF, 2), // 1 + 2*(2^32-1)
        ];
        for &(acc, n, m) in cases {
            let wasm = compile(
                vec![
                    IrOp::Mov { dst: 0, imm: n },
                    IrOp::Mov { dst: 1, imm: m },
                    IrOp::Mov { dst: 3, imm: acc },
                    IrOp::Umaddl { dst: 2, n: 0, m: 1, a: 3 },
                    IrOp::Branch { target: 0x100 },
                ],
                vec![BlockExit::Branch(0x100)],
            );
            let mut exe = WasmiExecutor::new();
            let mut regs = [0u64; 31];
            let mut host = RamHost::new(0x1000);
            exe.run_block(&wasm, &mut regs, &mut host).unwrap();
            let expected = acc.wrapping_add(
                (n as u32 as u64).wrapping_mul(m as u32 as u64)
            );
            assert_eq!(regs[2], expected, "umaddl({acc:#x}, {n:#x}, {m:#x})");
        }
    }

    #[test]
    fn backend_umsubl_values() {
        // UMSUBL: dst = a - (n as u32 as u64) * (m as u32 as u64).
        let cases: &[(u64, u64, u64)] = &[
            (100, 3, 4),      // 100 - 12 = 88
            (0, 1, 1),         // 0 - 1 = MAX (wrapping)
        ];
        for &(acc, n, m) in cases {
            let wasm = compile(
                vec![
                    IrOp::Mov { dst: 0, imm: n },
                    IrOp::Mov { dst: 1, imm: m },
                    IrOp::Mov { dst: 3, imm: acc },
                    IrOp::Umsubl { dst: 2, n: 0, m: 1, a: 3 },
                    IrOp::Branch { target: 0x100 },
                ],
                vec![BlockExit::Branch(0x100)],
            );
            let mut exe = WasmiExecutor::new();
            let mut regs = [0u64; 31];
            let mut host = RamHost::new(0x1000);
            exe.run_block(&wasm, &mut regs, &mut host).unwrap();
            let expected = acc.wrapping_sub(
                (n as u32 as u64).wrapping_mul(m as u32 as u64)
            );
            assert_eq!(regs[2], expected, "umsubl({acc:#x}, {n:#x}, {m:#x})");
        }
    }

    #[test]
    fn backend_sysreg_roundtrip() {
        // MSR then MRS across SCTLR_EL1, TPIDR_EL1, DAIF, CNTHCTL_EL2.
        let wasm = compile(
            vec![
                IrOp::ReadSys { dst: 5, reg: SysReg::Daif },
                IrOp::Mov { dst: 1, imm: 0xdead },
                IrOp::WriteSys { src: 1, reg: SysReg::SctlrEl1 },
                IrOp::WriteSys { src: 1, reg: SysReg::TpidrEl1 },
                IrOp::WriteSys { src: 1, reg: SysReg::Daif },
                IrOp::WriteSys { src: 1, reg: SysReg::CnthctlEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::CpacrEl1 },
                IrOp::WriteSys { src: 1, reg: SysReg::MdscrEl1 },
                IrOp::WriteSys { src: 1, reg: SysReg::MairEl1 },
                IrOp::WriteSys { src: 1, reg: SysReg::TcrEl1 },
                IrOp::WriteSys { src: 1, reg: SysReg::Ttbr0El1 },
                IrOp::WriteSys { src: 1, reg: SysReg::Ttbr1El1 },
                // P3 (2026-10-03): GICv3 CPU interface + PMU + timers +
                // FP/SIMD + debug round-trip through the host sysregs.
                IrOp::WriteSys { src: 1, reg: SysReg::IccPmrEl1 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccCtlrEl1 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccIgrpen1El1 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccSreEl1 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccEoir1El1 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccDirEl1 },
                IrOp::WriteSys { src: 1, reg: SysReg::PmcntEnClrEl0 },
                IrOp::WriteSys { src: 1, reg: SysReg::PmovsclrEl0 },
                IrOp::WriteSys { src: 1, reg: SysReg::PmxevtyperEl0 },
                IrOp::WriteSys { src: 1, reg: SysReg::PmxevcntrEl0 },
                IrOp::WriteSys { src: 1, reg: SysReg::PmuserenrEl0 },
                IrOp::WriteSys { src: 1, reg: SysReg::CntkctlEl1 },
                IrOp::WriteSys { src: 1, reg: SysReg::TpidrEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::Fpcr },
                IrOp::WriteSys { src: 1, reg: SysReg::Fpsr },
                IrOp::WriteSys { src: 1, reg: SysReg::OsdlrEl1 },
                // P4 (2026-10-03): debug / EL2 / GIC / misc round-trip through the host sysregs.
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr0 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr1 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr2 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr3 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr4 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr5 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr6 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr7 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr8 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr9 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr10 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr11 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr12 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr13 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr14 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbvr15 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr0 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr1 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr2 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr3 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr4 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr5 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr6 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr7 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr8 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr9 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr10 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr11 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr12 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr13 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr14 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgbcr15 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr0 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr1 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr2 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr3 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr4 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr5 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr6 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr7 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr8 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr9 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr10 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr11 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr12 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr13 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr14 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwvr15 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr0 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr1 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr2 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr3 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr4 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr5 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr6 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr7 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr8 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr9 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr10 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr11 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr12 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr13 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr14 },
                IrOp::WriteSys { src: 1, reg: SysReg::Dbgwcr15 },
                IrOp::WriteSys { src: 1, reg: SysReg::VpidrEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::VmpidrEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::CptrEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::MdcrEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::HstrEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::ZcrEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::VbarEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::IchHcrEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::VttbrEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::SpsrEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::ElrEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::PmscrEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccSreEl2 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccBpr1El1 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccAp0r0El1 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccAp0r1El1 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccAp0r2El1 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccAp0r3El1 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccAp1r0El1 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccAp1r1El1 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccAp1r2El1 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccAp1r3El1 },
                IrOp::WriteSys { src: 1, reg: SysReg::IccSgi1rEl1 },
                IrOp::WriteSys { src: 1, reg: SysReg::DisrEl1 },
                IrOp::WriteSys { src: 1, reg: SysReg::LorcEl1 },
                IrOp::WriteSys { src: 1, reg: SysReg::PmccntrEl0 },
                IrOp::WriteSys { src: 1, reg: SysReg::ZcrEl1 },
                IrOp::ReadSys { dst: 2, reg: SysReg::SctlrEl1 },
                IrOp::ReadSys { dst: 3, reg: SysReg::TpidrEl1 },
                IrOp::ReadSys { dst: 4, reg: SysReg::CnthctlEl2 },
                IrOp::ReadSys { dst: 6, reg: SysReg::CpacrEl1 },
                IrOp::ReadSys { dst: 7, reg: SysReg::MdscrEl1 },
                IrOp::ReadSys { dst: 8, reg: SysReg::MairEl1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::TcrEl1 },
                IrOp::ReadSys { dst: 10, reg: SysReg::Ttbr0El1 },
                IrOp::ReadSys { dst: 11, reg: SysReg::Ttbr1El1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr0 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr3 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr4 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr5 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr6 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr7 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr8 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr9 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr10 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr11 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr12 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr13 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr14 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbvr15 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr0 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr3 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr4 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr5 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr6 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr7 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr8 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr9 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr10 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr11 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr12 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr13 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr14 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgbcr15 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr0 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr3 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr4 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr5 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr6 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr7 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr8 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr9 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr10 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr11 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr12 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr13 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr14 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwvr15 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr0 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr3 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr4 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr5 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr6 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr7 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr8 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr9 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr10 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr11 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr12 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr13 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr14 },
                IrOp::ReadSys { dst: 9, reg: SysReg::Dbgwcr15 },
                IrOp::ReadSys { dst: 9, reg: SysReg::VpidrEl2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::VmpidrEl2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::CptrEl2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::MdcrEl2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::HstrEl2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::ZcrEl2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::VbarEl2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::IchHcrEl2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::VttbrEl2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::SpsrEl2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::ElrEl2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::PmscrEl2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::IccSreEl2 },
                IrOp::ReadSys { dst: 9, reg: SysReg::IccBpr1El1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::IccAp0r0El1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::IccAp0r1El1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::IccAp0r2El1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::IccAp0r3El1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::IccAp1r0El1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::IccAp1r1El1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::IccAp1r2El1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::IccAp1r3El1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::DisrEl1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::LorcEl1 },
                IrOp::ReadSys { dst: 9, reg: SysReg::PmccntrEl0 },
                IrOp::ReadSys { dst: 9, reg: SysReg::ZcrEl1 },
                IrOp::Branch { target: 0x10 },
            ],
            vec![BlockExit::Branch(0x10)],
        );
        let mut exe = WasmiExecutor::new();
        let mut regs = [0u64; 31];
        let mut host = RamHost::new(0x1000);
        let (exit, _) = exe.run_block(&wasm, &mut regs, &mut host).unwrap();
        assert_eq!(exit, 0x10);
        // DAIF default (Linux ARM64 boot protocol) read before the store.
        assert_eq!(regs[5], 0x3c0);
        // Stored values round-trip through the host.
        assert_eq!(regs[2], 0xdead);
        assert_eq!(regs[3], 0xdead);
        assert_eq!(regs[4], 0xdead);
        assert_eq!(host.sysregs.sctlr_el1, 0xdead);
        assert_eq!(host.sysregs.tpidr_el1, 0xdead);
        assert_eq!(host.sysregs.daif, 0xdead);
        assert_eq!(host.sysregs.cnthctl_el2, 0xdead);
        // CPACR_EL1 (GB-9, host-call index 9) round-trips through the host.
        assert_eq!(regs[6], 0xdead);
        assert_eq!(host.sysregs.cpacr_el1, 0xdead);
        // MDSCR_EL1 (GB-10, host-call index 10) round-trips through the host.
        assert_eq!(regs[7], 0xdead);
        assert_eq!(host.sysregs.mdscr_el1, 0xdead);
        // MAIR_EL1 (GB-13, host-call index 11) round-trips through the host.
        assert_eq!(regs[8], 0xdead);
        assert_eq!(host.sysregs.mair_el1, 0xdead);
        // TCR_EL1 (GB-17, host-call index 12) round-trips through the host.
        assert_eq!(regs[9], 0xdead);
        assert_eq!(host.sysregs.tcr_el1, 0xdead);
        // TTBR0_EL1 (GB-18, host-call index 13) round-trips through the host.
        assert_eq!(regs[10], 0xdead);
        assert_eq!(host.sysregs.ttbr0_el1, 0xdead);
        // TTBR1_EL1 (GB-19, host-call index 14) round-trips through the host.
        assert_eq!(regs[11], 0xdead);
        assert_eq!(host.sysregs.ttbr1_el1, 0xdead);
        // P3 (host-call indices 27..=44): the new registers store and read
        // back through the same host-call path the kernel will use.
        assert_eq!(host.sysregs.icc_pmr_el1, 0xdead);
        assert_eq!(host.sysregs.icc_ctlr_el1, 0xdead);
        assert_eq!(host.sysregs.icc_igrpen1_el1, 0xdead);
        assert_eq!(host.sysregs.icc_sre_el1, 0xdead);
        assert_eq!(host.sysregs.icc_eoir1_el1, 0xdead);
        assert_eq!(host.sysregs.icc_dir_el1, 0xdead);
        assert_eq!(host.sysregs.pmcnt_enclr_el0, 0xdead);
        assert_eq!(host.sysregs.pmovsclr_el0, 0xdead);
        assert_eq!(host.sysregs.pmxevtyper_el0, 0xdead);
        assert_eq!(host.sysregs.pmxevcntr_el0, 0xdead);
        assert_eq!(host.sysregs.pmuserenr_el0, 0xdead);
        assert_eq!(host.sysregs.cntkctl_el1, 0xdead);
        assert_eq!(host.sysregs.tpidr_el2, 0xdead);
        assert_eq!(host.sysregs.fpcr, 0xdead);
        assert_eq!(host.sysregs.fpsr, 0xdead);
        assert_eq!(host.sysregs.osdlr_el1, 0xdead);
        // P4 (host-call indices 45..=135): spot-check the new registers
        // round-trip through the same host-call path the kernel will use.
        assert_eq!(host.sysregs.zcr_el1, 0xdead);
        assert_eq!(host.sysregs.icc_sre_el2, 0xdead);
        assert_eq!(host.sysregs.pmccntr_el0, 0xdead);
        assert_eq!(host.sysregs.dbg_bvr[7], 0xdead);
        assert_eq!(host.sysregs.dbg_bcr[0], 0xdead);
        assert_eq!(host.sysregs.dbg_wvr[15], 0xdead);
        assert_eq!(host.sysregs.dbg_wcr[15], 0xdead);
        assert_eq!(host.sysregs.icc_ap0r[2], 0xdead);
        assert_eq!(host.sysregs.icc_ap1r[3], 0xdead);
        assert_eq!(host.sysregs.icc_bpr1_el1, 0xdead);
        assert_eq!(host.sysregs.icc_sgi1r_el1, 0xdead);
        assert_eq!(host.sysregs.vbar_el2, 0xdead);
        assert_eq!(host.sysregs.cptr_el2, 0xdead);
        assert_eq!(host.sysregs.elr_el2, 0xdead);
        assert_eq!(host.sysregs.disr_el1, 0xdead);
        assert_eq!(host.sysregs.lorc_el1, 0xdead);
        // The last ReadSys (ZcrEl1) left 0xdead in x9.
        assert_eq!(regs[9], 0xdead);
    }

    #[test]
    fn backend_p3_tval_derives_from_cval() {
        // P3 slice D: CNTP_TVAL_EL0 is the honest CVAL alias. Pre-load the
        // host's compare, then MSR TVAL and read it back through the host
        // calls the compiled block will use.
        let mut exe = WasmiExecutor::new();
        let mut regs = [0u64; 31];
        let mut host = RamHost::new(0x1000);
        host.sysregs.cntpct_el0 = 1_000_000;
        host.sysregs.cntp_cval_el0 = 2_000_000;
        // Read TVAL: expect 1_000_000 (low 32 bits of 2M − 1M).
        let wasm = compile(
            vec![
                IrOp::ReadSys {
                    dst: 0,
                    reg: SysReg::CntpTvalEl0,
                },
                IrOp::Branch { target: 0x10 },
            ],
            vec![BlockExit::Branch(0x10)],
        );
        exe.run_block(&wasm, &mut regs, &mut host).unwrap();
        assert_eq!(regs[0], 1_000_000);
        // Write TVAL = 500_000: CVAL becomes counter + 500_000.
        let wasm = compile(
            vec![
                IrOp::Mov { dst: 1, imm: 500_000 },
                IrOp::WriteSys {
                    src: 1,
                    reg: SysReg::CntpTvalEl0,
                },
                IrOp::Branch { target: 0x10 },
            ],
            vec![BlockExit::Branch(0x10)],
        );
        exe.run_block(&wasm, &mut regs, &mut host).unwrap();
        assert_eq!(host.sysregs.cntp_cval_el0, 1_500_000);
    }

    #[test]
    fn backend_p3_gic_defaults() {
        // P3 slice A: ICC_SRE_EL1 defaults to 1 (sysreg path) and
        // ICC_PMR_EL1 to 0 (all masked) in a fresh host.
        let wasm = compile(
            vec![
                IrOp::ReadSys {
                    dst: 0,
                    reg: SysReg::IccSreEl1,
                },
                IrOp::ReadSys {
                    dst: 1,
                    reg: SysReg::IccPmrEl1,
                },
                IrOp::Branch { target: 0x10 },
            ],
            vec![BlockExit::Branch(0x10)],
        );
        let mut exe = WasmiExecutor::new();
        let mut regs = [0u64; 31];
        let mut host = RamHost::new(0x1000);
        exe.run_block(&wasm, &mut regs, &mut host).unwrap();
        assert_eq!(regs[0], 0x1);
        assert_eq!(regs[1], 0x0);
    }

    #[test]
    fn backend_daif_rmw() {
        // GB-11: MSR DAIFClr, #0x8 (measured step 7461) clears D from the
        // persistent DAIF (default 0x3c0 -> 0x1c0); a following op with
        // set=0x80, clr=0x340 then yields (0x1c0 | 0x80) & !0x340 = 0x80.
        let wasm = compile(
            vec![
                IrOp::DaifRmw { set: 0, clr: 0x200 },
                IrOp::DaifRmw { set: 0x80, clr: 0x340 },
                IrOp::Branch { target: 0x10 },
            ],
            vec![BlockExit::Branch(0x10)],
        );
        let mut exe = WasmiExecutor::new();
        let mut regs = [0u64; 31];
        let mut host = RamHost::new(0x1000);
        let (exit, _) = exe.run_block(&wasm, &mut regs, &mut host).unwrap();
        assert_eq!(exit, 0x10);
        assert_eq!(host.sysregs.daif, 0x80);
    }

    #[test]
    fn backend_traps_loudly() {
        let wasm = compile(vec![IrOp::Trap { reason: "boom" }], vec![BlockExit::ExitVm]);
        let mut exe = WasmiExecutor::new();
        let mut regs = [0u64; 31];
        let mut host = RamHost::new(0x1000);

        let err = exe.run_block(&wasm, &mut regs, &mut host).unwrap_err();
        assert!(err.contains("trap"), "expected a trap error, got: {err}");
    }

    #[test]
    fn backend_wfi_flag() {
        let wasm = compile(vec![IrOp::Wfi], vec![BlockExit::FallThrough(0x4)]);
        let mut exe = WasmiExecutor::new();
        let mut regs = [0u64; 31];
        let mut host = RamHost::new(0x1000);

        let (exit, wfi) = exe.run_block(&wasm, &mut regs, &mut host).unwrap();
        assert_eq!(exit, 0x4);
        assert!(wfi, "wfi_seen must be true after env.wfi fires");
        assert_eq!(host.wfi_calls, 1);
    }

    #[test]
    fn backend_host_mem_roundtrip() {
        // x1 = 0x200; x2 = 0xdeadbeef; store x2 -> [x1]; x3 = load [x1].
        let wasm = compile(
            vec![
                IrOp::Mov { dst: 1, imm: 0x200 },
                IrOp::Mov {
                    dst: 2,
                    imm: 0xdead_beef,
                },
                IrOp::StoreDyn {
                    src: 2,
                    base: 1,
                    off: 0,
                    size: 8,
                },
                IrOp::LoadDyn {
                    dst: 3,
                    base: 1,
                    off: 0,
                    size: 8,
                },
                IrOp::Branch { target: 0x108 },
            ],
            vec![BlockExit::Branch(0x108)],
        );
        let mut exe = WasmiExecutor::new();
        let mut regs = [0u64; 31];
        let mut host = RamHost::new(0x1000);

        let (exit, _) = exe.run_block(&wasm, &mut regs, &mut host).unwrap();
        assert_eq!(exit, 0x108);
        assert_eq!(regs[3], 0xdead_beef);

        // Confirm the bytes actually traveled through the host RAM, not a
        // hidden engine memory: read the raw host buffer directly.
        let mut raw = 0i64;
        for (i, b) in host.ram[0x200..0x208].iter().enumerate() {
            raw |= (*b as i64) << (8 * i);
        }
        assert_eq!(raw, 0xdead_beef);
    }

    #[test]
    fn invalidate_clears_cache() {
        let wasm = compile(
            vec![IrOp::Mov { dst: 0, imm: 1 }, IrOp::Branch { target: 0x4 }],
            vec![BlockExit::Branch(0x4)],
        );
        let mut exe = WasmiExecutor::new();
        let mut regs = [0u64; 31];
        let mut host = RamHost::new(0x1000);

        exe.run_block(&wasm, &mut regs, &mut host).unwrap();
        assert_eq!(exe.cache_len(), 1);
        exe.invalidate();
        assert_eq!(exe.cache_len(), 0);
    }
}
