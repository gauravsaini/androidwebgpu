.text
.global _start

.macro semihost_exit
    mov x0, #0x18
    movz x1, #0x26
    movk x1, #2, lsl #16
    hlt #0xf000
1:  b 1b
.endm

.macro report label
    mov x0, #4
    adr x1, \label
    hlt #0xf000
.endm

.macro pass
    report msg_pass
    semihost_exit
.endm

.macro fail
    report msg_fail
    semihost_exit
.endm

_start:
#ifdef T99_ORACLE_SVC
    movz x0, #0x4010, lsl #16
    msr VBAR_EL1, x0
    mov x9, #0x40300000
    mov sp, x9
    mov x9, #0x40200000
    msr SP_EL0, x9
    adr x9, el0_start
    msr ELR_EL1, x9
    mov x9, #0
    msr SPSR_EL1, x9
    eret
.org 0x400
lower_sync:
    cbz x24, first_svc
    mrs x19, ESR_EL1
    movz x20, #0x5678
    movk x20, #0x5600, lsl #16
    cmp x19, x20
    b.ne fail_case
    adr x20, el0_after_second
    mrs x21, ELR_EL1
    cmp x21, x20
    b.ne fail_case
    mrs x21, SPSR_EL1
    cmp x21, #0
    b.ne fail_case
    pass
first_svc:
    mrs x19, ESR_EL1
    movz x20, #0x1234
    movk x20, #0x5600, lsl #16
    cmp x19, x20
    b.ne fail_case
    adr x20, el0_after_first
    mrs x21, ELR_EL1
    cmp x21, x20
    b.ne fail_case
    mrs x21, SPSR_EL1
    cmp x21, #0
    b.ne fail_case
    eret
fail_case:
    fail
.org 0x800
el0_start:
    svc #0x1234
el0_after_first:
    mov x24, #0x1234
    svc #0x5678
el0_after_second:
    b fail_case
#endif

#ifdef T99_ORACLE_ERET
    movz x0, #0x4010, lsl #16
    msr VBAR_EL1, x0
    mov x9, #0x40300000
    mov sp, x9
    mov x9, #0x40200000
    msr SP_EL0, x9
    adr x9, el0_user
    msr ELR_EL1, x9
    movz x9, #0xa000, lsl #16
    msr SPSR_EL1, x9
    eret
.org 0x400
lower_sync:
    mrs x19, ESR_EL1
    movz x20, #0x0200, lsl #16
    cmp x19, x20
    b.ne fail_case
    adr x20, el0_eret
    mrs x21, ELR_EL1
    cmp x21, x20
    b.ne fail_case
    movz x20, #0xa000, lsl #16
    mrs x21, SPSR_EL1
    cmp x21, x20
    b.ne fail_case
    movz x20, #0x4020, lsl #16
    cmp x25, x20
    b.ne fail_case
    mrs x21, SP_EL0
    cmp x21, x20
    b.ne fail_case
    pass
fail_case:
    fail
.org 0x800
el0_user:
    mov x25, sp
el0_eret:
    eret
#endif

#ifdef T99_ORACLE_QPAIR
    movz x0, #0x4010, lsl #16
    msr VBAR_EL1, x0
    mov x9, #0x00300000
    msr CPACR_EL1, x9
    isb
    mov x24, #0
    adr x0, qdata
    adr x1, qout
    ldp q0, q1, [x0]
    stp q0, q1, [x1, #16]!
    adr x20, qout
    add x20, x20, #16
    cmp x1, x20
    b.ne fail_case
    ldr x19, [x1]
    movz x20, #0x0100
    movk x20, #0x0302, lsl #16
    movk x20, #0x0504, lsl #32
    movk x20, #0x0706, lsl #48
    cmp x19, x20
    b.ne fail_case
    ldr x19, [x1, #8]
    movz x20, #0x0908
    movk x20, #0x0b0a, lsl #16
    movk x20, #0x0d0c, lsl #32
    movk x20, #0x0f0e, lsl #48
    cmp x19, x20
    b.ne fail_case
    ldr x19, [x1, #16]
    movz x20, #0x1110
    movk x20, #0x1312, lsl #16
    movk x20, #0x1514, lsl #32
    movk x20, #0x1716, lsl #48
    cmp x19, x20
    b.ne fail_case
    ldr x19, [x1, #24]
    movz x20, #0x1918
    movk x20, #0x1b1a, lsl #16
    movk x20, #0x1d1c, lsl #32
    movk x20, #0x1f1e, lsl #48
    cmp x19, x20
    b.ne fail_case
    mov x9, #0
    msr SCTLR_EL1, x9
    isb
    mov x24, #0
    adr x0, qdata
    add x0, x0, #1
.Lfaulting_ldp:
    ldp q2, q3, [x0]
    b fail_case
.Lafter_ldp_fault:
.Lfaulting_stp:
    stp q2, q3, [x0]
    b fail_case
.org 0x200
current_sync:
    cbnz x24, .Lcheck_stp_fault
    mrs x19, ESR_EL1
    movz x20, #0x0021
    movk x20, #0x9600, lsl #16
    cmp x19, x20
    b.ne fail_case
    adr x20, .Lfaulting_ldp
    mrs x21, ELR_EL1
    cmp x21, x20
    b.ne fail_case
    adr x20, qdata
    add x20, x20, #1
    mrs x21, FAR_EL1
    cmp x21, x20
    b.ne fail_case
    mov x24, #1
    adr x20, .Lafter_ldp_fault
    msr ELR_EL1, x20
    eret
.Lcheck_stp_fault:
    mrs x19, ESR_EL1
    movz x20, #0x0061
    movk x20, #0x9600, lsl #16
    cmp x19, x20
    b.ne fail_case
    adr x20, .Lfaulting_stp
    mrs x21, ELR_EL1
    cmp x21, x20
    b.ne fail_case
    adr x20, qdata
    add x20, x20, #1
    mrs x21, FAR_EL1
    cmp x21, x20
    b.ne fail_case
    pass
fail_case:
    fail
.org 0x2000
qdata: .byte 0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39
.org 0x3000
qout: .space 64
#endif

#ifdef T99_ORACLE_CPACR
    movz x0, #0x4010, lsl #16
    msr VBAR_EL1, x0
    mov x9, #0
    msr CPACR_EL1, x9
    isb
fp_instruction:
    fadd s0, s1, s2
    b fail_case
.org 0x200
current_sync:
    mrs x19, ESR_EL1
    movz x20, #0x1fe0, lsl #16
    cmp x19, x20
    b.ne fail_case
    adr x20, fp_instruction
    mrs x21, ELR_EL1
    cmp x21, x20
    b.ne fail_case
    pass
fail_case:
    fail
#endif

.org 0x5000
msg_pass: .asciz "T99-ORACLE-PASS"
msg_fail: .asciz "T99-ORACLE-FAIL"
