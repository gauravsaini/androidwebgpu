.text
.global _start
_start:
  // UBFM/SBFM/BFM: operate on the 64-bit source 0x0123456789abcdef.
  movz x0, #0xcdef
  movk x0, #0x89ab, lsl #16
  movk x0, #0x4567, lsl #32
  movk x0, #0x0123, lsl #48
  ubfm x1, x0, #8, #23
  sbfm x2, x0, #0, #7

  movz x3, #0x7788
  movk x3, #0x5566, lsl #16
  movk x3, #0x3344, lsl #32
  movk x3, #0x1122, lsl #48
  bfm x3, x0, #8, #15

  // N=1 in the logical shifted-register encoding selects BIC.
  mov x4, #0xffff
  mov x5, #0x0f0f
  bic x6, x4, x5

  movz x12, #0x0400
  movk x12, #0x4010, lsl #16
  br x12

  // QEMU's next-TB CPU record is the vector result snapshot.
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
