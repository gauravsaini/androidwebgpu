# DEATH NOTE — T722 (EGL native-fence support)

Verdict: FAIL. Agent 0106e8ed (second recovery), 16 Nov 2026. No commits;
certification is the driver's. Full evidence: findings-t722.md.

## Question asked

Can the SimpleFB EGL stub be given a working eglDupNativeFenceFDANDROID, so
RenderEngine stops logging `failed to dup EGL native fence sync` and the
present path gains real native-fence syncs at the EGL layer? Scaffold stays
REVERTED (revert system 0ba6d6d3).

## Last observed state

- Flight-01 flew 2461.281 s guest (orphaned QEMU, manual SIGTERM 14:25:25
  AEDT). Dup-fail rate 8.126 per 1000 s (20 lines, all pid 2492) — ABOVE both
  pre-stated baselines (T721 flight-01 4.057, T721 revert 6.661). Primary
  criterion FAILED.
- All holds intact: T721 fence log present (fd=10 tl=9), REJECT 0, EPERM 0,
  kills 0, Invalid present fence 0, sys.boot_completed present, simplefb
  mapped present. No crash, no new failure class. The patch is INEFFECTIVE,
  not harmful.
- A/B control NOT FLOWN: disk 7.47 GiB below the 8 GiB supervisor gate at
  flight end. Gap named honestly; T721 flights supply the control baselines.
- Post-flight investigation ruled out every implementation-level cause
  (wrong image, shadow copy, crash, addressing defect, kernel barrier,
  SELinux, namespace, missing mount) and left one surviving explanation:
  the patched stub is never invoked — META-EGL dispatch does not reach it
  (WALL-TYPE-TAG: EGL-DISPATCH-UNREACHABLE). See findings S5.

## Open items (for the successor)

1. Answer the dispatch question FIRST, before any rebuild: disassemble the
   system libEGL.so eglDup wrapper + driver function-table population; or a
   guest probe that dlopens the driver .so and calls eglDup directly as uid
   1000; or a syscall trace of surfaceflinger across a present.
2. If dispatch is the wall: patch the DISPATCH point (META-EGL wrapper /
   function table) or the RenderEngine call sites — not the stub again.
3. Carry a reachability gate in the successor brief: prove the patched
   function is invoked in the guest before building (findings S7 reflection).
4. Chain direction after the EGL wall is understood: display completion /
   frame commit. GateKeeper NPE stays a separate named lane.
5. The A/B control was not flown; if the successor needs a same-day control,
   it needs disk above the 8 GiB gate first (driver authority for pruning).

## Artifacts preserved

- Patched vendor/CPIO/DTB in guest-inputs/ (SHAs in findings S3); trampoline
  + original cave bytes on the box (/home/muse/t722-work); restore word for
  the patch site 0x12800000; original cave bytes 244 B SHA afba2db6.
- Flight evidence in flights/smoke/ and flights/flight-01/ (UART SHAs in
  findings S8). Progress record intact (both WAIT-DISK exits preserved).
- Outcome file: outcome-t722.txt (FAIL). Timeline: TIMELINE.md (rewritten).
