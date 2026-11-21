# T743 timeline — 1960-killer probe (all times AEDT, 21 Nov 2026)

- **11:40** — T743 launched (wedge 14:40). Brief `track743-1960-killer.md` (box SHA 0ac236d0). Worktree `/Users/Shared/wt-track743-1960-killer`, branch `t743`, base T742 commit `6e716e5`.
- **11:40–11:44** — Step 1 DONE. All 6 input SHAs verified (kernel 70d3859d, system af40403a, vendor fe0bc359, CPIO c1203336, DTB 4ecab141, early-ramdisk 6804341a). T742 findings read in full (SHA 55cecf6a).
- **11:44–11:49** — Step 2 DONE. Tombstone mining on flight-20 UART (29,547 lines, SHA 28a47389). Kill-verb grep first: 2 lines, both ActivityManager killing apps — reproduces T742. Exactly ONE tombstone file (tombstone_00, fingerprint HAL pid 2688, SIGABRT). Exactly ONE `system_server_watchdog` entry (pid 705 only) — watchdog RULED OUT for 1960. **KILLER FOUND: system_server self-kill** — `system_server_crash@1699659367335.txt` (L23203–23245): GateKeeper NPE (`IGateKeeperService.reportDeviceSetupComplete()`, LockSettingsService.java:3394) + `KillApplicationHandler`, death L24670 ~3.4 s later. Cross-confirmed in flight-18: pid 678 died the same way (crash file L16807, death L18508).
- **11:47–11:49** — Analyzer + supervisor extended (patch `/tmp/patch-t743.py`): 4 new analyzer sections (tombstone-mining, system_server_crash, kmsg, meminfo); supervisor gains two live taps (kmsg every poll, meminfo-pressure every poll). Both compile OK. Regression vs flight-20: diff exactly 109 lines (first-boot correction 1167.684 → 1167.317 s + 4 new sections); all T742 counts reproduce.
- **11:50:39** — Disk gate OPEN at launch (10,570,672 KiB free, 96%; gate needs 9,769,216 KiB).
- **11:51:17** — **service-flight-21 LAUNCHED.** Supervisor PID 96645, QEMU PID 96666. Identical stack, GDB port 1269.
- **12:16:44** — **Flight-21 COMPLETE.** Guest target reached (1523.9 s ≥ 1500 s), host 1527.76 s, QEMU exit 0. UART 50,960 lines / 6,726,819 B, SHA c00a068c.
- **12:17–12:21** — Analysis. Extended analyzer exit 0 → `analysis-t743.txt` (662 lines). Adjudication:
  - pid 683 watchdog-killed (104 s block, KILLING L13549, death L13972) — known T742-705 mode.
  - **pid 1970 GateKeeper-NPE self-kill — the 1960-mode recurrence, CONFIRMED** (NPE L28993 23:31:45.142, am_crash L29022, crash file L27538–27570 ts 1699659105424, death L29526 23:31:51.754).
  - pid 3235 native SIGSEGV in binder thread (fault 23:38:26.098, addr 0xd1008403, death L50040 23:39:18.939) — NEW question, no tombstone (UID-1000 dumps ignored), trigger unidentified.
  - Dup 0 in 641.6 s presenting → 0.000/1000 s (below both T722 baselines). Boot first fired 882.325 s. All holds intact. kmsg tap exact-matches UART (0 OOM). meminfo tap: 28× "Not killing", lmkd drops are death symptoms (<1 s before each death report).
- **12:22** — Deliverables written: findings-t743.md, outcome-t743.txt (PASS), TIMELINE-t743.md, progress appended, heartbeat CLOSED. Sync to box pending.
