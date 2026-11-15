# Track T715 — installd early-boot trigger (Vector 1)

Verdict: FAIL

WALL-TYPE-TAG: guest type-1

Rust par bhi lagu: haan — the Rust emulator must expose the simplefb framebuffer through a path the guest composer service can open; the /dev/mem EPERM wall below is a guest device-access wall, and the Rust device model inherits the same question (which device node, which permissions, which address range the composer may map).

## 1. Summary

The T715 trigger WORKED. installd starts in early-boot, the Installer connects
on the first try, fixAppsDataOnBoot completes, the SystemServerInitThreadPool
unblocks, SensorService.onBootPhase returns, and sys.boot_completed reaches 1
— the exact wall that killed T712, T713 and T714 (10 Watchdog kills each) is
gone: 0 Watchdog kills in 2402 s of guest time.

The lane still FAILS its Step 3b criteria: queueBuffer 0, first present
dispatch 0, and 3 Launcher3 FATAL EXCEPTIONs (criterion: 0). The first failing
link is the composer backend: `simplefb: open /dev/mem failed: Operation not
permitted` (flight-01 L23338, repeated L61491). SurfaceFlinger runs, the
display device is added, Launcher3 starts and dexopts (status=PERFORMED), but
the composer cannot map the framebuffer, so no frame is ever presented
(929 `Invalid present fence` lines downstream). A second, parallel wall kills
system_server repeatedly: a GateKeeper NPE in LockSettingsService (crash file
L23926-23973) — 6 system_server lifetimes in the flight, 0 of them killed by
the Watchdog.

## 2. What was built (Step 2)

### 2.1 Re-derivation: why `class_start main` never fires

Read from the flown image itself (read-only debugfs on the Mac; the mounted
system.img's files, NOT the ramdisk copies):

- The mounted `/system/etc/init/hw/init.rc` is 1366 lines (58,376 B;
  preserved in this worktree as `step2-evidence-mounted-hw-init.rc`).
  `on late-init` (line 532) triggers exactly 10 triggers: early-fs, fs,
  post-fs, late-fs, post-fs-data, load_bpf_programs, zygote-start,
  firmware_mounts_complete, early-boot, boot. `nonencrypted` is NOT among
  them, and there are NO `vold.decrypt` triggers anywhere in the file.
- `class_start main` exists ONLY in `on nonencrypted` (lines 1214-1216).
  `on boot` (line 1104) contains only `class_start hal` and `class_start
  core`.
- T714 flight-01 UART proof: `nonencrypted` 0 matches in 56,604 lines;
  `class_start` 0; no `processing action (boot)` line. But `post-fs-data`
  DID fire and `early-boot` DID fire (T714 UART L9264, guest 89.48 s).
  INFERENCE (labelled): init queues `nonencrypted` conditionally in code
  (the init binary contains the string) and the condition is never met in
  this boot sequence, so the trigger never fires and `class main` services
  (including installd) never start.
- switch_root CONFIRMED: T714 UART L532, guest 8.93 s: `init: Switching root
  to '/system'`. Second-stage init parses the MOUNTED image's files; the
  ramdisk-root files are unreachable post-switch. The ramdisk
  `/init.virt.rc` option is DEAD (no such file at the image root). The
  trigger MUST live in a mounted-image .rc file.
- `/system/bin/installd` EXISTS in the flown system.img (inode 380, mode
  0755, 482,464 B) — the binary-presence gate passes.

### 2.2 The fix

Appended a separate block at end-of-file of `/system/etc/init/installd.rc`
in a box-side COPY of the T714 system.img (Linux debugfs, T706 extent-safe
procedure — new file + metadata + link swap; NO loopback mount — the box
user has no sudo; NEVER macOS debugfs write):

```
# T715: explicit installd trigger. on nonencrypted never fires here, so
# class_start main never runs. early-boot fires (proven, guest ~89s).
on early-boot
    start installd
```

Why this placement: it modifies the service's own .rc file at a trigger
PROVEN to fire in this exact boot sequence, after the /system mount
(guest 8.92 s) and ~60 s before PackageManager's first fixAppsDataOnBoot
attempt (guest ~160 s). Ruled out: ramdisk init.virt.rc (dead —
switch_root); a new file in /system/etc/init/ (new inode allocation,
riskier); vendor.img init.virt.rc (two repacks + DTB realign — strictly
more work).

### 2.3 Repack verification (all on the box, script kept at /home/muse/t715-work/)

- Vault base re-verified before copy: `ea5af412…`, and re-verified
  untouched after. Original installd.rc: 4,783 B, SHA `fcb59d1c…`. New
  installd.rc: 4,958 B, SHA `3ea3466b…`, inode 2548, mode 0644, uid 0,
  gid 0, xattr `u:object_r:system_file:s0` (25 B, no trailing NUL — T706
  precedent, recorded honestly).
- `e2fsck -f -y` exit 0, `e2fsck -f -n` exit 0 (7,342 files, 477,627
  blocks). Block 145414 SHA UNCHANGED `4ea21150…`. Dumped installd.rc
  byte-identical to intended. services.jar byte-identical `d4108052…`.
- NEW IMAGE SHA-256: `1283cbf279038f692ae42adc7bab571e89ef4ffd97efb0d982ee46e8050bfe3c`
  (2,059,816,960 B). Transferred box → sandbox → Mac with SHA re-verified
  at every hop. Initramfs `edba515e…` and DTB `d60bc98a…` UNCHANGED — no
  CPIO repack, no DTB realign.
- Mac `system.img` re-hashed `ea5af412…` AFTER all read-only debugfs
  reads: unchanged, inspection modified nothing.
- VARIANCE (Step 2): trigger shipped via system.img repack, NOT via CPIO
  member replacement. The brief anticipates the trigger living "in the
  system init configuration if that is where the trigger belongs" —
  re-derivation proved that is where it belongs.

## 3. Smoke flight (Step 3a) — PASS

Flight `flights/smoke/`: host 300.49 s (host-cap stop; the guest kernel
clock reached 225.527 s — kernel output quiets after boot, so guest_max is
a lower bound), exit 0, 13,687 lines, 1,428,498 B, UART SHA
`0e5221ab9c5832af36ca6e3f10aa4df1c39a2afbe0976425b1d2d6d9cea8c4a0`.

- Structural: Freeing unused kernel memory 960K @ guest 3.44 s; init
  first stage @ 9.19 s; fb0 simplefb @ 0.32 s; /dev/block/vda mounted as
  /system @ 9.59 s. hals load failures 0; sensorservice avc denials 0;
  primeCache errors 0; system_server 1 start; Watchdog kills 0.
- TRIGGER PROOF: L9355 init started service installd, pid 2422, guest
  94.20 s; L9356 `Command 'start installd' action=early-boot
  (/system/etc/init/installd.rc:109) took 239ms and succeeded`; L9686
  `installd firing up`.
- `installd not found`: 0 (T714: 2,750). fixAppsData lines: 0 — no stack
  dumps; the call returned silently (AOSP logs nothing on success).
  installd serviced the fix-ups: 91 installd lines including binder
  rename/create/setattr, restorecon runs, per-package prepare/mark
  (from L12563).
- Phase 200 STARTED (L13195). SensorService onBootPhase RETURNED: L13297
  `Service com.android.server.sensors.SensorService took 7231 ms in
  onBootPhase` — the T714 wall broken inside the smoke window.
- No literal `/dev/socket/installd` line (0) — the socket's appearance is
  not logged; functional proof is the binder traffic above.
- VARIANCE (Step 3a): smoke guest target 240 s (host cap 300 s), NOT the
  120 s in the Step 3a heading. The brief's own hard criterion
  (fixAppsDataOnBoot completion, first observable at guest ~160 s;
  system_server fork at ~150 s) is unobservable in a 120 s window that
  ends before system_server starts. 240 s matches the actual T714 smoke
  window (274.8 s guest) and is the minimum window that can observe the
  criterion.

## 4. Flight-01 (Step 3b) — the trigger holds; the display wall stands

Flight `flights/flight-01/`: stop `Guest target reached (2402.4s >=
2400s)`, host 2403.24 s, guest 2402.412 s, exit 0, 244,911 lines,
32,291,152 B, UART SHA
`8d69ee3b540b292b4bbc0089eb892ece6d1b0bc33123e372100b7f1ff395937c`.

### 4.1 The installd wall is gone (all counts grep -n on the final UART)

- installd started: L9319 `processing action (early-boot) from
  (/system/etc/init/installd.rc:108)` + L9320 `init: starting service
  'installd'...`, guest 88.32 s; L9676 `installd firing up`, PID 2429.
  468 installd lines total. (The `Command 'start installd'` completion
  line seen in the smoke UART does not appear in flight-01's UART;
  the start itself is proven by the firing-up + servicing lines.)
- `installd not found`: 0. fixAppsData lines: 0. FutureTask.get lines: 0.
- SensorService onBootPhase returned: L13333 `took 6456 ms in
  onBootPhase` (PID 2581). onBootPhase lines total: 283 (services
  completing boot phases across all system_server lifetimes).
- Watchdog KILLING lines: 0. (T714: 10.)
- sys.boot_completed=1: 17 lines; init processed `sys.boot_completed=1`
  actions from guest 505.43 s (L21346) and boot_completed-gated wifi
  actions from 512.08 s (L21767). system_server fork state: 6
  `Entered the Android system server!` lines (L11032, L26552, L51908,
  L81705, L190443, L232474) — see §4.3 for why 6.
- Holds from earlier lanes: `Failed to load subHal config file` 0;
  `output buffer not gpu writeable` 0; ISensors/default 1; sensorservice
  avc denials 0; kernel Oops/panic 0.

### 4.2 The display criteria fail — first failing link

- queueBuffer: 0. presentDisplay / present-dispatch lines: 0.
- FIRST FAILING LINK: L23338 (repeated L61491):
  `E/android.hardware.graphics.composer@2.1-service( 2417): simplefb:
  open /dev/mem failed: Operation not permitted`
  The composer service cannot open /dev/mem to map the simplefb
  framebuffer. Everything upstream works: SurfaceFlinger runs (261
  lines; `main thread ready to run. Initializing graphics H/W...`
  L10526), the display device is added (`Built-in Screen`, 1280x720,
  L12019, and again for each new system_server), Launcher3 starts and
  draws (3,143 Launcher3 lines). Everything downstream fails: 929
  `Invalid present fence` lines (Scheduler trackPendingFrame) — frames
  are produced but no present ever dispatches.
- Launcher3 FATAL EXCEPTION: 3 (criterion: 0). L25338 DeadSystemException
  (collateral — the system died underneath it); L46864 + L72770 genuine:
  `java.lang.RuntimeException: Couldn't removeRegionSamplingListener`
  at `CompositionSamplingListener.nativeUnregister` — Launcher3's
  display-composition sampling path failing against the broken composer.
  Total FATAL EXCEPTION lines: 7 (L25338, L25348, L25354, L25357,
  L25444, L46864, L72770); the other 4 are DeadSystemException
  collateral in settings, devicelockcontroller, permissioncontroller,
  and one unnamed PID.
- Dexopt criterion PASSES: L13561 Dexopt result for
  com.android.launcher3: `status=PERFORMED, dex2oatWallTimeMillis=33456`.
  No SKIPPED line mentions launcher3 (the 92 skipped lines are other
  packages).

### 4.3 Parallel wall: system_server crash loop (GateKeeper NPE)

system_server died 5 times (6 lifetimes), 0 by the Watchdog: zygote
received signal 9 at guest 550.4 s, 812.6 s, 1065.3 s (and later). The
first death's cause is proven by the crash file at L23926-23973:

```
java.lang.NullPointerException: Attempt to invoke interface method 'void
android.service.gatekeeper.IGateKeeperService.reportDeviceSetupComplete()'
on a null object reference
    at com.android.server.locksettings.LockSettingsService$DeviceProvisionedObserver
        .reportDeviceSetupComplete(LockSettingsService.java:3394)
```

The GateKeeper service is null (no GateKeeper HAL in this virt stack);
when device-provisioned flips at setup completion, LockSettingsService
NPEs and system_server dies. INFERENCE (labelled): the later deaths
share this cause (each new system_server re-hits setup completion), but
only the first death's crash file was read. The UART also carries 28+
tombstone files plus ANR/native-crash dropbox entries — the system is
broadly sick past boot, but the GateKeeper NPE is the proven first
system_server killer.

## 5. Flight-ab control (Step 3b)

Flight `flights/flight-ab/`: canonical kernel `05c43917…`, same DTB /
initramfs / system.img / vendor.img. QEMU exited on its own at host
20.03 s, guest 19.276 s, exit 0, 575 lines, 47,928 B, UART SHA
`abaf26bc86af698d6e392c7a8b0064bb596ab3e0ae2c5a9c82de17bb12088ada`
(differs from T714's `6a2a008c…` only because the kernel cmdline carries
the t715_* tags and the new system SHA; same 575-line / 47,928 B shape,
same early-exit pattern, last line `reboot: Restarting system with
command 'bootloader'`). system_server starts 0, installd lines 0,
queueBuffer 0. The control behaves as in T714: the canonical kernel
cannot boot this stack, so the flight-01 results are attributable to the
T705 kernel + T715 image, not to the harness.

## 6. Exact next fix (for the successor lane)

1. PRIMARY — composer framebuffer access. Diagnose why
   `open("/dev/mem")` returns EPERM for
   `android.hardware.graphics.composer@2.1-service` (PID 2417): the
   service's user/group, the /dev/mem node permissions, the SELinux
   context (permissive mode rules SELinux out as the blocker — INFERENCE
   labelled: EPERM from open() in permissive mode points at DAC or
   CONFIG_STRICT_DEVMEM, not an AVC denial), or a missing device node.
   Fix by the smallest route that grants the mapping: service user,
   ueventd permissions, or an fb0-based mapping instead of /dev/mem.
   Suggested base: the T708 rebuilt Composer ELF (`8d62a642…`) + this
   lane's image; oracle: the first `present` dispatch + queueBuffer > 0
   in a 2400 s flight. Do NOT change the installd trigger — it works.
2. SECONDARY — GateKeeper NPE stability wall. Provide a GateKeeper HAL
   stub (or a LockSettingsService guard) so system_server survives setup
   completion; verify with a full flight showing 1 system_server
   lifetime past boot_completed. This wall does not block the display
   criteria directly (frames fail at the composer regardless), but it
   blocks any long-window stability claim.
3. Vector 2 (thread-pool patch) and Vector 3 (onBootPhase decoupling)
   are NOT needed: the pool unblocked and onBootPhase returns. Do not
   spend a lane on them.

## 7. Evidence index

- `flights/smoke/`: uart.log (SHA `0e5221ab…`), uart.log.gz,
  uart.log.numbered.txt, flight-end.json, heartbeat.json,
  preflight.json, qemu-command.txt, qemu-stderr.log.
- `flights/flight-01/`: uart.log (SHA `8d69ee3b…`), uart.log.gz,
  uart.log.numbered.txt, flight-end.json, heartbeat.json,
  preflight.json, qemu-command.txt, qemu-stderr.log.
- `flights/flight-ab/`: uart.log (SHA `abaf26bc…`), uart.log.gz,
  uart.log.numbered.txt, flight-end.json, heartbeat.json,
  preflight.json, qemu-command.txt, qemu-stderr.log.
- `guest-inputs/t715-system.img`: SHA `1283cbf2…` (2,059,816,960 B).
  All other guest inputs: see `scripts/verify_inputs_t715.py` (16/16
  PASS, run on the Mac after transfer).
- `step2-evidence-mounted-hw-init.rc`: the mounted image's hw/init.rc
  (1366 lines) the derivation in §2.1 cites.
- `scripts/`: supervisor_t715.py, launch_smoke_t715.py,
  launch_flight_01_t715.py, launch_flight_ab_t715.py,
  verify_inputs_t715.py, verify_smoke_t715.py,
  analyze_t715_flight01.py, verify_flight_ab_t715.py. Every count in
  this file was produced by one of these scripts or by direct `grep -n`
  / `grep -c` on the preserved UARTs; all cited line numbers are
  grep -n numbers on the final UART files.
- Script lessons (this lane): (a) the first smoke launch died on a
  doubled-backslash regex transcribed from tool-escaped output —
  runtime import-tests now precede every launch; (b) Python
  `splitlines()` and text-mode newline translation both inflate UART
  line counts on logs containing binary blobs — the analysis scripts
  read bytes and split on `\n` only, matching `grep -n` / `wc -l`
  exactly (244,911 lines, verified three ways).
- TEN-PASSES was NOT run (one flight + A/B only, no 10/10 claim —
  recorded honestly per the brief).
- Disk: Step 0 gate passed at 12 Gi avail; 11 Gi at image arrival;
  ~9.6 Gi during flight-01 — above the 8 GiB gate throughout.
