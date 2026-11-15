# TIMELINE — Track T720: kernel fence source

All times AEDT (Australia/Sydney), Monday 16 November 2026,
from `date` reads in the same commands.

- 08:34:30 — Lane launch. Brief re-hashed (bf2f1782, 36,666 B)
  and read in full. Worktree
  `/Users/Shared/wt-track720-kernel-fence-source`, branch
  `t720`, base `de8a953`.
- 08:34:54 — Step 0 disk gate PASS: 10 Gi free, 95% used.
- 08:35:14/08:35:50 — Step 1 inputs verified PASS (Mac + box).
- 08:35–08:39 — Provenance ESTABLISHED: box tree HEAD
  `4da740c10dc0` + Route B lds patch; tree Image byte-identical
  to flown `53d9563e`. Baseline config delta recorded (99
  lines). Config change applied with the full build environment
  (first attempt without LD=ld.lld caught and redone).
- 08:39:46–08:42:47 — Kernel build, exit 0. New Image
  `70d3859d` (20,396,544 B). Pre-flight gates PASS.
- 08:46 — Probe v1 built (`015dc39b`, 7,568 B), not
  host-executed.
- 08:47 — CPIO v1 (`d623d48c`) + DTB v1 (`92c6fab8`) built.
- 08:49:45–08:51:19 — Transfers box → sandbox → Mac, SHA-verified.
- 08:55:22 — Scripts import-tested on the Mac; fixture-verified
  against the T719 UARTs (exact reproduction).
- 08:55:44 — Smoke attempt 1 (v1) launched. QEMU exit 0 at
  guest 9.6 s: probe PASSED, but the v1 sysfs mount aborted
  init (EBUSY). Archived `flights/smoke-attempt-1/`.
- 09:00:22 — Attempt-1 archive verified (UART `58534665`,
  flight-end `d4a5e264`).
- 09:01 — Probe v2 (`d25aee60`) + CPIO v2 (`e37ec1c7`) + DTB
  v2 (`b1594857`) built; supervisor pins updated.
- 09:02:24 — v2 stack verified on the Mac.
- 09:02:37 — Smoke attempt 2 (v2) launched. QEMU exit 0 at
  guest 10.0 s: probe PASSED, silent reboot after switch_root.
  Archived `flights/smoke-attempt-2/` (UART `fc5731e7`).
- 09:06:09–09:11:35 — Bisection A (new kernel + old CPIO/DTB):
  HEALTHY, guest 192.7 s, UART `ee250495`.
- 09:11:53–09:17:16 — Bisection B (old kernel + new CPIO/DTB):
  HEALTHY, guest 188.1 s, UART `43d659ee`, probe FAIL expected.
- 09:14 — Probe v3 (`61140fc7`) built: unmount + rmdir on all
  paths, zero persistent state.
- 09:18 — CPIO v3 (`848a470f`) + DTB v3 (`3ff2bb8e`) built;
  supervisor pins updated; verified on the Mac 09:18:52.
- 09:19:06 — Smoke attempt 3 (v3) launched. Guest target
  reached (260.1 s), QEMU exit 0, 13,683 lines, UART
  `67f92b81`. Smoke PASS (verified 09:24:32).
- 09:25:00 — Flight-01 launched.
- 10:05:57 — Flight-01 complete: guest 2,403.3 s, QEMU exit 0,
  320,917 lines, UART `f1ee6d02`. Analysis PASS (§7 findings).
- 10:07:53 — Flight-ab control launched. QEMU self-exit 0 at
  guest 20.1 s, 595 lines, UART `ff06cba6`. Differential
  PROVEN (probe FAILs on the canonical kernel).
- 10:08–10:15 — Adjudication: findings + outcome + TIMELINE
  written, heartbeat CLOSED. Verdict: PASS. No commit (driver
  owns certification, registry, vault, delivery).

Wedge trigger was 11:34:30 AEDT; the lane closed well inside it.
