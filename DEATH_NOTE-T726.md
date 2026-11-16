# DEATH NOTE — Track T726

## Question asked

Does the composer actually call into the EGL dispatch path? Re-fly the T723
probe with the T725-proven `.rc` shape (service + `on property:sys.boot_completed=1`
+ `start`, dual-trigger marker) and let the probe's own DONE verdict answer it.

## Last observed state

- Full 600 s guest window flown (probe-flight, 05:05:43–05:15:44 AEDT 17 Nov 2026,
  QEMU exit 0, UART SHA e9d3d30e, 27,917 lines).
- Trigger FIRED: `sys.boot_completed=1` at guest 501.540 s; stock actions
  processed (hw/init.rc:1224, bootstat, dmesgd, wifi).
- Registration PROVEN again: marker `T726_PROBE_OK` x2 (guest 104.493 s on-boot,
  guest 506.508 s property action, post-flood).
- Probe NEVER ran: 0 `T723_PROBE` lines, no DONE. Service never started:
  0 citations of `t726probe` / `t723-probe` / `first_stage` in the UART.
- Holds intact (T720 DONE PASS 1, T721 fence log 1, Invalid present fence 0,
  REJECT 0, EPERM 0, simplefb mapped 1, Watchdog KILLING 0, panic/Oops/BUG 0).
- Verdict: FAIL (findings-t726.md, SHA 3c2b4926).

## Wall

WALL-TYPE-TAG: SERVICE-START-SILENTLY-FAILED (guest). The T724 wall
ADDED-RC-SILENTLY-IGNORED is REFUTED for this file shape — both actions'
writes executed; only the `start t726probe` leg left no trace.

Rust par bhi lagu: haan

## Open items (for the successor)

1. Discriminate the start-leg failure: re-fly the identical property-action
   shape with a known-good service binary (`/system/bin/toybox echo` to a
   distinct marker). If it runs, the fault is the probe path
   (`/first_stage_ramdisk/t723-probe` unproven from second-stage); if it
   does not, the fault is the start leg itself.
2. Prove `/first_stage_ramdisk` is reachable from second-stage init (or move
   the probe binary to `/system/bin`) before trusting any probe path again.
3. EGL dispatch question (R1/R3/R4) still OPEN. Display completion / frame
   commit follows only after a probe binary actually runs and returns DONE.
