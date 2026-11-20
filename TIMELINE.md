# T733 TIMELINE — minimal no-libc binary discrimination

- 2026-11-20 20:26:48 AEDT — lane launched (agent 96a895a6, brief
  1dcf6423, worktree /Users/Shared/wt-track733-minimal-binary, branch
  t733, base 928027c). Wedge 23:26:48 AEDT.
- 20:27 AEDT — Step 0 PASS: df avail 22,198,032 KiB, GATE OPEN.
  Step 1 PASS: inputs verified on Mac, all SHAs match T732 (kernel
  70d3859d, DTB 73d1904b, CPIO a9d6040a, vendor 5e0607ad, binary
  19db5d31, T732 image f7980cfb, patch d7d1cee5, T732 rc f9baa479).
- 20:30–20:33 AEDT — Step 2 PASS: minimal binary built on the box
  (1,648 B, SHA fd189c08, ELF64 EXEC AArch64, entry 0x400120 = _start,
  no INTERP/DYNAMIC; openat/write/exit_group confirmed by disassembly).
  New rc t733probe-new.rc 722 B SHA 79ad3cc3 (T732 rc + 3 renames only,
  diff-verified). Parser VERDICT ACCEPT, 0 errors, 1 service, 2
  actions, 0 imports. Image built on the box: new image SHA af40403a,
  patch SHA 4d74f4d1 (6 chunks, 6,291,456 B). Label verified from the
  new image: new binary u:object_r:system_file:s0, new rc xattr-empty.
  Marker uniqueness: 0 T733_MINIMAL_OK / T733_PROBE_OK in earlier UARTs.
- 20:35 AEDT — Step 3 PASS: image/patch/rc/binaries verified at every
  hop (box = sandbox = Mac). Scripts adapted from T732 (supervisor /
  launch_probe / analyze / df_poll, T733_, 850/1100, image af40403a,
  flight service-flight-3, GDB port 1261). Analyzer sanity: T732 UART
  reproduced driver counts exactly.
- 20:35:46 AEDT — Step 4: service-flight-3 LAUNCHED. Preflight
  verified (QEMU 11.1.2, kernel 70d3859d, DTB 73d1904b, CPIO a9d6040a,
  system t733-system.img af40403a, vendor 5e0607ad, target 850).
  Launch-time disk poll GATE_OPEN 16,234,540 KiB.
- 20:40 AEDT — Poll 1: host ~275 s, guest 254.23 s, ratio ~0.92,
  projected guest at host cap ~1015 s — trigger leg NOT at risk. df
  avail 14,049,756 KiB, GATE OPEN. T733_PROBE_OK 1 (L10652, guest
  117.83 s), T733_MINIMAL_OK 0, T728_PROBE_OK 1 (L10651, guest 117.77 s,
  control leg 1 OK), boot_completed 0. Interim: on-boot action executed;
  the exec RETURNED but the minimal binary wrote no marker.
- 20:42 AEDT — Poll: host ~421 s, guest 399.7 s, ratio ~0.95. Markers
  unchanged. Kernel in quiet period.
- 20:44 AEDT — Poll: host ~545 s, guest 542.8 s, ratio ~1.00. Markers
  unchanged. Kernel resumed logging.
- 20:46:52 AEDT — Poll 2: host ~665 s, guest 663.8 s, ratio ~1.00.
  TRIGGER FIRED (sys.boot_completed 11 lines). T733_PROBE_OK 2 (L10652
  guest 117.83 s, L22750 guest 633.74 s — property action ran),
  T728_PROBE_OK 2 (control legs both fired), T733_MINIMAL_OK 0.
  DECISIVE INTERIM: the minimal no-libc binary produced NO marker under
  EITHER leg. The fault is in init's exec dispatch, not the payload.
- 20:49:11 AEDT — Poll: host 796.7 s, guest 785.7 s. QEMU running.
- 20:49:58 AEDT — Flight COMPLETE: guest target reached (851.1 s >=
  850 s). Host 851.93 s, guest 851.062 s, QEMU exit 0. UART 33,821
  lines, 4,001,204 B, SHA acc4763e.
- 20:51 AEDT — Step 5 adjudication: analyzer run on the flight UART.
  T733_MINIMAL_OK 0, T733_PROBE_OK 2, T728_PROBE_OK 2 (control intact),
  T728_SERVICE_OK 0, sys.boot_completed 11 (first guest 628.63 s,
  L22420). Control service refused with the SELinux label /
  domain-transition error at L22700; the t733probe start's log lines
  were lost in the Bluetooth ANR crash-dump flood (L22714–L22752).
  FATAL EXCEPTION 4 (DeadSystemException, late flight, ~guest 672 s).
- Verdict: FAIL. Wall: INIT-EXEC-DISPATCH-SILENTLY-FAILED (guest, NEW)
  — supersedes BINARY-EXEC-SILENTLY-FAILED as the fault location.
  Findings + outcome + death note written; heartbeat CLOSED.
