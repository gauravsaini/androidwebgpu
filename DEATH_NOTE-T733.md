# DEATH NOTE — Track T733 (minimal no-libc binary discrimination)

- Track: T733. Flight: service-flight-3, 2026-11-20 20:35:46–20:49:58 AEDT.
- Host 851.93 s, guest 851.062 s, QEMU exit 0. Stop: guest target reached
  (851.1 s >= 850 s). UART 33,821 lines, 4,001,204 B, SHA256
  acc4763e71a8ff5ab8a7f6a83e3e124b30bc8e4b6ce5bca5ff7adf70e6ebaff0.
- Verdict: FAIL. `T733_MINIMAL_OK` 0 lines — the minimal no-libc binary
  (nb-t733probe.bin, 1,648 B, SHA fd189c08) never wrote its marker under
  either leg, even though both actions' writes fired (T733_PROBE_OK x2:
  L10652 guest 117.83 s, L22750 guest 633.74 s) and the control
  (T728_PROBE_OK x2: L10651 guest 117.77 s, L22702 guest 633.64 s) proves
  the guest behaved exactly as in T732.
- Cause of death: the fault is in init's exec dispatch, not the payload.
  The on-boot `exec /system/bin/t733probe` ran and RETURNED (the write
  after it fired at 117.83 s), but the minimal binary — no libc, no
  _start -> __libc_start_main -> main chain, just openat/write/exit_group
  from _start — produced no marker, no starting-service line, no exit
  line, no exec failure line, and no boot stall. The property leg's
  `start t733probe` was attempted (its write fired at 633.74 s); its
  processing and start-command log lines were lost in the Bluetooth ANR
  crash-dump flood at the trigger moment (L22714–L22752), and the
  identical control service was refused with the SELinux label /
  domain-transition error at L22700.
- Wall: INIT-EXEC-DISPATCH-SILENTLY-FAILED (guest, NEW in T733) —
  supersedes T732's BINARY-EXEC-SILENTLY-FAILED as the fault location.
  WALL-TYPE-TAG: guest. Rust par bhi lagu: haan.
- Trigger leg: FIXED (second consecutive flight) — sys.boot_completed
  fired at guest 628.63 s (L22420, 11 lines).
- What would have saved it: nothing in this track's scope — the binary
  is proven minimal and correct (ELF verified, label system_file, mode
  0755, bytes verified from the flown image). The next discrimination
  must trace init's exec dispatch itself (init-side tracing).
- Successor (driver's call): trace init's exec dispatch — init-side
  tracing (strace on init, init logging, or a binary the guest kernel
  itself execs at a different point). Do NOT re-test the start path, the
  trigger leg, or the payload's startup chain — all three are settled.
- Evidence: findings-t733.md, outcome-t733.txt (FAIL), analysis-t733.txt,
  flight-end.json, preflight.json, qemu-command.txt, qemu-stderr.log,
  uart.log + .gz + .numbered.txt, progress-t733.txt, TIMELINE.md,
  heartbeat.json (CLOSED). No commit — the driver commits.
