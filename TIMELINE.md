# TIMELINE — T724 (EGL dispatch probe via init .rc service trigger)

All times AEDT 16 Nov 2026 unless noted. Guest times are guest kernel seconds.

- 17:27 — Lane launched (driver clock read at spawn; wedge 20:27).
  Worktree `/Users/Shared/wt-track724-egl-rc-trigger`, branch `t724`, base
  `63a0e15`. Brief 46,353 B, SHA `608c39e5`.
- 17:32 — Step 0 disk gate: df 10,988,536 KiB free, 95% used, gate
  8,388,608 KiB — OPEN by ~2.5 GiB.
- ~17:40 — Step 1 COMPLETE: all 13 guest-input SHAs re-hashed on the Mac,
  every one matches the brief. Embedded-vendor rule PASS (CPIO member
  `ramdisk-images/vendor.img` = `5e0607ad`).
- ~17:45 — Step 2 design DECIDED: SYSTEM ROUTE. New file
  `/system/etc/init/t723probe.rc` in the revert system image (original
  `0ba6d6d3` preserved). Ramdisk route REJECTED. Probe path proven:
  `/first_stage_ramdisk/t723-probe` (init binary strings); `/t723-probe`
  does not exist post-switch_root. The T723 hook's `/t723probe` path was
  doubly wrong (wrong name + wrong namespace).
- 17:54 — Step 2 build COMPLETE (box Linux debugfs only). New image SHA
  `6b66cabecbd1219366c00c1eead087ec9abc6dbc19eddd4cf7eaf6d2b513acb9`.
  Neighbours byte-identical; `e2fsck -f -n` clean; file count 7342 → 7343.
  Block-delta patch (4 chunks, 4,194,379 B, SHA `bc29e135`) applied on the
  Mac; final SHA `6b66cabe` — MATCH.
- 17:55:54 — Smoke flight `smoke-t724` launched (supervisor PID 41861,
  QEMU 41868). df at launch 8,390,516 KiB (gate open by only 1,908 KiB).
- 17:57:11 — Smoke flight ENDED at guest 72.707 s / host 75.15 s: supervisor
  disk gate fired (8.00 GiB < 8 GiB). Clean exit, QEMU exit 0. UART
  `2f523454` (9,124 lines, 768,431 B). INCONCLUSIVE (ended before any
  trigger phase). Lane scratch cleaned (134 MB + ~4 MB); df 8,530,960 KiB.
- 17:59:46 — Probe flight `probe-flight` launched (supervisor PID 42164,
  QEMU 42168). df 8,530,820 KiB (gate open ~139 MB).
- ~18:02 — Mid-flight check (guest ~165 s): 0 `t723probe` mentions.
  Guest at 165 s, past the installd early-boot point (99.58 s in T723).
- ~18:03 — Diagnostics: early-boot actions processed (89.30 s, 90.61 s);
  installd started (90.62 s); `on boot` processing line absent; surfaceflinger
  started 100.13 s; T720_PROBE DONE PASS present (L228).
- ~18:04 — Root-cause narrowing: T723 boot action processed at 99.63 s;
  stock `on late-init` queues `trigger early-boot` + `trigger boot`
  (hw/init.rc:562–563); `on boot` is at hw/init.rc:1104.
- ~18:05 — Guest clock reading stalls at 205.8 s (supervisor guest_max);
  UART still growing on logcat lines. df 8,524,800 KiB.
- ~18:06–18:11 — Guest progresses through system_server startup; system_server
  pid 2892 self-dumps (wall 23:23:57, 23:24:28); new system_server pid 4298
  appears (wall 23:25:48).
- 18:09:54 — Probe flight ENDED: guest 604.976 s, host 605.9 s, QEMU exit 0.
  UART `e1bfd466` (28,381 lines, 3,294,964 B). flight-end.json written.
- 18:12 — Adjudication greps: `sys.boot_completed` 5; first stock action
  (hw/init.rc:1224) at guest 498.413 s (L21486); `T723_PROBE` 0;
  `t723probe` 0; `processing action (boot)` 0.
- 18:14 — Analyzer run: holds all intact (T720 PASS 1, T721 fence 1,
  Invalid present fence 0, REJECT 0, EPERM 0, mapped 1, kills 0, panic 0);
  dup-fail 4, rate 6.612/1000 s. df 8,057,612 KiB (below gate).
- ~18:15–18:22 — Root-cause proof: (1) flown image contains t723probe.rc
  SHA `9c6b3e12` (box debugfs); file is regular, mode 0664, LF, no CR.
  (2) `class_start hal`/`core` exist ONLY in `on boot` (hw/init.rc:1210/1212)
  across all system .rc files; no vendor .rc file has any `class_start` —
  so the HAL/core starts at 94–100 s prove the boot action RAN.
  (3) Init logs `Parsing file` only for /apex/... files; no per-file parse
  log for /system/etc/init/*.rc; no parse error logged.
  (4) `processing action (nonencrypted)` / `(charger)` also absent although
  their services demonstrably started — the missing processing lines are a
  kmsg flood loss, not non-execution.
- 18:24 — Deliverables written; df 8,504,064 KiB. Lane CLOSED FAIL.
  Wedge 20:27 NOT hit.
