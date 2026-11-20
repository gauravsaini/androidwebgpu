# DEATH NOTE — Track T739 (scope LD_LIBRARY_PATH to the probe line only)

**Question asked:** Scope LD_LIBRARY_PATH to the probe line only so toybox
can unmount /sysimg before the handoff (closing the T738 wall
INIT-UMOUNT-SELF-BLOCKED and the T737 wall INIT-SYSTEM-MOUNT-LOOP0-UNBOUND),
then fly the full 850 s flight, confirm dup -1 in the surfaceflinger context,
and test a sw_sync timeline-access fix route.

**Verdict: FAIL** (honest). The lane's own fix WORKED and both target walls
are CLOSED — but the guest never completed boot inside the 850 s window, so
there is no surfaceflinger context to confirm dup -1 in and no fix route
could be tested.

**Last observed state (service-flight-14, guest 851.145 s, QEMU exit 0):**
- Full 850 s flight RAN (target reached). UART 13,683 lines, SHA 918ef4b4.
- Probe RAN (27 T723_PROBE): SP1/SP2 dup -1 vs IMAGE libEGL (0x7f9ada97a8),
  SP3 fd=-13. Dispatch answered a 4th time.
- All three unmounts rc=0 ([T739] L589/618/647); handoff state verified
  (L649/650); no HANDOFF ABORTED. Init mounted /system from vda (guest
  22.287 s) and switched root (22.293 s). T737 + T738 walls CLOSED.
- system_server (pid 701) forked and was still in OnBootPhase_500 at the
  target (83/143 services); sys.boot_completed 0; surfaceflinger started
  and connected display 0 but never presented (fence-log 0, dup-fail 0).
- The T729 wall BOOT-COMPLETED-NEVER-FIRED has RECURREND: boot times on
  identical inputs are 495 s (T730), 790 s (T734), >851 s (T739). The 850 s
  window does not reliably contain the boot.

**Open items (for the successor, T740):**
1. Extend the flight window (guest target ~1200 s, host cap ~1500 s;
   T734-calibrated need ~957 s + margin for the observed spread).
2. Kill the orphaned background scripts (/t493-probe.sh,
   /t501-keystore2-dump.sh) at the handoff — ~4,800 lines of UART spam +
   wasted guest CPU in tight loops post-handoff.
3. Once the boot completes and surfaceflinger presents: confirm dup -1 in
   the surfaceflinger context, then test the timeline-access fix route
   (probe-context mechanism test first; the surfaceflinger-context fix must
   live in the booted guest).
4. Investigate the storaged-retry loop (492 retries) and the missing
   artd/media.metrics/storaged services as boot-slowness suspects.
5. Do NOT reintroduce the LD_LIBRARY_PATH export (T738 wall stays closed
   only while the path stays scoped to the probe line).

**Evidence:** findings-t739.md (SHA 3e346924); outcome-t739.txt (FAIL);
flights/service-flight-14/ (UART 918ef4b4, analysis-t739.txt 8fe102ff);
build CPIO df2a6c9a / DTB e4b0ff79 / early-ramdisk d75f5658 /
supervisor 8f6cc5ea / analyzer 67b77198. Worktree
/Users/Shared/wt-track739-scope-ld-library-path; box
/mnt/sdc3/muse/kernel-primary/t739/.
