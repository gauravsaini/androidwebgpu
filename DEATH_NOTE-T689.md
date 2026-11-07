# Death note — Track T689: Kernel rebuild per the T688 recipe + re-flight (box)

Question asked: rebuild the canonical kernel per the T688 rebuild recipe
(Route A LTO restore first, then Route B linker-script globs, Route C
fallback) and re-fly it on the box QEMU, proving the T686 boot hang is
fixed.

Last observed state (driver-verified 2026-11-07 ~16:34 AEDT):
- Route A build DONE: BUILD_EXIT=0 at 13:41 AEDT; Image
  11adf23681682ea302ecd5c3fd312b381dbaabd7f65b93543e9a72462df5e367
  (20,384,256 B) in src/arch/arm64/boot/; vmlinux 307,656,840 B;
  LTO trio restored in routeA.config (CONFIG_LTO=y, CONFIG_THINLTO=y,
  CONFIG_LTO_CLANG=y). Full hardening parity NOT applied.
- Then the agent wedged: log frozen 10,458,480 B since 13:39 AEDT,
  heartbeat frozen 02:37:00Z ("step 2 complete"), progress frozen at
  Step 2 (02:34:59Z). No symbol-gate run, no flight, no verdict filed.
- Parent driver killed box opencode PID 3112135 + serve child 3112145
  via TERM ~16:34 AEDT (elapsed 03:03:40, past the 3h mark); both
  gone, no orphan, no -9.
- Verdict: honest FAIL (driver-certified). Findings reconstructed by
  the driver from verified evidence (RECONSTRUCTED-TAG, 2026-11-07).

Open items (all T690's job, same turn):
- Symbol gate on the Route A Image (printk/__die/panic/dump_stack/
  show_regs vs [__init_begin,__init_end]); if any symbol remains
  inside the init range, Route A failed — file FAIL, do NOT fly.
- If the gate passes: re-flight on box QEMU 10.1.0 + canonical A/B
  control on the same QEMU + TEN-PASSES.
- If the gate fails: Route B linker-script rebuild is the next lane's
  job (T691), not T690's.
- T690 must stage findings-t688.md from the box vault — it was absent
  on the box during T689 staging.
