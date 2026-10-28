# T631 QEMU oracle vectors (QEMU 11.1.2, cortex-a53, mach-virt)

Bare-metal guest `t631-q.s`, assembled with aarch64-elf-as, run under
`/opt/homebrew/bin/qemu-system-aarch64 -machine virt -cpu cortex-a53`.
The guest enables FP/SIMD first (`mov x2, #(3 << 20); msr cpacr_el1, x2;
isb` — reset state traps the first LDP Q to VBAR+0x200, verified by
register dump: PC 0x200, PSTATE EL1h, FPU disabled, X0/X1 already holding
the input/output addresses), then executes five LDP/STP Q variants and
parks in a WFI loop. Results are read back with HMP `pmemsave` on the
`output` symbol (address resolved from `aarch64-elf-nm` at run time, not
hardcoded — an earlier hardcoded address was off by 16 bytes after the
FP-enable lines moved `.data`, which showed up as a one-slot rotation
and was corrected, not papered over).

Independent encoding check: the assembler emits exactly `ad400400` for
`ldp q0, q1, [x0]` — the T631 wall word.

Input table (14 Q pairs, little-endian lo then hi):
in+0 (1122334455667788, 99aabbccddeeff00), in+16 (0123456789abcdef,
fedcba9876543210), in+32 (1111111111111111, 2222222222222222), in+48
(3333333333333333, 4444444444444444), in+64 (5555555555555555,
6666666666666666), in+80 (7777777777777777, 8888888888888888), in+96
(9999999999999999, aaaaaaaaaaaaaaaa), in+112 (bbbbbbbbbbbbbbbb,
cccccccccccccccc), in+128 (dddddddddddddddd, eeeeeeeeeeeeeeee), in+144
(1020304050607080, 090a0b0c0d0e0f10), in+160 (1112131415161718,
191a1b1c1d1e1f20), in+176 (2122232425262728, 292a2b2c2d2e2f30), in+192
(3132333435363738, 3940414243444546), in+208 (4748494a4b4c4d4e,
4f50515253545556).

Observed output (all 10 slots match — run 2026-10-28, QEMU 11.1.2):
- out+0: (1122334455667788, 99aabbccddeeff00) — A: LDP Q0,Q1,[X0]
- out+16: (0123456789abcdef, fedcba9876543210) — A second half
- out+32: (9999999999999999, aaaaaaaaaaaaaaaa) — B: LDP Q2,Q3,[X0,#32]!
- out+48: (bbbbbbbbbbbbbbbb, cccccccccccccccc) — B second half
- out+64: (dddddddddddddddd, eeeeeeeeeeeeeeee) — C: LDP Q4,Q5,[X0],#32
- out+80: (1020304050607080, 090a0b0c0d0e0f10) — C second half
- out+96: (1112131415161718, 191a1b1c1d1e1f20) — D: LDP Q6,Q7,[X0,#-16]
- out+112: (2122232425262728, 292a2b2c2d2e2f30) — D second half
- out+128: (3132333435363738, 3940414243444546) — E: LDNP Q8,Q9,[X0]
- out+144: (4748494a4b4c4d4e, 4f50515253545556) — E second half

Semantics confirmed byte-for-byte: first register takes mem[addr..+16]
(lo = first 8 bytes LE), second takes mem[addr+16..+32]; pre-index adds
the offset before the access and writes it back; post-index accesses
then writes back; negative offsets sign-extend and scale by 16;
non-temporal forms behave identically on this data path. The engine's
lifter emits exactly these address sequences (u2 T631 tests), and the
wasmi backend round-trips one pair end to end (u15-exec-wasmi T631
test); the full boot is the system-level check.
