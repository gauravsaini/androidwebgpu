# TIMELINE — Track T721: composer sw_sync retire fence

All times AEDT (Australia/Sydney), from clock reads in the same command
unless marked derived.

- 2026-11-16 10:22:53 — Lane launched (sandbox subagent, parent spawn).
  Worktree `/Users/Shared/wt-track721-composer-retire-fence`, branch
  `t721`, HEAD `fecf5e2`. Wedge deadline 13:22:53.
- 10:23:20 — Step 0 PASS: Mac data volume 9.7 GiB available (96% used),
  above the 8 GiB gate.
- 10:26 — Step 1 PASS: every brief input SHA re-hashed and matched; no
  network fetch.
- 10:26–10:35 — Step 2 characterisation: present path re-verified,
  writeFence contract, sw_sync helpers dead-but-usable, debugfs mount
  question resolved, binary-patch route justified, trampoline host
  `sync_file_info` (0xb7670) + `.bss` extension chosen.
- 10:47 — Step 2 implementation recorded: trampoline (241 B), splice,
  vendor patch, revert image, supervisor + launchers + analysis scripts.
- 10:49:05 — Smoke #1 launched (QEMU pid 19591). WRONG CPIO (embedded
  original vendor). Host 300.5 s, guest 206.8 s, 13,733 lines. Allocator
  / holds PASS, T721 log 0. Archived to
  `flights/smoke-cpio-vendor-mismatch/`.
- 10:55:12 — Flight-01 #1 launched (QEMU pid 19837). WRONG CPIO.
- 11:06:24 — Flight-01 #1 killed (guest 669.26 s, 27,880 lines) after
  the embedded-vendor discovery (UART L339 + CPIO member hash
  `bac3cdc1...`). Archived to `flights/flight-01-cpio-vendor-mismatch/`.
- 11:06–11:15 — CPIO rebuild (`t721-fix.cpio.gz`, `6a302597...`) + DTB
  re-align (`t721-modified.dtb`, `91b45a10...`); supervisor + launchers
  updated, re-verified on the Mac.
- 11:15:37 — Smoke redo launched (QEMU pid 20476). PASS: host 300.6 s,
  guest 186.3 s, 13,829 lines; holds PASS; fence prong deferred.
- 11:22:34 — Flight-01 redo launched (QEMU pid 20636).
- 11:35:24 — MILESTONE: first `T721 fd=10 tl=9` log line (guest 765 s).
- 11:45 — Revert image verified on the Mac (2,059,816,960 B, `0ba6d6d3...`).
- ~11:46 — Supervisor disk-gate exit (7.96 GiB < 8 GiB) at host 1,382 s;
  QEMU orphaned, kept running.
- 12:03:31 — Manual QEMU SIGTERM at guest 2,464.46 s. Flight-01 UART
  166,172 lines, SHA `8ecdf8e4...`. Manual flight-end record written.
- 12:06:32 — Scaffold-revert flight launched (supervisor pid 22110,
  QEMU pid 22116).
- 12:16:33 (derived: launch + host 601.3 s) — Revert flight complete:
  guest 600.6 s, QEMU exit 0, UART 24,748 lines, SHA `76d47743...`.
  T721 log 1, fence errors 0 WITHOUT the scaffold — DECISIVE.
- 12:19:13 — A/B control (240 s) launched (QEMU pid 22545).
- 12:19:54 — A/B 240 s ABORTED: design flaw (ends before first present
  ~565 s, vacuous). Partial UART (2,112 lines) archived to
  `flights/flight-ab-240s-aborted/`.
- 12:20:27 — A/B control LONG (700 s guest / 800 s host) launched
  (supervisor pid 22614, QEMU pid 22618).
- 12:31:50 — A/B control completes (ended_utc 01:31:50Z): guest 700.2 s,
  QEMU exit 0, UART 29,393 lines, SHA `99246fba...`. T721 log 0, fence
  errors 72 — the differential is proven. Step 4 adjudication: PASS.

- Lane closed (clock read below) — Step 4 adjudication PASS; findings + outcome + TIMELINE delivered; heartbeat CLOSED.
