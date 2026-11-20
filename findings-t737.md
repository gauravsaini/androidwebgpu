# Findings — Track T737 (full 850 s flight of the T723 probe)

## Verdict

Verdict: FAIL

The full 850 s flight did NOT run: service-flight-12 ended at guest 11.965 s,
when first-stage init failed to mount /system and rebooted to the bootloader.
The dispatch question is ANSWERED again (SP1/SP2 both dup -1, now against the
IMAGE's libEGL.so), and both T736 gaps are CLOSED with guest-origin proof
below — but the brief's PASS contract is conjunctive (full flight AND dup -1
confirmed in the surfaceflinger context AND a timeline-access fix route
TESTED), and the flight never reached the surfaceflinger context, so no fix
route could be tested. The failure is a NEW guest wall, not the probe and not
dispatch.

## WALL-TYPE-TAG

WALL-TYPE-TAG: INIT-SYSTEM-MOUNT-LOOP0-UNBOUND (guest)

## Rust par bhi lagu

Rust par bhi lagu: haan. The wall is in the guest's first-stage init mount
path, not in QEMU: any emulator (QEMU or the Rust emulator) whose pre-init
script leaves the system block device mounted at the handoff will hit the
same fallback — init tries /dev/block/loop0 for /system, finds no backing
file, and reboots to the bootloader. The lesson carries directly: the handoff
device state is part of the boot contract, and the fix (unmount the probe's
mounts before the handoff) applies to both engines.

## Dispatch answer

ANSWERED — against the IMAGE's libEGL.so this time (see "What this lane
closed"). SP1 (direct driver call) dup rc=-1 (L376); SP2 (META-EGL dispatch
call) dup rc=-1 (L384); SP3 (sw_sync control) open timeline fd=-13 (L371).
The resolved function lives in the image's file (L383, see below). Dispatch
forwards; the dup still fails because the sw_sync trampoline cannot open the
timeline as uid 1000. The fix point remains timeline access, NOT dispatch.
No dispatch fix is owed.

## What this lane closed (the two T736 gaps)

1. **LD_LIBRARY_PATH mechanism — RESOLVED and PROVEN by the flight.**
   The T736 gap (SP2 loaded the ramdisk's libEGL, mechanism unexplained) is
   closed: the image's lib64 is at `/sysimg/system/lib64`, not
   `/sysimg/lib64` (the T736 path does not exist in the mounted image). The
   T737 script sets `LD_LIBRARY_PATH=/sysimg/system/lib64:/sysimg/lib64`
   (export + exec prefix). Proof, guest-origin, flight-12 L383:
   `T723_PROBE SP2: map addr=0x7f7e2bc7a8 start=0x7f7e2b3000
   path=/sysimg/system/lib64/libEGL.so`. The analyzer counts IMAGE libEGL 1,
   RAMDISK libEGL 0 (corrected run; see Variances).
2. **/proc scoping — PROVEN by the flight.** The T736 wall
   INIT-PROC-MOUNT-CONFLICT is CLOSED: the script mounts /proc for the
   probe's maps_lookup only, then unmounts it before the handoff.
   Proof: `[T737] unmount proc (/proc) rc=0` (L392) and `[T737] proc
   unmounted: /proc/self/maps unreadable` (L393). Init first stage started
   cleanly afterwards (L594, guest 11.374 s) with NO "Device or resource
   busy" proc abort — the T736 failure line is absent from this UART.

## Flight

service-flight-12 — 02:45:15Z to 02:45:30Z (02:45:15–02:45:30 AEDT 21 Nov 2026).
Host 15.02 s, guest 11.965 s, QEMU exit code 0, stop reason "QEMU process exited
with returncode 0". UART 607 lines, 47,331 B, SHA
64f535ee639da35c0cc43d9da02be92027e53b7ef8485618e7bfa7a7247b4052. The guest
rebooted to the bootloader at guest 11.965 s; the flight ended there, not at
the 850 s guest target (the supervisor's target was 850 s; QEMU exited on
the guest reboot — this is the flight's outcome, not a variance from the brief).

Inputs (all SHA-verified before launch; DTB initrd-end re-asserted
0x113341dd4 = 0x110000000 + 53,747,156; the assertion transfers to the Mac
files by identical SHAs):
- kernel Image 70d3859d (20,396,544 B)
- QEMU 8e5c46dc (31,498,864 B)
- DTB c075b453 (2,049 B)
- CPIO 266deb91 (53,747,156 B)
- system.img af40403a (2,059,816,960 B)
- vendor.img 5e0607ad (134,217,728 B)
- supervisor scripts/supervisor_t737.py eed00206; analyzer
  scripts/analyze_t737_probe.py 25741448 (corrected re-run, see Variances);
  rebuilder rebuild_cpio_t737.py 9e493a57; early-ramdisk 32b1430c;
  patcher patch_dtb_initrd.py 1f896a01; rc 79ad3cc3 (parser citation:
  VERDICT ACCEPT, unchanged from T734)
- Disk gate OPEN at launch: 15,975,984 KiB free (93%) at the supervisor
  start (02:45 AEDT 21 Nov), gate 9,769,216 KiB. The gate was CLOSED at lane
  launch (800,496 KiB free) and reopened on its own ~02:35 AEDT 21 Nov.

## Counts (analyzer: flights/service-flight-12/analysis-t737.txt, exit 0)

- T723_PROBE lines: 27 (start 2, DONE 1, SP1 DONE 1, SP2 DONE 1, SP3 DONE 1,
  DROP_FAIL 0, RESOLVE_FAIL 0, GETPROCADDRESS_NULL 0).
- SP1 dup rc=-1 (L376); SP2 dup rc=-1 (L384); SP2 resolved-address
  0x7f7e2bc7a8 (L382); SP2 maps path=/sysimg/system/lib64/libEGL.so (L383);
  SP1/SP2 create sync=0x5001 (L375/L381); SP3 open timeline fd=-13 (L371).
- [T737] hook lines: 10 — mount /sysimg (vda) rc=0 (L345); INTERP tripwire
  (L347–348); mount /vendor (loop1) rc=0 (L350); mount debugfs
  (/sys/kernel/debug) rc=0 (L352); mount proc (/proc) rc=0 (L353);
  t723-probe finished rc=0 (L389); unmount proc (/proc) rc=0 (L392); proc
  unmounted (L393); handoff (L500).
- T734_KERNEL_OK: 1 (UART L194, guest 3.909 s). [T734] kernel-init probe
  finished rc=0 (L196). The exec-path control FIRED.
- T720_PROBE DONE status=PASS: 1 (L228).
- T733_PROBE_OK: 0. T728_PROBE_OK: 0. T733_MINIMAL_OK: 0.
- sys.boot_completed: 0. T721 fence-log: 0. simplefb: 0. dup-fail: 0
  (rate 0.000/1000 s — meaningless: the guest died at 11.97 s before any
  presenting; reported as void, not a pass).
- FATAL EXCEPTION: 0. EPERM: 0. avc: denied: 0. Watchdog KILLING: 0.
- "inaccessible or not found": 0. ELOOP: 0. Kernel panic/Oops/BUG: 0.
- IMAGE libEGL maps lines: 1 (L383). RAMDISK libEGL maps lines: 0.

## Holds

- T720 fence source: HELD (T720_PROBE DONE status=PASS 1, L228).
- T734 exec-path control: HELD (T734_KERNEL_OK 1, guest 3.91 s).
- T721 fence-log, simplefb, sys.boot_completed, dup-fail rate: VOID, not
  failed. The guest died at 11.97 s, before Android init mounted /system,
  so none of them could fire. sys.boot_completed is 0 because the guest
  never booted — this is a consequence of the mount failure, not a
  boot-chain failure. TRIGGER-FIRED-FIRST: no trigger fired (no Android
  init past first stage), so no leg-2 claim is made.

## Exact first failing link

First failing link: `[libfs_mgr] Can't read '/dev/block/loop0' superblock:
File exists` (UART L601, guest 11.830918 s).

Root cause: `[libfs_mgr] __mount(source=/dev/block/loop0,target=/system,
type=ext4)=-1: Invalid argument` (UART L604) → `init: Failed to mount
/system: Invalid argument` (L605) → `reboot: Restarting system with command
'bootloader'` (L607, guest 11.965271 s).

## Mechanism (proven facts, then the best-supported hypothesis)

Proven facts:

1. fstab.virt — byte-identical in the T734 and T737 CPIOs (the T737
   rebuilder verified every non-early-ramdisk entry byte-identical) — maps
   /system to /dev/block/vda:
   `/dev/block/vda /system ext4 ro,barrier=1 wait,first_stage_mount`.
   Its header comment says: "Diskless Android boot: early-ramdisk attaches
   loop0/loop1 before /init. First-stage init recreates block nodes under
   /dev/block from sysfs."
2. T734 flight-9 mounted /system from vda: `[libfs_mgr]
   __mount(source=/dev/block/vda,target=/system,type=ext4)=0: Success`
   (guest 20.346 s), then `Switching root to '/system'`. The T734 guest
   booted the full 850 s.
3. T737 flight-12's init attempted /system from /dev/block/loop0 instead,
   although the fstab, the DTB (identical except initrd-end, patcher-verified
   at every patch in the chain), the kernel, and the system image are all
   identical to T734's. Both flights' init printed the same lines through
   "DSU not detected, proceeding with normal boot" — the divergence starts
   at the superblock read.
4. loop0 has no backing file in T737: the script attaches loop1=vendor.img
   and loop2=userdata.img only (T705 block, "loop0 path removed"); the
   [T431] keeper only creates the /dev/block/loop0 NODE (block 7:0), it
   attaches nothing. An unbound loop device's superblock read fails:
   `print_req_error: I/O error, dev loop0, sector 2` (L602),
   `EXT4-fs (loop0): unable to read superblock` (L603).
5. The ONLY differing input between T734's booting flight and T737's
   aborting flight is the early-ramdisk script's handoff device state —
   proven by diffing the T734 early-ramdisk (13,867 B, extracted from the
   box T734 CPIO) against the T737 script: T734 mounted NOTHING at the
   handoff except /data + /data/media (loop2/ram0/tmpfs); T737 left vda
   MOUNTED at /sysimg, loop1 MOUNTED at /vendor, and debugfs MOUNTED at
   /sys/kernel/debug — the probe's mounts, all added in T736 and kept in
   T737.

Best-supported mechanism (hypothesis, NOT kernel-proven): with its fstab
device vda already mounted at /sysimg, first-stage init's mount path did
not use vda for /system and fell back to /dev/block/loop0 — the
diskless-boot fallback device the fstab header comment anticipates
("early-ramdisk attaches loop0/loop1 before /init"). loop0 is unbound, so
the superblock read fails with an I/O error, the mount fails with EINVAL,
and first-stage init treats a /system mount failure as fatal (reboot to
bootloader). What IS proven is the discrimination (facts 1–5); the internal
libfs_mgr device-selection step is not proven from the UART alone and is
recorded as a hypothesis for the successor to confirm or refute.

## Step 4 adjudication

- Full 850 s flight: NOT run (guest 11.965 s).
- dup -1 in the surfaceflinger context: NOT confirmed. The guest never
  booted and system_server never ran, so the surfaceflinger context is
  un-runnable in this flight. The route that would run it: the T738
  unmount fix below → the guest boots → surfaceflinger runs → the
  dup-fail rate is measurable against the T721 baselines.
- Timeline-access fix route: NOT tested — there is no booted guest to test
  in. The two candidate routes from the brief (open the timeline as root
  and hand the fd down; relax the debugfs file's permissions) are unchanged;
  they need the T738 boot first.

## Successor direction (for the driver)

T738: unmount the probe's mounts after the probe exec, before `exec /init`
— /sysimg (vda), /vendor (loop1), debugfs — mirroring the /proc scoping
pattern that already works in this lane (mount → probe → unmount). That
restores EXACTLY T734's handoff device state (only /data + /data/media
mounted), which is proven to boot. Alternative, only if the unmount route
fails: attach vda to loop0 read-only via `losetup -r /dev/loop0 /dev/vda`
before the handoff, so init's loop0 attempt finds the system image. Do NOT
re-run the T734 bisect. Do NOT rebuild the kernel. The T737 build (CPIO
266deb91 / DTB c075b453 / supervisor eed00206 / analyzer 25741448) is
reusable except for the early-ramdisk change + CPIO rebuild + DTB re-patch.

## Variances from the brief

1. The flight ended at guest 11.965 s (guest reboot to bootloader), not the
   850 s guest target — the supervisor's target was 850 s; QEMU exited on
   the guest reboot. This is the flight's outcome, not a deviation.
2. Analyzer correction: the first analysis run double-counted the SP2 maps
   line — the RAMDISK pattern `/system/lib64/libEGL\.so` matched inside
   `/sysimg/system/lib64/libEGL.so` (L383 counted for both). The pattern
   was anchored to `path=/system/lib64/libEGL\.so` and the analysis re-run;
   the filed analysis-t737.txt is the corrected run (IMAGE 1, RAMDISK 0).
   The analyzer SHA changed ea9df2fb → 25741448; the corrected copy is
   synced to the Mac worktree scripts/ and the box.
3. Disk gate: CLOSED at lane launch (800,496 KiB free), OPEN at flight
   launch (15,975,984 KiB free at 02:45 AEDT 21 Nov — reopened on its own
   ~02:35). The supervisor's own 8 GiB gate also passed at launch.
4. The sandbox CPIO mirror was skipped (sandbox quota, rsync exit 122) —
   the box copy (canonical, SHA 266deb91) and the Mac worktree copy
   (SHA-verified) stand; the driver certifies from those.

## Reflection (FAIL)

Root cause: the T736 lane added the probe's mounts (/sysimg, /vendor,
debugfs) to REACH the probe and never asked what init would do with them
still mounted at the handoff — the same blind-spot class as T735 (a mount's
second-order effect on the next stage: the /system mount broke the linker)
and T736's own /proc mount (the mount broke init's own mount). The missing
check was "what device state does init expect at the handoff?" — answerable
from the T734 flight (vda unmounted, loop1 attached-but-unmounted) and the
fstab header comment ("early-ramdisk attaches loop0/loop1 before /init").
The /proc fix in this lane proved the pattern that would have caught it:
scope every mount to the probe (mount → probe → unmount), and the handoff
device state is now part of the Step 2 exit checklist, verified against the
last booting flight's state.

What changes: (1) every mount the script adds for the probe gets a matching
unmount before the handoff — the /proc pattern, generalized to /sysimg,
/vendor, debugfs; (2) the Step 2 exit records the handoff device state
explicitly and diffs it against the last booting flight (T734: only /data +
/data/media mounted); (3) the analyzer's maps counters are anchored on
`path=` (this lane's correction) so the IMAGE-vs-RAMDISK counts cannot
double-count again.

What survives: the T737 build (CPIO 266deb91 / DTB c075b453 /
early-ramdisk 32b1430c / supervisor eed00206 / analyzer 25741448), the
flight evidence (service-flight-12, UART 64f535ee), the two T736-gap
closures (LD_LIBRARY_PATH mechanism proven by L383; /proc scoping proven
by L392–393 + clean first-stage start), and the full mechanism record
above. Nothing needs rebuilding for the successor except the early-ramdisk
change (unmounts) + CPIO rebuild + DTB re-patch.

## Evidence

- Flight: flights/service-flight-12/ (flight-end.json, preflight.json,
  qemu-command.txt, qemu-stderr.log 0 B, supervisor.log, uart.log 607
  lines SHA 64f535ee, uart.log.gz, uart.log.numbered.txt,
  analysis-t737.txt).
- Build: CPIO 266deb91, DTB c075b453, early-ramdisk 32b1430c, supervisor
  eed00206, analyzer 25741448, rebuilder 9e493a57, patcher 1f896a01 — all
  in the worktree, sandbox ~/workspace/t737-scratch/, and box
  /mnt/sdc3/muse/kernel-primary/t737/ (the sandbox CPIO mirror was skipped
  on quota; the box copy is canonical).
- Mechanism evidence (gathered in this lane, not in the flight): box
  debugfs stat/dump of the image's libEGL.so (fed36a9b) + image-root
  layout (system/ subdir, no root lib64) + CPIO linkerconfig empty dir;
  T734 flight-9 UART (sandbox, /system mounted from vda L591); T734
  early-ramdisk extracted from the box T734 CPIO (13,867 B, no
  /vendor-/sysimg-/debugfs-/proc mounts); fstab.virt from the box T734
  CPIO unpack (vda → /system, loop1 → /vendor, header comment naming the
  loop0/loop1 diskless-boot design).
