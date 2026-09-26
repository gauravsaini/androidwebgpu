//! U3 — `wasm-jit`: IR block → deterministic WASM module bytes.
//!
//! PURE: `compile` is a total, deterministic function of `&IrBlock`.
//! No I/O, no time, no threads, no hidden state. Same input bytes →
//! byte-identical output (contract with the orchestrator, U12).
//!
//! Codegen model (minimal but honest):
//! - One function `() -> i64`. The i64 result is the *exit address* the
//!   block transfers control to: `FallThrough(a)` / `Branch(a)` yield `a`,
//!   `ExitVm` (or an empty exit list) yields `-1` as a sentinel.
//! - 256 `i64` locals model the register file; `IrOp` register indices are
//!   `u8`, so every index is in range and the module always validates.
//! - `Add` / `Mov` lower to real stack-machine arithmetic.
//! - `IrOp::Branch { target }` terminates the block: `i64.const target;
//!   return`. Ops after it are dead by definition, so emission stops there.
//! - `Load` / `Store` lower to `unreachable`. Guest memory is not modeled in
//!   this standalone module (that is U4's job); trapping loudly beats
//!   faking a memory access. Same for `IrOp::Trap`, per LLD §3.
//! - `Trap { reason }`: the reason string is a host-side diagnostic and is
//!   deliberately NOT encoded — encoding it would break byte-determinism
//!   across builds. The trap itself is always emitted.
//! - With several exits, only the FIRST is lowered (documented); multi-exit
//!   dispatch belongs to the orchestrator.

use pathn_contracts::cpu::{BlockExit, IrBlock, IrOp, WasmModule};

// WASM opcodes (MVP core).
const OP_UNREACHABLE: u8 = 0x00;
const OP_RETURN: u8 = 0x0F;
const OP_LOCAL_GET: u8 = 0x20;
const OP_LOCAL_SET: u8 = 0x21;
const OP_I64_CONST: u8 = 0x42;
const OP_I64_ADD: u8 = 0x7C;
const OP_END: u8 = 0x0B;

const VALTYPE_I64: u8 = 0x7E;

/// One i64 local per possible u8 register index: every register operand is
/// always a valid local index, so the module validates without clamping.
const REG_LOCALS: u32 = 256;

/// Sentinel result when the block exits the VM (or declares no exits).
const EXIT_VM_SENTINEL: i64 = -1;

/// Compile one IR block to a structurally valid WASM module.
pub fn compile(block: &IrBlock) -> WasmModule {
    let mut out = Vec::new();
    // Magic + version.
    out.extend_from_slice(b"\0asm");
    out.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);

    // Type section (id 1): a single type `() -> (i64)`.
    let mut ty = Vec::new();
    uleb(1, &mut ty); // one type
    ty.push(0x60); // func
    uleb(0, &mut ty); // no params
    uleb(1, &mut ty); // one result
    ty.push(VALTYPE_I64);
    section(1, &ty, &mut out);

    // Function section (id 3): one function of type 0.
    let mut func = Vec::new();
    uleb(1, &mut func);
    uleb(0, &mut func);
    section(3, &func, &mut out);

    // Code section (id 10): one body.
    let mut body = Vec::new();
    uleb(1, &mut body); // one local entry: REG_LOCALS x i64
    uleb(u64::from(REG_LOCALS), &mut body);
    body.push(VALTYPE_I64);

    let mut branched = false;
    for op in &block.ops {
        match op {
            IrOp::Add { dst, a, b } => {
                body.push(OP_LOCAL_GET);
                uleb(u64::from(*a), &mut body);
                body.push(OP_LOCAL_GET);
                uleb(u64::from(*b), &mut body);
                body.push(OP_I64_ADD);
                body.push(OP_LOCAL_SET);
                uleb(u64::from(*dst), &mut body);
            }
            IrOp::Mov { dst, imm } => {
                body.push(OP_I64_CONST);
                sleb(*imm as i64, &mut body);
                body.push(OP_LOCAL_SET);
                uleb(u64::from(*dst), &mut body);
            }
            // No guest memory in this module: trap loudly, never fake.
            IrOp::Load { .. } | IrOp::Store { .. } | IrOp::Trap { .. } => {
                body.push(OP_UNREACHABLE);
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
                OP_LOCAL_GET | OP_LOCAL_SET => {
                    read_uleb(expr, &mut pos);
                }
                _ => {}
            }
        }
        last
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
                IrOp::Trap { reason: "x" },
            ],
            exits: vec![BlockExit::Branch(0x200), BlockExit::ExitVm],
        };
        assert_eq!(compile(&block).bytes, compile(&block).bytes);
    }

    // ---- structural validity ----

    #[test]
    fn structural_magic_version_and_section_order() {
        let m = compile(&sample_block());
        let b = &m.bytes;
        assert_eq!(&b[0..4], b"\0asm");
        assert_eq!(&b[4..8], &[0x01, 0x00, 0x00, 0x00]);
        let ids: Vec<u8> = sections(b).iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![1, 3, 10]); // type, function, code — ascending
                                         // Section sizes are well-formed: a raw re-walk consumes the module exactly.
        let mut p = 8;
        while p < b.len() {
            p += 1; // section id
            let size = read_uleb(b, &mut p) as usize;
            p += size;
        }
        assert_eq!(p, b.len());
    }

    #[test]
    fn structural_code_body_declares_256_i64_locals() {
        let m = compile(&sample_block());
        let secs = sections(&m.bytes);
        let code = secs.iter().find(|(id, _)| *id == 10).unwrap().1;
        let mut pos = 0;
        assert_eq!(read_uleb(code, &mut pos), 1);
        let body_size = read_uleb(code, &mut pos) as usize;
        let body_end = pos + body_size;
        assert_eq!(read_uleb(code, &mut pos), 1); // one local entry
        assert_eq!(read_uleb(code, &mut pos), 256); // 256 locals
        assert_eq!(code[pos], VALTYPE_I64);
        assert!(body_end <= code.len());
    }

    // ---- codegen ----

    #[test]
    fn codegen_mov_add_emits_expected_opcodes() {
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
        // i64.const 5; local.set 1
        assert!(expr.windows(4).any(|w| w == [0x42, 0x05, 0x21, 0x01]));
        // i64.const 7; local.set 2
        assert!(expr.windows(4).any(|w| w == [0x42, 0x07, 0x21, 0x02]));
        // local.get 1; local.get 2; i64.add; local.set 3
        assert!(expr
            .windows(7)
            .any(|w| w == [0x20, 0x01, 0x20, 0x02, 0x7C, 0x21, 0x03]));
    }

    #[test]
    fn codegen_mov_max_imm_encodes_negative_one() {
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

    #[test]
    fn codegen_trap_emits_unreachable_first() {
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
    fn codegen_load_store_trap_loudly() {
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
        // Both memory ops lower to unreachable; nothing is silently faked.
        assert_eq!(expr[0], OP_UNREACHABLE);
        assert_eq!(expr[1], OP_UNREACHABLE);
    }

    #[test]
    fn codegen_branch_returns_target_and_stops() {
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
    fn codegen_exit_fallthrough_encodes_target_address() {
        let block = IrBlock {
            entry_addr: 0x4000,
            ops: vec![],
            exits: vec![BlockExit::FallThrough(0x4008)],
        };
        let expr = code_expr(&compile(&block).bytes);
        assert_eq!(last_const(&expr), 0x4008);
    }

    #[test]
    fn codegen_exit_vm_encodes_sentinel() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![],
            exits: vec![BlockExit::ExitVm],
        };
        let expr = code_expr(&compile(&block).bytes);
        assert_eq!(last_const(&expr), -1);
    }

    #[test]
    fn codegen_first_exit_wins_deterministically() {
        let block = IrBlock {
            entry_addr: 0,
            ops: vec![],
            exits: vec![BlockExit::Branch(0x300), BlockExit::FallThrough(0x400)],
        };
        let expr = code_expr(&compile(&block).bytes);
        assert_eq!(last_const(&expr), 0x300);
    }
}
