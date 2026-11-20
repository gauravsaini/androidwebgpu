# DEATH NOTE — Track T740

**Question asked:** Extend the flight window to ~1200 s, kill the orphaned
background scripts at the handoff, confirm dup -1 in the surfaceflinger
context, and test a sw_sync timeline-access fix route.

**Verdict: FAIL** (honest FAIL — the question is left partly unanswered, not
unflown).

**Last observed state (2026-11-21 ~07:25 AEDT):**

- Flight-15 (1200 s) RAN in full: guest 1202.06 s, QEMU exit 0, UART 26,242
  lines / 3,505,378 B / SHA `0befc1fe`. **Boot COMPLETED: `sys.boot_completed`
  first fired guest 1175.836 s (12 lines).** The T729 wall
  BOOT-COMPLETED-NEVER-FIRED is CLOSED.
- Orphaned scripts killed at the handoff (post-handoff spam 0/0 in all three
  flights, vs 4480/337 in T739). All unmounts rc=0; handoff verified.
- **Unmeasured, not refuted:** dup -1 in the surfaceflinger context.
  RenderEngine dup-fail 0 in only 26.2 s of post-boot presenting time; the
  T734 calibration needs ~40 s to the first dup-fail. First failing link.
- **Mechanism test PASSED (flight-17):** the timeline-access barrier is DAC
  traversal on `/sys/kernel/debug` (mode 0700 root), not the file mode, not
  SELinux, not dispatch. After `chmod 0755` on the debugfs parent dirs, SP3
  opens the timeline (fd=4), SP1 dup rc=5, SP2 dup rc=7 — all three sub-probes
  mint real signaled sw_sync fences. Flight-16 (file chmod only) still failed
  fd=-13, proving the file was never the barrier.
- The composer (root) opened the timeline and minted a fence in the booted
  guest (T721 fd=10 tl=9, guest ~890 s) — it bypasses the 0700 dir via
  CAP_DAC_OVERRIDE, which is why it succeeded where the uid-1000 probe failed.

**Open items for the successor (T741):**

1. Build the booted-guest fix: vendor init rc making `/sys/kernel/debug`
   traversable by uid 1000 (mechanism proven in flight-17).
2. Fly a longer window: guest target ~1500 s (boot-tail ~1180 s + ≥60 s
   post-boot margin), host cap ~1800 s; T722 differential criteria apply.
3. Investigate the system_server watchdog kill at ~guest 935 s (main thread
   blocked 94 s; gralloc pid 614 native crash) as a boot-slowness contributor.

**What is NOT owed:** no LD_LIBRARY_PATH export reintroduction; no kernel
rebuild; no T734 bisect re-run (void); no system-image mount onto /system
(refuted, T735); no dispatch fix (answered five times now).

**Evidence:** findings-t740.md (full counts, all three flights);
flights/service-flight-15/ (9 files), service-flight-16/ (8 files),
service-flight-17/ (8 files). Nothing committed; the driver certifies.
