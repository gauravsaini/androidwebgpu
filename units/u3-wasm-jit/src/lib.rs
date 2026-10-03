//! U3 — `wasm-jit`: IR block → deterministic WASM module bytes.
//!
//! PURE: `compile` is a total, deterministic function of `&IrBlock`.
//! No I/O, no time, no threads, no hidden state. Same input bytes →
//! byte-identical output (contract with the orchestrator, U12).
//!
//! Codegen model (Wave 4, contract `pathn_contracts::wasm_abi` U3-G1):
//! - One function `run() -> i64`, exported. The i64 result is the *exit
//!   address* the block transfers control to: `FallThrough(a)` / `Branch(a)`
//!   yield `a`, `ExitVm` (or an empty exit list) yields `-1` as a sentinel.
//! - The register file is 31 IMPORTED mutable `i64` globals (`env.r0` …
//!   `env.r30` = X0–X30). The host sets them before the call and reads them
//!   back after, so machine state persists across blocks. There is no `r31`:
//!   register 31 is XZR in the lifted forms — sources compile to
//!   `i64.const 0`, destinations are dropped.
//! - U2's `SCRATCH` (index 32) maps to the single declared `i64` local.
//!   It is dead after each instruction's op sequence, so one local suffices.
//! - Memory/console go through imported host functions `env.mem_load` /
//!   `env.mem_store` (the host owns guest RAM and the console model); `WFI`
//!   becomes a call to imported `env.wfi` (the host records it and yields).
//! - `IrOp::Branch { target }` / `CondBranch` terminate the block:
//!   `i64.const target; return`. Ops after them are dead by definition.
//! - `IrOp::BranchDyn { reg }` (Wave 5, BL/RET scope) terminates the block
//!   too: `global.get reg; return` — the returned i64 IS the next guest PC,
//!   so no new imports are needed. The declared exit is
//!   `BlockExit::Dynamic`; a `Dynamic` first exit with no `BranchDyn` op is
//!   malformed and lowers to `unreachable`.
//! - `Load` / `Store` (static, GB-12) lower to `env.mem_load` /
//!   `env.mem_store` with an `i64.const` address: same host-call contract as
//!   the dynamic forms, with the address known at lift time.
//!   `IrOp::Trap` still lowers to `unreachable`, per LLD §3.
//! - `Trap { reason }`: the reason string is a host-side diagnostic and is
//!   deliberately NOT encoded — encoding it would break byte-determinism
//!   across builds. The trap itself is always emitted.
//! - With several exits, only the FIRST is lowered (documented); multi-exit
//!   dispatch belongs to the orchestrator. `CondBranch` takes its
//!   fallthrough address from the block's `BlockExit::FallThrough`; a block
//!   without one is malformed and compiles the branch to `unreachable`.

use pathn_contracts::cpu::{BlockExit, IrBlock, IrOp, WasmModule};

// WASM opcodes (MVP core). NOTE: i32 and i64 bitwise opcodes differ only in
// the high nibble (0x7_ = i32, 0x8_ = i64) — 2026-09-27 fixed a latent bug
// where the i32 forms (0x72/0x74/0x75/0x76) were used for i64 ops; wasmtime
// rejected every OrrShift block with "expected i32, found i64". Caught only
// when the real guest first executed ORR past the Wave-4 BL halt.
const OP_UNREACHABLE: u8 = 0x00;
const OP_IF: u8 = 0x04;
const OP_ELSE: u8 = 0x05;
const OP_END: u8 = 0x0B;
const OP_RETURN: u8 = 0x0F;
const OP_CALL: u8 = 0x10;
const OP_DROP: u8 = 0x1A;
const OP_LOCAL_GET: u8 = 0x20;
const OP_LOCAL_SET: u8 = 0x21;
const OP_GLOBAL_GET: u8 = 0x23;
const OP_GLOBAL_SET: u8 = 0x24;
const OP_I64_CONST: u8 = 0x42;
const OP_I64_EQZ: u8 = 0x50;
const OP_I64_LT_U: u8 = 0x54;
const OP_I64_ADD: u8 = 0x7C;
const OP_I64_SUB: u8 = 0x7D;
const OP_I64_CLZ: u8 = 0x79;
const OP_I64_MUL: u8 = 0x7E;
const OP_I64_AND: u8 = 0x83;
const OP_I64_OR: u8 = 0x84;
const OP_I64_XOR: u8 = 0x85;
const OP_I64_SHL: u8 = 0x86;
const OP_I64_SHR_S: u8 = 0x87;
const OP_I64_SHR_U: u8 = 0x88;
const OP_I64_ROTR: u8 = 0x8A;
const OP_I64_EXTEND_I32_U: u8 = 0xAD;

const VALTYPE_I64: u8 = 0x7E;

// Imported-function indices in the module (contract order: mem_load,
// mem_store, wfi — see pathn_contracts::wasm_abi).
const FUNC_MEM_LOAD: u32 = 0;
const FUNC_MEM_STORE: u32 = 1;
const FUNC_WFI: u32 = 2;
const FUNC_SYSREG_LOAD: u32 = 3;
const FUNC_SYSREG_STORE: u32 = 4;
/// PSCI hypervisor call (P0, 2026-10-03): dispatched by the host.
const FUNC_HVC: u32 = 5;
/// Index of `run` (6 imported functions precede it).
const FUNC_RUN: u32 = 6;

/// Architectural register 31 (XZR): sources are const 0, dests are dropped.
const XZR: u8 = 31;
/// U2's scratch index maps to the single declared local.
const SCRATCH: u8 = 32;

/// Sentinel result when the block exits the VM (or declares no exits).
const EXIT_VM_SENTINEL: i64 = -1;

/// Compile one IR block to a structurally valid WASM module.
pub fn compile(block: &IrBlock) -> WasmModule {
    let mut out = Vec::new();
    // Magic + version.
    out.extend_from_slice(b"\0asm");
    out.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);

    // Type section (id 1): six types.
    //   0: () -> (i64)            — run
    //   1: (i64, i64) -> (i64)    — mem_load, sysreg_load
    //   2: (i64, i64, i64) -> ()  — mem_store
    //   3: () -> ()               — wfi
    //   4: (i64, i64) -> ()       — sysreg_store
    //   5: (i64) -> (i64)         — hvc (P0)
    let mut ty = Vec::new();
    uleb(6, &mut ty);
    func_type(&[], &[VALTYPE_I64], &mut ty);
    func_type(&[VALTYPE_I64, VALTYPE_I64], &[VALTYPE_I64], &mut ty);
    func_type(&[VALTYPE_I64, VALTYPE_I64, VALTYPE_I64], &[], &mut ty);
    func_type(&[], &[], &mut ty);
    func_type(&[VALTYPE_I64, VALTYPE_I64], &[], &mut ty);
    func_type(&[VALTYPE_I64], &[VALTYPE_I64], &mut ty);
    section(1, &ty, &mut out);

    // Import section (id 2): env.mem_load, env.mem_store, env.wfi,
    // env.sysreg_load, env.sysreg_store, env.hvc,
    // then env.r0 .. env.r30 as mutable i64 globals. ORDER IS CONTRACT.
    let mut im = Vec::new();
    uleb(6 + 31, &mut im);
    import_func("mem_load", 1, &mut im);
    import_func("mem_store", 2, &mut im);
    import_func("wfi", 3, &mut im);
    import_func("sysreg_load", 1, &mut im);
    import_func("sysreg_store", 4, &mut im);
    import_func("hvc", 5, &mut im);
    for i in 0..31u8 {
        import_global(&format!("r{i}"), &mut im);
    }
    section(2, &im, &mut out);

    // Function section (id 3): one function of type 0 (run).
    let mut func = Vec::new();
    uleb(1, &mut func);
    uleb(0, &mut func);
    section(3, &func, &mut out);

    // Export section (id 7): "run" -> func FUNC_RUN.
    let mut ex = Vec::new();
    uleb(1, &mut ex);
    uleb(3, &mut ex);
    ex.extend_from_slice(b"run");
    ex.push(0x00); // kind: func
    uleb(u64::from(FUNC_RUN), &mut ex);
    section(7, &ex, &mut out);

    // Code section (id 10): one body, 12 i64 locals (local 0 = U2 SCRATCH;
    // locals 1-11 are temporaries for Umulh/Smulh).
    let mut body = Vec::new();
    uleb(1, &mut body); // one local entry: 12 x i64
    uleb(12, &mut body);
    body.push(VALTYPE_I64);

    let mut branched = false;
    for op in &block.ops {
        match op {
            IrOp::Add { dst, a, b } => {
                reg_get(&mut body, *a);
                reg_get(&mut body, *b);
                body.push(OP_I64_ADD);
                reg_set(&mut body, *dst);
            }
            IrOp::Sub { dst, a, b } => {
                reg_get(&mut body, *a);
                reg_get(&mut body, *b);
                body.push(OP_I64_SUB);
                reg_set(&mut body, *dst);
            }
            IrOp::Clz { dst, src } => {
                reg_get(&mut body, *src);
                body.push(OP_I64_CLZ);
                reg_set(&mut body, *dst);
            }
            IrOp::Madd { dst, n, m, a } => {
                // Xd = Ra + Rn * Rm: stack holds a, n, m; mul -> n*m;
                // add -> a + (n*m). reg_get/reg_set give XZR semantics free.
                reg_get(&mut body, *a);
                reg_get(&mut body, *n);
                reg_get(&mut body, *m);
                body.push(OP_I64_MUL);
                body.push(OP_I64_ADD);
                reg_set(&mut body, *dst);
            }
            IrOp::Umulh { dst, n, m } => {
                emit_umulh(&mut body, *n, *m, *dst);
            }
            IrOp::Smulh { dst, n, m } => {
                emit_smulh(&mut body, *n, *m, *dst);
            }
            IrOp::Umull { dst, n, m } => {
                emit_umull(&mut body, *n, *m, *dst);
            }
            IrOp::Smull { dst, n, m } => {
                emit_smulh_long(&mut body, *n, *m, *dst);
            }
            IrOp::Smaddl { dst, n, m, a } => {
                emit_smaddl(&mut body, *n, *m, *a, *dst);
            }
            IrOp::Smsubl { dst, n, m, a } => {
                emit_smsubl(&mut body, *n, *m, *a, *dst);
            }
            IrOp::Umaddl { dst, n, m, a } => {
                emit_umaddl(&mut body, *n, *m, *a, *dst);
            }
            IrOp::Umsubl { dst, n, m, a } => {
                emit_umsubl(&mut body, *n, *m, *a, *dst);
            }
            IrOp::Mov { dst, imm } => {
                body.push(OP_I64_CONST);
                sleb(*imm as i64, &mut body);
                reg_set(&mut body, *dst);
            }
            // GB-12: static guest memory goes through the host's mem_load /
            // mem_store with a constant address — same host-call contract as
            // the dynamic forms, with the address known at lift time.
            IrOp::Load { dst, addr, size } => {
                body.push(OP_I64_CONST);
                sleb(*addr as i64, &mut body);
                body.push(OP_I64_CONST);
                sleb(*size as i64, &mut body);
                body.push(OP_CALL);
                uleb(u64::from(FUNC_MEM_LOAD), &mut body);
                reg_set(&mut body, *dst);
            }
            IrOp::Store { src, addr, size } => {
                body.push(OP_I64_CONST);
                sleb(*addr as i64, &mut body);
                body.push(OP_I64_CONST);
                sleb(*size as i64, &mut body);
                reg_get(&mut body, *src);
                body.push(OP_CALL);
                uleb(u64::from(FUNC_MEM_STORE), &mut body);
            }
            // IrOp::Trap stays unreachable: a deliberate halt, never faked.
            IrOp::Trap { .. } => {
                body.push(OP_UNREACHABLE);
            }
            IrOp::LoadDyn {
                dst,
                base,
                off,
                size,
            } => {
                reg_get(&mut body, *base);
                body.push(OP_I64_CONST);
                sleb(*off as i64, &mut body);
                body.push(OP_I64_ADD);
                body.push(OP_I64_CONST);
                sleb(*size as i64, &mut body);
                body.push(OP_CALL);
                uleb(u64::from(FUNC_MEM_LOAD), &mut body);
                reg_set(&mut body, *dst);
            }
            IrOp::StoreDyn {
                src,
                base,
                off,
                size,
            } => {
                reg_get(&mut body, *base);
                body.push(OP_I64_CONST);
                sleb(*off as i64, &mut body);
                body.push(OP_I64_ADD);
                body.push(OP_I64_CONST);
                sleb(*size as i64, &mut body);
                reg_get(&mut body, *src);
                body.push(OP_CALL);
                uleb(u64::from(FUNC_MEM_STORE), &mut body);
            }
            IrOp::CondBranch {
                reg,
                target,
                when_zero,
            } => {
                let fallthrough = block.exits.iter().find_map(|e| match e {
                    BlockExit::FallThrough(a) => Some(*a),
                    _ => None,
                });
                let Some(fall) = fallthrough else {
                    // Malformed block: a conditional branch with nowhere to
                    // fall through. Loud trap, never a guessed address.
                    body.push(OP_UNREACHABLE);
                    continue;
                };
                let (taken, not_taken) = if *when_zero {
                    (*target, fall)
                } else {
                    (fall, *target)
                };
                reg_get(&mut body, *reg);
                body.push(OP_I64_EQZ);
                body.push(OP_IF);
                body.push(VALTYPE_I64); // if (result i64)
                body.push(OP_I64_CONST);
                sleb(taken as i64, &mut body);
                body.push(OP_ELSE);
                body.push(OP_I64_CONST);
                sleb(not_taken as i64, &mut body);
                body.push(OP_END);
                body.push(OP_RETURN);
                branched = true;
                break;
            }
            IrOp::OrrShift {
                dst,
                a,
                b,
                shift,
                amount,
            } => {
                let shift_op = match shift {
                    0 => OP_I64_SHL,
                    1 => OP_I64_SHR_U,
                    2 => OP_I64_SHR_S,
                    // The decoder/lifter never emit 3 (reserved); a
                    // hand-built IrOp with shift > 2 is a contract violation
                    // and traps loudly rather than picking a meaning.
                    _ => {
                        body.push(OP_UNREACHABLE);
                        continue;
                    }
                };
                reg_get(&mut body, *a);
                reg_get(&mut body, *b);
                body.push(OP_I64_CONST);
                sleb(*amount as i64, &mut body);
                body.push(shift_op);
                body.push(OP_I64_OR);
                reg_set(&mut body, *dst);
            }
            IrOp::AndShift {
                dst,
                a,
                b,
                shift,
                amount,
                invert,
                is_32,
            } => {
                let shift_op = match shift {
                    0 => OP_I64_SHL,
                    1 => OP_I64_SHR_U,
                    2 => OP_I64_SHR_S,
                    3 => OP_I64_ROTR,
                    _ => {
                        body.push(OP_UNREACHABLE);
                        continue;
                    }
                };
                reg_get(&mut body, *a);
                reg_get(&mut body, *b);
                if *amount > 0 {
                    body.push(OP_I64_CONST);
                    sleb(*amount as i64, &mut body);
                    body.push(shift_op);
                }
                if *invert {
                    body.push(OP_I64_CONST);
                    sleb(-1, &mut body);
                    body.push(OP_I64_XOR);
                }
                body.push(OP_I64_AND);
                if *is_32 {
                    body.push(OP_I64_CONST);
                    sleb(0xFFFF_FFFF, &mut body);
                    body.push(OP_I64_AND);
                }
                reg_set(&mut body, *dst);
            }
            IrOp::OrShift {
                dst,
                a,
                b,
                shift,
                amount,
                invert,
                is_32,
            } => {
                let shift_op = match shift {
                    0 => OP_I64_SHL,
                    1 => OP_I64_SHR_U,
                    2 => OP_I64_SHR_S,
                    3 => OP_I64_ROTR,
                    _ => {
                        body.push(OP_UNREACHABLE);
                        continue;
                    }
                };
                reg_get(&mut body, *a);
                reg_get(&mut body, *b);
                if *amount > 0 {
                    body.push(OP_I64_CONST);
                    sleb(*amount as i64, &mut body);
                    body.push(shift_op);
                }
                if *invert {
                    body.push(OP_I64_CONST);
                    sleb(-1, &mut body);
                    body.push(OP_I64_XOR);
                }
                body.push(OP_I64_OR);
                if *is_32 {
                    body.push(OP_I64_CONST);
                    sleb(0xFFFF_FFFF, &mut body);
                    body.push(OP_I64_AND);
                }
                reg_set(&mut body, *dst);
            }
            IrOp::EorShift {
                dst,
                a,
                b,
                shift,
                amount,
                invert,
                is_32,
            } => {
                let shift_op = match shift {
                    0 => OP_I64_SHL,
                    1 => OP_I64_SHR_U,
                    2 => OP_I64_SHR_S,
                    3 => OP_I64_ROTR,
                    _ => {
                        body.push(OP_UNREACHABLE);
                        continue;
                    }
                };
                reg_get(&mut body, *a);
                reg_get(&mut body, *b);
                if *amount > 0 {
                    body.push(OP_I64_CONST);
                    sleb(*amount as i64, &mut body);
                    body.push(shift_op);
                }
                if *invert {
                    body.push(OP_I64_CONST);
                    sleb(-1, &mut body);
                    body.push(OP_I64_XOR);
                }
                body.push(OP_I64_XOR);
                if *is_32 {
                    body.push(OP_I64_CONST);
                    sleb(0xFFFF_FFFF, &mut body);
                    body.push(OP_I64_AND);
                }
                reg_set(&mut body, *dst);
            }
            IrOp::Bitfield {
                dst,
                src,
                opc,
                immr,
                imms,
                is_32,
            } => {
                let datasize: u64 = if *is_32 { 32 } else { 64 };
                let r = (*immr as u64) & (datasize - 1);
                let s = (*imms as u64) & (datasize - 1);
                let d = (s.wrapping_sub(r)) & (datasize - 1);
                let mask = if datasize == 64 { !0u64 } else { 0xFFFF_FFFF };

                let welem = if s == datasize - 1 {
                    mask
                } else {
                    (1u64 << (s + 1)) - 1
                };
                let wmask = if r == 0 {
                    welem
                } else {
                    ((welem >> r) | (welem << (datasize - r))) & mask
                };
                let tmask = if d == datasize - 1 {
                    mask
                } else {
                    (1u64 << (d + 1)) - 1
                };

                match opc {
                    2 => {
                        // UBFM
                        reg_get(&mut body, *src);
                        if *is_32 {
                            body.push(OP_I64_CONST);
                            sleb(0xFFFF_FFFF, &mut body);
                            body.push(OP_I64_AND);
                        }
                        if r > 0 {
                            if *is_32 {
                                reg_set(&mut body, SCRATCH);
                                reg_get(&mut body, SCRATCH);
                                body.push(OP_I64_CONST);
                                sleb(r as i64, &mut body);
                                body.push(OP_I64_SHR_U);
                                reg_get(&mut body, SCRATCH);
                                body.push(OP_I64_CONST);
                                sleb((32 - r) as i64, &mut body);
                                body.push(OP_I64_SHL);
                                body.push(OP_I64_OR);
                            } else {
                                body.push(OP_I64_CONST);
                                sleb(r as i64, &mut body);
                                body.push(OP_I64_ROTR);
                            }
                        }
                        let eff_mask = wmask & tmask;
                        body.push(OP_I64_CONST);
                        sleb(eff_mask as i64, &mut body);
                        body.push(OP_I64_AND);
                        reg_set(&mut body, *dst);
                    }
                    1 => {
                        // BFM: (dst & ~wmask) | (bot & wmask)
                        reg_get(&mut body, *dst);
                        let dst_keep_mask = if *is_32 { (!wmask) & 0xFFFF_FFFF } else { !wmask };
                        body.push(OP_I64_CONST);
                        sleb(dst_keep_mask as i64, &mut body);
                        body.push(OP_I64_AND);

                        reg_get(&mut body, *src);
                        if *is_32 {
                            body.push(OP_I64_CONST);
                            sleb(0xFFFF_FFFF, &mut body);
                            body.push(OP_I64_AND);
                        }
                        if r > 0 {
                            if *is_32 {
                                reg_set(&mut body, SCRATCH);
                                reg_get(&mut body, SCRATCH);
                                body.push(OP_I64_CONST);
                                sleb(r as i64, &mut body);
                                body.push(OP_I64_SHR_U);
                                reg_get(&mut body, SCRATCH);
                                body.push(OP_I64_CONST);
                                sleb((32 - r) as i64, &mut body);
                                body.push(OP_I64_SHL);
                                body.push(OP_I64_OR);
                            } else {
                                body.push(OP_I64_CONST);
                                sleb(r as i64, &mut body);
                                body.push(OP_I64_ROTR);
                            }
                        }
                        body.push(OP_I64_CONST);
                        sleb(wmask as i64, &mut body);
                        body.push(OP_I64_AND);

                        body.push(OP_I64_OR);
                        reg_set(&mut body, *dst);
                    }
                    0 => {
                        // SBFM: extract bits [d:0] and sign-extend from bit d
                        reg_get(&mut body, *src);
                        if *is_32 {
                            body.push(OP_I64_CONST);
                            sleb(0xFFFF_FFFF, &mut body);
                            body.push(OP_I64_AND);
                        }
                        if r > 0 {
                            if *is_32 {
                                reg_set(&mut body, SCRATCH);
                                reg_get(&mut body, SCRATCH);
                                body.push(OP_I64_CONST);
                                sleb(r as i64, &mut body);
                                body.push(OP_I64_SHR_U);
                                reg_get(&mut body, SCRATCH);
                                body.push(OP_I64_CONST);
                                sleb((32 - r) as i64, &mut body);
                                body.push(OP_I64_SHL);
                                body.push(OP_I64_OR);
                            } else {
                                body.push(OP_I64_CONST);
                                sleb(r as i64, &mut body);
                                body.push(OP_I64_ROTR);
                            }
                        }
                        let eff_mask = wmask & tmask;
                        body.push(OP_I64_CONST);
                        sleb(eff_mask as i64, &mut body);
                        body.push(OP_I64_AND);
                        // Sign-extend from bit d
                        let shift_amount = 63 - d;
                        if shift_amount > 0 {
                            body.push(OP_I64_CONST);
                            sleb(shift_amount as i64, &mut body);
                            body.push(OP_I64_SHL);
                            body.push(OP_I64_CONST);
                            sleb(shift_amount as i64, &mut body);
                            body.push(OP_I64_SHR_S);
                        }
                        if *is_32 {
                            body.push(OP_I64_CONST);
                            sleb(0xFFFF_FFFF, &mut body);
                            body.push(OP_I64_AND);
                        }
                        reg_set(&mut body, *dst);
                    }
                    _ => {
                        body.push(OP_UNREACHABLE);
                    }
                }
            }
            IrOp::ShiftVar {
                dst,
                a,
                b,
                shift,
                is_32,
            } => {
                reg_get(&mut body, *a);
                if *is_32 {
                    body.push(OP_I64_CONST);
                    sleb(0xFFFF_FFFF, &mut body);
                    body.push(OP_I64_AND);
                }
                reg_get(&mut body, *b);
                body.push(OP_I64_CONST);
                sleb(if *is_32 { 31 } else { 63 }, &mut body);
                body.push(OP_I64_AND);
                let op = match shift {
                    0 => OP_I64_SHL,   // 00 = LSLV
                    1 => OP_I64_SHR_U, // 01 = LSRV
                    2 => OP_I64_SHR_S, // 10 = ASRV
                    3 => OP_I64_ROTR,  // 11 = RORV
                    _ => OP_UNREACHABLE,
                };
                body.push(op);
                if *is_32 {
                    body.push(OP_I64_CONST);
                    sleb(0xFFFF_FFFF, &mut body);
                    body.push(OP_I64_AND);
                }
                reg_set(&mut body, *dst);
            }
            IrOp::Movk {
                dst,
                imm,
                hw,
                is_32,
            } => {
                let shift = (*hw as u64) * 16;
                let mask = 0xFFFF_u64 << shift;
                let insert_val = (*imm as u64) << shift;
                reg_get(&mut body, *dst);
                body.push(OP_I64_CONST);
                sleb((!mask) as i64, &mut body);
                body.push(OP_I64_AND);
                body.push(OP_I64_CONST);
                sleb(insert_val as i64, &mut body);
                body.push(OP_I64_OR);
                if *is_32 {
                    body.push(OP_I64_CONST);
                    sleb(0xFFFF_FFFF, &mut body);
                    body.push(OP_I64_AND);
                }
                reg_set(&mut body, *dst);
            }
            IrOp::Wfi => {
                body.push(OP_CALL);
                uleb(u64::from(FUNC_WFI), &mut body);
                // Falls through to the exit epilogue; the host records the
                // WFI and the orchestrator yields the vCPU after return.
            }
            // P0 (2026-10-03): HVC/PSCI via host call. Push X0 (function ID),
            // call host, write result back to X0.
            IrOp::Hvc => {
                reg_get(&mut body, 0);
                body.push(OP_CALL);
                uleb(u64::from(FUNC_HVC), &mut body);
                reg_set(&mut body, 0);
            }
            // GB-sysreg2: persistent system-register access via host calls.
            // sysreg_load(reg, _) -> val; sysreg_store(reg, val).
            // XZR dest drops (reg_set(31) = drop); XZR src reads 0.
            IrOp::ReadSys { dst, reg } => {
                body.push(OP_I64_CONST);
                sleb(*reg as u8 as i64, &mut body);
                body.push(OP_I64_CONST);
                sleb(0, &mut body);
                body.push(OP_CALL);
                uleb(u64::from(FUNC_SYSREG_LOAD), &mut body);
                reg_set(&mut body, *dst);
            }
            IrOp::WriteSys { src, reg } => {
                body.push(OP_I64_CONST);
                sleb(*reg as u8 as i64, &mut body);
                reg_get(&mut body, *src);
                body.push(OP_CALL);
                uleb(u64::from(FUNC_SYSREG_STORE), &mut body);
            }
            // GB-11: DAIF read-modify-write through the existing sysreg host
            // calls. Stack discipline: the load leaves the value on top, the
            // OR/AND immediates fold in, and SCRATCH holds the result while
            // the (reg, val) pair for sysreg_store is pushed in order.
            // SysReg::Daif is host-call index 0 (contracts pins the order).
            IrOp::DaifRmw { set, clr } => {
                body.push(OP_I64_CONST);
                sleb(0, &mut body);
                body.push(OP_I64_CONST);
                sleb(0, &mut body);
                body.push(OP_CALL);
                uleb(u64::from(FUNC_SYSREG_LOAD), &mut body);
                if *set != 0 {
                    body.push(OP_I64_CONST);
                    sleb(*set as i64, &mut body);
                    body.push(OP_I64_OR);
                }
                if *clr != 0 {
                    body.push(OP_I64_CONST);
                    sleb(!*clr as i64, &mut body);
                    body.push(OP_I64_AND);
                }
                reg_set(&mut body, SCRATCH);
                body.push(OP_I64_CONST);
                sleb(0, &mut body);
                reg_get(&mut body, SCRATCH);
                body.push(OP_CALL);
                uleb(u64::from(FUNC_SYSREG_STORE), &mut body);
            }
            IrOp::Branch { target } => {
                body.push(OP_I64_CONST);
                sleb(*target as i64, &mut body);
                body.push(OP_RETURN);
                branched = true;
                break;
            }
            // Wave 5 (BL/RET scope): indirect branch. `run`'s i64 result IS
            // the next guest PC, so the register value is simply returned —
            // no new imports needed. Register 31 reads as 0 (XZR) via
            // reg_get; a 0 target then fetch-faults honestly on the host.
            IrOp::BranchDyn { reg } => {
                reg_get(&mut body, *reg);
                body.push(OP_RETURN);
                branched = true;
                break;
            }
        }
    }

    if !branched {
        // Exit epilogue: result = where control goes next.
        match block.exits.first() {
            Some(BlockExit::FallThrough(a)) | Some(BlockExit::Branch(a)) => {
                body.push(OP_I64_CONST);
                sleb(*a as i64, &mut body);
            }
            // A dynamic exit with no BranchDyn op emitted is a malformed
            // block (the orchestrator only declares Dynamic alongside
            // BranchDyn, which always returns early): trap loudly rather
            // than invent an address.
            Some(BlockExit::Dynamic) => body.push(OP_UNREACHABLE),
            Some(BlockExit::ExitVm) | None => {
                body.push(OP_I64_CONST);
                sleb(EXIT_VM_SENTINEL, &mut body);
            }
        }
    }
    body.push(OP_END);

    let mut code = Vec::new();
    uleb(1, &mut code); // one function body
    uleb(body.len() as u64, &mut code);
    code.extend_from_slice(&body);
    section(10, &code, &mut out);

    WasmModule { bytes: out }
}

/// Push a register source: XZR -> `i64.const 0`, SCRATCH -> the one local,
/// X0-X30 -> the imported global. Anything else is a contract violation and
/// traps loudly.
fn reg_get(body: &mut Vec<u8>, r: u8) {
    match r {
        XZR => {
            body.push(OP_I64_CONST);
            sleb(0, body);
        }
        SCRATCH => {
            body.push(OP_LOCAL_GET);
            uleb(0, body);
        }
        0..=30 => {
            body.push(OP_GLOBAL_GET);
            uleb(u64::from(r), body);
        }
        _ => body.push(OP_UNREACHABLE),
    }
}

/// Pop the stack into a register: XZR -> `drop`, SCRATCH -> the one local,
/// X0-X30 -> the imported global. Anything else traps loudly.
fn reg_set(body: &mut Vec<u8>, r: u8) {
    match r {
        XZR => body.push(OP_DROP),
        SCRATCH => {
            body.push(OP_LOCAL_SET);
            uleb(0, body);
        }
        0..=30 => {
            body.push(OP_GLOBAL_SET);
            uleb(u64::from(r), body);
        }
        _ => body.push(OP_UNREACHABLE),
    }
}

/// Emit WASM for `Umulh { dst, n, m }`: `dst = ((n as u128 * m as u128) >> 64) as u64`.
/// Uses locals 1-10 as temporaries (local 0 is SCRATCH).
/// Algorithm: 32-bit split with explicit carry (Hacker's Delight, "High-order product").
///   a_lo = a & 0xFFFFFFFF; a_hi = a >> 32; (same for b)
///   p00 = a_lo*b_lo; p01 = a_lo*b_hi; p10 = a_hi*b_lo; p11 = a_hi*b_hi
///   sum = p01 + p10; ov = (sum < p01)  // 1 if p01+p10 overflowed
///   hi = p11 + (sum >> 32) + (ov ? 2^32 : 0)
///   t = (sum & 0xFFFFFFFF) << 32
///   carry = ((t + p00) < t)  // 1 if t+p00 overflowed
///   dst = hi + carry
fn emit_umulh(body: &mut Vec<u8>, n: u8, m: u8, dst: u8) {
    // Locals: 1=a_lo, 2=a_hi, 3=b_lo, 4=b_hi, 5=p00, 6=p01, 7=p10, 8=p11, 9=sum, 10=t
    // Split a
    reg_get(body, n); // [a]
    body.push(OP_I64_CONST); sleb(0xFFFF_FFFF, body); // [a, mask]
    body.push(OP_I64_AND); // [a_lo]
    body.push(OP_LOCAL_SET); uleb(1, body); // L1 = a_lo
    reg_get(body, n); // [a]
    body.push(OP_I64_CONST); sleb(32, body); // [a, 32]
    body.push(OP_I64_SHR_U); // [a_hi]
    body.push(OP_LOCAL_SET); uleb(2, body); // L2 = a_hi
    // Split b
    reg_get(body, m); // [b]
    body.push(OP_I64_CONST); sleb(0xFFFF_FFFF, body);
    body.push(OP_I64_AND); // [b_lo]
    body.push(OP_LOCAL_SET); uleb(3, body); // L3 = b_lo
    reg_get(body, m); // [b]
    body.push(OP_I64_CONST); sleb(32, body);
    body.push(OP_I64_SHR_U); // [b_hi]
    body.push(OP_LOCAL_SET); uleb(4, body); // L4 = b_hi
    // p00 = a_lo * b_lo -> L5
    body.push(OP_LOCAL_GET); uleb(1, body);
    body.push(OP_LOCAL_GET); uleb(3, body);
    body.push(OP_I64_MUL);
    body.push(OP_LOCAL_SET); uleb(5, body);
    // p01 = a_lo * b_hi -> L6
    body.push(OP_LOCAL_GET); uleb(1, body);
    body.push(OP_LOCAL_GET); uleb(4, body);
    body.push(OP_I64_MUL);
    body.push(OP_LOCAL_SET); uleb(6, body);
    // p10 = a_hi * b_lo -> L7
    body.push(OP_LOCAL_GET); uleb(2, body);
    body.push(OP_LOCAL_GET); uleb(3, body);
    body.push(OP_I64_MUL);
    body.push(OP_LOCAL_SET); uleb(7, body);
    // p11 = a_hi * b_hi -> L8
    body.push(OP_LOCAL_GET); uleb(2, body);
    body.push(OP_LOCAL_GET); uleb(4, body);
    body.push(OP_I64_MUL);
    body.push(OP_LOCAL_SET); uleb(8, body);
    // sum = p01 + p10 -> L9
    body.push(OP_LOCAL_GET); uleb(6, body);
    body.push(OP_LOCAL_GET); uleb(7, body);
    body.push(OP_I64_ADD);
    body.push(OP_LOCAL_SET); uleb(9, body);
    // hi = p11 + (sum >> 32)
    body.push(OP_LOCAL_GET); uleb(8, body); // [p11]
    body.push(OP_LOCAL_GET); uleb(9, body); // [p11, sum]
    body.push(OP_I64_CONST); sleb(32, body); // [p11, sum, 32]
    body.push(OP_I64_SHR_U); // [p11, sum>>32]
    body.push(OP_I64_ADD); // [p11 + (sum>>32)]
    // ov = (sum < p01) ? 2^32 : 0; hi += ov
    body.push(OP_LOCAL_GET); uleb(9, body); // [hi, sum]
    body.push(OP_LOCAL_GET); uleb(6, body); // [hi, sum, p01]
    body.push(OP_I64_LT_U); // [hi, ov_i32]
    body.push(OP_I64_EXTEND_I32_U); // [hi, ov_i64]
    body.push(OP_I64_CONST); sleb(32, body); // [hi, ov, 32]
    body.push(OP_I64_SHL); // [hi, ov<<32]
    body.push(OP_I64_ADD); // [hi + (ov<<32)]
    // t = (sum & 0xFFFFFFFF) << 32
    body.push(OP_LOCAL_GET); uleb(9, body); // [hi, sum]
    body.push(OP_I64_CONST); sleb(0xFFFF_FFFF, body); // [hi, sum, mask]
    body.push(OP_I64_AND); // [hi, sum_lo]
    body.push(OP_I64_CONST); sleb(32, body); // [hi, sum_lo, 32]
    body.push(OP_I64_SHL); // [hi, t]
    // carry = ((t + p00) < t) ? 1 : 0
    body.push(OP_LOCAL_SET); uleb(10, body); // [hi] ; L10 = t
    body.push(OP_LOCAL_GET); uleb(10, body); // [hi, t]
    body.push(OP_LOCAL_GET); uleb(5, body); // [hi, t, p00]
    body.push(OP_I64_ADD); // [hi, t+p00]
    body.push(OP_LOCAL_GET); uleb(10, body); // [hi, t+p00, t]
    body.push(OP_I64_LT_U); // [hi, carry_i32]
    body.push(OP_I64_EXTEND_I32_U); // [hi, carry_i64]
    body.push(OP_I64_ADD); // [hi + carry]
    reg_set(body, dst);
}

/// Emit WASM for `Smulh { dst, n, m }`: `dst = ((n as i128 * m as i128) >> 64) as u64`.
/// Uses the identity: smulh(a,b) = umulh(a,b) - (a<0 ? b : 0) - (b<0 ? a : 0).
/// (Locals 1-10 are used by emit_umulh; the adjustments below use only the stack.)
fn emit_smulh(body: &mut Vec<u8>, n: u8, m: u8, dst: u8) {
    // Compute u = umulh(n, m) into local 1
    // (emit_umulh uses locals 1-9; we'll save its result from dst to L1)
    // To avoid clobbering, emit umulh to dst first, then move to L1 if needed.
    // Simpler: emit umulh directly, then adjust.
    emit_umulh(body, n, m, dst);
    // u is now in dst (register). Move to stack for adjustments.
    // We need: u - (a<0 ? b : 0) - (b<0 ? a : 0)
    // mask_a = (a as i64 >> 63) as u64  // all 1s if a<0 else 0
    // mask_b = (b as i64 >> 63) as u64
    // u -= (mask_a & b); u -= (mask_b & a)
    match dst {
        XZR => {
            // dst is XZR: result is dropped; still need to consume stack correctly.
            // emit_umulh already did reg_set(dst) which dropped. Nothing to adjust.
        }
        _ => {
            // Get u back onto stack
            reg_get(body, dst); // [u]
            // Compute mask_a & b
            reg_get(body, n); // [u, a]
            body.push(OP_I64_CONST); sleb(63, body); // [u, a, 63]
            body.push(OP_I64_SHR_S); // [u, mask_a]
            reg_get(body, m); // [u, mask_a, b]
            body.push(OP_I64_AND); // [u, mask_a & b]
            body.push(OP_I64_SUB); // [u - (mask_a & b)]
            // Compute mask_b & a
            reg_get(body, m); // [u', b]
            body.push(OP_I64_CONST); sleb(63, body);
            body.push(OP_I64_SHR_S); // [u', mask_b]
            reg_get(body, n); // [u', mask_b, a]
            body.push(OP_I64_AND); // [u', mask_b & a]
            body.push(OP_I64_SUB); // [u' - (mask_b & a)]
            reg_set(body, dst);
        }
    }
}

/// Emit WASM for `Umull { dst, n, m }`: `dst = (n as u32 as u64) * (m as u32 as u64)`.
/// Masks both operands to 32 bits, then a single `i64.mul`. Uses only the stack.
fn emit_umull(body: &mut Vec<u8>, n: u8, m: u8, dst: u8) {
    reg_get(body, n); // [n]
    body.push(OP_I64_CONST); sleb(0xFFFF_FFFF, body); // [n, mask]
    body.push(OP_I64_AND); // [n_lo]
    reg_get(body, m); // [n_lo, m]
    body.push(OP_I64_CONST); sleb(0xFFFF_FFFF, body); // [n_lo, m, mask]
    body.push(OP_I64_AND); // [n_lo, m_lo]
    body.push(OP_I64_MUL); // [product]
    reg_set(body, dst);
}

/// Emit WASM for `Smull { dst, n, m }`: `dst = (n as i32 as i64) * (m as i32 as i64)`.
/// Sign-extends both operands (shl 32 then shr_s 32), then a single `i64.mul`.
/// Uses only the stack.
fn emit_smulh_long(body: &mut Vec<u8>, n: u8, m: u8, dst: u8) {
    reg_get(body, n); // [n]
    body.push(OP_I64_CONST); sleb(32, body); // [n, 32]
    body.push(OP_I64_SHL); // [n << 32]
    body.push(OP_I64_CONST); sleb(32, body); // [n << 32, 32]
    body.push(OP_I64_SHR_S); // [sext(n)]
    reg_get(body, m); // [sext(n), m]
    body.push(OP_I64_CONST); sleb(32, body); // [sext(n), m, 32]
    body.push(OP_I64_SHL); // [sext(n), m << 32]
    body.push(OP_I64_CONST); sleb(32, body); // [sext(n), m << 32, 32]
    body.push(OP_I64_SHR_S); // [sext(n), sext(m)]
    body.push(OP_I64_MUL); // [product]
    reg_set(body, dst);
}

/// Emit WASM for `Smaddl { dst, n, m, a }`: `dst = a + sext(n) * sext(m)`.
fn emit_smaddl(body: &mut Vec<u8>, n: u8, m: u8, a: u8, dst: u8) {
    emit_smulh_long_no_store(body, n, m); // [product]
    reg_get(body, a); // [product, a]
    body.push(OP_I64_ADD); // [product + a]
    reg_set(body, dst);
}

/// Emit WASM for `Smsubl { dst, n, m, a }`: `dst = a - sext(n) * sext(m)`.
fn emit_smsubl(body: &mut Vec<u8>, n: u8, m: u8, a: u8, dst: u8) {
    reg_get(body, a); // [a]
    emit_smulh_long_no_store(body, n, m); // [a, product]
    body.push(OP_I64_SUB); // [a - product]
    reg_set(body, dst);
}

/// Emit WASM for `Umaddl { dst, n, m, a }`: `dst = a + (n as u32 as u64) * (m as u32 as u64)`.
fn emit_umaddl(body: &mut Vec<u8>, n: u8, m: u8, a: u8, dst: u8) {
    emit_umull_no_store(body, n, m); // [product]
    reg_get(body, a); // [product, a]
    body.push(OP_I64_ADD); // [product + a]
    reg_set(body, dst);
}

/// Emit WASM for `Umsubl { dst, n, m, a }`: `dst = a - (n as u32 as u64) * (m as u32 as u64)`.
fn emit_umsubl(body: &mut Vec<u8>, n: u8, m: u8, a: u8, dst: u8) {
    reg_get(body, a); // [a]
    emit_umull_no_store(body, n, m); // [a, product]
    body.push(OP_I64_SUB); // [a - product]
    reg_set(body, dst);
}

/// Helper: `Umull` product without the final store. Leaves [product] on stack.
fn emit_umull_no_store(body: &mut Vec<u8>, n: u8, m: u8) {
    reg_get(body, n); // [n]
    body.push(OP_I64_CONST); sleb(0xFFFF_FFFF, body); // [n, mask]
    body.push(OP_I64_AND); // [n_lo]
    reg_get(body, m); // [n_lo, m]
    body.push(OP_I64_CONST); sleb(0xFFFF_FFFF, body); // [n_lo, m, mask]
    body.push(OP_I64_AND); // [n_lo, m_lo]
    body.push(OP_I64_MUL); // [product]
}

/// Helper: `Smull` product without the final store. Leaves [product] on stack.
fn emit_smulh_long_no_store(body: &mut Vec<u8>, n: u8, m: u8) {
    reg_get(body, n); // [n]
    body.push(OP_I64_CONST); sleb(32, body); // [n, 32]
    body.push(OP_I64_SHL); // [n << 32]
    body.push(OP_I64_CONST); sleb(32, body); // [n << 32, 32]
    body.push(OP_I64_SHR_S); // [sext(n)]
    reg_get(body, m); // [sext(n), m]
    body.push(OP_I64_CONST); sleb(32, body); // [sext(n), m, 32]
    body.push(OP_I64_SHL); // [sext(n), m << 32]
    body.push(OP_I64_CONST); sleb(32, body); // [sext(n), m << 32, 32]
    body.push(OP_I64_SHR_S); // [sext(n), sext(m)]
    body.push(OP_I64_MUL); // [product]
}

/// Append a `(param*) -> (result*)` function type.
fn func_type(params: &[u8], results: &[u8], out: &mut Vec<u8>) {
    out.push(0x60);
    uleb(params.len() as u64, out);
    out.extend_from_slice(params);
    uleb(results.len() as u64, out);
    out.extend_from_slice(results);
}

/// Append one import entry: `env.<name>`, kind func, type index `type_idx`.
fn import_func(name: &str, type_idx: u32, out: &mut Vec<u8>) {
    uleb(3, out);
    out.extend_from_slice(b"env");
    uleb(name.len() as u64, out);
    out.extend_from_slice(name.as_bytes());
    out.push(0x00); // kind: func
    uleb(u64::from(type_idx), out);
}

/// Append one import entry: `env.<name>`, kind global, `(mut i64)`.
fn import_global(name: &str, out: &mut Vec<u8>) {
    uleb(3, out);
    out.extend_from_slice(b"env");
    uleb(name.len() as u64, out);
    out.extend_from_slice(name.as_bytes());
    out.push(0x03); // kind: global
    out.push(VALTYPE_I64);
    out.push(0x01); // mutability: var
}

/// Append an unsigned LEB128 value.
fn uleb(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

/// Append a signed LEB128 value.
fn sleb(mut v: i64, out: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7; // arithmetic shift on i64
        let done = (v == 0 && b & 0x40 == 0) || (v == -1 && b & 0x40 != 0);
        if done {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

/// Append one section: id byte, u32 LEB128 payload size, payload.
fn section(id: u8, payload: &[u8], out: &mut Vec<u8>) {
    out.push(id);
    uleb(payload.len() as u64, out);
    out.extend_from_slice(payload);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pathn_contracts::cpu::SysReg;

    fn sample_block() -> IrBlock {
        IrBlock {
            entry_addr: 0x4000,
            ops: vec![
                IrOp::Mov { dst: 1, imm: 5 },
                IrOp::Mov { dst: 2, imm: 7 },
                IrOp::Add { dst: 3, a: 1, b: 2 },
                IrOp::Trap {
                    reason: "unimplemented",
                },
            ],
            exits: vec![BlockExit::FallThrough(0x4008)],
        }
    }

    // ---- tiny independent WASM reader, used only by tests ----

    fn read_uleb(bytes: &[u8], pos: &mut usize) -> u64 {
        let mut v = 0u64;
        let mut shift = 0;
        loop {
            let b = bytes[*pos];
            *pos += 1;
            v |= u64::from(b & 0x7F) << shift;
            if b & 0x80 == 0 {
                return v;
            }
            shift += 7;
        }
    }

    fn read_sleb(bytes: &[u8], pos: &mut usize) -> i64 {
        let mut v = 0i64;
        let mut shift = 0;
        let mut b;
        loop {
            b = bytes[*pos];
            *pos += 1;
            v |= i64::from(b & 0x7F) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                break;
            }
        }
        if shift < 64 && b & 0x40 != 0 {
            v |= !0 << shift;
        }
        v
    }

    /// Split the module into (section_id, payload) pairs.
    fn sections(bytes: &[u8]) -> Vec<(u8, &[u8])> {
        let mut pos = 8; // skip magic + version
        let mut out = Vec::new();
        while pos < bytes.len() {
            let id = bytes[pos];
            pos += 1;
            let size = read_uleb(bytes, &mut pos) as usize;
            out.push((id, &bytes[pos..pos + size]));
            pos += size;
        }
        out
    }

    /// Extract the raw expression bytes of the first code body.
    fn code_expr(bytes: &[u8]) -> Vec<u8> {
        let secs = sections(bytes);
        let code = secs.iter().find(|(id, _)| *id == 10).unwrap().1;
        let mut pos = 0;
        assert_eq!(read_uleb(code, &mut pos), 1); // one body
        let body_size = read_uleb(code, &mut pos) as usize;
        let body_end = pos + body_size;
        let local_entries = read_uleb(code, &mut pos);
        for _ in 0..local_entries {
            read_uleb(code, &mut pos); // count
            pos += 1; // valtype
        }
        let expr = code[pos..body_end].to_vec();
        assert_eq!(*expr.last().unwrap(), OP_END);
        expr
    }

    /// Decode the last `i64.const` operand in an expression.
    fn last_const(expr: &[u8]) -> i64 {
        let mut pos = 0;
        let mut last = 0i64;
        while pos < expr.len() {
            let op = expr[pos];
            pos += 1;
            match op {
                OP_I64_CONST => last = read_sleb(expr, &mut pos),
                OP_LOCAL_GET | OP_LOCAL_SET | OP_GLOBAL_GET | OP_GLOBAL_SET | OP_CALL => {
                    read_uleb(expr, &mut pos);
                }
                _ => {}
            }
        }
        last
    }

    /// Parse the import section into (module, name, kind, detail) tuples.
    /// kind 0 = func (detail: type index), kind 3 = global (detail: mutability).
    fn imports(bytes: &[u8]) -> Vec<(String, String, u8, u64)> {
        let secs = sections(bytes);
        let im = secs.iter().find(|(id, _)| *id == 2).unwrap().1;
        let mut pos = 0;
        let n = read_uleb(im, &mut pos);
        let mut out = Vec::new();
        for _ in 0..n {
            let mlen = read_uleb(im, &mut pos) as usize;
            let module = String::from_utf8(im[pos..pos + mlen].to_vec()).unwrap();
            pos += mlen;
            let nlen = read_uleb(im, &mut pos) as usize;
            let name = String::from_utf8(im[pos..pos + nlen].to_vec()).unwrap();
            pos += nlen;
            let kind = im[pos];
            pos += 1;
            let detail = match kind {
                0x00 => read_uleb(im, &mut pos),
                0x03 => {
                    pos += 1; // valtype (i64)
                    let mutability = im[pos] as u64;
                    pos += 1;
                    mutability
                }
                _ => panic!("unexpected import kind {kind}"),
            };
            out.push((module, name, kind, detail));
        }
        out
    }

    // ---- determinism ----

    /// Validate the compiled module with wasmparser. Called by codegen tests
    /// so an invalid opcode can never go latent again (2026-09-27: i32
    /// opcodes were emitted for i64 OR/SHL/SHR and only failed at wasmtime
    /// translation, two units downstream).
    fn assert_valid(block: &IrBlock) -> Vec<u8> {
        let m = compile(block);
        wasmparser::Validator::new()
            .validate_all(&m.bytes)
            .expect("U3 emitted invalid WASM");
        code_expr(&m.bytes)
    }

    #[test]
    fn determin_compile_same_block_twice_byte_identical() {
        let a = compile(&sample_block());
        let b = compile(&sample_block());
        assert_eq!(a.bytes, b.bytes);
        assert!(!a.bytes.is_empty());
    }

    #[test]
    fn determin_empty_block_is_stable() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![],
            exits: vec![],
        };
        assert_eq!(compile(&block).bytes, compile(&block).bytes);
    }

    #[test]
    fn determin_all_op_kinds_stable() {
        let block = IrBlock {
            entry_addr: 0x100,
            ops: vec![
                IrOp::Add { dst: 0, a: 1, b: 2 },
                IrOp::Sub { dst: 0, a: 1, b: 2 },
                IrOp::Clz { dst: 0, src: 1 },
                IrOp::Madd {
                    dst: 0,
                    n: 1,
                    m: 2,
                    a: 3,
                },
                IrOp::Mov {
                    dst: 3,
                    imm: u64::MAX,
                },
                IrOp::Load {
                    dst: 4,
                    addr: 0x8000,
                    size: 8,
                },
                IrOp::Store {
                    src: 4,
                    addr: 0x9000,
                    size: 8,
                },
                IrOp::LoadDyn {
                    dst: 5,
                    base: 6,
                    off: 8,
                    size: 1,
                },
                IrOp::StoreDyn {
                    src: 5,
                    base: 6,
                    off: 8,
                    size: 1,
                },
                IrOp::CondBranch {
                    reg: 7,
                    target: 0x200,
                    when_zero: true,
                },
                IrOp::BranchDyn { reg: 30 },
                IrOp::OrrShift {
                    dst: 8,
                    a: 9,
                    b: 10,
                    shift: 0,
                    amount: 4,
                },
                IrOp::Wfi,
                IrOp::Trap { reason: "x" },
            ],
            exits: vec![BlockExit::Branch(0x200), BlockExit::ExitVm],
        };
        assert_eq!(compile(&block).bytes, compile(&block).bytes);
    }

    #[test]
    fn wave5_every_opcode_validates_under_wasmparser() {
        // One block per op kind (plus the Dynamic-exit malformed case):
        // all must be structurally valid WASM, not just byte-plausible.
        let cases: Vec<(Vec<IrOp>, Vec<BlockExit>)> = vec![
            (
                vec![IrOp::Add { dst: 0, a: 1, b: 2 }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::Sub { dst: 0, a: 1, b: 2 }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::Clz { dst: 0, src: 1 }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::Madd {
                    dst: 0,
                    n: 1,
                    m: 2,
                    a: 3,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::Mov { dst: 0, imm: 1 }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::Load {
                    dst: 0,
                    addr: 0x8000,
                    size: 8,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::Store {
                    src: 0,
                    addr: 0x8000,
                    size: 8,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::LoadDyn {
                    dst: 0,
                    base: 1,
                    off: 8,
                    size: 1,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::StoreDyn {
                    src: 0,
                    base: 1,
                    off: 8,
                    size: 1,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::Branch { target: 0x100 }],
                vec![BlockExit::Branch(0x100)],
            ),
            (vec![IrOp::BranchDyn { reg: 30 }], vec![BlockExit::Dynamic]),
            (
                vec![IrOp::CondBranch {
                    reg: 0,
                    target: 0x100,
                    when_zero: true,
                }],
                vec![BlockExit::Branch(0x100), BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::OrrShift {
                    dst: 0,
                    a: 1,
                    b: 2,
                    shift: 0,
                    amount: 0,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::OrrShift {
                    dst: 0,
                    a: 1,
                    b: 2,
                    shift: 1,
                    amount: 4,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::OrrShift {
                    dst: 0,
                    a: 1,
                    b: 2,
                    shift: 2,
                    amount: 63,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::AndShift {
                    dst: 0,
                    a: 1,
                    b: 2,
                    shift: 0,
                    amount: 0,
                    invert: false,
                    is_32: false,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::AndShift {
                    dst: 0,
                    a: 1,
                    b: 2,
                    shift: 1,
                    amount: 8,
                    invert: true,
                    is_32: true,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::OrShift {
                    dst: 0,
                    a: 1,
                    b: 2,
                    shift: 0,
                    amount: 0,
                    invert: true,
                    is_32: false,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::EorShift {
                    dst: 0,
                    a: 1,
                    b: 2,
                    shift: 2,
                    amount: 16,
                    invert: false,
                    is_32: false,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::Bitfield {
                    dst: 0,
                    src: 1,
                    opc: 2, // UBFM
                    immr: 16,
                    imms: 19,
                    is_32: false,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::Bitfield {
                    dst: 0,
                    src: 1,
                    opc: 0, // SBFM
                    immr: 0,
                    imms: 3,
                    is_32: true,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::Bitfield {
                    dst: 0,
                    src: 1,
                    opc: 1, // BFM
                    immr: 8,
                    imms: 15,
                    is_32: false,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::ShiftVar {
                    dst: 0,
                    a: 1,
                    b: 2,
                    shift: 0, // LSLV
                    is_32: false,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::ShiftVar {
                    dst: 0,
                    a: 1,
                    b: 2,
                    shift: 1, // LSRV
                    is_32: true,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::Movk {
                    dst: 0,
                    imm: 0x1234,
                    hw: 1,
                    is_32: false,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (
                vec![IrOp::Movk {
                    dst: 0,
                    imm: 0x5678,
                    hw: 0,
                    is_32: true,
                }],
                vec![BlockExit::FallThrough(0x4)],
            ),
            (vec![IrOp::Wfi], vec![BlockExit::FallThrough(0x4)]),
            (vec![IrOp::Trap { reason: "x" }], vec![BlockExit::ExitVm]),
            (vec![], vec![BlockExit::Dynamic]), // malformed -> unreachable, still valid
            (vec![], vec![BlockExit::ExitVm]),
        ];
        for (ops, exits) in &cases {
            let block = IrBlock {
                entry_addr: 0x4000,
                ops: ops.clone(),
                exits: exits.clone(),
            };
            assert_valid(&block);
        }
    }

    #[test]
    fn wave5_codegen_branchdyn_returns_register_value() {
        // RET X30: global.get 30; return; end — the host takes the i64
        // result as the next guest PC.
        let block = IrBlock {
            entry_addr: 0x4000,
            ops: vec![IrOp::BranchDyn { reg: 30 }],
            exits: vec![BlockExit::Dynamic],
        };
        let expr = assert_valid(&block);
        assert_eq!(expr, vec![0x23, 0x1E, 0x0F, 0x0B]);
    }

    #[test]
    fn wave5_codegen_bl_link_then_branch() {
        // BL as the lifter emits it: X30 = return addr, then static branch.
        let block = IrBlock {
            entry_addr: 0x4000_004C,
            ops: vec![
                IrOp::Mov {
                    dst: 30,
                    imm: 0x4000_0050,
                },
                IrOp::Branch {
                    target: 0x4000_0008,
                },
            ],
            exits: vec![BlockExit::Branch(0x4000_0008)],
        };
        let expr = assert_valid(&block);
        // i64.const 0x40000050; global.set 30; i64.const 0x40000008; return; end
        let ret = expr.iter().position(|&b| b == OP_RETURN).unwrap();
        assert_eq!(last_const(&expr[..ret]), 0x4000_0008);
        assert!(expr.windows(2).any(|w| w == [0x24, 0x1E])); // global.set 30
        assert_eq!(&expr[ret..], &[OP_RETURN, OP_END]);
    }

    #[test]
    fn wave5_codegen_branchdyn_xzr_source_is_zero() {
        // RET XZR is nonsense but must not read a nonexistent global 31.
        let block = IrBlock {
            entry_addr: 0x4000,
            ops: vec![IrOp::BranchDyn { reg: 31 }],
            exits: vec![BlockExit::Dynamic],
        };
        let expr = assert_valid(&block);
        assert_eq!(expr, vec![0x42, 0x00, 0x0F, 0x0B]); // i64.const 0; return; end
    }

    #[test]
    fn wave5_codegen_dynamic_exit_without_branchdyn_traps() {
        // Malformed: Dynamic exit declared but no BranchDyn op emitted.
        // Loud trap, never an invented address.
        let block = IrBlock {
            entry_addr: 0x4000,
            ops: vec![],
            exits: vec![BlockExit::Dynamic],
        };
        let expr = assert_valid(&block);
        assert_eq!(expr[0], OP_UNREACHABLE);
    }

    #[test]
    fn wave5_codegen_andshift_bic() {
        let block = IrBlock {
            entry_addr: 0x4000,
            ops: vec![IrOp::AndShift {
                dst: 1,
                a: 2,
                b: 3,
                shift: 0,
                amount: 0,
                invert: true,
                is_32: false,
            }],
            exits: vec![BlockExit::FallThrough(0x4004)],
        };
        let expr = assert_valid(&block);
        // global.get 2; global.get 3; i64.const -1; i64.xor; i64.and; global.set 1
        let seq = vec![
            0x23, 0x02, // global.get 2
            0x23, 0x03, // global.get 3
            0x42, 0x7F, // i64.const -1
            OP_I64_XOR, // 0x85
            OP_I64_AND, // 0x83
            0x24, 0x01, // global.set 1
        ];
        assert!(expr.windows(seq.len()).any(|w| w == seq.as_slice()));
    }

    #[test]
    fn wave5_codegen_shiftvar_lslv() {
        let block = IrBlock {
            entry_addr: 0x4000,
            ops: vec![IrOp::ShiftVar {
                dst: 8,
                a: 9,
                b: 10,
                shift: 0,
                is_32: false,
            }],
            exits: vec![BlockExit::FallThrough(0x4004)],
        };
        let expr = assert_valid(&block);
        let seq = vec![
            0x23, 0x09, // global.get 9
            0x23, 0x0A, // global.get 10
            0x42, 0x3F, // i64.const 63
            OP_I64_AND, // 0x83
            OP_I64_SHL, // 0x86
            0x24, 0x08, // global.set 8
        ];
        assert!(expr.windows(seq.len()).any(|w| w == seq.as_slice()));
    }

    #[test]
    fn wave5_codegen_movk_and_bitfield() {
        let block = IrBlock {
            entry_addr: 0x4000,
            ops: vec![
                IrOp::Movk {
                    dst: 5,
                    imm: 0x1234,
                    hw: 1,
                    is_32: false,
                },
                IrOp::Bitfield {
                    dst: 6,
                    src: 7,
                    opc: 2, // UBFM
                    immr: 2,
                    imms: 5,
                    is_32: true,
                },
                IrOp::Bitfield {
                    dst: 8,
                    src: 9,
                    opc: 0, // SBFM
                    immr: 0,
                    imms: 7,
                    is_32: false,
                },
            ],
            exits: vec![BlockExit::FallThrough(0x400C)],
        };
        assert_valid(&block);
    }

    // ---- structural validity (Wave 4: U3-G1 module shape) ----

    #[test]
    fn wave4_structural_section_order() {
        let m = compile(&sample_block());
        let b = &m.bytes;
        assert_eq!(&b[0..4], b"\0asm");
        assert_eq!(&b[4..8], &[0x01, 0x00, 0x00, 0x00]);
        let ids: Vec<u8> = sections(b).iter().map(|(id, _)| *id).collect();
        // type, import, function, export, code — ascending, as WASM orders them.
        assert_eq!(ids, vec![1, 2, 3, 7, 10]);
        let mut p = 8;
        while p < b.len() {
            p += 1; // section id
            let size = read_uleb(b, &mut p) as usize;
            p += size;
        }
        assert_eq!(p, b.len());
    }

    #[test]
    fn gbsysreg2_sysreg_emit_uses_host_call_indices() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![
                IrOp::ReadSys { dst: 2, reg: SysReg::SctlrEl1 },
                IrOp::WriteSys { src: 3, reg: SysReg::TpidrEl1 },
                IrOp::Branch { target: 0x8 },
            ],
            exits: vec![BlockExit::Branch(0x8)],
        };
        let expr = assert_valid(&block);
        // Walk the body; at each call record pending i64.consts and the
        // last global.get (register source).
        let mut pos = 0;
        let mut calls: Vec<String> = Vec::new();
        let mut consts: Vec<i64> = Vec::new();
        let mut last_get: Option<u64> = None;
        while pos < expr.len() {
            let op = expr[pos];
            pos += 1;
            match op {
                OP_I64_CONST => {
                    let v = read_sleb(&expr, &mut pos);
                    consts.push(v);
                }
                OP_GLOBAL_GET => {
                    last_get = Some(read_uleb(&expr, &mut pos));
                }
                OP_CALL => {
                    let f = read_uleb(&expr, &mut pos);
                    calls.push(format!("call {f} consts={consts:?} get={last_get:?}"));
                    consts.clear();
                    last_get = None;
                }
                OP_GLOBAL_SET | OP_DROP | OP_I64_ADD => {
                    if op != OP_I64_ADD {
                        consts.clear();
                        last_get = None;
                    }
                }
                _ => {}
            }
        }
        // ReadSys SctlrEl1 (SysReg index 2): const 2, const 0, call 3.
        assert!(
            calls.iter().any(|c| c == "call 3 consts=[2, 0] get=None"),
            "read calls: {calls:?}"
        );
        // WriteSys TpidrEl1 (SysReg index 1) from x3: const 1, get g3, call 4.
        assert!(
            calls.iter().any(|c| c == "call 4 consts=[1] get=Some(3)"),
            "write calls: {calls:?}"
        );
    }

    #[test]
    fn gb11_daif_rmw_uses_sysreg_host_calls() {
        // DaifRmw lowers to load(DAIF) -> or/and immediates -> store(DAIF).
        // call 3 = sysreg_load, call 4 = sysreg_store (contract order).
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![
                IrOp::DaifRmw { set: 0, clr: 0x200 },
                IrOp::DaifRmw { set: 0x80, clr: 0 },
                IrOp::Branch { target: 0x8 },
            ],
            exits: vec![BlockExit::Branch(0x8)],
        };
        let expr = assert_valid(&block);
        let mut pos = 0;
        let mut calls: Vec<String> = Vec::new();
        let mut consts: Vec<i64> = Vec::new();
        while pos < expr.len() {
            let op = expr[pos];
            pos += 1;
            match op {
                OP_I64_CONST => consts.push(read_sleb(&expr, &mut pos)),
                OP_CALL => {
                    let f = read_uleb(&expr, &mut pos);
                    calls.push(format!("call {f} consts={consts:?}"));
                    consts.clear();
                }
                _ => {}
            }
        }
        // Load DAIF (index 0): const 0, const 0, call 3.
        assert!(
            calls.iter().any(|c| c == "call 3 consts=[0, 0]"),
            "load calls: {calls:?}"
        );
        // clr=0x200 -> and with !0x200 = -513; then const 0 (reg), call 4.
        assert!(
            calls.iter().any(|c| c == "call 4 consts=[-513, 0]"),
            "clr-store calls: {calls:?}"
        );
        // set=0x80 -> or with 128; then const 0 (reg), call 4.
        assert!(
            calls.iter().any(|c| c == "call 4 consts=[128, 0]"),
            "set-store calls: {calls:?}"
        );
    }

    #[test]
    fn wave4_structural_imports_match_contract_order() {
        let im = imports(&compile(&sample_block()).bytes);
        // 6 funcs + 31 register globals, in contract declaration order.
        assert_eq!(im.len(), 37);
        assert_eq!(im[0], ("env".to_string(), "mem_load".to_string(), 0x00, 1));
        assert_eq!(im[1], ("env".to_string(), "mem_store".to_string(), 0x00, 2));
        assert_eq!(im[2], ("env".to_string(), "wfi".to_string(), 0x00, 3));
        assert_eq!(im[3], ("env".to_string(), "sysreg_load".to_string(), 0x00, 1));
        assert_eq!(im[4], ("env".to_string(), "sysreg_store".to_string(), 0x00, 4));
        assert_eq!(im[5], ("env".to_string(), "hvc".to_string(), 0x00, 5));
        for (i, entry) in im[6..].iter().enumerate() {
            assert_eq!(
                entry,
                &(
                    "env".to_string(),
                    format!("r{i}"),
                    0x03,
                    1 // mutability: var
                )
            );
        }
    }

    #[test]
    fn wave4_structural_run_export_is_func_6() {
        let module = compile(&sample_block());
        let secs = sections(&module.bytes);
        let ex = secs.iter().find(|(id, _)| *id == 7).unwrap().1;
        let mut pos = 0;
        assert_eq!(read_uleb(ex, &mut pos), 1); // one export
        assert_eq!(read_uleb(ex, &mut pos), 3); // name len
        assert_eq!(&ex[pos..pos + 3], b"run");
        pos += 3;
        assert_eq!(ex[pos], 0x00); // kind: func
        pos += 1;
        assert_eq!(read_uleb(ex, &mut pos), 6); // func index 6 (6 imports precede run)
    }

    #[test]
    fn wave4_structural_one_scratch_local() {
        let module = compile(&sample_block());
        let secs = sections(&module.bytes);
        let code = secs.iter().find(|(id, _)| *id == 10).unwrap().1;
        let mut pos = 0;
        assert_eq!(read_uleb(code, &mut pos), 1); // one body
        let body_size = read_uleb(code, &mut pos) as usize;
        let body_end = pos + body_size;
        assert_eq!(read_uleb(code, &mut pos), 1); // one local entry
        // 12 locals: local 0 = SCRATCH, locals 1-11 = Umulh/Smulh temporaries
        // (2026-10-03: was 1, expanded for multiply-high).
        assert_eq!(read_uleb(code, &mut pos), 12); // twelve locals
        assert_eq!(code[pos], VALTYPE_I64);
        assert!(body_end <= code.len());
    }

    // ---- codegen ----

    #[test]
    fn wave4_codegen_mov_add_use_globals() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![
                IrOp::Mov { dst: 1, imm: 5 },
                IrOp::Mov { dst: 2, imm: 7 },
                IrOp::Add { dst: 3, a: 1, b: 2 },
            ],
            exits: vec![],
        };
        let expr = assert_valid(&block);
        // i64.const 5; global.set 1
        assert!(expr.windows(4).any(|w| w == [0x42, 0x05, 0x24, 0x01]));
        // i64.const 7; global.set 2
        assert!(expr.windows(4).any(|w| w == [0x42, 0x07, 0x24, 0x02]));
        // global.get 1; global.get 2; i64.add; global.set 3
        assert!(expr
            .windows(7)
            .any(|w| w == [0x23, 0x01, 0x23, 0x02, 0x7C, 0x24, 0x03]));
    }

    #[test]
    fn gb5_codegen_mov_sub_use_globals() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![
                IrOp::Mov { dst: 1, imm: 10 },
                IrOp::Mov { dst: 2, imm: 3 },
                IrOp::Sub { dst: 3, a: 1, b: 2 },
            ],
            exits: vec![],
        };
        let expr = assert_valid(&block);
        // i64.const 10; global.set 1
        assert!(expr.windows(4).any(|w| w == [0x42, 0x0A, 0x24, 0x01]));
        // i64.const 3; global.set 2
        assert!(expr.windows(4).any(|w| w == [0x42, 0x03, 0x24, 0x02]));
        // global.get 1; global.get 2; i64.sub; global.set 3
        assert!(expr
            .windows(7)
            .any(|w| w == [0x23, 0x01, 0x23, 0x02, 0x7D, 0x24, 0x03]));
    }

    #[test]
    fn gb7_codegen_clz_uses_globals() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![IrOp::Clz { dst: 5, src: 5 }],
            exits: vec![],
        };
        let expr = assert_valid(&block);
        // global.get 5; i64.clz; global.set 5
        assert!(expr
            .windows(5)
            .any(|w| w == [0x23, 0x05, 0x79, 0x24, 0x05]));
    }

    #[test]
    fn gb8_codegen_madd_uses_globals() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![IrOp::Madd {
                dst: 5,
                n: 6,
                m: 7,
                a: 8,
            }],
            exits: vec![],
        };
        let expr = assert_valid(&block);
        // global.get 8; global.get 6; global.get 7; i64.mul; i64.add; global.set 5
        assert!(expr.windows(10).any(|w| w
            == [0x23, 0x08, 0x23, 0x06, 0x23, 0x07, 0x7E, 0x7C, 0x24, 0x05]));
    }

    #[test]
    fn wave4_codegen_xzr_source_is_const_zero_and_dest_drops() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![
                // ORR X11, XZR, X10 -> X11 = 0 | X10
                IrOp::OrrShift {
                    dst: 11,
                    a: 31,
                    b: 10,
                    shift: 0,
                    amount: 0,
                },
                // ADD XZR, X1, X2 -> computed then dropped
                IrOp::Add {
                    dst: 31,
                    a: 1,
                    b: 2,
                },
            ],
            exits: vec![],
        };
        let expr = assert_valid(&block);
        // XZR source: i64.const 0 (0x42 0x00), never global.get 31.
        assert!(expr.windows(2).any(|w| w == [0x42, 0x00]));
        assert!(!expr.windows(2).any(|w| w == [0x23, 0x1F]));
        // XZR dest: drop (0x1A), never global.set 31.
        assert!(expr.contains(&OP_DROP));
        assert!(!expr.windows(2).any(|w| w == [0x24, 0x1F]));
    }

    #[test]
    fn wave4_codegen_scratch32_uses_the_one_local() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![
                // ADD X1, X2, #5 as the lifter emits it: Mov32; Add.
                IrOp::Mov { dst: 32, imm: 5 },
                IrOp::Add {
                    dst: 1,
                    a: 2,
                    b: 32,
                },
            ],
            exits: vec![],
        };
        let expr = assert_valid(&block);
        // i64.const 5; local.set 0 ... local.get 0
        assert!(expr.windows(4).any(|w| w == [0x42, 0x05, 0x21, 0x00]));
        assert!(expr.windows(2).any(|w| w == [0x20, 0x00]));
    }

    #[test]
    fn wave4_codegen_loaddyn_calls_mem_load() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![IrOp::LoadDyn {
                dst: 2,
                base: 0,
                off: 0x100,
                size: 1,
            }],
            exits: vec![],
        };
        let expr = assert_valid(&block);
        // global.get 0; i64.const 0x100; i64.add; i64.const 1; call 0; global.set 2
        let seq: Vec<u8> = vec![
            0x23, 0x00, // global.get 0
            0x42, 0x80, 0x02, // i64.const 0x100 (sleb)
            0x7C, // i64.add
            0x42, 0x01, // i64.const 1
            0x10, 0x00, // call 0 (mem_load)
            0x24, 0x02, // global.set 2
        ];
        assert!(expr.windows(seq.len()).any(|w| w == seq.as_slice()));
    }

    #[test]
    fn wave4_codegen_storedyn_calls_mem_store() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![IrOp::StoreDyn {
                src: 31, // WZR -> const 0
                base: 11,
                off: 0,
                size: 1,
            }],
            exits: vec![],
        };
        let expr = assert_valid(&block);
        // global.get 11; i64.const 0; i64.add; i64.const 1; i64.const 0; call 1
        let seq: Vec<u8> = vec![
            0x23, 0x0B, // global.get 11
            0x42, 0x00, // i64.const 0
            0x7C, // i64.add
            0x42, 0x01, // i64.const 1
            0x42, 0x00, // i64.const 0 (WZR)
            0x10, 0x01, // call 1 (mem_store)
        ];
        assert!(expr.windows(seq.len()).any(|w| w == seq.as_slice()));
    }

    #[test]
    fn wave4_codegen_cbz_taken_on_eqz() {
        let block = IrBlock {
            entry_addr: 0x4000,
            ops: vec![IrOp::CondBranch {
                reg: 2,
                target: 0x4010,
                when_zero: true,
            }],
            exits: vec![BlockExit::Branch(0x4010), BlockExit::FallThrough(0x4004)],
        };
        let expr = assert_valid(&block);
        // global.get 2; i64.eqz; if (result i64); i64.const 0x4010; else;
        // i64.const 0x4004; end; return; end
        let seq: Vec<u8> = vec![
            0x23, 0x02, // global.get 2
            0x50, // i64.eqz
            0x04, 0x7E, // if (result i64)
            0x42, 0x90, 0x80, 0x01, // i64.const 0x4010 (sleb)
            0x05, // else
            0x42, 0x84, 0x80, 0x01, // i64.const 0x4004 (sleb)
            0x0B, // end (if)
            0x0F, // return
            0x0B, // end (func)
        ];
        assert!(expr.windows(seq.len()).any(|w| w == seq.as_slice()));
    }

    #[test]
    fn wave4_codegen_cbnz_swaps_arms() {
        let block = IrBlock {
            entry_addr: 0x4000,
            ops: vec![IrOp::CondBranch {
                reg: 0,
                target: 0x4000,
                when_zero: false,
            }],
            exits: vec![BlockExit::Branch(0x4000), BlockExit::FallThrough(0x4004)],
        };
        let expr = assert_valid(&block);
        // CBNZ: eqz true (reg==0) -> fallthrough 0x4004; else -> target 0x4000.
        let seq: Vec<u8> = vec![
            0x23, 0x00, // global.get 0
            0x50, // i64.eqz
            0x04, 0x7E, // if (result i64)
            0x42, 0x84, 0x80, 0x01, // i64.const 0x4004 (fallthrough first)
            0x05, // else
            0x42, 0x80, 0x80, 0x01, // i64.const 0x4000
            0x0B, 0x0F, 0x0B,
        ];
        assert!(expr.windows(seq.len()).any(|w| w == seq.as_slice()));
    }

    #[test]
    fn wave4_codegen_condbranch_without_fallthrough_traps() {
        let block = IrBlock {
            entry_addr: 0x4000,
            ops: vec![IrOp::CondBranch {
                reg: 2,
                target: 0x4010,
                when_zero: true,
            }],
            exits: vec![BlockExit::Branch(0x4010)], // malformed: no fallthrough
        };
        let expr = assert_valid(&block);
        assert_eq!(expr[0], OP_UNREACHABLE);
    }

    #[test]
    fn wave4_codegen_orrshift_lsl() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![IrOp::OrrShift {
                dst: 2,
                a: 31,
                b: 1,
                shift: 0,
                amount: 56,
            }],
            exits: vec![],
        };
        let expr = assert_valid(&block);
        // i64.const 0; global.get 1; i64.const 56; i64.shl; i64.or; global.set 2
        let seq: Vec<u8> = vec![
            0x42, 0x00, // i64.const 0 (XZR)
            0x23, 0x01, // global.get 1
            0x42, 0x38, // i64.const 56
            0x86, // i64.shl (0x86, not the i32 form 0x74)
            0x84, // i64.or (0x84, not the i32 form 0x72)
            0x24, 0x02, // global.set 2
        ];
        assert!(expr.windows(seq.len()).any(|w| w == seq.as_slice()));
    }

    #[test]
    fn wave4_codegen_orrshift_bad_shift_traps() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![IrOp::OrrShift {
                dst: 2,
                a: 1,
                b: 1,
                shift: 3, // reserved: contract violation
                amount: 0,
            }],
            exits: vec![],
        };
        let expr = assert_valid(&block);
        assert_eq!(expr[0], OP_UNREACHABLE);
    }

    #[test]
    fn wave4_codegen_wfi_calls_host_then_falls_through() {
        let block = IrBlock {
            entry_addr: 0x4000,
            ops: vec![IrOp::Wfi],
            exits: vec![BlockExit::FallThrough(0x4004)],
        };
        let expr = assert_valid(&block);
        // call 2 (wfi); i64.const 0x4004; end
        let seq: Vec<u8> = vec![
            0x10, 0x02, // call 2
            0x42, 0x84, 0x80, 0x01, // i64.const 0x4004 (epilogue)
            0x0B,
        ];
        assert!(expr.windows(seq.len()).any(|w| w == seq.as_slice()));
    }

    #[test]
    fn gb12_codegen_static_load_calls_mem_load() {
        // Mirrors the measured GB-11 halt: LDR X5, [PC, #84] at 0x40c03674
        // (word 0x580002a5) lifts to Load { dst: 5, addr: 0x40c036c8, size: 8 }.
        let block = IrBlock {
            entry_addr: 0x40c03674,
            ops: vec![IrOp::Load {
                dst: 5,
                addr: 0x40c036c8,
                size: 8,
            }],
            exits: vec![],
        };
        let expr = assert_valid(&block);
        // i64.const 0x40c036c8; i64.const 8; call 0 (mem_load); global.set 5
        let seq: Vec<u8> = vec![
            0x42, 0xC8, 0xED, 0x80, 0x86, 0x04, // i64.const 0x40c036c8 (sleb)
            0x42, 0x08, // i64.const 8
            0x10, 0x00, // call 0 (mem_load)
            0x24, 0x05, // global.set 5
        ];
        // The empty-exits epilogue appends unreachable by design
        // (malformed-block rule); the lowered op sequence above is the real pin.
        assert!(expr.windows(seq.len()).any(|w| w == seq.as_slice()));
    }

    #[test]
    fn gb12_codegen_static_store_calls_mem_store() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![IrOp::Store {
                src: 1,
                addr: 0x9000,
                size: 8,
            }],
            exits: vec![],
        };
        let expr = assert_valid(&block);
        // i64.const 0x9000; i64.const 8; global.get 1; call 1 (mem_store)
        let seq: Vec<u8> = vec![
            0x42, 0x80, 0xA0, 0x02, // i64.const 0x9000 (sleb)
            0x42, 0x08, // i64.const 8
            0x23, 0x01, // global.get 1
            0x10, 0x01, // call 1 (mem_store)
        ];
        // The empty-exits epilogue appends unreachable by design
        // (malformed-block rule); the lowered op sequence above is the real pin.
        assert!(expr.windows(seq.len()).any(|w| w == seq.as_slice()));
    }

    #[test]
    fn wave4_codegen_trap_emits_unreachable_first() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![IrOp::Trap {
                reason: "unimplemented",
            }],
            exits: vec![],
        };
        let expr = assert_valid(&block);
        assert_eq!(expr[0], OP_UNREACHABLE);
    }

    #[test]
    fn wave4_codegen_branch_returns_target_and_stops() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![
                IrOp::Branch { target: 0x5000 },
                IrOp::Mov { dst: 9, imm: 1 }, // dead: must not be emitted
            ],
            exits: vec![BlockExit::ExitVm],
        };
        let expr = assert_valid(&block);
        // ... i64.const 0x5000; return; end — nothing after return.
        let ret = expr.iter().position(|&b| b == OP_RETURN).unwrap();
        assert_eq!(last_const(&expr[..ret]), 0x5000);
        assert_eq!(&expr[ret..], &[OP_RETURN, OP_END]);
    }

    #[test]
    fn wave4_codegen_exit_fallthrough_encodes_target_address() {
        let block = IrBlock {
            entry_addr: 0x4000,
            ops: vec![],
            exits: vec![BlockExit::FallThrough(0x4008)],
        };
        let expr = assert_valid(&block);
        assert_eq!(last_const(&expr), 0x4008);
    }

    #[test]
    fn wave4_codegen_exit_vm_encodes_sentinel() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![],
            exits: vec![BlockExit::ExitVm],
        };
        let expr = assert_valid(&block);
        assert_eq!(last_const(&expr), -1);
    }

    #[test]
    fn wave4_codegen_first_exit_wins_deterministically() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![], // no CondBranch; plain epilogue
            exits: vec![BlockExit::Branch(0x300), BlockExit::FallThrough(0x400)],
        };
        let expr = assert_valid(&block);
        assert_eq!(last_const(&expr), 0x300);
    }

    #[test]
    fn wave4_codegen_mov_max_imm_encodes_negative_one() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![IrOp::Mov {
                dst: 0,
                imm: u64::MAX,
            }],
            exits: vec![],
        };
        let expr = assert_valid(&block);
        // i64.const -1 is a single 0x7F byte in signed LEB128.
        assert!(expr.windows(2).any(|w| w == [0x42, 0x7F]));
        assert_eq!(last_const(&expr), -1);
    }
}
