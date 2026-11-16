# DEATH NOTE — T724 (EGL dispatch probe via init .rc service trigger)

- **Track:** T724 — fire the byte-identical T723 probe (SHA `4b275e48`) with an
  init `.rc` service trigger (`on property:sys.boot_completed=1`).
- **Verdict:** FAIL. The probe never ran. The EGL dispatch question is
  UNANSWERED. No fix was built (probe-only branch).
- **What died:** the `.rc` service-trigger route as flown. The added file
  `/system/etc/init/t723probe.rc` (SHA `9c6b3e12`, in flown system image SHA
  `6b66cabe`) produced zero executed actions in a full 600 s flight:
  0 `t723probe` mentions, 0 `T723_PROBE`, 0 service start, 0 .rc citations,
  0 SP1/SP2/SP3.
- **What did NOT die:** the guest booted; `sys.boot_completed` was set (first
  stock action at guest 498.413 s); the trigger mechanism works (the stock
  `on boot` action ran at ~94–100 s, proven by HAL/core service starts; the
  stock boot_completed action ran at 498.413 s). All T720/T721 holds intact
  (T720_PROBE DONE PASS 1, T721 fence log 1, Invalid present fence 0,
  REJECT 0, EPERM 0, mapped 1, kills 0, panic 0).
- **Root cause (evidence level):** silent non-registration of the added .rc
  file. The file was present in the flown image byte-identical, the directory
  is parsed by second-stage init, the triggers dispatch, and no parse error
  was logged — yet none of the file's three actions executed. The mechanism
  (silent parse rejection vs registration failure vs trigger-matching failure)
  is NOT proven: init emits no per-file parse record for
  `/system/etc/init/*.rc`. See findings-t724.md §The trigger evidence.
- **WALL-TYPE-TAG: ADDED-RC-SILENTLY-IGNORED**
- **Rust par bhi lagu: haan** — the wall is a guest-side init mechanism;
  the Rust emulator must solve the same registration question (or use a
  different trigger route) before it can run guest probes via init .rc files.
- **Flights:** smoke `smoke-t724` (guest 72.707 s, disk-gate exit, UART
  `2f523454`, inconclusive); probe-flight (guest 604.976 s, host 605.9 s,
  QEMU exit 0, UART `e1bfd466`, 28,381 lines, 3,294,964 B).
- **Secondary findings:** (1) Only the first boot_completed action (1224)
  processed; dmesgd/wifi boot_completed actions never did (T723: 2–3 s
  later). (2) Boot-stability regression vs T723: system_server began
  self-dumping BEFORE boot_completed (guest ≈461.8 s, ≈492.5 s; boot_completed
  498.413 s; FATAL cascade ≈538–539 s; restart as pid 4298 ≈573 s).
  (3) The [T723] hook re-fired (L2012) — re-confirms RAMDISK-MOUNT-BLIND.
  (4) Disk slid below the 8 GiB gate after the flight (8,057,612 KiB at
  18:14; 8,504,064 KiB at 18:24) — the successor needs the gate comfortably
  open.
- **Successor (recommended T725):** minimal-.rc registration experiment
  (single trivial action, smoke past guest ~120 s) BEFORE another 600 s
  flight; plus an offline init-parser check on `t723probe.rc`
  (SHA `9c6b3e12`). If a minimal file registers, re-fly the probe; if even a
  minimal file is silently ignored, the .rc route is dead and the successor
  must use a different trigger.
- **Reflection:** see findings-t724.md §Reflection. Headline lessons: (a) a
  smoke that ends before the first trigger proves nothing about registration
  — the T724 smoke ended at 72.7 s on the disk gate; (b) a missing log line
  in guest 60–200 s is NOT proof of non-execution (kmsg flood loss — proven
  in this flight and in T723); only post-flood absences are probative;
  (c) the lane needed a positive registration proof designed in from the
  start, on a quiet-window trigger.
- **Evidence:** findings-t724.md (SHA to be stamped by driver),
  analysis-t724.txt, flights/probe-flight/uart.log (`e1bfd466`),
  flights/probe-flight/flight-end.json, flights/smoke-t724/uart.log
  (`2f523454`), flights/smoke-t724/flight-end.json.
