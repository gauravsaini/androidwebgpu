# T741 findings — vendor init rc fix for the sw_sync DAC barrier

- Track: T741 (vendor-rc-fix). Lane agent `e34acd85-4dc4-4d99-8325-fa3e8e129d56`.
- Flight: service-flight-18. Supervisor `scripts/supervisor_t741.py`, guest target 1500 s, host cap 1800 s.
- Flight window: started 2026-11-20T21:24:03Z (08:24:03 AEDT 21 Nov 2026), ended 2026-11-20T21:49:20Z (08:49:20 AEDT).
- Guest 1512.44 s, host 1516.98 s, QEMU exit 0. Stop reason: "Guest target reached (1512.4s >= 1500s)".
- UART: 133,388 lines, 18,497,574 B, SHA256 `bba22ffe402f69c229128a8f22c19070688f46751f9d532f7f76a64bb8c21342`.
- Analyzer: `scripts/analyze_t741_probe.py` (SHA `e9a5b7a8fafb` on the box), exit 0, output `flights/service-flight-18/analysis-t741.txt` (361 lines, SHA `1c8c8eb555ee`).
- Findings written 2026-11-21 09:14 AEDT.

## Verdict: PASS

The vendor-rc fix CURED the dup -1 in the surfaceflinger context. The T722 condition was
reproduced in the guest by the early-ramdisk probe (SP1 dup rc=-1, SP2 dup rc=-1, SP3 fd=-13,
uid 1000, IMAGE libEGL) — and with the fix flown, RenderEngine logged ZERO dup-fails in
991.1 s of presenting time, rate 0.000 per 1000 s, BELOW BOTH T722 baselines (4.058 / 6.660).

WALL-TYPE-TAG: POST-BOOT-WINDOW-TOO-SHORT — CLOSED (T740 wall; this flight measured
991.1 s of presenting time, ~25x the T734 ~40 s calibration window).

Verdict-contract checklist (brief § Verdict contract), each clause with its evidence:

1. Full 1500 s flight runs — guest 1512.44 s, host 1516.98 s, QEMU exit 0, flight-end.json
   stop reason "Guest target reached (1512.4s >= 1500s)". PASS.
2. Boot completes — sys.boot_completed 8 lines, first firing L14375 at guest 521.327 s.
   The FASTEST boot_completed in the chain (T734 790 s, T740 1175.836 s). PASS.
3. dup -1 confirmed in the surfaceflinger context with guest-origin evidence — the
   early-ramdisk T723 probe (pre-handoff, pre-fix control) shows SP1 dup rc=-1 (L377),
   SP2 dup rc=-1 (L385), SP3 open timeline fd=-13 (L372), uid 1000, IMAGE libEGL
   `/sysimg/system/lib64/libEGL.so` (L384). The T722 condition is reproduced. PASS.
4. Vendor-rc fix FLOWN and dup-fail rate BELOW BOTH baselines — embedded vendor in the
   flown CPIO is `fe0bc359` (supervisor-pinned, flight-end.json inputs match); RenderEngine
   EGL dup-fail lines 0; rate 0 / 991.1 s * 1000 = 0.000 per 1000 s vs baselines 4.058
   (T721 flight-01) and 6.660 (T721 revert). PASS.
5. All T721/T722 holds intact — T720_PROBE DONE status=PASS 1, FAIL 0; T721 fence-log 1
   (L16401, composer pid 508, fd=10 tl=9, guest ~554.3 s, post-boot); Invalid present
   fence 0; T628 REJECT allocate2 0; EPERM 0; simplefb: mapped 1; Watchdog KILLING 0;
   sys.boot_completed 8; Kernel panic/Oops/BUG 0. PASS.
6. [T739] unmount hooks fire — unmount /sysimg rc=0 (L525), unmount /vendor rc=0 (L554),
   unmount debugfs rc=0 (L583), handoff-state-verified (L585), /data mounted (L586),
   HANDOFF ABORTED 0. PASS.
7. Orphaned scripts killed at the handoff — post-handoff spam 0/0 (can't create /dev/null 0,
   inaccessible-or-not-found 0; T739 flight-14 totals were 4480/337). PASS.

First failing link: NONE — no link failed. The flight, the boot, the fix, the measurement,
and every hold all passed.

## The fix, flown — embedded-vendor rule

- Base vendor `5e0607ad` (134,217,728 B) repacked on the box (debugfs, T718 procedure) to
  `t741-vendor.img` `fe0bc359`. One file changed: `/etc/init/android.hardware.graphics.composer@2.1-service.rc`,
  320 B → 391 B, SHA `485a59c9` → `cdeb88bf`. The existing `on post-fs-data` action was
  extended with `chmod 0755 /sys/kernel/debug` and `chmod 0755 /sys/kernel/debug/sync`.
- Composer `f7600821` + shim `3daa5a49` + libEGL `34b79da2` byte-identical before AND after
  the repack; e2fsck `-f -y` exit 0, `-f -n` exit 0 clean; base image untouched.
- CPIO rebuilt on the Mac (`rebuild_cpio_t741.py`): base T740 CPIO `724dff6c` (241 entries),
  only member replaced `ramdisk-images/vendor.img` `5e0607ad` → `fe0bc359`. New CPIO
  `t741-fix.cpio.gz` `c1203336` (53,753,325 B). Early-ramdisk carried forward BYTE-IDENTICAL
  (`6804341a`). All other 240 entries byte-identical.
- DTB re-patched (`patch_dtb_initrd.py`): `t741-modified.dtb` `4ecab141`, initrd-end cell
  byte 268 = 0x1133435ed; end−start = 53,753,325 = CPIO size.
- Supervisor-pinned SHAs (kernel `70d3859d`, DTB `4ecab141`, system `af40403a`, CPIO
  `c1203336`, vendor `fe0bc359`, QEMU `8e5c46dc`) all match the flight-end.json `inputs`
  block exactly. The guest consumed the fixed vendor image: the embedded member of the
  FLOWN CPIO is `fe0bc359`, not the base.
- The vendor-rc log patterns are 0 (init parsing the composer rc 0, post-fs-data action 0,
  debugfs mount 0, chmod 0755 lines 0) — the EXPECTED baseline, proven by T740 flight-15:
  init logs `Command... took Xms` only for slow commands (~114–140 ms); mount/chmod take
  <1 ms and are not logged. The functional proof is the dup-fail rate (0 in 991.1 s), not
  the log lines. Rust par bhi lagu: a host-side mode check on a file proves the file
  exists and is readable, not that the consuming uid-1000 context can REACH it — the
  Rust emulator's timeline-open path must be checked from the consuming context, not
  from the host.

## The dup-fail rate — presenting proof (the 0 is a cure, not a void)

- First sys.boot_completed firing: guest 521.327 s (L14375).
- Presenting time: guest max 1512.44 s − first boot_completed 521.327 s = 991.1 s.
- RenderEngine EGL dup-fail lines: 0. Rate 0 / 991.1 s * 1000 = 0.000 per 1000 s.
- T722 differential criteria: BELOW BOTH baselines 4.058 / 6.660 = fix works.
- Presenting DID happen, continuously, across the whole window:
  - CALL allocate2: 127 total, 122 AFTER first boot_completed (first post-boot L15735
    guest 538.797 s, last L102675 guest 1430.065 s).
  - T721 fence-log L16401: composer pid 508, `T721 fd=10 tl=9`, guest ~554.3 s
    (bounded by kmsg L16389 guest 554.095 s and L16405 guest 554.568 s) — a real sw_sync
    fence minted by the composer in the booted guest, after boot_completed.
  - simplefb: mapped 1. OnBootPhase_600 lines 306 (boot reached phase 600).
- The T734 calibration expects the first dup-fail at ~40 s into presenting. This flight
  presents for 991.1 s with 0. The fix works.

## Control markers — count 3, explained (not a void)

T733_PROBE_OK 3 (L2779 guest 77.081 s, L14926 guest 527.364 s, L27219 guest 727.740 s) and
T728_PROBE_OK 3 (L2768 guest 76.983 s, L14921 guest 527.291 s, L27218 guest 727.736 s).
The brief's control expected 2 (legs 1+2). The third firing is a repeat of leg 2, and the
mechanism is verified from the FLOWN system image (box debugfs, `t733-system.img`
`af40403a`): both rc files are 722 B and SHA-identical to the staged copies
(t728probe.rc `f9baa4799e55`, t733probe.rc `79ad3cc3c965`).

- BOTH rc files contain an init builtin in EACH action: `write /dev/kmsg T*_PROBE_OK`.
  The marker is written by INIT itself when the action runs — it fires even when the
  `start`/`exec` in the same action FAILS. The markers are TRIGGER markers (the action
  was processed), not exec markers (the binary ran).
- Leg 1 (on boot, ~77 s) fired once. Leg 2 (property `sys.boot_completed=1`) fired TWICE:
  L14916 guest 527.208 s and L27104 guest 727.313 s — because `sys.boot_completed=1` was
  processed twice (L26357 guest 723.078 s re-processed the init.rc:1224 action). The
  re-processing was done by the RESTARTED system_server (pid 2176, see below), which
  re-ran boot completion after the SIGKILL.
- BOTH leg-2 `start t728probe` commands FAILED with the SELinux label error (L14917,
  L27105): `File /system/bin/t728probe (labeled "u:object_r:system_file:s0") has incorrect
  label or no domain transition from u:r:init:s0 to another SELinux domain defined.` The
  `start t733probe` command line does not appear in the UART (0 lines) — the t733probe.rc
  action's marker still fired (L14926, L27219) via the init `write` builtin. The two
  service-refusal lines are the ONLY init service-refusal lines in the UART.
- Consequence: the control proves the triggers fired (on-boot + property, twice). It does
  NOT prove the probe binaries exec'd under init — that was never what the markers proved,
  and the T722 condition itself is proven by the early-ramdisk probe (§ Counts), which ran
  before any of this. The count-3 variance is recorded openly; it changes no verdict.

## Watchdog investigation — the flight-15 kill did NOT recur

- Watchdog KILLING lines: 0. ALL W/Watchdog lines: 0. The flight-15 kill did not recur.
  The parser fix works: the T741 analyzer's fixed pattern (`W/Watchdog\(\s*\d+\):.*WATCHDOG
  KILLING`) catches flight-15 L14509 in regression (count 1 where the T740 parser reported
  0); on flight-18 it correctly reports 0.
- INSTEAD, system_server died by SIGKILL, post-boot, at guest ~596 s:
  - L18507: `E/Zygote (506): Zygote failed to write to system_server FD: Connection refused`
  - L18508: `I/Zygote (506): Process 678 exited due to signal 9 (Killed)`
  - L18527: `E/AndroidRuntime (2040): DeadSystemException: The system died; earlier logs
    will point to the root cause` (com.android.phone).
  - The killer is UNIDENTIFIED in the UART: no lmkd kill line names pid 678, no init
    "Sending signal 9 to service 'system_server'" line, no `kill.*system_server` line.
    The watchdog is RULED OUT (0 W/Watchdog lines). Candidates: LMK under TCG memory
    pressure (kill line lost in the kmsg flood) or an explicit kill from a privileged
    process. Open question for T742 — NOT a verdict wall: the boot had already completed
    (521.3 s) and presenting continued to 1430.1 s.
  - Zygote reforked system_server pid 2176 (first lines L18994, logcat 23:26:27, guest
    ~623 s; `system_server( 2176): Using CollectorTypeCC GC.`). The new system_server
    re-processed boot_completed at guest 723.078 s (L26357) and re-fired the property
    actions (leg-2 markers at 727.7 s).
  - The 6 FATAL EXCEPTION lines (L18391, L18452, L18499, L18516, L18522, L18525 — all
    within 0.64 s, logcat 23:25:59–23:26:00, guest ~581 s) are COLLATERAL of the
    system_server death, not its cause: com.android.sdksetup (pid 2000),
    com.android.permissioncontroller (pid 1281, RoleControllerService),
    com.android.launcher3 (pid 1652), com.android.phone (pid 2040), plus pids 2110 and
    2099 (no Process line). No FATAL EXCEPTION carries a system_server pid.
  - The last system_server (678) lines before the death (L18259–L18262, logcat 23:25:54)
    are a WifiThreadRunner stack trace and "Slow operation: 57ms so far, now at
    attachApplicationLocked" — 6 s before the kill, no smoking gun.
  - A debuggerd dump of process 678 ran at guest ~443–444 s (L9489 `libdebuggerd_client:
    started dumping process 678`, L9521 `done dumping process 678`) — 150 s before the
    death; 678 lived on. A tombstone_00 exists in /data/tombstones (read denials at
    L18509, L18585–L18590); its owner is not attributed in the UART.
- media.c2 one-second waits: 786 (flight-15: 155) — FIVE times the flight-15 count, with
  NO watchdog kill and the FASTEST boot in the chain (521.3 s). Camera service
  unavailable: 121 (flight-15: 117). The media.c2 waits are NOT the boot-slowness
  driver: in flight-15 the kill followed a 94 s MAIN-thread block; here the waits
  accumulated on other threads while the boot completed in 521 s. The waits alone do
  not predict a kill.
- Boot-slowness comparison (storaged not found / artd could not be found / Waiting for
  service 'media.metrics'):

  | flight | storaged | artd | media.metrics | first boot_completed |
  |---|---|---|---|---|
  | T734 flight-9 | 316 | 1 | 4 | 790 s |
  | T739 flight-14 | 492 | 6 | 4 | never |
  | T740 flight-15 | 492 | 0 | 8 | 1175.836 s |
  | T741 flight-18 | 308 | 0 | 10 | 521.327 s |

  Flight-18's storaged count (308) is closest to T734's (316); artd 0 matches T740.
  The 521.3 s boot is the fastest boot_completed in the chain. Boot-time distribution
  (first boot_completed): T734 790 s, T740 1175.836 s, T741 521.327 s.

## § Counts (analyzer + independent verification)

From `analysis-t741.txt` (analyzer exit 0) and the lane's own recounts
(`scripts/verify_t741_step4.py`, `scripts/verify_t741_step4b.py`, run on the Mac
against the UART bytes, split on `\n`, literal patterns):

- Total lines 133,388; total bytes 18,497,574; guest max 1512.440044 s.
- T723_PROBE 27 lines: start 2 (L357–358, guest 4.683 s, uid 1000), DONE 1 (L388),
  SP1 DONE 1 (L379), SP2 DONE 1 (L387), SP3 DONE 1 (L373). SP3 open timeline fd=-13;
  SP1 dup rc=-1; SP2 dup rc=-1 error=0x3000. DROP_FAIL 0, SP1 RESOLVE_FAIL 0,
  SP2 GETPROCADDRESS_NULL 0. maps IMAGE libEGL 1 (L384), RAMDISK libEGL 0.
- T734_KERNEL_OK 1 (L194, guest 3.489 s).
- [T737] hooks: mount /sysimg rc=0 1 (L346), /vendor rc=0 1 (L351), debugfs rc=0 1
  (L353), proc rc=0 1 (L354), unmount proc rc=0 1 (L392), proc-unmounted 1 (L393),
  t723-probe finished 1 (L390). All FAILED counts 0.
- [T739] hooks: unmount /sysimg rc=0 1 (L525), /vendor rc=0 1 (L554), debugfs rc=0 1
  (L583), handoff-state-verified 1 (L585), /data mounted 1 (L586), unmount proc
  (verify) rc=0 1 (L589), T739 handoff 1 (L590). All FAILED/DIRTY/UNVERIFIED/ABORTED 0.
- [T740] hooks: kill-block entry 1 (L493), t493-probe.sh killed 1 (L495),
  t501-keystore2-dump.sh killed 1 (L496), STILL ALIVE 0/0. Post-handoff spam 0/0.
- T733_PROBE_OK 3, T728_PROBE_OK 3 (see § Control markers). T733_MINIMAL_OK 0
  (expected — minimal binary silent under init, reproduces T733).
- init service-refusal lines 2 (L14917, L27105 — both `start t728probe`, SELinux label).
- T720_PROBE DONE status=PASS 1 (L232), FAIL 0. T721 fence-log 1 (L16401).
  Invalid present fence 0. T628 REJECT allocate2 0. CALL allocate2 127 (122 post-boot).
  EPERM 0. simplefb: mapped 1. ALL sw_sync lines 1459.
- sys.boot_completed 8 (first L14375 guest 521.327 s; re-processed L26357 guest
  723.078 s after the system_server restart).
- Watchdog KILLING 0. FATAL EXCEPTION 6 (collateral, see § Watchdog investigation).
  Kernel panic/Oops/BUG 0.
- RenderEngine EGL dup-fail 0. Dup-fail rate whole-span 0.000/1000 s; presenting-time
  basis 0.000/1000 s (0 / 991.1 s).
- OnBootPhase_500 286, _550 288, _600 306. ALL system_server lines 536.
  system_server(tag) pids: 678 (102 lines, first L3769) → 2176 (226 lines, first L18994).
- gralloc memfd lines 75. tombstone lines 1634.
- ALL sw_sync lines 1459. `avc: denied` lines present (permissive=1 throughout; no EPERM hold breach).

## § Reflection (REFLECT-ON-EVERY-PROBLEM)

What went wrong, or nearly: (1) The brief's control expected T733/T728_PROBE_OK == 2 and
the flight produced 3 — the lane's first read was "extra firing, mechanism unknown".
The near-miss was treating the marker as proof the binary exec'd: the rc files prove
the marker is an init-written trigger marker that fires even when the service start
fails. Had the lane stopped at the count, the findings would have claimed an exec that
never happened. The durable fix: the control's meaning is now written into these
findings (trigger marker, not exec marker) and the flown rc files were SHA-verified
against the staged copies — verify the artifact the guest ACTUALLY consumes, including
its trigger semantics. (2) The system_server SIGKILL was first read as "another watchdog
kill" — it is not: 0 W/Watchdog lines, and Zygote names signal 9 explicitly. The lesson
is to read the death line before naming the killer; the watchdog is one suspect among
several, and the UART names the signal. (3) Two transient egress-proxy failures
(macssh PREFLIGHT_FAIL, boxssh Broken pipe) hit mid-verification; both cleared on a
single retry — transient transport failures are retried once before being reported, per
the best-effort rule. What changes: T742's brief should carry the trigger-marker
semantics and a SIGKILL-source probe (lmkd kill lines + `dumpsys` of the LMK state
post-flight); the analyzer should count the `start <probe>` failure lines as a named
section rather than burying them in the service-refusal dump. No repeat of the same
root cause is expected: the marker semantics are now on the record.

## Successor direction

- The dup -1 question is CLOSED: the vendor-rc fix (chmod 0755 on /sys/kernel/debug
  and /sys/kernel/debug/sync in the composer's `on post-fs-data` action) cures it in the
  booted guest. T742 should NOT re-diagnose dispatch or the dup path.
- T742's question: the post-boot system_server SIGKILL (guest ~596 s, killer
  unidentified). Probe: lmkd kill lines for pid 678 (check for flood loss), guest RAM
  pressure at ~590 s, and whether the SIGKILL recurs on a repeat flight. If it is LMK,
  the fix is guest RAM / LMK tuning, not a display-path change. If it recurs and blocks
  long post-boot runs, it becomes the next wall.
- The vendor-rc fix carries forward unchanged into T742 (same vendor `fe0bc359`, same
  CPIO `c1203336`, same DTB `4ecab141` unless T742 changes something else).
- The analyzer's fixed Watchdog pattern is the new baseline; keep the regression test
  against flight-15 L14509.

## Evidence inventory

- `flights/service-flight-18/uart.log` — SHA `bba22ffe402f69c229128a8f22c19070688f46751f9d532f7f76a64bb8c21342`,
  133,388 lines, 18,497,574 B.
- `flights/service-flight-18/flight-end.json` — guest 1512.44 s, host 1516.98 s, QEMU exit 0.
- `flights/service-flight-18/analysis-t741.txt` — analyzer output, 361 lines, SHA `1c8c8eb555ee`.
- `flights/service-flight-18/preflight.json`, `qemu-command.txt`, `supervisor.log`,
  `qemu-stderr.log`, `uart.log.gz`, `uart.log.numbered.txt`.
- `scripts/verify_t741_step4.py` (SHA `3f9a0fb2c9b3`) + `scripts/verify_t741_step4b.py`
  (SHA `8c9a401bccb4`) — the lane's independent recounts.
- `findings-t741.md` (this file), `outcome-t741.txt` (`PASS\n`), `TIMELINE.md`,
  `progress-t741.txt`, `heartbeat.json` (CLOSED).
- Flown stack: kernel `70d3859d`, DTB `4ecab141`, system `af40403a`, CPIO `c1203336`,
  vendor `fe0bc359`, QEMU `8e5c46dc`.
