.text
.global _start
_start:
  movz x20, #0
  movk x20, #0x4020, lsl #16

  // LDR/STR unsigned immediate.
  movz x0, #0xbeef
  movk x0, #0x0123, lsl #16
  movk x0, #0x4567, lsl #32
  movk x0, #0x89ab, lsl #48
  str x0, [x20, #8]
  ldr x1, [x20, #8]

  // LDR/STR register offset (X21 scaled by the 8-byte access size).
  mov x21, #2
  movz x2, #0x5678
  movk x2, #0x1234, lsl #16
  movk x2, #0xabcd, lsl #32
  movk x2, #0xef01, lsl #48
  str x2, [x20, x21, lsl #3]
  ldr x3, [x20, x21, lsl #3]

  // Pre-indexed and post-indexed LDR/STR, including base writeback.
  movz x22, #0x0040
  movk x22, #0x4020, lsl #16
  mov x4, #0x00d1
  str x4, [x22, #8]!
  mov x5, #0x00a5
  str x5, [x22], #8
  ldr x6, [x22, #-8]!
  ldr x7, [x22], #8

  // STP pre-indexed followed by LDP post-indexed.
  movz x23, #0x0100
  movk x23, #0x4020, lsl #16
  mov x10, #0x10
  mov x11, #0x20
  stp x10, x11, [x23, #16]!
  ldp x12, x13, [x23], #16

  movz x14, #0x0400
  movk x14, #0x4010, lsl #16
  br x14

  .org 0x400
  mov x0, #0x20
  movz x1, #0
  movk x1, #0x4030, lsl #16
  movz w2, #0x0026
  movk w2, #0x0002, lsl #16
  str w2, [x1]
  mov w2, #0
  str w2, [x1, #4]
  hlt #0xf000
