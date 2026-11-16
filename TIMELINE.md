# TIMELINE — T723 recovery lane (EGL dispatch probe flight)

All times AEDT, 16 Nov 2026. Guest times are kernel seconds since QEMU start.

- **15:22:01** — First T723 agent exits cleanly on WAIT-DISK (df 7,765,764 KiB < 8,388,608 KiB gate). State preserved; no flight, no verdict. Hold recorded.
- **16:13** — Mac disk gate opens: df 11,689,072 KiB (≈ 3.9 GiB freed by unknown cause).
- **16:22** — Driver verifies df 11,143,296 KiB (gate open by ≈ 2.63 GiB) and launches the recovery agent.
- **16:24:30** — Recovery agent launched (driver clock read; wedge 19:24:30 AEDT).
- **16:24–16:25** — Step 1 re-verified: 8/8 input SHAs match (probe `4b275e48`, CPIO `a9d6040a`, DTB `73d1904b`, kernel `70d3859d`, revert system `0ba6d6d3`, vendor `5e0607ad`, T722 CPIO `42f1b2ec`, T722 DTB `32eaab25`); embedded CPIO vendor member PASS (exit 0); scripts re-verified (supervisor `4c0891c5`, launcher `cc335b01`, analyzer `b1526f5d`). Disk rechecked 11,147,184 → 11,145,880 KiB — gate open.
- **16:27:17** — Probe flight launched (supervisor PID 35186, QEMU pid 35194; guest target 600 s, host cap 700 s, GDB 1258).
- **≈16:27:50** — Guest ≈ 33.4 s: the wait-loop prints its timeout — `[T723] boot_completed never detected in 600s; probe not run` (UART L2018). The loop has burned all 300 iterations in ≈ 30 s; the probe will never run. (Recognised only in post-flight analysis — the line carries no guest timestamp and the flight was otherwise healthy.)
- **16:32–16:33** — Mid-flight checks (host ≈ 300–375 s): supervisor + QEMU alive; UART 1.51 MB; `T723_PROBE` 0; `boot_completed` 0; guest_max frozen at 196.8 s (later understood: kernel timestamps stop during the logcat-only dexopt phase — not a guest hang). system_server pid 2848 stable, Dexopt PERFORMED, SensorService onBootPhase returned (6,839 ms), no Watchdog kills. QEMU cmdline + preflight re-verified: flown stack as briefed.
- **16:35:53** — Guest 516.118 s: init processes `sys.boot_completed=1` (UART L21534) — 482.7 s after the wait-loop had already exited.
- **≈16:36:37** — Guest ≈ 560 s: system_server (2848) crashes (zygote SIGKILL 560.16 s; dropbox crash file 23:25:43); 7 × DeadSystemException FATALs cluster 23:25:47. init restarts the stack; new system_server pid 4367 up by flight end.
- **16:37:38** — Flight ends: guest target reached (620.2 s ≥ 600 s). Host 620.93 s. QEMU exit 0. Supervisor writes flight-end.json, uart.log.numbered.txt, uart.log.gz. No supervisor or QEMU processes remain.
- **16:38–16:45** — Investigation: 1 `t723` line in the UART (the timeout); T552 control loop shows the same blindness (48/48 status polls empty, 0 milestones); 877 `inaccessible or not found` lines; hook source read (lines 252–268); T552 loop bound confirmed (240 steps).
- **16:45–16:55** — Box debugfs (read-only) proves toybox / getprop / logcat all exist in the flown system image (`0ba6d6d3`) — context defect, not image defect. Analyzer run (exit 0): 0 `T723_PROBE`, DONE 0, SP1/SP2/SP3 0; holds all intact; dup-fail 4, rate 6.449/1000 s.
- **17:06:55** — Adjudication complete: **FAIL** (probe never ran; question unanswered; decision reached ≈17:02 on the analyzer output). Findings, death note, TIMELINE written; progress appended; heartbeat CLOSED FAIL. No agent commits. Gates G723-1/2 remain UNCHECKED for driver adjudication.
