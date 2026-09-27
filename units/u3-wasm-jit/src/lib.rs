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
//! - `Load` / `Store` (static) STILL lower to `unreachable`: the static forms
//!   are not in Wave-4 scope — trapping loudly beats faking a memory access.
//!   Same for `IrOp::Trap`, per LLD §3.
//! - `Trap { reason }`: the reason string is a host-side diagnostic and is
//!   deliberately NOT encoded — encoding it would break byte-determinism
//!   across builds. The trap itself is always emitted.
//! - With several exits, only the FIRST is lowered (documented); multi-exit
//!   dispatch belongs to the orchestrator. `CondBranch` takes its
//!   fallthrough address from the block's `BlockExit::FallThrough`; a block
//!   without one is malformed and compiles the branch to `unreachable`.

use pathn_contracts::cpu::{BlockExit, IrBlock, IrOp, WasmModule};

// WASM opcodes (MVP core).
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
const OP_I64_OR: u8 = 0x72;
const OP_I64_SHL: u8 = 0x74;
const OP_I64_SHR_S: u8 = 0x75;
const OP_I64_SHR_U: u8 = 0x76;
const OP_I64_ADD: u8 = 0x7C;

const VALTYPE_I64: u8 = 0x7E;

// Imported-function indices in the module (contract order: mem_load,
// mem_store, wfi — see pathn_contracts::wasm_abi).
const FUNC_MEM_LOAD: u32 = 0;
const FUNC_MEM_STORE: u32 = 1;
const FUNC_WFI: u32 = 2;
/// Index of `run` (3 imported functions precede it).
const FUNC_RUN: u32 = 3;

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

    // Type section (id 1): four types.
    //   0: () -> (i64)            — run
    //   1: (i64, i64) -> (i64)    — mem_load
    //   2: (i64, i64, i64) -> ()  — mem_store
    //   3: () -> ()               — wfi
    let mut ty = Vec::new();
    uleb(4, &mut ty);
    func_type(&[], &[VALTYPE_I64], &mut ty);
    func_type(&[VALTYPE_I64, VALTYPE_I64], &[VALTYPE_I64], &mut ty);
    func_type(&[VALTYPE_I64, VALTYPE_I64, VALTYPE_I64], &[], &mut ty);
    func_type(&[], &[], &mut ty);
    section(1, &ty, &mut out);

    // Import section (id 2): env.mem_load, env.mem_store, env.wfi,
    // then env.r0 .. env.r30 as mutable i64 globals. ORDER IS CONTRACT.
    let mut im = Vec::new();
    uleb(3 + 31, &mut im);
    import_func("mem_load", 1, &mut im);
    import_func("mem_store", 2, &mut im);
    import_func("wfi", 3, &mut im);
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

    // Code section (id 10): one body, one i64 local (U2 SCRATCH).
    let mut body = Vec::new();
    uleb(1, &mut body); // one local entry: 1 x i64
    uleb(1, &mut body);
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
            IrOp::Mov { dst, imm } => {
                body.push(OP_I64_CONST);
                sleb(*imm as i64, &mut body);
                reg_set(&mut body, *dst);
            }
            // No static guest memory in this module: trap loudly, never fake.
            IrOp::Load { .. } | IrOp::Store { .. } | IrOp::Trap { .. } => {
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
            IrOp::Wfi => {
                body.push(OP_CALL);
                uleb(u64::from(FUNC_WFI), &mut body);
                // Falls through to the exit epilogue; the host records the
                // WFI and the orchestrator yields the vCPU after return.
            }
            IrOp::Branch { target } => {
                body.push(OP_I64_CONST);
                sleb(*target as i64, &mut body);
                body.push(OP_RETURN);
                branched = true;
                break;
            }
        }
    }

    if !branched {
        // Exit epilogue: result = where control goes next.
        let exit_addr: i64 = match block.exits.first() {
            Some(BlockExit::FallThrough(a)) | Some(BlockExit::Branch(a)) => *a as i64,
            Some(BlockExit::ExitVm) | None => EXIT_VM_SENTINEL,
        };
        body.push(OP_I64_CONST);
        sleb(exit_addr, &mut body);
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
    fn wave4_structural_imports_match_contract_order() {
        let im = imports(&compile(&sample_block()).bytes);
        // 3 funcs + 31 register globals, in contract declaration order.
        assert_eq!(im.len(), 34);
        assert_eq!(im[0], ("env".to_string(), "mem_load".to_string(), 0x00, 1));
        assert_eq!(im[1], ("env".to_string(), "mem_store".to_string(), 0x00, 2));
        assert_eq!(im[2], ("env".to_string(), "wfi".to_string(), 0x00, 3));
        for (i, entry) in im[3..].iter().enumerate() {
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
    fn wave4_structural_run_export_is_func_3() {
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
        assert_eq!(read_uleb(ex, &mut pos), 3); // func index 3
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
        assert_eq!(read_uleb(code, &mut pos), 1); // one local
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
        let expr = code_expr(&compile(&block).bytes);
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
        let expr = code_expr(&compile(&block).bytes);
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
        let expr = code_expr(&compile(&block).bytes);
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
        let expr = code_expr(&compile(&block).bytes);
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
        let expr = code_expr(&compile(&block).bytes);
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
        let expr = code_expr(&compile(&block).bytes);
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
        let expr = code_expr(&compile(&block).bytes);
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
        let expr = code_expr(&compile(&block).bytes);
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
        let expr = code_expr(&compile(&block).bytes);
        // i64.const 0; global.get 1; i64.const 56; i64.shl; i64.or; global.set 2
        let seq: Vec<u8> = vec![
            0x42, 0x00, // i64.const 0 (XZR)
            0x23, 0x01, // global.get 1
            0x42, 0x38, // i64.const 56
            0x74, // i64.shl
            0x72, // i64.or
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
        let expr = code_expr(&compile(&block).bytes);
        assert_eq!(expr[0], OP_UNREACHABLE);
    }

    #[test]
    fn wave4_codegen_wfi_calls_host_then_falls_through() {
        let block = IrBlock {
            entry_addr: 0x4000,
            ops: vec![IrOp::Wfi],
            exits: vec![BlockExit::FallThrough(0x4004)],
        };
        let expr = code_expr(&compile(&block).bytes);
        // call 2 (wfi); i64.const 0x4004; end
        let seq: Vec<u8> = vec![
            0x10, 0x02, // call 2
            0x42, 0x84, 0x80, 0x01, // i64.const 0x4004 (epilogue)
            0x0B,
        ];
        assert!(expr.windows(seq.len()).any(|w| w == seq.as_slice()));
    }

    #[test]
    fn wave4_codegen_static_load_store_still_trap() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![
                IrOp::Load {
                    dst: 1,
                    addr: 0x8000,
                    size: 8,
                },
                IrOp::Store {
                    src: 1,
                    addr: 0x8000,
                    size: 8,
                },
            ],
            exits: vec![],
        };
        let expr = code_expr(&compile(&block).bytes);
        // Both static memory ops lower to unreachable; nothing is silently faked.
        assert_eq!(expr[0], OP_UNREACHABLE);
        assert_eq!(expr[1], OP_UNREACHABLE);
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
        let expr = code_expr(&compile(&block).bytes);
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
        let expr = code_expr(&compile(&block).bytes);
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
        let expr = code_expr(&compile(&block).bytes);
        assert_eq!(last_const(&expr), 0x4008);
    }

    #[test]
    fn wave4_codegen_exit_vm_encodes_sentinel() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![],
            exits: vec![BlockExit::ExitVm],
        };
        let expr = code_expr(&compile(&block).bytes);
        assert_eq!(last_const(&expr), -1);
    }

    #[test]
    fn wave4_codegen_first_exit_wins_deterministically() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![], // no CondBranch; plain epilogue
            exits: vec![BlockExit::Branch(0x300), BlockExit::FallThrough(0x400)],
        };
        let expr = code_expr(&compile(&block).bytes);
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
        let expr = code_expr(&compile(&block).bytes);
        // i64.const -1 is a single 0x7F byte in signed LEB128.
        assert!(expr.windows(2).any(|w| w == [0x42, 0x7F]));
        assert_eq!(last_const(&expr), -1);
    }
}
