.text
.global _start
_start:
  // Barriers around a store and reload.
  movz x3, #0x0080
  movk x3, #0x4020, lsl #16
  mov x2, #0x4321
  str x2, [x3]
  dmb ish
  ldr x4, [x3]
  dsb sy
  ldr x5, [x3]
  isb

  // TPIDR_EL1 must retain the value written by MSR for the following MRS.
  movz x0, #0xbeef
  movk x0, #0xdead, lsl #16
  msr tpidr_el1, x0
  mov x0, #0
  mrs x1, tpidr_el1

  // SVC at EL1 vectors to VBAR_EL1 + 0x200 (current EL, SPx, sync).
  movz x6, #0
  movk x6, #0x4010, lsl #16
  msr vbar_el1, x6
  isb
  svc #0x266

  .org 0x200
  mrs x8, esr_el1
  mrs x9, elr_el1
  mrs x10, spsr_el1
  movz x11, #0x0400
  movk x11, #0x4010, lsl #16
  br x11

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
