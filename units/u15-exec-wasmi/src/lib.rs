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
    use pathn_contracts::cpu::{BlockExit, IrBlock, IrOp};

    /// Minimal host: a flat RAM with bounds checking. Panics are fine —
    /// these tests never trigger unexpected calls by construction.
    struct RamHost {
        ram: Vec<u8>,
        wfi_calls: u32,
    }

    impl RamHost {
        fn new(size: usize) -> Self {
            Self {
                ram: vec![0; size],
                wfi_calls: 0,
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
