# T632 QEMU oracle note (QEMU 11.1.2, /opt/homebrew/bin/qemu-system-aarch64)

Command (2026-10-29):
  qemu-system-aarch64 -machine virt -cpu cortex-a53 -m 1024 -nographic \
    -kernel /Users/Shared/guest-inputs/Image \
    -initrd /Users/Shared/guest-inputs/initramfs.cpio \
    -append "console=ttyAMA0" -serial file:/tmp/t632-qemu-serial.log \
    -monitor unix:/tmp/q632-mon,server,nowait -display none

Inputs (SHA-256 verified before boot, same as the engine flights):
  Image           05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
  initramfs.cpio  49b607426468f63404047e1321f7ea4505978fee301160ba3ac33c7439928f38
  minimal-virt-fixed.dtb (engine flight) f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0
  (QEMU generates its own virt DTB; equivalent for this boot.)

Observed (serial log, 2026-10-29):
  [    0.681477] Run /init as init process
  [PATHN-TRACK-B] ARM64 STATIC /INIT LAUNCHED (PID 1)
  [PATHN-TRACK-B] UART MARKER: PL011 TTYAMA0 DRIVER AT 0x09000000 OK
  [PATHN-TRACK-B] RAM MARKER: MAGIC 0x504154484E5F5241 0x4D5F4D41524B4552 OK
  [PATHN-TRACK-B] INITRAMFS ROOTFS MOUNTED AT / OK
  [PATHN-TRACK-B] STATUS: PASS - USERSPACE INITIALIZATION COMPLETE
  (marker block appears twice: _start issues write(1) then write(2).)

ELF facts (initramfs /init, extracted 2026-10-29):
  Entry point 0x210158 == the T631 engine halt PC exactly.
  _start at 0x210158 in the R-E LOAD segment (0x210158, .text).
  First instruction: adr x0, 2201c0 (ram_marker_buffer).
  The binary CONTAINS the pinned PID-1 marker bytes (5x "PATHN-TRACK-B").
  CORRECTION to the T631 findings note: the T612 initramfs does NOT lack the
  marker; the marker never printed on the engine because /init never executed
  a single instruction (halt at its first fetch).

Conclusion: the kernel side is proven and 0x210158 is the true /init entry.
The engine defect is on the fetch path, not the entry: see the T632 fix
(EL-aware execute-never, commit 8571d3d) and the findings file.
