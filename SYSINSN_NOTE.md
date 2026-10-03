# SYSINSN Analysis: 0xDAC00129 is RBIT, not a system instruction

## Initial hypothesis (WRONG)

The parent task hypothesized 0xDAC00129 was a system instruction (MRS/MSR/HINT/barrier)
based on the 0xDA top byte. This was incorrect.

## Correct identification

Capstone disassembly proves:
```
0xDAC00129 = rbit x9, x9
```

**RBIT** (Reverse Bits) is a Data Processing (1-source) instruction, not a system instruction.
It reverses the bit order of the source register.

### Encoding breakdown

```
0xDAC00129 = 1101 1010 1100 0000 0000 0001 0010 1001
- bits[31:27] = 11011 (Data Processing - 1 source class)
- bits[30:24] = 0b1011010 (0x5A, S=0)
- bits[23:21] = 0b110
- bits[20:16] = 00000
- bits[15:10] = 0b000000 (opcode = RBIT)
- bits[9:5]   = 01001 (Rn = X9)
- bits[4:0]   = 01001 (Rd = X9)
```

1-source opcodes (bits[15:10]):
- 0b000000 = RBIT
- 0b000001 = REV16
- 0b000010 = REV32/REV
- 0b000011 = REV64
- 0b000100 = CLZ
- 0b000101 = CLS (probably)

## Implementation

Added RBIT to the u12-orchestrator fast-path interpreter (`execute_arm64`, section 6e),
following the existing REV pattern:

```rust
// RBIT 64-bit: reverse all 64 bits.
(1, 0b000000) => rn_val.reverse_bits(),
// RBIT 32-bit: reverse low 32 bits, zero-extend.
(0, 0b000000) => (rn_val as u32).reverse_bits() as u64,
```

### Why fast-path only (not u2-ir-lift)

- The boot uses the u12 fast-path interpreter.
- u2-ir-lift has no `IrOp::Rbit` and explicitly traps RBIT (test
  `trap_clz_sibling_stays_unsupported` asserts this).
- Adding a new IrOp + lifter + WASM backend support is a larger change;
  the fast-path fix unblocks boot with correct semantics.
- The u2 trap test remains valid (u2 still doesn't support RBIT, but the
  fast-path handles it before U2 is invoked).

## Tests

- `gb27_rbit_64bit`: Exact kernel word 0xDAC00129, verifies bit reversal.
- `gb27_rbit_32bit`: 32-bit form (0x5AC00129), verifies zero-extension.
- Full u12 suite: 118 passed (116 existing + 2 new), 5 RAM-heavy skipped.

## Boot remeasurement

**RBIT fix verified working.** Boot progressed from 20,647,443 to 23,500,000+ steps
(+2.9M steps) before entering a tight loop.

| Metric | Before RBIT fix | After RBIT fix |
|--------|----------------|----------------|
| Steps | 20,647,443 (halt) | 23,500,000+ (still running at 30M cap) |
| Halt PC | 0xffffff80085c9610 | N/A (no halt, tight loop) |
| Halt word | 0xDAC00129 (RBIT) | N/A |
| Loop PC | N/A | 0xffffff80081842c0 |

**New behavior:** After passing the RBIT instruction, the kernel continues for ~2.9M steps
then enters a tight loop at PC 0xffffff80081842c0 (PC stable for 2M+ steps). This appears
to be a hardware spin-wait (device register poll), not an instruction halt. Identifying
the device is a separate task.

**Verdict:** RBIT implementation is correct and unblocks boot. The halt-chasing phase
continues with a new (non-instruction) blocker.
