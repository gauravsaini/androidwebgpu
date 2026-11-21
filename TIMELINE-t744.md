# TIMELINE — T744 recovery 1 (service-flight-22)

All times AEDT, Saturday 21 November 2026. Lane agent
`6d5d51d5-fd94-4b8d-9e56-ecc30dcd7a25`, spawned 15:43:33 AEDT, wedge ~18:43 AEDT.
Brief: `track744-sigsegv-binder-thread-recovery1.md`, SHA `80a202ad` (13,916 B).
Worktree `/Users/Shared/wt-track744-sigsegv-binder-thread`, branch `t744`, base `b78ac6e`.

## Before this lane (first lane, agent 066a65aa)

- 12:47:16 — T744 launched (driver). Mining of flight-21 names the 3235 SIGSEGV
  trigger (launcher3 RenderThread 0x8893 → binder death → NPE connectionToken
  must not be null → disposed channel → binder thread 4302 SIGSEGV 0xd1008403).
- 13:06 — First lane exits WAIT-DISK (sandbox EDQUOT + Mac disk 100%). Trigger
  NAMED; findings NOT written per the brief.
- 13:08:22 — First lane completed. Registry row 2865 WAIT-DISK.
- 13:14–15:34 — Transfer saga: EDQUOT root cause verified 13:45; /var/tmp ENOSPC
  13:56; /srv relay; VM replaced ~14:40; EDQUOT ep.4 ~14:52; /srv relay recreated
  14:59; stage 1 complete ~15:24; transfer COMPLETE 15:34:56 — all 5 inputs on
  the Mac, sizes + SHAs driver-verified 15:41.
- 15:23 — Mac disk freed itself: 81% / 40,092,792 KiB free. Autocompare clear
  offer MOOT.

## Recovery lane 1 — this lane

- 15:43:33 — Recovery lane spawned (driver). Brief SHA `80a202ad`.
- 15:44 — Step 1 DONE: all 7 SHAs re-verified on the Mac (t720-Image `70d3859d`,
  QEMU 11.1.2 `8e5c46dc`, t733-system.img `af40403a`, t741-vendor.img `fe0bc359`,
  t741-fix.cpio.gz `c1203336`, t741-modified.dtb `4ecab141`, early-ramdisk-t741
  `6804341a`).
- 15:45 — Step 2 DONE: all 5 mining counts reproduce exactly on the flight-21
  UART (SHA `c00a068c`): connectionToken NPE 1, `0xd1008403` 1, `0x8893` 5,
  `createPatternedIndexBuffer` 10, disposed-without-removal 7.
- 15:46 — Disk gate re-checked immediately before launch: OPEN
  (40,722,232 KiB free, 81%; need ≥ 9,769,216 KiB).
- 15:46:14 — Step 3: service-flight-22 LAUNCHED (supervisor pid 68738, QEMU pid
  68806, GDB port 1269). Preflight verified all 5 inputs by SHA.
- 15:51:16 — Progress: host 301 s, guest 294.6 s, pid 68806 alive.
- 15:56:17 — Progress: host 603 s, guest 502.6 s.
- 16:01:18 — Progress: host 904 s, guest 901.5 s.
- 16:06:20 — Progress: host 1206 s, guest 1204.4 s.
- 16:10:03 — Status check: host 1416.6 s, guest 1415.1 s of 1500 s; both
  processes alive; heartbeat fresh.
- 16:11:17 — Flight ENDS: guest target reached (1500.6 s >= 1500 s). Host
  1501.98 s, guest 1500.555 s, QEMU exit code 0. UART 27,606 lines /
  3,740,533 B, SHA `9653b78c`.
- 16:13:16 — Flight-end verified (flight-end.json present; supervisor exited
  cleanly; no orphan QEMU).
- 16:13 — Step 4: analyzer run (`scripts/analyze_t744.py`, exit 0, 674 lines,
  `analysis-t744.txt` SHA `d224e174`).
- 16:14–16:22 — Independent verification of the decisive lines: the 731 SIGKILL
  death window (L16110–16112), the 4 DeadSystemException lines (consequences,
  not causes), the 1975/1885 routine kills (both by 731), the 2 bluetooth
  SIGSEGVs (1848 @ 0x67750, 3022 @ 0x9b51e), the empty binder-death tap (0
  lines), the death-window tap (native intercepts failed "due to input" twice
  for pid 731), the kmsg tap (2,477 lines, 0 OOM / 0 Killed process).
- 16:24 — Verdict adjudicated: PASS. Findings written (`findings-t744.md`,
  SHA `cabbe4b4`).

## Flight-22 key events (guest time)

- Guest 1044.345 s — first `sys.boot_completed` firing (L13688, concatenated
  line). 22 `sys.boot_completed` lines total.
- Guest ~1063.6 s — pid 1975 SIGKILL (routine, by system_server 731).
- Guest ~1132.8 s — pid 1885 (com.android.phone) SIGKILL (routine, by 731,
  "bg anr").
- Guest ~1145.27 s — system_server pid 731 SIGKILL, killer UNIDENTIFIED (NEW
  WALL; GateKeeper-NPE, watchdog, OOM/LMK, init, AM all excluded).
- Guest ~1233 s — system_server 2437 created (wall 23:32:06); flight continues.
- Guest 1500.555 s — target reached; flight ends, QEMU exit 0.

## Verdict

Verdict: PASS — the 3235 SIGSEGV trigger is NAMED with flight-21 mining
evidence; full 1500 s flight, QEMU exit 0; boot completes; dup 0 in 456.2 s
presenting (0.000/1000 s, below both T722 baselines); all holds intact;
unmount hooks fire; post-handoff spam 0. Caveats recorded in the findings:
flight-22 did NOT confirm the trigger's one inferential link (the chain did not
recur — the launcher RenderThread crash is non-deterministic), and flight-22
produced a NEW WALL (system_server 731 SIGKILL, killer unidentified, T743
mechanism ruled out) for the successor track.
