# TIMELINE — Track T726 (recovery 3)

All times AEDT 17 Nov 2026 unless noted. Clock-sourced from `date` reads.

- 02:40:50 — Recovery 3 launched (wedge 05:40:50). Brief
  track726-recovery3-waitdisk.md (52,374 B, SHA 46a59757).
- 02:42:46 — Step 0 re-verification PASS: Mac image a7141b45, trigger 0372c10b,
  parser d42b5b6e, scripts AST OK; box image a7141b45, patch 4f1728d1;
  /mnt/sdb1 free 21,028,136 KiB. Mac df 5,730,464 KiB — gate CLOSED.
- 02:48:29–04:57:15 — Polls 1–25: disk flat ~5.73 GiB (polls 1–17), then
  ~7.14 GiB (polls 18–25) after a +1.35 GiB recovery at poll 18 (04:21:16,
  re-read 04:21:29 confirmed 7,140,768 KiB). Shortfall ~3.94 GiB, then
  ~2.57 GiB. Gate never opened.
  - Poll 9: first two macssh attempts preflight-failed ~03:30 (egress proxy);
    third attempt landed 03:33:45. Route blind ~3 min, below the 20-min
    notify threshold — no user notice.
  - Poll 23: first attempt killed by a runtime restart ~3 min into its sleep
    (exec log empty, progress verified at poll 22); re-ran cleanly 04:46:43.
- 05:02:33 — Poll 26: GATE OPENED. Mac free 10,044,940 KiB (threshold
  9,769,216; margin +275,724 KiB). Re-read 05:02:55: 10,022,788 KiB — the
  +2.84 GiB jump confirmed real. Margin above the 8 GiB supervisor gate
  ~1.56 GiB (DISK-MARGIN satisfied).
- 05:05:13 — Pre-flight df 9,958,148 KiB. Scripts import-tested (supervisor +
  launcher + analyzer + parser). CPIO embedded vendor 5e0607ad MATCH. QEMU
  cmdline: system image only as virtio-blk drive, NO vendor drive.
  flights/probe-flight did not exist — fresh folder.
- 05:05:43 — Step 4 flight LAUNCHED (supervisor PID 87893, QEMU pid 87904).
- 05:08:07 — Host 135 s, guest_max 118.0 s. Disk 9,814,828 KiB.
- 05:10:27 — Host 270 s, guest_max 203.3 s. Marker T726_PROBE_OK x1
  (L10606, guest 104.493 s). T723_PROBE 0. Disk 9,352,896 KiB.
- 05:12:54 — Host 420 s, guest_max 419.4 s. T723_PROBE 0.
  sys.boot_completed 0. Disk 9,376,848 KiB.
- 05:15:15 — Host 571 s, guest_max 566.9 s. T723_PROBE 0.
  sys.boot_completed 7. Disk 9,566,552 KiB.
- 05:15:44 — Flight ENDED: guest target reached (600.2 s >= 600 s). Host
  600.93 s, guest 600.168 s, QEMU exit 0. UART 27,917 lines, SHA e9d3d30e.
- 05:18:05 — Flight-end verified, no orphans (PID 88707 gone).
- Step 5 adjudication: analyzer run, findings-t726.md written (SHA 3c2b4926),
  outcome FAIL, death note written. Verdict: FAIL — probe never ran;
  trigger fired; registration proven; service-start leg failed silently.
  Wall: SERVICE-START-SILENTLY-FAILED (guest).

Prior lanes (summary): first lane 19:59:38–20:29 16 Nov (Steps 1–3 done,
WAIT-DISK exit); recovery 1 20:32:21–23:32:21 (34 polls, WAIT-DISK exit);
recovery 2 23:35:27–02:35:27 (34 polls, WAIT-DISK exit).
