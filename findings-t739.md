# T739 findings — scope LD_LIBRARY_PATH to the probe line only (restore T734 device state)

**Verdict: FAIL** (honest). The full 850 s flight RAN — service-flight-14
reached guest 851.145 s, the first full flight since T734 flight-9 — and the
scope fix WORKED: all three unmounts rc=0, the handoff device state verified,
Android init mounted /system from vda and booted past first stage. The T737
wall INIT-SYSTEM-MOUNT-LOOP0-UNBOUND is CLOSED and the T738 wall
INIT-UMOUNT-SELF-BLOCKED is CLOSED. But the brief's PASS contract is
conjunctive (full flight AND dup -1 confirmed in the surfaceflinger context
AND a timeline-access fix route TESTED), and the guest never presented a
frame: system_server was still grinding through OnBootPhase_500 when the
850 s target expired, sys.boot_completed never fired, so there is no
surfaceflinger context to confirm dup -1 in and no fix route could be tested.
The failure is a LATE-BOOT stall — the T729 wall BOOT-COMPLETED-NEVER-FIRED,
RECURRED — not the probe, not dispatch, and not the unmounts.

## Flight

- service-flight-14: 04:42:28–04:56:40 AEDT 21 Nov 2026 (2026-11-20T17:42:28Z–
  17:56:40Z). Host 852.35 s, guest 851.145 s (target 850 REACHED), QEMU exit 0.
  Stop reason: "Guest target reached (851.1s >= 850s)". UART 13,683 lines,
  1,568,512 B, SHA 918ef4b4.
- Flown stack: kernel 70d3859d, DTB e4b0ff79 (initrd-end 0x113342e21 =
  0x110000000 + 53,751,329), CPIO df2a6c9a (53,751,329 B, 241 entries),
  system af40403a, vendor 5e0607ad, supervisor 8f6cc5ea, analyzer 67b77198,
  rebuilder e48dd63b, early-ramdisk d75f5658 (18,359 B), patcher 1f896a01,
  rc 79ad3cc3 (parser VERDICT ACCEPT), QEMU 8e5c46dc.
- Disk gate OPEN at flight launch (93%, 15,672,472 KiB free at 04:42); gate
  was CLOSED at lane launch (6,341,956 KiB free, 98% at 04:14) and reopened
  on its own ~04:47 (16,820,316 KiB free). 15,257,848 KiB free at lane end.

## Counts (analyzer analysis-t739.txt, exit 0; driver-style byte recount on the sandbox UART copy — all match)

- T723_PROBE 27 (start 2 L356/357, DONE 1 L387, SP1 DONE 1 L378, SP2 DONE 1
  L386, SP3 DONE 1 L372); DROP_FAIL 0, RESOLVE_FAIL 0, GETPROCADDRESS_NULL 0.
- SP1 dup rc=-1 1 (L376), SP2 dup rc=-1 1 (L384), SP3 open timeline fd=-13 1
  (L371); SP1/SP2 create sync=0x5001 (L375/L381); SP2 resolved-address
  0x7f9ada97a8 (L382), maps path=/sysimg/system/lib64/libEGL.so (L383).
- IMAGE libEGL maps 1 (L383); RAMDISK libEGL maps 0 (anchored `path=` pattern
  — the T737 correction, not regressed).
- [T737] hooks 9 (L345, 347, 348, 350, 352, 353, 389, 391, 392); [T739] hooks 7
  (L589, 618, 647, 649, 650, 653, 654).
- T734_KERNEL_OK 1 (L194, guest 8.789 s) + [T734] probe finished rc=0 (L196);
  T720_PROBE DONE status=PASS 1 (L226).
- T733_PROBE_OK 1 (L7816, guest 102.053 s — leg 1 ONLY; the property leg
  never ran, the trigger never fired); T728_PROBE_OK 1 (L7812, guest
  102.002 s — leg 1 ONLY); T733_MINIMAL_OK 0 (expected, reproduces T733).
- sys.boot_completed 0, boot_completed (all) 0, T721 fence-log 0,
  simplefb: mapped 0, RenderEngine dup-fail 0 (rate 0.000/1000 s — VOID: the
  guest never presented a frame, so there is no presenting time to measure
  against), FATAL EXCEPTION 0, Kernel panic/Oops/BUG 0, EPERM 0,
  Watchdog KILLING 0 — the boot-completion holds are VOID (the guest never
  completed boot), not failed as faults.
- avc: denied (two spaces) 1257; CALL allocate2 2 (L8250, 8255); REJECT
  allocate2 0; "inaccessible or not found" 337 (the orphaned-background-
  script artifact, see §5); "can't create /dev/null" 4480 (same artifact).
- system_server: 95 lines (pid 701, forked and running); SurfaceFlinger: 12
  lines (started, display 0 connected, power mode 2 — never presented).

## 1. The scope fix worked — both the T737 and T738 walls are CLOSED

- `[T739] unmount /sysimg (vda) rc=0` (L589), `[T739] unmount /vendor (loop1)
  rc=0` (L618), `[T739] unmount debugfs (/sys/kernel/debug) rc=0` (L647) —
  ALL THREE unmounts rc=0, first time ever. No umount stderr appeared on the
  UART (no error to capture — the unmounts succeeded cleanly).
- `[T739] handoff state verified: /sysimg /vendor /sys/kernel/debug
  unmounted` (L649), `[T739] handoff state verified: /data mounted` (L650),
  `[T739] unmount proc (verify) rc=0` (L653), `[T739] kernel-init probe done;
  handing off to Android init` (L654). HANDOFF ABORTED 0. The handoff device
  state EXACTLY matches T734's (only /data + /data/media mounted).
- Init then proved it: `init: init first stage started!` (L666, guest
  21.660 s), `[libfs_mgr] __mount(source=/dev/block/vda,target=/system,
  type=ext4)=0: Success` (L675, guest 22.287 s), `Switching root to
  '/system'` (L676, guest 22.293 s). loop0 /system attempts: 0.
- **T737 wall INIT-SYSTEM-MOUNT-LOOP0-UNBOUND: CLOSED.** Init mounted /system
  from vda — the exact T734 behaviour — because the handoff state was
  restored. The T737 hypothesis (vda-mounted → loop0 fallback) is confirmed
  by the discrimination: same fstab/DTB/kernel/image as T737, only the
  handoff state changed, and init used vda this time.
- **T738 wall INIT-UMOUNT-SELF-BLOCKED: CLOSED.** Toybox unmounted /sysimg
  cleanly with the line-90 export removed — the tool no longer maps
  libraries from the mount it undoes. The Step 2 transitive-closure check
  (toybox's 13-library closure fully present in the ramdisk's /system/lib64/;
  libnetd_client not in toybox's closure) is confirmed by the flight.
- The probe RAN fully — the fourth time ever — with the IMAGE libEGL at a
  new address (0x7f9ada97a8): SP1 create sync=0x5001, dup rc=-1 (L375–376);
  SP2 create sync=0x5001 error=0x3000, getprocaddress dup=0x7f9ada97a8,
  map path=/sysimg/system/lib64/libEGL.so, dup rc=-1 error=0x3000
  (L381–384); SP3 open timeline fd=-13 (L371). Dispatch is ANSWERED a fourth
  time: dispatch forwards to the stub at base+0x5cb0; dup -1 is the sw_sync
  trampoline failing to open the timeline as uid 1000. No dispatch fix owed.

## 2. The full flight ran — and the guest never completed boot (first failing link)

- The flight reached the 850 s guest target (851.145 s, QEMU exit 0) — the
  first full flight since T734 flight-9. But sys.boot_completed is 0: the
  trigger NEVER fired in 851 s.
- First failing link: there is no single fault line — the guest was still
  ACTIVELY progressing at the target, not wedged. At the flight end
  (guest ~851 s) system_server (pid 701) was still starting services in
  OnBootPhase_500: L13656–13683 show DevicePolicyManagerService,
  TextToSpeech, AmbientContext, WearableSensing, storaged retry,
  SpeechRecognition, SearchUi, Smartspace, FontManager, TextServices,
  TextClassification, NetworkScore, NetworkStats, WifiService,
  WifiScanning, WifiP2p, ConnectivityService, NotificationManagerService —
  one every ~10–100 ms of logcat time, still moving. OnBootPhase_500: 83
  lines (of 143 in T734 flight-9); OnBootPhase_550: 0; OnBootPhase_600: 0.
  No FATAL EXCEPTION, no ANR, no Watchdog KILLING, no panic — the guest is
  SLOW, not stuck.
- Timing anchors (kernel timestamps): phase 500 STARTED at guest ~842 s in
  this flight (nearest kernel ts before the first OnBootPhase_500 line L13437
  is 841.717 s at L13291) — versus T734 flight-9, where phase 500 ran at
  guest ~669–675 s and boot_completed fired at 790.041 s. This flight's
  system_server reached phase 500 ~172 s LATER than T734's did. By the
  T734 calibration (phase 500 → boot_completed ≈ 115 s), this guest would
  need roughly 842 + 115 ≈ 957 s to complete boot — beyond the 850 s window.
- Boot-completion times across IDENTICAL inputs (same kernel/DTB/CPIO/
  system/vendor): T730 fired at 495.717 s, T734 at 790.041 s, T739 never in
  851.145 s. The boot-time distribution is wide and variable; the 850 s
  window does not reliably contain it.

## 3. What did NOT happen (why the verdict is FAIL, not PASS)

- dup -1 in the surfaceflinger context: NOT confirmed. SurfaceFlinger
  started (L8090–8096), connected display 0 (L8475), set power mode 2
  (L8517, L8608, L13494) — further than any flight since T734 — but never
  presented a frame: T721 fence-log 0, simplefb: mapped 0, RenderEngine
  dup-fail 0, CALL allocate2 only 2 (context, no presenting). The dup -1
  stands confirmed only in the early-ramdisk probe context (4th flight),
  not in the surfaceflinger context. The route that would run it: a longer
  flight window so the boot completes and surfaceflinger presents — then the
  dup-fail rate is measurable against the T721 baselines (4.058/6.660).
- Timeline-access fix route: NOT TESTED — blocked, not failed. With no
  presenting surfaceflinger there is no dup-fail signal to measure a fix
  against (the T722 differential criteria need a presenting guest). No fix
  was built in this lane: building one whose effect cannot be observed
  would be building on an unmeasurable outcome. The probe-context mechanism
  test (relax the debugfs file's permissions so the uid-1000 probe can open
  the timeline) is named for the successor — it proves the mechanism but
  does not by itself satisfy the surfaceflinger-context clause, because the
  early-ramdisk unmounts debugfs before the handoff and the booted guest
  remounts it with default permissions; the surfaceflinger-context fix must
  live in the booted guest (e.g. vendor init rc), not the early-ramdisk.
- The property legs (T733/T728 leg 2) never ran — TRIGGER-FIRED-FIRST: no
  trigger fired, so no leg-2 claim is made. Leg 1 fired at guest ~102 s
  (FASTER than T734's 196 s — the early boot was quicker this flight; the
  lateness is all in the dex2oat/PackageManager phase before phase 500).

## 4. Wall: BOOT-COMPLETED-NEVER-FIRED (T729 wall, RECURRENT)

- Type: guest. The guest boots (first stage, /system from vda, root switch,
  system_server forked, surfaceflinger started) but does not complete boot
  within the 850 s flight window: sys.boot_completed 0 in 851.145 s.
- WALL-TYPE-TAG: guest.
- Rust par bhi lagu: haan. Any emulator (QEMU or the Rust emulator) booting
  this guest image faces the same slow, variable boot-time distribution
  (495 s / 790 s / >851 s across identical inputs). The flight window must
  cover the distribution, and any boot-time reduction (fewer retries,
  missing services restored) carries to both engines.
- Slowness suspects (evidence, not proven cause): "storaged not found;
  trying again" 492 times (vs 316 in T734 flight-9 — the StorageManagerService
  retries a missing storaged service through the whole boot); "artd could
  not be found, trying to start it as a lazy AIDL service... may be stuck
  starting" 6 times (vs 1 in T734 — the ART daemon never starts, dexopt
  falls back); "Waiting for service 'media.metrics'" 4 times (never
  arrives; 'statscompanion' did arrive after 2514 ms). dex2oat 296 lines and
  PackageManager 549 lines are the SAME volume as T734 (288/568), so the
  work is the same — the guest just took ~172 s longer to get through it.
  The orphaned-background-script spam (4,480 "can't create /dev/null" +
  337 "inaccessible" lines) is LESS than T734's (6,646 + 876), so it is NOT
  the differentiator — but it still burns guest CPU in tight loops and
  should be killed at the handoff by the successor (see §7).
- This is a RECURRENCE, not a new wall mechanism: T730 (495.717 s) and T734
  (790.041 s) both fired the trigger on identical inputs, so the wall was
  never a fixed defect — it is the tail of the boot-time distribution
  crossing the 850 s window. The T729 wall disposition is therefore
  REOPENED by this flight; the window, not the guest's ability to boot, is
  what failed.

## 5. The orphaned-background-script artifact (known, T734)

- After `exec /init`, the backgrounded /t493-probe.sh and
  /t501-keystore2-dump.sh survive as orphans and loop forever: 4,480
  "can't create /dev/null" lines + 337 "/system/bin/toybox: inaccessible
  or not found" lines. L695 explains the toybox failure: the mounted
  image's /system/bin/toybox exists but its linker cannot find libm.so
  (the orphaned scripts run with the post-switch-root /system, where the
  linker's search path no longer resolves the image's libs the way the
  ramdisk's did). This is UART noise + wasted guest CPU, not a boot fault
  (the same artifact is in T734 flight-9, which completed boot). Successor
  fix: kill the backgrounded scripts before `exec /init` (or make them exit
  after the probe) — one line in the early-ramdisk, recorded here so the
  successor does not re-diagnose it.

## 6. Reflection (FAIL)

- **What happened:** the lane's own fix worked completely — the unmounts
  all succeeded, the handoff state was restored exactly, init mounted
  /system from vda, the full 850 s flight ran for the first time since
  T734, and the probe answered dispatch a fourth time. Then the flight
  expired with the guest still booting — a failure mode the lane did not
  anticipate because the two previous full flights (T730, T734) both
  completed boot inside the window.
- **Root cause of the FAIL:** the 850 s window is shorter than the tail of
  this guest image's boot-time distribution under TCG. The boot times on
  identical inputs are 495 s, 790 s, and now >851 s — the window was set
  when the sample was two flights, both under it. The missing check was
  "how variable is boot_completed across flights?" — answerable from the
  T730/T734 pair alone (a 295 s spread), and it was not asked.
- **Missed signal:** T734's own findings record boot_completed at 790 s —
  only 60 s inside the window — and nobody flagged the margin as thin. A
  60 s margin on a 295 s spread is a coin flip, and this flight lost it.
- **What changes:** (1) the successor extends the flight window (guest
  target ~1200 s, host cap ~1500 s — the T734-calibrated need is ~957 s,
  plus margin for the observed spread); (2) the successor kills the
  orphaned background scripts at the handoff; (3) the boot-time
  distribution (495/790/>851 s) is now part of the flight-planning record,
  and any future window is set from the distribution's tail, not from a
  single flight's time.
- **What survives:** the T739 build (CPIO df2a6c9a / DTB e4b0ff79 /
  early-ramdisk d75f5658 / supervisor 8f6cc5ea / analyzer 67b77198), the
  flight evidence (service-flight-14, UART 918ef4b4), the two wall closures
  (T737 + T738), and the fourth consecutive dispatch answer. Nothing needs
  rebuilding for the successor except the early-ramdisk change (kill the
  orphaned scripts) + CPIO rebuild + DTB re-patch + the longer window.

## 7. Successor direction (for the driver)

T740: (a) extend the flight window — guest target ~1200 s, host cap ~1500 s
(the T734-calibrated need is ~957 s; the observed spread demands margin);
(b) kill the orphaned background scripts (/t493-probe.sh,
/t501-keystore2-dump.sh) at the handoff, before `exec /init` — they spam
~4,800 UART lines and burn guest CPU in tight loops post-handoff; (c) once
the boot completes and surfaceflinger presents, confirm dup -1 in the
surfaceflinger context (dup-fail rate vs T721 baselines 4.058/6.660) and
THEN test the timeline-access fix route (probe-context mechanism test first:
relax the debugfs file's permissions so the uid-1000 probe opens the
timeline; the surfaceflinger-context fix must live in the booted guest);
(d) investigate the storaged-retry loop (492 retries) and the missing
artd/media.metrics/storaged services as boot-slowness suspects. Do NOT
re-run the T734 bisect. Do NOT rebuild the kernel. Do NOT reintroduce the
export (T738 wall stays closed only while LD_LIBRARY_PATH stays scoped to
the probe line). The T739 build is reusable except for the early-ramdisk
change + CPIO rebuild + DTB re-patch + the longer window.

## Adjudication (brief PASS contract — conjunctive)

- Full 850 s flight runs: YES (guest 851.145 s, target reached, QEMU exit 0).
- SP1/SP2 answered from the booted guest with the IMAGE libEGL: NO
  (answered from the early-ramdisk context only, as in T736/T737/T738; the
  guest never presented a frame, so there is no surfaceflinger context).
- [T739] unmount hooks all rc=0 + handoff state verified: YES (all three
  unmounts rc=0, state verified, /data mounted, no HANDOFF ABORTED).
- Timeline-access fix route TESTED: NO (blocked — no presenting
  surfaceflinger context to measure against).
- Verdict: FAIL. The T737 wall and the T738 wall are both CLOSED; the T729
  wall BOOT-COMPLETED-NEVER-FIRED has RECURRENDED. Successor: T740 (longer
  flight window + kill orphaned scripts at the handoff).

## Evidence

- Flight: flights/service-flight-14/ (flight-end.json, preflight.json,
  qemu-command.txt, qemu-stderr.log 81 B, supervisor.log, uart.log 13,683
  lines SHA 918ef4b4, uart.log.gz, uart.log.numbered.txt,
  analysis-t739.txt).
- Build: CPIO df2a6c9a, DTB e4b0ff79, early-ramdisk d75f5658, supervisor
  8f6cc5ea, analyzer 67b77198, rebuilder e48dd63b, patcher 1f896a01 — all in
  the worktree, sandbox ~/workspace/t739-scratch/, and box
  /mnt/sdc3/muse/kernel-primary/t739/ (the sandbox CPIO mirror was skipped
  on quota; the box copy is canonical, SHA df2a6c9a).
- The analyzer parser verdict is exit 0, `=== REPORT COMPLETE ===`; the
  parser's counts agree with the independent byte recount above on every
  line (T723_PROBE 27, SP1/SP2/SP3 DONE 1 each, T734_KERNEL_OK 1,
  sys.boot_completed 0, panic 0). The parser names the flight's outcome
  explicitly: probe lines present, boot holds void on the 851 s target
  expiry with system_server still in OnBootPhase_500.
- Progress: progress-t739.txt, TIMELINE.md, heartbeat.json (Mac worktree +
  sandbox + box; Step 2 exit + flight outcome recorded).
