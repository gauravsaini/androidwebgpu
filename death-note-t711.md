# Death note — Track T711 (driver-authored)

The recovery agent (Mac agyx PID 17582, launched 2026-11-09 ~19:02 AEDT) wrote
findings-t711.md (15,146 B, SHA bc18f0b926f3e2e38c373fd39dceb607e1877e234774abeb80b3aaf23b2be3c9,
Verdict: FAIL) at 20:20 AEDT, then died on AGY_ERROR UPSTREAM_ERROR
retryable:false (error_id 7f30bec1-2ed3-473b-a86e-51dacf9e3e95-291) before writing
outcome-t711.txt or a death note of its own.

This note is written by the driver, not the agent, because the agent was already
dead. It records only what the driver verified:

- Findings complete and driver-verified against the flight UARTs (see driver
  record hidden_files/t711-driver-verification-20261109.md). All cited SHAs,
  line counts and counts re-verified by the driver from the Mac worktree.
- outcome-t711.txt was created by the driver from the standard FAIL content
  ("FAIL\n", SHA 4f8e9e45f8a9e1843b81eaf3bdf52a6b778d415d23bf985774a9d34a43f69bd5),
  byte-identical to every prior FAIL lane.
- The agent's death is infrastructure, NOT a lane failure: UPSTREAM_ERROR
  retryable:false, the same class as T706 PID 82802 (~05:00), T710 PID 8364
  (16:40), T710 PID 12325 (17:40) and T711 first agent PID 16549 (~18:55) today.
  No relaunch: the lane finished with findings written; the certification cycle,
  not a recovery agent, closes the lane.
- First agent PID 16549 died ~18:55 AEDT on the same error class after Step 2
  PASS; recovery PID 17582 completed Steps 3a/3b, flight-01 and findings.
- Lane verdict: honest FAIL. The SELinux-domain fix is PROVED (dummy
  sensorservice registered on attempt 0, 0 avc denials); the new wall is
  SensorsMultiHal failing to load /odm/etc/sensors/hals.conf, blocking
  SensorService.onBootPhase on FutureTask.get (9 Watchdog kills).
