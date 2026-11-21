# T742 findings — post-boot system_server SIGKILL source (repeat flight + killer probe)

## Verdict: FAIL

The SIGKILL recurred — system_server died by SIGKILL twice in the full 1500 s flight — and for the second death the killer is still UNIDENTIFIED. First failing link: system_server pid 1960 SIGKILL at guest ~1230.1 s, killer UNIDENTIFIED. Wall (guest, NEW): SYSTEM_SERVER-SIGKILL-KILLER-UNIDENTIFIED.

WALL-TYPE-TAG: guest (NEW). Rust par bhi lagu: haan — the deaths are guest-side Android behavior (watchdog/LMK/init in the guest image); the Rust engine boots the same guest image, so it will meet the same deaths — the killer must be identified regardless of engine.

## Summary

- Evidence flight is service-flight-20 (flight-19 is VOID, cut on the disk gate). Flight-20 ran the full 1500 s guest window on the IDENTICAL T741 stack, QEMU exit 0. Boot completed (third consecutive boot). The dup-fail regression holds (0 in 349.4 s presenting, rate 0.000, below both baselines) — but on thin presenting traffic (2 post-boot allocate2 calls), because system_server died twice.
- system_server died by SIGKILL twice: pid 705 at guest ~841 s and pid 1960 at guest ~1230.1 s. A third system_server (pid 3285) was created at guest ~1240 s and was alive at flight end.
- Killer for 705: NAMED — the Watchdog. L11158: `W/Watchdog(  705): *** WATCHDOG KILLING SYSTEM PROCESS: Blocked in handler on main thread (main) for 111s` (guest ~832 s). Same distress signature as T740 flight-15 (pid 788, main thread blocked 94 s): media.c2 one-second waits 173, camera service unavailable 91.
- Killer for 1960: UNIDENTIFIED. Every candidate is exhausted by absence (see the 1960 section below). No watchdog line, no LMK kill line, no RAM-pressure signal, no init kill naming system_server, no ActivityManager kill naming system_server. Flood rate around the death is 2.6 lines/s — a kill line would still be visible.
- The verdict contract's FAIL clause fires on the 1960 death ("the SIGKILL recurs with the killer still UNIDENTIFIED"), and the Watchdog KILLING hold (requires 0, got 1) also fails. The dup regression, the boot, the hooks, and the spam checks all hold.

## Counts (service-flight-20; UART 29,547 lines / 3,832,505 B, SHA 28a47389)

- Flight: guest 1516.766 s, host 1519.31 s, QEMU exit 0, stop "Guest target reached (1516.8s >= 1500s)". Started 2026-11-20T23:28:12Z, ended 23:53:31Z.
- Inputs (flight-end.json, all match Step 1): kernel 70d3859d, QEMU 8e5c46dc, DTB 4ecab141, CPIO c1203336, system af40403a, vendor fe0bc359, early-ramdisk 6804341a. The flown CPIO is byte-identical to T741's (SHA c1203336); its vendor member was verified in T741 certification as fe0bc359 with composer rc cdeb88bf carrying the fix — same bytes, same member, carried verification.
- T723_PROBE: 27 (start 2 L356/357, DONE 1, SP1 DONE 1, SP2 DONE 1, SP3 DONE 1). SP1 dup rc=-1: 1. SP2 dup rc=-1: 1. SP3 fd=-13: 1. DROP_FAIL 0, RESOLVE_FAIL 0, GETPROCADDRESS_NULL 0. (Early-ramdisk control, pre-handoff — NOT fix failure.)
- T734_KERNEL_OK: 1 (L193, guest 7.795 s). [T734] hook: 1.
- T720_PROBE DONE status=PASS: 1 (L225), FAIL: 0.
- T721 fence-log: 1 (L23336, composer pid 510, fd=10, tl=9, post-boot). Invalid present fence: 0. REJECT: 0. EPERM: 0. simplefb: mapped: 1 (L23334).
- CALL method=allocate2: 7 total — 2 early (guest 93.97/93.98 s), 3 pre-boot-completed (guest 1154.20/1154.21/1154.21 s), 2 post-boot (L22062 guest 1189.392 s, L22244 guest 1191.720 s).
- RenderEngine dup-fail ("failed to dup EGL native fence sync"): 0. Dup-fail rate (presenting-time basis): 0 / 349.4 s * 1000 = 0.000 per 1000 s — BELOW BOTH baselines 4.058/6.660. Verdict signal: BELOW BOTH BASELINES.
- sys.boot_completed: 6 lines. First firing (init action-processing line): L20510, guest 1167.317 s, `init: processing action (sys.boot_completed=1) from (/system/etc/init/hw/init.rc:1224)`. Presenting time (guest max 1516.766 − first firing 1167.317): 349.4 s. (Analyzer's first_boot 1167.684 s is 0.367 s late — see Analyzer notes. The bootstat `setprop sys.bootstat.first_boot_completed 0` line is 0 in this flight and is NOT a firing in any case.)
- T733_PROBE_OK: 2 (L2767 guest 84.677 s leg 1; L20953 guest 1173.622 s leg 2). T728_PROBE_OK: 2 (L2764, L20940). T733_MINIMAL_OK: 0. Trigger-file parser verdict: the t728probe.rc/t733probe.rc files parsed — the markers fired at legs 1+2 and the leg-2 `start t728probe` command ran (and failed on the label, below); no parse failure.
- Init service-refusal ("Could not start service"): 1 (L20934, guest 1173.449 s, `start t728probe` SELinux label refusal — the known T728 wall, not a void).
- [T737] early-ramdisk hooks: 9. [T739] unmount-before-handoff hooks: 7. [T740] orphaned-script kill hooks: 3. Post-handoff spam: 0 / 0 (t493/t501 STILL ALIVE 0, can't-create 0, inaccessible 0).
- OnBootPhase_500: 357. OnBootPhase_550: 297. OnBootPhase_600: 153.
- Watchdog KILLING (fixed pattern `W/Watchdog\(\s*\d+\):.*WATCHDOG KILLING`): 1 (L11158, pid 705). ALL W/Watchdog lines: 24 (all L11158–11240, the 705 event). media.c2 one-second waits: 173. Camera service unavailable: 91.
- 'exited due to signal 9' (all): 4 — pids 705, 2620, 2588, 1960. 'exited due to signal 11' (SIGSEGV): 0.
- Zygote failed-to-write to system_server FD: 2 (L11308, L24668). 'Exit zygote because system server (pid N) has terminated': 2 (pid 705 L11310, pid 1960 L24671). 'System server process N has been created': 3 (pid 705 L3941, pid 1960 L12339, pid 3285 L25909).
- FATAL EXCEPTION: 7 (L11304, L24672, L24678, L24681, L24720, L24725, L24779) — collateral of the two system_server deaths (DeadSystemException), not the cause.
- Kernel panic: 0. Oops: 0. BUG:: 0.
- lmkd lines (any): 19. lowmemorykiller lines: 10 — all 10 are startup ("Using psi monitors", "Process polling is supported", "Process reaping is not supported") or lmkd data-connection established/dropped lines; ZERO kill-decision lines. 'Low on memory': 0. onTrimMemory: 0. meminfo: 0. dumpsys meminfo: 0. psi: 1 (L780, the lmkd startup line). 'Killing ' (kill verb, whole UART): 2 — both ActivityManager (pid 1960) killing apps 2588 (bluetooth, bg anr) and 2620 (phone, bg anr).
- am_kill: 2 (victims 2588, 2620 only). ActivityManager 'Killing ... system_server': 0. 'Force finishing': 1 (L24150, WindowManager finishing an ActivityRecord — not a kill). 'Process ... has died': 1 (L17483, WebViewLoader). 'has died' (all): 2 (L17483; L24306 "ActivityManager has died" — 1960's death noticed by ResourceManagerMetrics).
- init 'Sending signal 9 to service ...': 27, NONE naming system_server. Notably L11312 kills 'zygote' (pid 508) at guest 841.127 s (runtime restart after 705's death) and L23653 kills 'zygote' (pid 1830) at guest 1215.263 s — system_server 1960 logged AFTER that kill (WindowManager 1960 at 23:36:05, guest ~1225 s), so the zygote kill did not kill 1960.
- system_server lines (context): 362. storaged: 722 ("storaged not found; trying again"). artd: 92. media.metrics: 12 (waiting for service). avc: denied (two spaces): 2396. sw_sync: 161. maps IMAGE (path=/sysimg/system/lib64/libEGL.so): 1. maps RAMDISK: 0.

## Comparison table vs flight-18 (T741, driver-certified PASS)

| Measure | Flight-18 (T741) | Flight-20 (T742) |
|---|---|---|
| Guest / host seconds | 1512.44 / 1516.98 | 1516.766 / 1519.31 |
| QEMU exit | 0 | 0 |
| First boot_completed firing | 521.327 s | 1167.317 s |
| Presenting time | 991.1 s | 349.4 s |
| sys.boot_completed lines | 8 | 6 |
| RenderEngine dup-fail / rate | 0 / 0.000 | 0 / 0.000 |
| CALL allocate2 (post-boot) | 127 (122) | 7 (2) |
| T721 fence-log | 1 (composer pid 508) | 1 (composer pid 510) |
| system_server deaths | 678 SIGKILL ~597 s; 2176 SIGSEGV ~1465 s | 705 SIGKILL ~841 s (watchdog); 1960 SIGKILL ~1230.1 s (unidentified) |
| Watchdog KILLING | 0 | 1 |
| FATAL EXCEPTION | 6 | 7 |
| T733/T728_PROBE_OK | 3 / 3 | 2 / 2 |
| Service-refusal (t728probe) | 2 | 1 |
| signal-9 deaths (all) | 53 | 4 |
| lmkd / lowmemorykiller | 50 / 9 | 19 / 10 |
| am_kill | 23 | 2 |
| avc denied / sw_sync | 6592 / 1459 | 2396 / 161 |
| storaged / artd / media.metrics | 308 / 0 / 10 | 722 / 92 / 12 |
| OnBootPhase 500/550/600 | 286 / 288 / 306 | 357 / 297 / 153 |
| T723_PROBE / T734_KERNEL_OK / T720 PASS | 27 / 1 / 1 | 27 / 1 / 1 |
| [T737] / [T739] / [T740] hooks | 9 / 7 / 3 | 9 / 7 / 3 |

## The SIGKILL question — recurrence: YES

system_server died by SIGKILL twice in flight-20, against once in flight-18. The death sequence (guest times are the analyzer's interpolated values, bracketed by my independent recount):

- pid 705: created L3941 (guest ~107.6 s). Watchdog kill L11158 (guest ~832 s). Failed-to-write L11308. Exited signal 9 L11309 (guest ~841 s). Exit-zygote L11310. Refork: pid 1960 created L12339 (guest ~867.6 s).
- pid 1960: created L12339. Killed apps 2620 (guest ~1212.5 s, bg anr) and 2588 (guest ~1214.1 s, bg anr) — system_server was alive and working 18 s before its own death. Failed-to-write L24668. Exited signal 9 L24670 (guest ~1230.1 s). Exit-zygote L24671. Refork: pid 3285 created L25909 (guest ~1240.0 s), alive at flight end (guest 1516.766 s).
- Boot completed at guest 1167.317 s under pid 1960; pid 1960 died 62.8 s later. The third system_server (3285) did NOT re-process the boot_completed action (markers stay 2/2, not 3/3 as in flight-18) — noted, not a verdict item.

## The 705 death — killer NAMED: the Watchdog

L11158 (wall 11-10 23:29:48.337, guest ~832 s): `W/Watchdog(  705): *** WATCHDOG KILLING SYSTEM PROCESS: Blocked in handler on main thread (main) for 111s`. The main thread had been blocked since guest ~721 s. Preceding distress in the same seconds: hwservicemanager failing to start media.c2 IComponentStore/software as a lazy HAL, "Camera service is unavailable" (CameraManagerGlobal 705), "storaged not found; trying again". This is the T740 flight-15 pattern recurring (pid 788, main thread blocked 94 s, media.c2/camera waits): the guest's media/camera HALs never come up under QEMU, the main thread blocks waiting for them, the watchdog kills the process. The watchdog here is system_server's own watchdog thread killing its own process — the killer is named by the UART itself, and init's follow-up kill of zygote (pid 508, L11312, guest 841.127 s) is the standard runtime restart, not the killer.

## The 1960 death — killer UNIDENTIFIED (candidate exhaustion)

pid 1960 died by SIGKILL at guest ~1230.1 s (L24670; failed-to-write L24668; Exit-zygote L24671). No line in the UART names its killer:

- Watchdog: only 1 WATCHDOG KILLING line in the whole UART (L11158, pid 705). All 24 W/Watchdog lines cluster at L11158–11240 (the 705 event). No watchdog line for 1960.
- LMK: zero kill-decision lines. All 10 lowmemorykiller lines are lmkd startup or data-connection established/dropped. The conn-drop lines (L24298/24299, 23:36:10.227–228) precede the Zygote death report by ~0.5 s — the same pattern as flight-18 (L18388/18389 before the 678 death) — and are a death SYMPTOM, not a cause: system_server holds the lmkd connection, so when it dies the connection drops; the drop is logged before Zygote notices the death. 'Low on memory' 0, onTrimMemory 0, meminfo 0, dumpsys meminfo 0. No RAM-pressure evidence at the death.
- init: 27 'Sending signal 9' lines, 0 naming system_server. The zygote kill (L23653, guest 1215.263 s) demonstrably did not kill 1960: system_server 1960 logged afterwards (WindowManager 1960, L24150 'Force finishing', wall 23:36:05, guest ~1225 s).
- ActivityManager: 0 'Killing ... system_server' lines; the 2 am_kill lines name only apps 2588/2620 as victims, killed BY 1960.
- Flood loss: 2.6 timestamped lines/s in the +/-60 s window around guest 1230.109 s (316 lines). A kill line naming the pid would still be visible. No flood loss.
- The kill-verb grep ("Killing ", whole UART): 2 lines, both AM killing apps. No line in the UART kills system_server 1960.

## Dup regression — holds, on thin presenting traffic

Rate 0.000/1000 s on both the whole span (1516.8 s) and the presenting-time basis (349.4 s), below both baselines 4.058/6.660. The vendor-rc fix has NOT regressed. Caveat, stated plainly: the post-boot presenting sample is thin — 2 allocate2 calls (L22062/L22244, guest 1189.392/1191.720 s) and 1 T721 fence log (L23336) — because system_server died twice and the third incarnation (3285) logged no allocate2 calls in its ~276 s of life. The 0-rate is real (0 dup-fails in the whole flight, 0 in 349.4 s presenting) but rests on light presenting traffic, unlike flight-18's 122 post-boot allocate2 calls. A future flight with a longer-lived post-boot system_server would strengthen the regression evidence; it is not a wall.

## Holds (brief Verdict contract)

- T720_PROBE DONE status=PASS >=1: 1 — MET.
- T721 fence log >=1: 1 — MET.
- Invalid present fence 0: 0 — MET.
- REJECT 0: 0 — MET.
- EPERM 0: 0 — MET.
- simplefb: mapped >=1: 1 — MET.
- Watchdog KILLING 0: 1 — NOT MET (the 705 watchdog kill; this is the named half of the SIGKILL answer, recorded here as a hold failure per the contract's letter).
- sys.boot_completed >=1: 6 — MET.
- panic/Oops/BUG 0: 0 — MET.
- [T739] unmount hooks fire: 7 — MET. Post-handoff spam ~0: 0/0 — MET.

## Flight-19 — VOID

service-flight-19 was cut 10:25:44 AEDT on the disk gate (7.57 GiB < 8 GiB supervisor floor) at host 330.95 s / guest 261.054 s. Partial UART preserved: 6,475 lines, 870,178 B, SHA f6416a8d. Inputs block identical to flight-20 (all 6 SHAs match Step 1). Stack healthy before the cut (T723_PROBE start 2, T734_KERNEL_OK 1 per analysis-t742-partial-flight19.txt). No verdict from a 261 s flight — VOID as evidence. The lane re-flew as service-flight-20 at 10:28:01 on the identical stack (in-lane recovery decision, gate reopened at 13,252,576 KiB free).

## Analyzer notes

- Analyzer: scripts/analyze_t742_probe.py, SHA 1ce9f532 (v3: +SIGKILL probe + named start-probe failures + signal-11 distinction). Supervisor: scripts/supervisor_t742.py, SHA 4b40bb9a (diff vs T741 = track/flight/env-prefix/progress-file/docstring only).
- Regression test (Step 2, required): I re-ran the analyzer on flight-18's UART myself; the output is byte-identical to the first lane's analysis-t742-regression-flight18.txt (diff 0 lines, 605 lines each). The "T741 counts reproduce EXACTLY" claim is VERIFIED, not trusted. My re-run is preserved as analysis-t742-myreg-flight18.txt (SHA 588a0f6b).
- analysis-t742.txt is the analyzer run on flight-20's UART (SHA 81881151, 451 lines; Total lines 29547 / bytes 3832505 match flight-end.json). The analyzer's docstring still says "service-flight-19" — stale text from the first lane's authoring; the analyzed file is flight-20, proven by the totals.
- First-boot correction: the analyzer reports first boot_completed firing as guest 1167.684 s, but the true first firing is L20510 at guest 1167.317 s. L20510's line is concatenated with a maps-dump line, so the analyzer's regex (anchored at line start) misses it and takes the next matching line (L20564, guest 1167.684 s). Presenting time is 349.4 s, not 349.1 s. The 0.367 s difference does not change any rate (still 0.000). Future analyzers should search for the timestamp anywhere in the line, not anchor at line start.
- My independent recount (driver-style: bytes, split on \n, literal patterns) reproduces every analyzer count in the Counts section, including the T723 start line numbers (L356/357), the boot firings, the death lines, and the candidate-exhaustion greps.

## Reflection

- The recovery pattern worked: the first lane ended pre-verdict at 10:49:17 AEDT, but the flight evidence survived intact on the Mac (flight-20 folder complete, all 8 files), and the recovery lane certified the track from that evidence without re-flying. The interrupted `progress-t742.txt` append had already landed (driver-verified); the recovery did not duplicate it.
- The 10:39 mid-flight supervisor-lag note was correct: the last-1MB window lagged at guest 239.7 s while the whole-UART max was 710.25 s — kernel timestamps are sparse mid-boot, NOT a guest stall. No wall there.
- The lmkd data-connection drop is a death SYMPTOM, not a cause: it precedes the Zygote death report by ~0.5 s in both deaths (705 and 1960) and in flight-18 (678), because system_server holds the connection and Zygote notices the death afterwards. A future lane should not read the conn-drop as LMK involvement; the test is whether a kill-decision line names the pid (none does, in either flight).
- The kill-verb grep is the cheapest decisive check in this track's question: `grep "Killing "` over the whole UART returns 2 lines in flight-20, both ActivityManager killing apps. Any future SIGKILL probe should run that grep first, before the section-by-section candidate search.
- The watchdog kill of 705 recurs the T740 flight-15 pattern (media.c2/camera HAL stall -> main-thread block -> watchdog kill). The guest's media/camera HALs never coming up under QEMU is the standing weakness behind HALF the SIGKILL question; the other half (1960) has no mechanism at all yet.

## Successor direction (T743)

- Wall: SYSTEM_SERVER-SIGKILL-KILLER-UNIDENTIFIED (guest, NEW). The next track must identify the 1960 killer. The UART cannot answer it — every candidate is exhausted by absence — so T743 needs evidence the UART does not carry: (1) mine the 631 tombstone lines for a watchdog dump covering the 1960 death (a second watchdog event would leave a DropBox system_server_watchdog tag even if the KILLING line is absent); (2) capture a dedicated kmsg/dmesg stream around the death window (the UART mixes logcat and kmsg; a kill line could in principle be logcat-dropped, though the 2.6 lines/s flood rate makes loss unlikely); (3) add a periodic meminfo dump to the flight to test the RAM-pressure hypothesis directly (this flight has meminfo 0, so RAM pressure is unmeasured, not disproven); (4) test whether the watchdog killed 1960 too by checking the watchdog's dump state for pid 1960 in the tombstones.
- Parallel thread: the watchdog kill of 705 (and T740's 788) shares one root cause — the media.c2/camera lazy-HAL stall blocking the main thread. A track that removes that stall (or extends the watchdog timeout) would test whether the watchdog kills stop; if they do, the remaining question narrows to the 1960-style unidentified kill alone.
- The dup regression evidence is thin (2 post-boot allocate2 calls); a future flight with a longer-lived post-boot system_server should re-measure the rate on heavier presenting traffic. Not a wall, a caveat.
- Do NOT re-diagnose dispatch (T736 PASS) or the dup path (T741 PASS, holds again here). Do NOT rebuild the kernel or change the vendor image, CPIO, or DTB — the stack is identical and the flight is valid.
