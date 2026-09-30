//! U15-exec-wasmtime: the native [`BlockExecutor`] backend.
//!
//! Moved verbatim out of U12 (2026-09-28 browser-host scope): the wasmtime
//! Engine/Module/Store/Linker plumbing that used to live in
//! `u12-orchestrator`. The only behavioral changes vs the old code:
//! - the compile cache is keyed by module **bytes** (U3 compile is pure, so
//!   bytes are a valid deterministic key — strictly more correct than the old
//!   pc-key under self-modifying code),
//! - host errors are `String` (via [`HostOps`]) instead of `wasmtime::Error`,
//!   mapped back to `wasmtime::Error` at the import boundary so traps keep
//!   their messages.
//!
//! The guest memory / console-MMIO dispatch lives in U12's `WasmHost`, which
//! implements [`HostOps`]; this crate only forwards to it.

use pathn_contracts::cpu::WasmModule;
use pathn_contracts::execution::{BlockExecutor, HostOps};
use std::collections::HashMap;

/// Native wasmtime-backed executor. Owns the engine and the bytes-keyed
/// compile cache.
pub struct WasmtimeExecutor {
    engine: wasmtime::Engine,
    cache: HashMap<Vec<u8>, wasmtime::Module>,
}

impl WasmtimeExecutor {
    pub fn new() -> Self {
        Self {
            engine: wasmtime::Engine::default(),
            cache: HashMap::new(),
        }
    }

    fn compile(&mut self, wasm: &WasmModule) -> Result<wasmtime::Module, String> {
        if let Some(m) = self.cache.get(&wasm.bytes) {
            return Ok(m.clone());
        }
        let module =
            wasmtime::Module::new(&self.engine, &wasm.bytes).map_err(|e| format!("module: {e}"))?;
        self.cache.insert(wasm.bytes.clone(), module.clone());
        Ok(module)
    }
}

impl Default for WasmtimeExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// The store data: borrows the orchestrator's [`HostOps`] for exactly one
/// block execution and records whether the `wfi` import fired.
struct TrackingHost<'a> {
    ops: &'a mut dyn HostOps,
    wfi_seen: bool,
}

impl BlockExecutor for WasmtimeExecutor {
    fn run_block(
        &mut self,
        wasm: &WasmModule,
        regs: &mut [u64; 31],
        host: &mut dyn HostOps,
    ) -> Result<(i64, bool), String> {
        let module = self.compile(wasm)?;
        let mut store = wasmtime::Store::new(
            &self.engine,
            TrackingHost {
                ops: host,
                wfi_seen: false,
            },
        );
        let mut linker: wasmtime::Linker<TrackingHost> = wasmtime::Linker::new(&self.engine);
        linker
            .func_wrap(
                "env",
                "mem_load",
                |mut caller: wasmtime::Caller<'_, TrackingHost>, addr: i64, size: i64| {
                    caller
                        .data_mut()
                        .ops
                        .mem_load(addr, size)
                        .map_err(wasmtime::Error::msg)
                },
            )
            .map_err(|e| format!("link mem_load: {e}"))?;
        linker
            .func_wrap(
                "env",
                "mem_store",
                |mut caller: wasmtime::Caller<'_, TrackingHost>, addr: i64, size: i64, val: i64| {
                    caller
                        .data_mut()
                        .ops
                        .mem_store(addr, size, val)
                        .map_err(wasmtime::Error::msg)
                },
            )
            .map_err(|e| format!("link mem_store: {e}"))?;
        linker
            .func_wrap(
                "env",
                "wfi",
                |mut caller: wasmtime::Caller<'_, TrackingHost>| {
                    caller.data_mut().wfi_seen = true;
                    caller.data_mut().ops.wfi();
                },
            )
            .map_err(|e| format!("link wfi: {e}"))?;
        linker
            .func_wrap(
                "env",
                "sysreg_load",
                |mut caller: wasmtime::Caller<'_, TrackingHost>, reg: i64, _pad: i64| {
                    let idx = u8::try_from(reg).map_err(|_| {
                        wasmtime::Error::msg(format!("sysreg_load: bad index {reg}"))
                    })?;
                    caller
                        .data_mut()
                        .ops
                        .sysreg_load(idx)
                        .map_err(wasmtime::Error::msg)
                },
            )
            .map_err(|e| format!("link sysreg_load: {e}"))?;
        linker
            .func_wrap(
                "env",
                "sysreg_store",
                |mut caller: wasmtime::Caller<'_, TrackingHost>, reg: i64, val: i64| {
                    let idx = u8::try_from(reg).map_err(|_| {
                        wasmtime::Error::msg(format!("sysreg_store: bad index {reg}"))
                    })?;
                    caller
                        .data_mut()
                        .ops
                        .sysreg_store(idx, val)
                        .map_err(wasmtime::Error::msg)
                },
            )
            .map_err(|e| format!("link sysreg_store: {e}"))?;
        // Register file: checkpointed X0-X30 in, mutated X0-X30 out.
        let mut globals = Vec::with_capacity(31);
        for (i, reg) in regs.iter().enumerate() {
            let g = wasmtime::Global::new(
                &mut store,
                wasmtime::GlobalType::new(wasmtime::ValType::I64, wasmtime::Mutability::Var),
                wasmtime::Val::I64(*reg as i64),
            )
            .map_err(|e| format!("global r{i}: {e}"))?;
            linker
                .define(&mut store, "env", &format!("r{i}"), g)
                .map_err(|e| format!("define r{i}: {e}"))?;
            globals.push(g);
        }

        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| format!("instantiate: {e}"))?;
        let run = instance
            .get_typed_func::<(), i64>(&mut store, "run")
            .map_err(|e| format!("export 'run': {e}"))?;
        let exit_addr = run.call(&mut store, ()).map_err(|e| format!("trap: {e}"))?;
        for (i, g) in globals.iter().enumerate() {
            if let wasmtime::Val::I64(v) = g.get(&mut store) {
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
