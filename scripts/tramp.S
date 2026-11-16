// T721 retire-fence trampoline. Assembled at VMA 0xb7670 (repurposed
// sync_file_info, 536 B). Called via bl from present() patch site 0x5bff4.
// On entry: x0 = CommandWriterBase* (preserve), x19 = this (preserve).
// Returns: w1 = fence fd (>=0) or -1 (current behaviour on any failure).
// Static storage (new .bss tail 0x115bc8, 12 B, zero-init): +0 timeline_fd,
// +4 fence_counter, +8 logged_flag.
// NOTE: call targets are UNDEFINED here and resolved at LINK time via
// --defsym; .set absolute symbols make GAS resolve CALL26 at assembly
// time against base 0 (verified wrong on first attempt).
    .text
    .globl t721_tramp
    .set T721_STATIC, 0x115bc8

t721_tramp:
    stp x29, x30, [sp, #-16]!
    stp x0, x19, [sp, #-16]!
    sub sp, sp, #16
    adrp x19, T721_STATIC
    add x19, x19, #:lo12:T721_STATIC
    ldr w10, [x19]
    cmp w10, #0
    b.gt 1f
    bl SW_SYNC_TIMELINE_CREATE
    cmp w0, #0
    b.le 2f
    str w0, [x19]
    mov w10, w0
1:
    ldr w11, [x19, #4]
    add w11, w11, #1
    str w11, [x19, #4]
    mov w0, w10
    adrp x1, t721_name
    add x1, x1, #:lo12:t721_name
    mov w2, w11
    bl SW_SYNC_FENCE_CREATE
    cmp w0, #0
    b.le 2f
    str w0, [sp]
    ldr w0, [x19]
    mov w1, #1
    bl SW_SYNC_TIMELINE_INC
    ldr w12, [x19, #8]
    cbnz w12, 3f
    mov w12, #1
    str w12, [x19, #8]
    mov w0, #4
    adrp x1, t721_tag
    add x1, x1, #:lo12:t721_tag
    adrp x2, t721_fmt
    add x2, x2, #:lo12:t721_fmt
    ldr w3, [sp]
    ldr w4, [x19]
    bl ANDROID_LOG_PRINT
3:
    ldr w1, [sp]
    add sp, sp, #16
    ldp x0, x19, [sp], #16
    ldp x29, x30, [sp], #16
    ret
2:
    mov w1, #-1
    add sp, sp, #16
    ldp x0, x19, [sp], #16
    ldp x29, x30, [sp], #16
    ret
    .balign 8
t721_name:
    .asciz "t721retire"
    .balign 8
t721_tag:
    .asciz "T721"
    .balign 8
t721_fmt:
    .asciz "T721 fd=%d tl=%d"
