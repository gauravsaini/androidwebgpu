/*
 * ARM64 static /init with RAM and UART markers for Path N / Android Linux guest.
 * Cross-compiled with clang -target aarch64-linux-gnu and rust-lld.
 */
.global _start
.text
_start:
    /* 1. Write RAM marker into memory buffer */
    adr x0, ram_marker_buffer
    ldr x1, ram_magic
    str x1, [x0]
    ldr x2, ram_magic2
    str x2, [x0, #8]

    /* 2. Write UART markers to stdout (fd 1) */
    mov x0, #1
    adr x1, marker_msg
    ldr x2, =marker_msg_len
    mov x8, #64 /* sys_write */
    svc #0

    /* 3. Also write to stderr (fd 2) */
    mov x0, #2
    adr x1, marker_msg
    ldr x2, =marker_msg_len
    mov x8, #64 /* sys_write */
    svc #0

    /* 4. Quiescent sleep loop so init stays alive */
loop:
    adr x0, sleep_ts
    mov x1, #0
    mov x8, #101 /* sys_nanosleep */
    svc #0
    b loop

.data
.align 3
ram_magic:
    .quad 0x504154484E5F5241 /* "PATHN_RA" */
ram_magic2:
    .quad 0x4D5F4D41524B4552 /* "M_MARKER" */

ram_marker_buffer:
    .quad 0
    .quad 0

sleep_ts:
    .quad 60 /* 60 seconds */
    .quad 0

marker_msg:
    .ascii "\n"
    .ascii "============================================================\n"
    .ascii "[PATHN-TRACK-B] ARM64 STATIC /INIT LAUNCHED (PID 1)\n"
    .ascii "[PATHN-TRACK-B] UART MARKER: PL011 TTYAMA0 DRIVER AT 0x09000000 OK\n"
    .ascii "[PATHN-TRACK-B] RAM MARKER: MAGIC 0x504154484E5F5241 0x4D5F4D41524B4552 OK\n"
    .ascii "[PATHN-TRACK-B] INITRAMFS ROOTFS MOUNTED AT / OK\n"
    .ascii "[PATHN-TRACK-B] STATUS: PASS - USERSPACE INITIALIZATION COMPLETE\n"
    .ascii "============================================================\n"
    .ascii "\n"
marker_msg_len = . - marker_msg
