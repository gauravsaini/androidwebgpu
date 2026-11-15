# TIMELINE — Track T719: invalid present fence

All timestamps AEDT (Australia/Sydney) unless marked UTC. Every
timestamp below comes from a `date` read inside the command that
recorded it.

- 2026-11-16 06:08:42 AEDT — Lane task arrived (driver spawn).
- 2026-11-16 06:09:37 AEDT — Lane start (Mac clock read).
- 2026-11-16 06:10:50 AEDT — Step 0 PASS (Mac disk 12 Gi free, 95%) and
  Step 1 PASS (all input SHAs matched, 51 guest-inputs files
  byte-identical) recorded.
- 2026-11-16 06:11:53 AEDT — Framework binaries extracted read-only
  from the vault system.img (surfaceflinger `259d39f5`, libgui
  `5082485c`, libui `e14f836c`, libsync `e79e3721`).
- 2026-11-16 06:24:07 AEDT — Step 2 SMOKING GUN recorded: composer
  `setPresentFence(-1)` hardcoded; fence chain proven end-to-end;
  SKIP-not-DROP discrimination proven.
- 2026-11-16 06:28:06 AEDT — Patch built and verified on the box:
  patched surfaceflinger `fadee423`, new system.img `ac15a68a`.
- 2026-11-16 06:31 AEDT — Flight scripts transferred to the Mac and
  verified (compile + import OK).
- 2026-11-16 ~06:56 AEDT — Hop-1 transfer (box→sandbox) SHA-verified
  complete; hop 2 (sandbox→Mac) launched. (Hop 1 survived a sandbox
  runtime restart at ~61%; resumed with `--partial`, no duplicate
  effect.)
- 2026-11-16 07:28:19 AEDT — Smoke launched; 07:32:41 AEDT — smoke
  complete (guest 256.0 s, QEMU exit 0, 13,509 lines): SMOKE PASS,
  regression gate clear.
- 2026-11-16 07:34:52 AEDT — Flight-01 launched; 08:14:58 AEDT —
  flight-01 complete (guest 2,403.7 s, QEMU exit 0, 309,407 lines):
  fence errors 0, allocation + boot holds intact.
- 2026-11-16 08:16:03 AEDT — Flight-ab launched; 08:16:25 AEDT —
  control complete (guest 19.4 s, self-exit 0, 575 lines): pattern
  matches T716/T718.

Wedge trigger: 09:08 AEDT 16 Nov 2026 (driver ledger).
- 2026-11-16 08:21:39 AEDT — Lane closed: deliverables written (findings, outcome, death note, timeline, progress, heartbeat CLOSED). Verdict FAIL. All flights preserved.
