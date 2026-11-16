# T722 timeline — EGL native-fence support

All times AEDT, 16 Nov 2026. Worktree /Users/Shared/wt-track722-egl-native-fence,
branch t722, base d8e90b0. Gates G722-1/2 registered unchecked (driver adjudicates).

- 12:49:38 — First agent 0f3c3c2c launched.
- 12:50:13 / 12:50:53 — Step 0 df readings 8,380,272 / 8,380,108 KiB (7.99 GiB),
  both below the 8,388,608 KiB (8 GiB) gate.
- 12:51:52 — WAIT-DISK exit recorded (Step 0). Nothing built, no verdict.
- ~12:52 — Driver pruned certified+vaulted+pushed worktrees T714-T718
  (~476 MiB freed, disk 8.46 GiB) under standing authority.
- 12:54:14 — First recovery agent 81586262 launched (wedge 15:54:14).
- 12:54:44 — Step 0 re-check: 8,866,804 KiB, above gate.
- 12:58:04 — Step 1 PASS: all input SHAs matched; embedded-vendor check PASS.
- 13:12:59 — Step 2 characterization + fix build complete (stub returns -1;
  binary patch, T721 pattern; vendor/CPIO/DTB built + verified).
- 13:26:01 / 13:26:16 / 13:26:47 — Disk re-tripped the gate: 8,372,200 /
  8,370,880 / 8,369,596 KiB, sliding.
- 13:27:23 — WAIT-DISK exit recorded (Step 3). No flights, no supervisor/QEMU,
  no verdict. Steps 0-2 preserved.
- 13:33:32 — First recovery agent completed (follow-up: sandbox-wide stub-source
  grep found no buildable source).
- 13:33:54 — Driver verified the worktree state from heartbeat.json +
  progress-t722.txt.
- ~13:34 — Driver pruned certified T719/T720 worktrees (~260 MiB freed, disk
  8,882,532 KiB / 8.47 GiB; QEMU count verified 0 first).
- 13:35:29 — Second recovery agent 0106e8ed launched (wedge 16:35:29).
  Flights + adjudication only; no re-characterization, no rebuild.
- 13:36:00 / 13:36:30 — Step 0 re-check PASS: 8,894,796 / 8,870,236 KiB.
  Artifacts re-hashed (all matched); embedded-vendor checks PASS; QEMU
  command line read (no vendor drive).
- 13:38:18 — Smoke launched (supervisor PID 28771, QEMU PID 28776).
- 13:43:19 — Smoke stopped: host timeout 300 s (host 300.53 s, guest 196.41 s,
  QEMU exit 0).
- 13:44:20 — Smoke PASS recorded (holds intact; fence/boot prongs deferred as
  expected; dup-fail 0 vacuous).
- 13:44:22 — Flight-01 launched (supervisor PID 29101, QEMU PID 29106;
  2400 s guest target, 2500 s host cap).
- ~13:54 — Supervisor hit the disk gate mid-flight (FATAL after the 585.6 s
  tick, 7.49 GiB) and exited(10); QEMU orphaned, still running.
- 13:55:47 — Orphan event recorded (disk 7,857,404 KiB at 13:55:17). Orphaned
  QEMU left running to the guest target per the T721 V1 precedent.
- 13:56:47 / 14:07:10 / 14:17:20 — Orphan checks: guest 743.9 s / 1366.9 s /
  1974.3 s; QEMU alive, UART growing.
- 14:25:25 — Manual SIGTERM at guest_max 2461.281 s; QEMU gone 1 s later.
  Manual flight-end record written (UART SHA ae28c6a6, 256,896 lines).
- 14:25:26 — Disk 7,837,208 KiB (7.47 GiB). A/B control cannot launch
  (supervisor gate); control gap recorded honestly.
- After the stop — Analyzer: 20 dup-fails in 2461.3 s, rate 8.126 per 1000 s.
  PRIMARY CRITERION FAILED (baselines 4.057 and 6.661). All holds intact.
- Post-flight — Investigation: every implementation-level cause ruled out
  with evidence; surviving explanation is dispatch non-forwarding
  (EGL-DISPATCH-UNREACHABLE). See findings-t722.md S5.
- Close — Deliverables written (findings, death note, outcome FAIL, this
  timeline, progress appended, heartbeat CLOSED). No commits; driver
  certifies and launches the successor. Verdict: FAIL.
