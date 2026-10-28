    .text
    .globl _start
_start:
    // Reset state has FP/SIMD trapped (CPACR_EL1.FPEN == 0): the first
    // LDP Q would vector to VBAR+0x200. Enable FP/SIMD first (EL1h).
    mov x2, #(3 << 20)
    msr cpacr_el1, x2
    isb
    adrp x0, input
    add x0, x0, :lo12:input
    adrp x1, output
    add x1, x1, :lo12:output
    // Variant A: the T631 wall form, signed offset.
    ldp q0, q1, [x0]
    stp q0, q1, [x1]
    // Variant B: pre-index. X0 = input+64, then +32 -> input+96.
    adrp x0, input
    add x0, x0, :lo12:input
    add x0, x0, #64
    ldp q2, q3, [x0, #32]!
    stp q2, q3, [x1, #32]
    // Variant C: post-index. X0 = input+128, reads +128/+144, then +32.
    adrp x0, input
    add x0, x0, :lo12:input
    add x0, x0, #128
    ldp q4, q5, [x0], #32
    stp q4, q5, [x1, #64]
    // Variant D: negative offset. X0 = input+176, reads +160/+176.
    adrp x0, input
    add x0, x0, :lo12:input
    add x0, x0, #176
    ldp q6, q7, [x0, #-16]
    stp q6, q7, [x1, #96]
    // Variant E: non-temporal. X0 = input+192, reads +192/+208.
    adrp x0, input
    add x0, x0, :lo12:input
    add x0, x0, #192
    ldnp q8, q9, [x0]
    stnp q8, q9, [x1, #128]
1:  wfi
    b 1b

    .data
    .balign 16
input:
    .quad 0x1122334455667788, 0x99aabbccddeeff00
    .quad 0x0123456789abcdef, 0xfedcba9876543210
    .quad 0x1111111111111111, 0x2222222222222222
    .quad 0x3333333333333333, 0x4444444444444444
    .quad 0x5555555555555555, 0x6666666666666666
    .quad 0x7777777777777777, 0x8888888888888888
    .quad 0x9999999999999999, 0xaaaaaaaaaaaaaaaa
    .quad 0xbbbbbbbbbbbbbbbb, 0xcccccccccccccccc
    .quad 0xdddddddddddddddd, 0xeeeeeeeeeeeeeeee
    .quad 0x1020304050607080, 0x090a0b0c0d0e0f10
    .quad 0x1112131415161718, 0x191a1b1c1d1e1f20
    .quad 0x2122232425262728, 0x292a2b2c2d2e2f30
    .quad 0x3132333435363738, 0x3940414243444546
    .quad 0x4748494a4b4c4d4e, 0x4f50515253545556
    .balign 16
output:
    .space 160, 0
