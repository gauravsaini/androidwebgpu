# T738 findings — unmount probe mounts before handoff (restore T734 device state)

**Verdict: FAIL** (honest). The full 850 s flight did NOT run: service-flight-13
ended at guest 16.485 s when the early-ramdisk script ABORTED the handoff by
design — `umount /sysimg` failed (rc=1), the verification correctly reported
the state DIRTY, and the brief's STOP rule fired (exit 1, kernel panic
"Attempted to kill init"). The dispatch question is answered a THIRD time
(SP1/SP2 both dup -1, now with the IMAGE libEGL at a new address), and the
T737 wall's fix route is REFUTED in its present form — but the brief's PASS
contract is conjunctive (full flight AND dup -1 confirmed in the
surfaceflinger context AND a timeline-access fix route TESTED), and the
flight never reached the surfaceflinger context, so no fix route could be
tested. The failure is a NEW guest wall, not the probe and not dispatch.

## Flight

- service-flight-13: 03:40:36–03:41:01 AEDT 21 Nov 2026 (2026-11-20T16:40:36Z–
  16:41:01Z). Host 25.03 s, guest 16.485 s, QEMU exit 0. UART 613 lines,
  46,953 B, SHA 8796fb47.
- Flown stack: kernel 70d3859d, DTB 4be60332 (initrd-end 0x1133425ca =
  0x110000000 + 53,749,194), CPIO 5d7affd5 (53,749,194 B, 241 entries),
  system af40403a, vendor 5e0607ad, supervisor 800d4746, analyzer ea5923ee,
  rebuilder 07f3a513, early-ramdisk 7f4525a9 (17,247 B), patcher 1f896a01,
  rc 79ad3cc3 (parser VERDICT ACCEPT), QEMU 8e5c46dc.
- Disk gate OPEN at flight launch (93%, 16,125,236 KiB free at 03:39).

## Counts (analyzer analysis-t738.txt; driver-style byte recount on the sandbox UART copy — all match)

- T723_PROBE 27 (start 2 L357/358, DONE 1 L388, SP1 DONE 1 L379, SP2 DONE 1
  L387, SP3 DONE 1 L373); DROP_FAIL 0, RESOLVE_FAIL 0, GETPROCADDRESS_NULL 0.
- SP1 dup rc=-1 1 (L377), SP2 dup rc=-1 1 (L385), SP3 open timeline fd=-13 1
  (L372); SP1/SP2 create sync=0x5001 (L376/L382); SP2 resolved-address
  0x7f913407a8 (L383), maps path=/sysimg/system/lib64/libEGL.so (L384).
- IMAGE libEGL maps 1 (L384); RAMDISK libEGL maps 0 (anchored `path=` pattern
  — the T737 correction, not regressed).
- [T737] hooks 9 (L346, 348, 349, 351, 353, 354, 390, 393, 394); [T738] hooks 7
  (L542, 586, 587, 590, 591, 592, 593).
- T734_KERNEL_OK 1 (L194, guest 5.204 s) + [T734] probe finished rc=0 (L196);
  T720_PROBE DONE status=PASS 1 (L226).
- T733_PROBE_OK 0, T728_PROBE_OK 0, T733_MINIMAL_OK 0, sys.boot_completed 0,
  boot_completed (all) 0, T721 fence-log 0, simplefb 0, dup-fail 0,
  FATAL EXCEPTION 0, EPERM 0, avc: denied (two spaces) 0 — all VOID (the
  guest never reached Android init; the handoff was aborted at 16.479 s),
  not failed.
- Kernel panic/Oops/BUG 1 (L594 — the DESIGNED abort panic, see Mechanism §3,
  not a guest fault); "inaccessible or not found" 0.
- Linker lines 284 in the UART; 60 mention /sysimg/system/lib64/, 132 mention
  /system/lib64/ (the rest are the missing-linkerconfig + readlink-failed lines).

## 1. The probe ran — dispatch answered a third time (SP1/SP2/SP3)

- SP1: load_stub rc=0 base=0x7cf43a9000 size=52416; create=base+0x5c84,
  dup=base+**0x5cb0** (the T736 stub address, reproduced); create sync=0x5001;
  dup rc=-1.
- SP2: eglGetDisplay=0x3141; eglInitialize ok=1 (EGL 1.4) error=0x3000; create
  sync=0x5001; getprocaddress dup=0x7f913407a8; map path=
  /sysimg/system/lib64/libEGL.so; dup rc=-1 error=0x3000.
- SP3: open timeline fd=**-13** (EACCES, uid 1000), control_fd=-999.
- The T736 answer stands, third consecutive flight: dispatch FORWARDS to the
  stub at base+0x5cb0; dup -1 is the sw_sync trampoline failing to open the
  timeline as uid 1000, not a dispatch failure.

## 2. The unmount failed — first failing link

- `[T738] unmount /sysimg (vda) FAILED rc=1` (L542). This is the first failing
  link. Everything after it is the designed consequence, not a second fault.
- `[T738] unmount /vendor (loop1) rc=0` (L586) and `[T738] unmount debugfs
  (/sys/kernel/debug) rc=0` (L587) both SUCCEEDED — only /sysimg failed.
- The verification worked exactly as designed: the /proc/mounts grep printed
  `/dev/vda /sysimg ext4 ro,relatime 0 0` (L589), `[T738] handoff state DIRTY`
  (L590), `[T738] handoff state verified: /data mounted` (L591),
  `[T738] unmount proc (verify) rc=0` (L592), `[T738] HANDOFF ABORTED` (L593).
- The script exited 1; the kernel panicked at guest 16.479479 s (L594):
  "Kernel panic - not syncing: Attempted to kill init! exitcode=0x00000100",
  PID 1 Comm: early-ramdisk. QEMU exit 0, host 25.03 s.

## 3. Mechanism — the unmount tool maps libraries from the mount it unmounts (PROVEN)

- The early-ramdisk script EXPORTS the library path (line 90):
  `export LD_LIBRARY_PATH=/sysimg/system/lib64:/sysimg/lib64`. The probe line
  (92) also sets it inline (redundant given the export). Every child of the
  script — the backgrounded /t493-probe.sh and /t501-keystore2-dump.sh, their
  toybox children, and the unmount block's own toybox calls — inherits the
  /sysimg-first search path.
- The unmount tool is toybox (`"$TOYBOX" umount /sysimg`). Toybox's NEEDED set
  (readelf on the CPIO's system/bin/toybox, 560,720 B): libcrypto.so,
  libz.so, liblog.so, libprocessgroup.so, libselinux.so, libc.so, libm.so,
  libdl.so.
- The UART PROVES those libraries load from /sysimg: 60 linker lines name
  /sysimg/system/lib64/ — libcrypto, libz, liblog, libprocessgroup, libselinux
  (plus libc++, libbase, libcgrouprc, libpcre2, libpackagelistparser,
  libnetd_client as further libraries from the same directory) — while libc,
  libm, libdl load from /system/lib64/ (the ramdisk's own lib64, 132 lines).
  The "readlink(/proc/self/fd/N) failed" half of each warning is the /proc
  unmount (L393–394): the linker cannot resolve paths while /proc is down.
- A process cannot unmount a filesystem on which it has files mapped: the
  kernel refuses the umount (EBUSY) while the mappings are held. Toybox
  maps five libraries from /sysimg the moment it starts — so `toybox umount
  /sysimg` is SELF-BLOCKED: the tool is its own blocker. The backgrounded
  scripts' toybox sleeps (`toybox sleep 30` in t493, `toybox sleep 1` /
  dmesg|grep in t501) compound it — a live toybox holds the mappings at any
  moment — but they are NOT needed to explain the failure: even with the
  scripts killed, the umount process itself maps /sysimg libraries.
- Why /vendor and debugfs succeeded: no live process maps anything from them
  (the t720-probe had exited; /loop-node-keeper is STATIC — "[T431] static
  loop-node keeper started", L500 — and holds only device nodes).
- Why the export matters and the inline probe setting does not: the probe
  needs LD_LIBRARY_PATH=/sysimg/... (its SP2 must resolve the IMAGE libEGL),
  but the export gives it to EVERYTHING. The ramdisk's own /system/lib64/
  contains THIRTEEN of the fourteen libraries the warnings name (verified in
  the CPIO member list: libcrypto, libz, liblog, libprocessgroup, libselinux,
  libc, libm, libdl, libc++, libbase, libcgrouprc, libpcre2,
  libpackagelistparser). libnetd_client.so is ABSENT from the ramdisk's
  /system/lib64/. Toybox's eight DIRECT libraries are all present, so toybox
  starts fine on the ramdisk path — but libnetd_client is a transitive
  dependency of one of them, and the successor must verify where it resolves
  before assuming the scoped-path fix works (see §4).
- Exonerated: /system/bin/sh (NEEDED = libc.so only — the shells map nothing
  from /sysimg); /system/bin/getprop and /system/bin/logcat (ABSENT from the
  CPIO — the scripts' `[ -x ... ]` tests fail, so neither ever runs).

## 4. Wall: INIT-UMOUNT-SELF-BLOCKED (guest, NEW in T738)

- Type: guest. The failure is in the guest's early-ramdisk environment: the
  exported LD_LIBRARY_PATH + toybox's NEEDED set + the kernel's EBUSY on
  mapped files. Not QEMU, not the DTB, not the CPIO, not the probe.
- WALL-TYPE-TAG: guest.
- Rust par bhi lagu: haan. Any emulator whose ramdisk pre-mounts an image and
  then tries to unmount it with a dynamically-linked tool whose libraries
  resolve onto that mount hits the same EBUSY. The fix direction — scope the
  library path to the probe, or unmount with a static tool — carries to any
  initramfs design, Rust or not.
- The T737 wall INIT-SYSTEM-MOUNT-LOOP0-UNBOUND is NOT closed: the handoff
  state was never restored (/sysimg stayed mounted), so init never got the
  chance to mount /system from vda. The T738 route (unmount before handoff)
  is REFUTED in its present form; the wall itself is still open.
- Successor recommendation (T739): SCOPE LD_LIBRARY_PATH to the probe line
  only — remove the line-90 export (the probe line already sets it inline) —
  so every other toybox invocation loads from the ramdisk's /system/lib64/
  and maps nothing from /sysimg; then re-fly service-flight-14. CAVEAT: the
  ramdisk's /system/lib64/ holds 13 of the 14 libraries the warnings name —
  libnetd_client.so is absent — so the successor must FIRST verify the
  transitive closure on the ramdisk path (readelf the ramdisk's
  libprocessgroup/liblog/libselinux NEEDED; check where libnetd_client
  resolves, e.g. from /vendor/lib64/) and confirm toybox still starts and
  the /vendor unmount still succeeds. If the closure does not resolve
  cleanly on the ramdisk path, use the robust alternative instead: a STATIC
  umount binary (like /loop-node-keeper, which is static). Also: do NOT
  redirect the umount stderr to /dev/null — capture it so the next failure
  names its errno (see §6 gap 1).

## 5. Reflection lesson (binding on successors)

- The unmount tool's own library mappings are part of the handoff device
  state. A mount cannot be unmounted by a tool that maps files from it.
  Generalizes the T736 lesson (every mount gets a justification + scoping)
  and the T737 lesson (the handoff device state is part of the boot
  contract): every ENVIRONMENT VARIABLE that redirects library loading is
  part of the boot contract too. An `export` in the early-ramdisk script
  poisons every child; scope library paths the way mounts are scoped —
  set them on the one command that needs them, never exported for the
  whole script. And the tool that undoes a mount must not depend on the
  mount: verify the umount tool's NEEDED set against the mount it undoes
  before the flight, the way the EMBEDDED-VENDOR RULE verifies the artifact
  the guest consumes.

## 6. Honest gaps (do not change the verdict)

1. The exact errno of the failed umount is NOT on the UART — the script
   redirects umount stderr to /dev/null, so only rc=1 is recorded. EBUSY is
   the mechanism best supported by the evidence (live mapped libraries from
   /sysimg + the kernel's umount semantics + /vendor and debugfs unmounting
   cleanly from the same script), but the errno itself was not observed.
   Successor fix: capture the umount stderr on the UART.
2. The six further libraries observed loading from /sysimg (libc++, libbase,
   libcgrouprc, libpcre2, libpackagelistparser, libnetd_client) were not
   individually readelf-verified as transitive dependencies — the box
   transport was down at ~03:47–03:50 AEDT (three consecutive nc proxy
   broken-pipe failures; the Mac transport failed once in the same window,
   sandbox egress-proxy outage, both transports had worked minutes earlier).
   The five DIRECT libraries (libcrypto, libz, liblog, libprocessgroup,
   libselinux) ARE readelf-verified as toybox NEEDED and observed loading
   from /sysimg — any one of them alone blocks the unmount, so this gap does
   not weaken the mechanism.
3. The full 850 s flight did not run — all post-handoff holds are VOID (see
   Counts). No timeline-access fix route was tested (Step 4 not reached).
4. Box-side evidence sync is pending on the transport outage (§6.2): the
   Mac worktree + sandbox copies below are complete; the driver will need to
   retry the box sync at certification.

## Evidence (SHA-verified, sandbox + Mac worktree)

- findings-t738.md (this file); outcome.txt (canonical FAIL); death note
  (see DEATH_NOTE-T738.md).
- Flight service-flight-13 (Mac worktree flights/service-flight-13/; sandbox
  mirror t738-scratch/flights/service-flight-13/): uart.log 8796fb47
  (46,953 B, 613 lines), analysis-t738.txt (SHA below), flight-end.json
  9a381925, preflight.json 71a3bd5e, qemu-command.txt 87bbf0dd,
  qemu-stderr.log e3b0c442 (0 B), uart.log.gz 3eca8bc7, uart.log.numbered.txt
  a91339ad, supervisor.log (supervisor-flight13.log, Mac worktree root —
  to be copied into the flight folder when the transport recovers).
- Build (box t738/ canonical; Mac worktree verified): early-ramdisk-t738
  7f4525a9 (17,247 B), t738-fix.cpio.gz 5d7affd5 (53,749,194 B),
  t738-modified.dtb 4be60332 (2,049 B, initrd-end 0x1133425ca),
  rebuild_cpio_t738.py 07f3a513, patch_dtb_initrd.py 1f896a01,
  scripts/supervisor_t738.py 800d4746, scripts/analyze_t738_probe.py
  ea5923ee. FLOWN-CPIO members verified: early-ramdisk 7f4525a9,
  ramdisk-images/vendor.img 5e0607ad, t723-probe 4b275e48, t734probe b7440e1c,
  system/bin/linker64 ffd6af09.
- Progress: progress-t738.txt, TIMELINE.md, heartbeat.json (Mac worktree +
  sandbox; Step 2 exit + flight outcome recorded).

## Adjudication (brief PASS contract — conjunctive)

- Full 850 s flight runs: NO (guest 16.485 s, aborted by design).
- SP1/SP2 answered from the booted guest with the IMAGE libEGL: NO (answered
  from the early-ramdisk context only, as in T736/T737; the guest never booted).
- [T738] unmount hooks all rc=0 + handoff state verified: NO (/sysimg FAILED
  rc=1, state DIRTY, HANDOFF ABORTED).
- Timeline-access fix route TESTED: NO (Step 4 not reached).
- Verdict: FAIL. Successor: T739 (scope LD_LIBRARY_PATH to the probe line;
  re-fly).
