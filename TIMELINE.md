# T741 timeline — vendor init rc fix + 1500 s window

All times AEDT 21 Nov 2026 unless marked Z (UTC). Clock read from `TZ=Australia/Sydney date` at each stamp; no estimated timestamps.

| Time | Event |
|---|---|
| 07:50 | Lane launched (agent e34acd85). Wedge 10:50. |
| 07:56 | Step 0 disk gate: 18,196,364 KiB free, 92%. OPEN. |
| 07:57 | Step 1 input verification complete, all PASS (kernel 70d3859d, system af40403a, vendor 5e0607ad, T740 CPIO 724dff6c, DTB 6b6a118d, early-ramdisk 6804341a). |
| 08:20 | Step 2 build complete, PASS. Vendor repacked (fe0bc359), CPIO rebuilt (c1203336), DTB re-patched (4ecab141), analyzer Watchdog pattern fixed + regression-tested on flight-15 L14509. |
| 08:20:22Z | Heartbeat written (Step 2 complete). |
| 08:24:03 | Flight service-flight-18 launched (supervisor PID 48083, QEMU PID 48091). Disk recheck 15,899,196 KiB free, 93%. OPEN. |
| 08:40 | Mid-flight spot-check, guest ~1005 s: T734_KERNEL_OK 1, T723_PROBE 27, sys.boot_completed 8, WATCHDOG KILLING 0, UART 57,211 lines. |
| 08:49:20 | Flight ended: guest 1512.44 s, host 1516.98 s, QEMU exit 0. "Guest target reached (1512.4s >= 1500s)". UART 133,388 lines, SHA bba22ffe. |
| 09:14 | Step 4 adjudication + Step 5 findings complete. Verdict PASS. |

## Guest-time anchors (from the flight-18 UART)

| Guest s | Event |
|---|---|
| 3.489 | T734_KERNEL_OK (L194) |
| 4.683 | T723_PROBE start (L357–358); SP3 fd=-13, SP1/SP2 dup rc=-1 |
| 76.983 / 77.081 | T728_PROBE_OK / T733_PROBE_OK leg 1 (on boot) |
| 521.327 | First sys.boot_completed firing (L14375) — fastest in the chain |
| 527.208 / 527.211 | Property action processed; `start t728probe` FAILED (SELinux label, L14917) |
| 527.291 / 527.364 | T728/T733_PROBE_OK leg 2 (init-written trigger markers) |
| ~554.3 | T721 fence-log (L16401): composer pid 508, fd=10 tl=9 — real fence, post-boot |
| ~581 | 6 FATAL EXCEPTIONs within 0.64 s (sdksetup, permissioncontroller, launcher3, phone + 2 unnamed) |
| ~596 | Zygote: "Process 678 exited due to signal 9 (Killed)" (L18508) — system_server SIGKILL |
| ~623 | system_server pid 2176 first lines (L18994, "Using CollectorTypeCC GC") |
| 723.078 | sys.boot_completed re-processed (L26357) by the restarted system_server |
| 727.313 / 727.740 | Property action re-fired; T728/T733_PROBE_OK leg 2 repeat |
| 1430.065 | Last post-boot CALL allocate2 (L102675) |
| 1512.440 | Guest max; flight-end |
