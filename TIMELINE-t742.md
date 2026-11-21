# T742 timeline — post-boot system_server SIGKILL source

All times AEDT, Saturday 21 November 2026, unless marked UTC/Z.

- 09:50:58 — T742 LAUNCHED. Agent 04cfdea4-6cd8-47ad-bd99-2e54c3630f48, sandbox-subagent route. Brief track742-sigkill-source.md SHA f6b05b2c (13,625 B), brief-gate READY. Worktree /Users/Shared/wt-track742-sigkill-source, branch t742, base f39ecec. Registry row 2863 RUNNING. Disk gate OPEN at launch (11,496,112 KiB free, 95%).
- ~10:10 — Step 1 DONE: inputs verified (kernel 70d3859d, system af40403a, vendor fe0bc359, CPIO c1203336, DTB 4ecab141, early-ramdisk 6804341a).
- 10:20:03 — service-flight-19 launched (supervisor 13028, QEMU 13068; gate OPEN 13,534,352 KiB free, 94%).
- ~10:25 — Step 2 DONE: analyzer v3 (SHA 1ce9f532) + supervisor (SHA 4b40bb9a); regression vs flight-18 reproduces T741 counts exactly.
- 10:25:44 — service-flight-19 CUT on the disk gate (7.57 GiB < 8 GiB supervisor floor) at host 330.95 s / guest 261.054 s — VOID. Partial UART preserved: 6,475 lines, 870,178 B, SHA f6416a8d.
- 10:28:01 — service-flight-20 launched (supervisor 21533, QEMU 21543; gate OPEN 12,048,936 KiB free, 95%). In-lane recovery: re-fly on the identical stack in a new folder; flight-19 folder never reused.
- 10:39 — Mid-flight note: supervisor last-1MB window lagged at guest 239.7 s while whole-UART max was 710.25 s — kernel timestamps sparse mid-boot, NOT a guest stall.
- 10:49:17 — First lane agent ended pre-verdict (infrastructure death; no findings-t742.md).
- 10:53:31 — service-flight-20 FINISHED: guest 1516.766 s, host 1519.31 s, QEMU exit 0, stop "Guest target reached (1516.8s >= 1500s)". UART 29,547 lines / 3,832,505 B, SHA 28a47389.
- ~11:02 — Recovery 1 LAUNCHED. Agent 70f6b424-d75e-4e59-85e2-8fcb89b52aec, wedge 14:00 AEDT. Brief track742-sigkill-source-recovery1.md (box, 17,020 B); original-brief tail after the marker SHA-verified f6b05b2c (13,625 B) before any other work.
- 11:05 — analysis-t742.txt written (SHA 81881151, 451 lines). Regression re-run on flight-18's UART reproduces the first lane's regression file byte-identically (diff 0 lines) — saved as analysis-t742-myreg-flight18.txt (SHA 588a0f6b).
- 11:24 — Verdict FAIL (recovery lane): system_server SIGKILL recurred twice (pid 705 watchdog-named ~841 s; pid 1960 killer unidentified ~1230.1 s). Wall SYSTEM_SERVER-SIGKILL-KILLER-UNIDENTIFIED (guest, NEW). findings-t742.md SHA 55cecf6a (18,851 B); outcome-t742.txt "FAIL" SHA 4f8e9e45; DEATH_NOTE-T742.md written; heartbeat CLOSED FAIL.
- 11:28 — Box sync DONE: /mnt/sdc3/muse/kernel-primary/t742/ created (32 files: 12 root + flights/service-flight-19/ 8 + flights/service-flight-20/ 8 + scripts/ 2 + build files CPIO c1203336 / DTB 4ecab141 copied box-side from t741/). All SHAs verified Mac <-> box. Lane complete; certification is the driver's.
