# Track T740 — Longer flight window + kill orphaned background scripts at the handoff

## Verdict: FAIL

The full 1200 s flight ran and the guest completed boot — the first boot completion
since T734 — and the timeline-access fix route was TESTED and its mechanism PROVEN
in the probe context (flight-17: all three sub-probes mint real signaled sw_sync
fences). But the verdict is FAIL because the one clause the lane exists to answer
was left unmeasured: dup -1 in the surfaceflinger context. The boot completed at
guest 1175.836 s, leaving only 26.2 s of post-boot presenting time before the
1200 s window ended; the T734 calibration says the first RenderEngine dup-fail
appears ~40 s post-boot. The signal is unmeasured, not refuted — the route that
would measure it (a longer post-boot window) is named in § Successor.

**WALL-TYPE-TAG: engine (harness) — POST-BOOT-WINDOW-TOO-SHORT. Rust par bhi lagu: nahi.**
This is a flight-planning parameter, not a guest defect: the window contained the
boot but not the post-boot measurement the verdict contract requires. The T729
guest wall BOOT-COMPLETED-NEVER-FIRED is CLOSED by this flight (boot completed at
1175.836 s inside the 1200 s window).

**First failing link:** the surfaceflinger-context dup signal was unmeasured —
RenderEngine dup-fail 0 in 26.2 s of post-boot presenting time (flight-15),
against the T734 calibration of ~40 s to the first dup-fail. No other clause
failed. The fix route is not blocked: the mechanism test PASSED.

**The analyzer parser verdict is FAIL** (conjunctive contract, adjudicated line by
line in § Verdict contract adjudication). The parser here is
`scripts/analyze_t740_probe.py` (SHA `2e90935e`), run on all three flight UARTs;
every count below was re-verified by a byte recount on the sandbox UART copies
(read bytes, split on `\n` only), and the recounts match the parser exactly.

## Verdict contract adjudication (brief line 67, conjunctive)

1. **Full 1200 s flight runs: YES.** Flight-15: guest 1202.06 s (target 1200
   REACHED), host 1202.60 s, QEMU exit 0, stop "Guest target reached
   (1202.1s >= 1200s)".
2. **Boot completes (`sys.boot_completed` fires): YES.** `sys.boot_completed`
   12 lines, first firing L23295 at guest 1175.836 s
   (`init: processing action (sys.boot_completed=1) from
   (/system/etc/init/hw/init.rc:1224)`). `boot_completed` (all) 12.
3. **dup -1 confirmed in the surfaceflinger context: NO — the failing clause.**
   SP1/SP2 were answered from the early-ramdisk probe context only. The
   RenderEngine dup-fail signal in the booted guest is 0 in 26.2 s post-boot
   (1175.836 → 1202.06 s); T734 flight-9 calibration: boot_completed 790.041 s,
   first dup-fails x4 at guest ~830–850 s (RenderEngine pid 2830), i.e. ~40 s
   post-boot; CALL allocate2 at 811–829 s; simplefb + T721 at ~801 s, 11 s
   post-boot. A 0 in 26 s is VOID as a differential signal, not evidence that
   dup works. The "route un-runnable" alternative does not apply: the route IS
   runnable (a longer post-boot window measures it), it is just unmeasured in
   this flight. Clause 3 fails → verdict FAIL regardless of clause 4.
4. **Timeline-access fix route TESTED: YES — and the mechanism PASSED.**
   Flight-16 (file chmod only): SP3 still fd=-13 — the file-permission route
   as specified FAILED. Flight-17 (file + parent-dir chmod): SP3 opens the
   timeline fd=4, SP1 dup rc=5, SP2 dup rc=7 — all three sub-probes mint real
   signaled sw_sync fences (fence_check: poll POLLIN, FILE_INFO num_fences=1,
   FILE_INFO2 status=1 signaled, driver_name=sw_sync). The barrier was DAC
   traversal on `/sys/kernel/debug` (mode `drwx------`, 0700 root), NOT the
   file mode, NOT SELinux, NOT dispatch. The booted-guest fix must live in the
   booted guest (vendor init rc), per the brief — a successor lane's build.
5. **[T740] unmount hooks fire: YES.** All three unmounts rc=0 ([T739] L589 /
   L618 / L647 in flight-15), handoff state verified (L649/650), /data mounted,
   no HANDOFF ABORTED.
6. **Orphaned scripts killed at the handoff: YES.** [T740] kill both PIDs
   (201/202 in flight-15); post-handoff spam 0/0 ("can't create /dev/null" 0,
   "inaccessible or not found" 0 — T739 flight-14 had 4480 / 337).

**A/B control:** no fix was flown in the booted guest (the mechanism flights
were probe-context only), so per the brief the T739 flight supplies the baseline
and no new control is owed. The dup-fail rate in flight-15 is 0.000/1000 s
(0 in 1202 s) against the T721 baselines 4.058 / 6.660 — VOID as a differential
signal for the reason in clause 3 (only 26 s post-boot), not evidence of a fix.

## Flights

### service-flight-15 (the 1200 s flight) — 06:22:26–06:42:31 AEDT 21 Nov 2026

- Supervisor PID 87228, QEMU PID 87291, GDB 1269. Started 2026-11-20T19:22:28Z,
  ended 2026-11-20T19:42:31Z. Host 1202.60 s, guest 1202.06 s, QEMU exit 0.
- UART 26,242 lines, 3,505,378 B, SHA `0befc1fe`. qemu-stderr.log 0 B at launch
  (T739's 81 B was the supervisor SIGTERM; check at certification).
- Disk gate OPEN at launch: 22,111,368 KiB free, 90%. Disk drained slowly
  during the flight (20,420,696 KiB free at ~06:33) but never near the gate.

**Flown stack:** kernel `70d3859d` (20,396,544 B); DTB `6b6a118d` (2,049 B;
initrd-end 0x1133435c9 = 0x110000000 + 53,753,289); CPIO `724dff6c`
(53,753,289 B, 241 entries); system `af40403a`; vendor `5e0607ad`; supervisor
`f07de12c`; analyzer `2e90935e`; rebuilder `2db3ef8e`; early-ramdisk `6804341a`
(20,098 B); patcher `1f896a01`; rc `79ad3cc3`; QEMU `8e5c46dc`.

**Counts (parser + byte recount, all match):** T723_PROBE 27 (start 2 L357/358,
DONE 1 L388 — DONE line reads `T723_PROBE DONE` with NO colon; SP1 DONE 1
L379, SP2 DONE 1 L387, SP3 DONE 1 L373); DROP_FAIL 0, RESOLVE_FAIL 0,
GETPROCADDRESS_NULL 0; SP1 dup rc=-1 1 (L377), SP2 dup rc=-1 1 (L385), SP3 open
timeline fd=-13 1 (L372); SP1/SP2 create sync=0x5001 (L376/L382); SP2
resolved-address 0x7fb1ac97a8 (L383, repeated L384); SP2 maps
path=/sysimg/system/lib64/libEGL.so (L384); IMAGE libEGL maps 1 (L384,
anchored `path=` pattern), RAMDISK libEGL maps 0; [T737] hooks 9 (L346, 348,
349, 351, 353, 354, 390, 392, 393); [T739] hooks 7 (L525, 554, 583, 585, 586,
589, 590); [T740] hooks 3 (L494 kill entry, L495 t493 killed pid=201, L496
t501 killed pid=202; STILL ALIVE 0); T734_KERNEL_OK 1 (L194, guest 3.892 s) +
[T734] probe finished rc=0 (L195); T720_PROBE DONE status=PASS 1 (L227);
T733_PROBE_OK 2 (L2766 guest 82.710 s leg 1; L23851 guest 1184.309 s leg 2);
T728_PROBE_OK 2 (L2762 guest 82.678 s leg 1; L23849 guest 1184.193 s leg 2);
T733_MINIMAL_OK 0 (expected, reproduces T733); sys.boot_completed 12 (L23295,
23314, 23502, 23509, 23512, 23513, 23517, 23544, 24414, 24542, 24619, 24729),
first firing L23295 guest 1175.836 s; boot_completed (all) 12; T721 fence-log
1 (L12439: `T721 fd=10 tl=9`, composer pid 508, guest ~890 s); simplefb:
mapped 1 (L12436, guest ~890 s); CALL method=allocate2 10 (guest 90.920/
90.930, 875.943/875.949/875.954, 1165.153/1165.159/1165.161, 1198.436,
1199.931); REJECT allocate2 0; RenderEngine dup-fail 0; dup-fail rate
0.000/1000 s; FATAL EXCEPTION 2 (L14748 Launcher3, L14841
permissioncontroller — both DeadSystemException, consequences of the
system_server restart, see below); Kernel panic/Oops/BUG 0; EPERM 0; Watchdog
KILLING 0 per the parser — VARIANCE: the parser's `Watchdog:.*KILLING`
pattern MISSES the actual line format `W/Watchdog( 788): *** WATCHDOG
KILLING SYSTEM PROCESS:` (L14509) — the watchdog kill DID happen; the parser
pattern needs the `W/Watchdog(...):` form (see § Reflection); avc: denied
(two spaces) 2440 (vs 1257 in T739); avc denied for /dev/mem 6; sensorservice
3; "inaccessible or not found" 0 (total); "can't create /dev/null" 0 (total);
system_server 269 lines across two pids (788, then 2323); SurfaceFlinger 21
lines (pid 583 throughout); OnBootPhase_500 286, 550 306, 600 306; "storaged
not found" 492 (identical count to T739 flight-14 — same retry loop, NOT the
differentiator); "artd could not be found" 0 (vs 6 in T739 — the T739 variance
stands: artd started and ran dex2oat); "Waiting for service 'media.metrics'"
8 (vs 4 in T739); dex2oat 283 lines; PackageManager 958 lines; "Watchdog"
(bare) 86 lines (mostly thread-name mentions); BOOT_COMPLETED 2.

**The system_server death + restart (mid-flight, ~guest 935 s):** first
system_server (pid 788, lines 3956–14125) was killed by the Android Watchdog:
L14509 `W/Watchdog( 788): *** WATCHDOG KILLING SYSTEM PROCESS: Blocked in
handler on on main thread (main) for 94s` (logcat 23:29:25.565; guest ~935 s
estimated from logcat/kernel anchoring — no kernel timestamp on the line
itself). Preceding distress: repeated `Waited one second for
android.hardware.media.c2@1.0::IComponentStore/software` + `Camera service is
unavailable` (L14117–14123); gralloc HAL pid 614 (binder:614_2,
memfd:t628-gralloc) natively crashed/dumped (tombstoned dump at 23:29:15,
L14112–14115; earlier dump request L10244 at 23:27:46). Launcher3 (pid 2115)
+ permissioncontroller (pid 1592) FATAL EXCEPTIONs with DeadSystemException at
L14748/L14841 are consequences, not the cause. Second system_server (pid 2323,
lines 15936–26135) started guest 1018.329 s (L15939 kernel ts), "Entered the
Android system server!" (L15947), and completed boot at 1175.836 s.
SurfaceFlinger (pid 583) survived throughout ("Boot is finished (785595 ms)"
L12232, guest ~887 s, under the FIRST system_server; "Boot is finished
(1085750 ms)" L25460 under the second).

**The surfaceflinger-context dup question (the crux):** the composer (pid 508,
hal_graphics_composer domain, scontext
`u:r:hal_graphics_composer_default:s0`) OPENED the timeline (T721 tl=9) and
CREATED a fence (fd=10) at guest ~890 s, and issued SW_SYNC ioctls CREATE_FENCE
(0x5700) x135 + INC (0x5701) x133 — 268 avc denials logged, but permissive mode
allows them through (denials are policy noise in permissive mode). The composer
was also avc-denied { open } on `/sys/kernel/debug/sync/sw_sync` (3 lines,
e.g. L12142–12143, HwBinder:508_3) — yet the open succeeded (tl=9), because
permissive allows after logging. The composer runs as root (it bypasses the
0700 debugfs dir via CAP_DAC_OVERRIDE — see flight-17's finding below), so
its success does not contradict the probe's failure: the probe's barrier was
never SELinux policy. The probe's open, by contrast, HARD-fails EACCES
(fd=-13) in the same permissive guest — and flight-17 proves the barrier is
DAC traversal on the debugfs parent dir.

**Boot-time distribution (now):** 495 s (T730) / 790 s (T734) / >851 s (T739) /
1175.8 s (T740, after a system_server watchdog restart). Margin in this flight:
24 s — still thin. The T739 reflection lesson (window from the distribution's
tail) was applied to the boot time but not to the post-boot measurement time
(see § Reflection).

### service-flight-16 (mechanism test 1: file chmod only) — 07:00:13–07:02:25 AEDT

- Supervisor PID 14616, QEMU PID 14620, GDB 1270. Started 2026-11-20T20:00:15Z,
  ended 20:02:25Z. Host 130.26 s, guest 126.741 s, QEMU exit 0, stop "Guest
  target reached (126.7s >= 120s)".
- UART 3,968 lines, 577,113 B, SHA `56354691`. qemu-stderr.log 81 B (supervisor
  SIGTERM at target expiry, expected).
- CPIO `b03ff37b` (53,753,865 B, 241 entries); DTB `5d2f4f14` (2,049 B;
  initrd-end 0x113343809 = 0x110000000 + 53,753,865); early-ramdisk-t740-mech
  `83dcb08e` (21,119 B); rebuilder `a3a98980`. Kernel/system/vendor/QEMU
  unchanged from flight-15.
- The [T740M] block: sw_sync file present (L351); mode before `-rw-r--r--`
  (0644, root:root, L380); `chmod 0666` rc=0 (L409); mode after `-rw-rw-rw-`
  (L438).
- **Counts (parser + byte recount, all match):** T723_PROBE 27 (start 2 L443/
  444, DONE 1 L474, SP1 DONE 1 L465, SP2 DONE 1 L473, SP3 DONE 1 L459);
  DROP_FAIL 0, RESOLVE_FAIL 0, GETPROCADDRESS_NULL 0; **SP3 open timeline
  fd=-13 1 (L458) — STILL FAILS after chmod 0666**; SP1 dup rc=-1 1 (L463),
  SP2 dup rc=-1 1 (L471); SP1/SP2 create sync=0x5001 (L462/L468); SP2
  resolved-address 0x7faa16b7a8 (L469); SP2 maps path=/sysimg/system/lib64/
  libEGL.so (L470); IMAGE maps 1, RAMDISK 0; [T737] hooks 9; [T739] hooks 7
  (unmounts rc=0 L610/639/668, handoff verified L670/671, HANDOFF ABORTED 0);
  [T740] kill 3 (L579–581, pids 204/205); post-handoff spam 0/0;
  T734_KERNEL_OK 1 (L194, guest 3.695 s); T720_PROBE DONE status=PASS 1
  (L226); T733_PROBE_OK 1 (L2848, guest 81.110 s); T728_PROBE_OK 1 (L2843,
  guest 81.079 s); CALL allocate2 2 (L3310, 3314); avc: denied 1093; FATAL
  EXCEPTION 0; Kernel panic/Oops/BUG 0; EPERM 0; sys.boot_completed 0
  (short flight, expected); T721 0; simplefb: mapped 0.
- **Adjudication: the file-permission route as specified FAILED.** The file was
  ALREADY world-readable (0644) and SP3 opens O_RDONLY (probe source line
  548, verified on the box: `sys3(NR_openat, AT_FDCWD,
  "/sys/kernel/debug/sync/sw_sync", O_RDONLY)`), so DAC on the file could
  never have been the barrier. No avc denial for the probe's open exists
  anywhere in flight-15's first 600 lines (the probe window) — SELinux logged
  nothing against the probe. The barrier had to be elsewhere on the open path.
  That "elsewhere" is flight-17's finding.

### service-flight-17 (mechanism test 2: file + parent-dir chmod) — 07:17:40–07:19:45 AEDT

- Supervisor PID 21028, QEMU PID 21041, GDB 1271. Started 2026-11-20T20:17:40Z,
  ended 20:19:45Z. Host 125.27 s, guest 123.601 s, QEMU exit 0, stop "Guest
  target reached (123.6s >= 120s)".
- UART 3,817 lines, 548,912 B, SHA `10f6365a`. qemu-stderr.log 81 B (supervisor
  SIGTERM at target expiry, expected).
- CPIO `1f8c00f2` (53,753,970 B, 241 entries); DTB `534f4c39` (2,049 B;
  initrd-end 0x113343872 = 0x110000000 + 53,753,970); early-ramdisk-t740-mech2
  `3c9aa19d` (21,761 B); rebuilder `93afe6f0`. Kernel/system/vendor/QEMU
  unchanged from flight-15.
- **Process note:** the first launch attempt failed on a supervisor checksum
  refusal — the lane typed a fabricated DTB SHA tail into the launch env var;
  the supervisor's SHA gate caught it and refused to launch (no QEMU
  started). Corrected from the supervisor's own error message (real SHA
  `534f4c393197d147568e55b8a74421a3ad175ed09a864bb1c7c5450a25a9c47c`) and
  re-launched. The checksum gate worked exactly as designed; no evidence was
  affected. Never type a SHA from memory — always copy it from the tool output
  that produced it.
- The [T740M2] block: parent dir modes BEFORE — `/sys` drwxr-xr-x, `/sys/kernel`
  drwxr-xr-x, **`/sys/kernel/debug` drwx------ (0700, root:root)**,
  `/sys/kernel/debug/sync` drwxr-xr-x (L471, the `drwx------` line); `chmod
  0755 /sys/kernel/debug/sync /sys/kernel/debug` rc=0 (L501); modes after
  (L502).
- **Counts (parser + byte recount, all match):** T723_PROBE **42** (start 2
  L537/538, DONE 1 L583, SP1 DONE 1 L570, SP2 DONE 1 L582, SP3 DONE 1 L560 —
  42 not 27 because SP3 now runs the full CREATE/INC/poll/FILE_INFO path and
  SP1/SP2 run their fence_check lines); DROP_FAIL 0, RESOLVE_FAIL 0,
  GETPROCADDRESS_NULL 0; **SP3 open timeline fd=4 1 (L552); SP3 CREATE rc=0
  fence=5 (L553); SP3 INC rc=0 (L554); SP3 poll rc=1 revents=1 POLLIN signaled
  (L555); SP3 FILE_INFO rc=0 num_fences=1 (L556); SP3 FILE_INFO2 rc=0 status=1
  signaled (L557); driver_name=sw_sync (L558); obj_name=t723-probe (L559);
  SP3 DONE control_fd=5 (L560)**; **SP1 dup rc=5 1 (L564)**; SP1 poll signaled
  (L565), FILE_INFO num_fences=1 (L566), FILE_INFO2 status=1 (L567),
  driver_name sw_sync (L568), SP1 DONE direct_fd=5 (L570); **SP2 dup rc=7 1
  (L576)** — the `error=0x3000` on the line is the stale EGL error from the
  create path (the create also shows error=0x3000 while returning sync=0x5001,
  as in T736–T739); the dup fd is real: poll signaled (L577), FILE_INFO
  num_fences=1 (L578), FILE_INFO2 status=1 (L579), driver_name sw_sync
  (L580), SP2 DONE meta_fd=7 (L582); T723_PROBE DONE drop_ok=1 control_fd=5
  direct_fd=5 meta_fd=7 (L583); SP1/SP2 create sync=0x5001 (L563/L573); SP2
  resolved-address 0x7f9d8357a8 (L574); SP2 maps path=/sysimg/system/lib64/
  libEGL.so (L575); IMAGE maps 1, RAMDISK 0; [T737] hooks 9; [T739] hooks 7
  (unmounts rc=0 L720/749/778, handoff verified L780/781, HANDOFF ABORTED 0);
  [T740] kill 3 (L688–691, pids 207/208); post-handoff spam 0/0;
  T734_KERNEL_OK 1 (L194, guest 4.095 s); T720_PROBE DONE status=PASS 1
  (L228); T733_PROBE_OK 1 (L2923, guest 79.053 s); T728_PROBE_OK 1 (L2921,
  guest 79.015 s); CALL allocate2 2 (L3274, 3278); avc: denied 1053; FATAL
  EXCEPTION 0; Kernel panic/Oops/BUG 0; EPERM 0; sys.boot_completed 0 (short
  flight, expected); T721 0; simplefb: mapped 0.
- **Adjudication: mechanism PASSES.** The timeline-access fix route is
  VALIDATED in the probe context. The barrier was DAC traversal on
  `/sys/kernel/debug` (0700 root: the debugfs mount root), not the sw_sync
  file's mode (0644 → 0666 changed nothing), not SELinux (no denial logged
  against the probe, and the guest is permissive), not dispatch (SP1/SP2
  answered from the IMAGE libEGL at a new address, 0x7f9d8357a8, and both
  minted real fences the moment the dir was traversable). The T726 extension
  of PATTERN-PROOF applies: a host-side mode check on the file proved the
  file existed and was readable — it did not prove the consuming uid-1000
  context could REACH it; the proof had to come from that context, and
  flight-17 is that proof.
- **Variance:** the parser's "boot_completed lines: 1" (L2948) is the bootstat
  `setprop sys.bootstat.first_boot_completed 0` line, NOT a boot-completed
  firing — same class as the T739 "8th line" variance (a property-name match,
  not the `sys.boot_completed=1` action). sys.boot_completed is correctly 0.
- **Variance:** flight-17's [T740] kill entry line L688 is concatenated with
  the preceding `[T431] static loop-node keeper started` (missing newline
  between the two writes). Cosmetic only; the kill lines themselves (L690,
  L691) are clean.

## What T739 proved (carried, driver-certified FAIL 2026-11-21 ~05:15 AEDT)

Full 850 s flight ran (guest 851.145 s); scope fix worked (all three unmounts
rc=0, handoff verified, init mounted /system from vda, T737 + T738 walls
CLOSED); boot never completed in the window; T729 wall BOOT-COMPLETED-NEVER-
FIRED (RECURRENT) was the wall. This lane closes that wall (see above) and
carries the T737/T738 closures forward (both still closed in all three T740
flights).

## Boot-slowness comparison

| count | T734 flight-9 | T739 flight-14 | T740 flight-15 |
|---|---|---|---|
| "storaged not found" retries | 316 | 492 | 492 |
| "artd could not be found" lines | 1 | 6 | 0 |
| "Waiting for service 'media.metrics'" | 4 | 4 | 8 |
| dex2oat lines | 288 | 296 | 283 |
| PackageManager lines | 568 | 549 | 958 |

The storaged retry count is IDENTICAL in T739 and T740 (492) and the dex2oat /
PackageManager volumes are the same work — the guest took ~325 s longer than
T734 to get through it (boot 1175.8 s vs 790.0 s), and the difference is the
system_server watchdog restart at ~935 s (main thread blocked 94 s on
media.c2 / camera waits, gralloc pid 614 native crash), not a missing service.
The T739 variance stands: artd DID start and ran dex2oat; the servicemanager
"artd could not be found" lines are pre-registration retry noise. No missing
service was proven to be the blocker; no service fix is owed by this lane.

## The orphaned-script kill (Step 2, proven in all three flights)

The [T740] block in early-ramdisk-t740 (`6804341a`) captures the PIDs of
/t493-probe.sh and /t501-keystore2-dump.sh at launch and, immediately before
the T739 unmount block, SIGKILLs both, `wait`s to reap them (so kill -0 is a
true liveness test — a zombie would answer kill -0), then verifies with
kill -0. /loop-node-keeper is NOT captured or killed (static, holds device
nodes). Diff vs early-ramdisk-t739 shows ONLY the intended changes; NO
`export LD_LIBRARY_PATH` anywhere (grep-verified in the script AND in the
flown CPIO member). Post-handoff spam is 0/0 in all three flights (T739
flight-14: 4480 / 337). The kill worked perfectly.

## Handoff device state (explicit, diffed against T734's)

T739 unmount block carried forward verbatim; at the handoff only /data +
/data/media are mounted — IDENTICAL to T734's proven-booting state. No new
mounts added. The kill block adds no mounts and touches no dispatch path (it
runs after the probe exec). The unmount tool is toybox (560,720 B); its 8
direct NEEDED libraries are ALL present in the ramdisk's /system/lib64/
(verified T739; T740 rebuilder byte-verified every CPIO member identical
except early-ramdisk). With NO export, toybox maps nothing from /sysimg —
proven by all three flights' rc=0 unmounts.

## INTERP chain (readelf on the box, from the T740 CPIO unpack + system image)

toybox INTERP `/system/bin/linker64`, NEEDED libcrypto/libz/liblog/
libprocessgroup/libselinux/libc/libm/libdl; sh INTERP `/system/bin/linker64`,
NEEDED libc.so only; t723-probe INTERP `/system/bin/linker64`, NEEDED libEGL.so
only; t734probe STATIC; /init is a symlink (16 B) to /system/bin/init
(2,770,616 B, SHA `646f5efa`), INTERP `/system/bin/bootstrap/linker64`
(resolves from the mounted image after root switch). Each INTERP resolves AFTER
the mount, from the mounted state: proven in-flight by the [T737] INTERP
tripwire + probe exec rc=0 + T734_KERNEL_OK in every flight (T736–T740).

## § Reflection (REFLECT-ON-EVERY-PROBLEM)

- **What happened:** the 1200 s window contained the boot (1175.836 s) but left
  only 26 s of post-boot presenting time, below the ~40 s the T734 calibration
  needs for the first dup-fail — so the lane's central question went
  unmeasured. The mechanism test then proved the fix route in the probe
  context, so the lane still produced the answer the successor needs.
- **Root cause:** the window was sized from the boot-time distribution's tail
  (T739's lesson) but the sizing counted only the BOOT. The verdict contract
  also needs POST-BOOT measurement time, and that time was never added to the
  window. A 1200 s window against a boot that can take 1175 s is a 24 s margin
  on a 680 s spread — the same coin flip T739 lost, re-lost on a different
  edge. Second root cause: the mechanism test's first design checked only the
  FILE's mode. The T726 PATTERN-PROOF extension already said a host-side
  membership check proves existence, not reachability from the consuming
  context — and the file check repeated that error one level down (file mode
  vs traversal path).
- **Missed signal:** T734 flight-9's own numbers were in the brief (first
  dup-fail ~40 s post-boot) and nobody added 40 s + margin to the window when
  the 1200 s target was set. The boot-tail math (957 s need → 1200 s window)
  looked comfortable and hid the missing post-boot term.
- **What changes:** (1) successor windows = boot-tail + ≥60 s post-boot margin
  (guest target ~1500 s for this distribution); the window-sizing rule is now
  "boot-tail + measurement-window", never boot-tail alone. (2) Mechanism tests
  must check the FULL DAC path (every parent dir + the file) from the
  consuming context before concluding — record `ls -ld` of the whole path in
  the test. (3) The analyzer's Watchdog pattern must match the
  `W/Watchdog(...): *** WATCHDOG KILLING SYSTEM PROCESS:` form (it currently
  misses it and reports 0 while a kill happened). (4) Never type a SHA from
  memory into a launch env var — copy it from the tool output that produced
  it; the supervisor's checksum gate is the backstop, not the method.
- **Where it lives:** this section; the successor brief (T741) must carry the
  "boot-tail + measurement-window" rule and the full-DAC-path check; the
  analyzer fix belongs in `scripts/analyze_t740_probe.py`'s successor.

## Successor direction (T741)

1. **Build the booted-guest fix:** vendor init rc that makes
   `/sys/kernel/debug` traversable by uid 1000 (e.g. `chmod 0755
   /sys/kernel/debug` at boot) — the mechanism flight-17 just proved. Vendor
   repack under the T718/T721 patch discipline (verify the T721 patched
   composer `f7600821…` and T718 shim `3daa5a49…` bit-for-bit afterwards;
   EMBEDDED-VENDOR RULE). Do NOT touch the T722 patched libEGL
   (`34b79da2…`) or the SYS_RAWIO fix.
2. **Fly a longer window:** guest target ~1500 s (boot-tail ~1180 s + ≥60 s
   post-boot margin), host cap ~1800 s. The dup-fail signal needs ≥40 s of
   post-boot presenting time; the T722 differential criteria apply (dup-fail
   rate BELOW BOTH baselines 4.058/6.660 on a normalized presenting-time basis
   + all T721/T722 holds intact) with the T739 flight as the A/B baseline.
3. **Investigate the system_server watchdog kill** (main-thread block 94 s on
   media.c2/camera; gralloc pid 614 native crash) as a boot-slowness
   contributor — evidence only, no fix in the successor unless the flight
   proves it is the blocker.
4. **Do NOT** reintroduce the LD_LIBRARY_PATH export; do NOT rebuild the
   kernel; do NOT re-run the T734 bisect (void); do NOT mount the system image
   onto /system (refuted, T735).

## Evidence

- findings-t740.md (this file); outcome-t740.txt (`FAIL`); DEATH_NOTE-T740.md;
  TIMELINE.md; progress-t740.txt; heartbeat.json (CLOSED).
- flights/service-flight-15/ (9 files): uart.log `0befc1fe` (26,242 lines,
  3,505,378 B), analysis-t740.txt, flight-end.json, preflight.json,
  qemu-command.txt, qemu-stderr.log (0 B at launch), supervisor-flight15.log,
  uart.log.gz, uart.log.numbered.txt.
- flights/service-flight-16/ (8 files): uart.log `56354691` (3,968 lines,
  577,113 B), analysis-t740.txt `55125163`, flight-end.json, preflight.json,
  qemu-command.txt, qemu-stderr.log (81 B), uart.log.gz, uart.log.numbered.txt.
- flights/service-flight-17/ (8 files): uart.log `10f6365a` (3,817 lines,
  548,912 B), analysis-t740.txt `b67a1252`, flight-end.json, preflight.json,
  qemu-command.txt, qemu-stderr.log (81 B), uart.log.gz, uart.log.numbered.txt.
- Build artifacts (worktree root + box t740/): early-ramdisk-t740 `6804341a`,
  t740-fix.cpio.gz `724dff6c`, t740-modified.dtb `6b6a118d`,
  rebuild_cpio_t740.py `2db3ef8e`, early-ramdisk-t740-mech `83dcb08e`,
  t740-mech-fix.cpio.gz `b03ff37b`, t740-mech-modified.dtb `5d2f4f14`,
  rebuild_cpio_t740_mech.py `a3a98980`, early-ramdisk-t740-mech2 `3c9aa19d`,
  t740-mech2-fix.cpio.gz `1f8c00f2`, t740-mech2-modified.dtb `534f4c39`,
  rebuild_cpio_t740_mech2.py `93afe6f0`, supervisor_t740.py `f07de12c`,
  analyze_t740_probe.py `2e90935e`, patch_dtb_initrd.py `1f896a01`.
- Nothing committed; nothing touches the registry or GATES.md. The driver
  certifies and launches the successor.
