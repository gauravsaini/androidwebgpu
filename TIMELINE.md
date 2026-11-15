# TIMELINE — Track T715 (all times AEDT, 15-16 Nov 2026)

- 22:29:54, 15 Nov — User taps "Run T715 via a sandbox subagent driving the Mac".
- 22:31:59, 15 Nov — Lane agent spawned (sandbox subagent driving the Mac via macssh; no agyx/agent launch on the Mac, per the route constraint).
- 22:33, 15 Nov — Step 0 PASS (disk 12 Gi avail, 95%) + Step 1 PASS (all 13 brief inputs SHA-verified, no network fetch). guest-inputs/ hardlinked from the T714 worktree.
- 22:34-22:40, 15 Nov — Step 2 re-derivation: class_start main lives only in `on nonencrypted`, which never fires; switch_root proven; /system/bin/installd exists; early-boot trigger point proven. Decision: append `start installd` in `on early-boot` to installd.rc in a box-side system.img copy.
- 22:47, 15 Nov — Box-side repack starts (Linux debugfs, T706 procedure; no sudo, no loopback mount).
- 22:52, 15 Nov — Repack COMPLETE + verified (e2fsck clean, block 145414 + services.jar unchanged). New image SHA 1283cbf2… recorded in progress-t715.txt.
- 22:55-23:14, 15 Nov — Transfer hop 1: box → sandbox (25.7 min), SHA re-verified on arrival.
- 23:14-23:43, 15 Nov — Transfer hop 2: sandbox → Mac guest-inputs/t715-system.img (29.4 min).
- 23:44, 15 Nov — Image SHA-verified on the Mac; verify_inputs_t715.py 16/16 PASS. First smoke launch dies on a doubled-backslash regex (transcription error); fixed + runtime import-tested.
- 23:45:36, 15 Nov — Smoke flight launched (supervisor 87529, QEMU 87535; 240 s guest target — brief variance, recorded).
- 23:50, 15 Nov — Smoke PASS: installd started guest 94.2 s, "installd not found" 0, SensorService onBootPhase returned in 7231 ms. UART SHA 0e5221ab… (13,687 lines).
- 23:52:10, 15 Nov — Flight-01 launched (supervisor 87705, QEMU 87710; 2400 s guest / 2500 s host cap).
- 00:12, 16 Nov — Mid-flight peek (read-only): guest 1245 s, 0 kills, boot_completed=1 actions seen, Launcher3 FATALs + queueBuffer 0 noted; no verdict yet.
- 00:32:14, 16 Nov — Flight-01 COMPLETE: guest target reached (2402.4 s), exit 0, 244,911 lines, UART SHA 8d69ee3b….
- 00:35:19, 16 Nov — Flight-ab launched (canonical kernel control).
- 00:35:39, 16 Nov — Flight-ab COMPLETE: QEMU exited on its own at guest 19.28 s, 575 lines, UART SHA abaf26bc… (control pattern matches T714).
- 00:35-00:50, 16 Nov — Step 4 adjudication from the preserved UARTs (analysis scripts fixed for byte-exact grep-consistent line numbering; all counts cross-checked with grep -n/-c). Verdict: FAIL — composer /dev/mem EPERM is the first failing link; GateKeeper NPE is the parallel stability wall.
- 00:40, 16 Nov — Deliverables written (findings, outcome, death note, progress, TIMELINE, CLOSED heartbeat). No commit. Lane closed; driver owns certification, commit, push, vault.
