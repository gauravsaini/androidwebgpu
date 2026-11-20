# Findings — Track T735 (re-fly T723 EGL-dispatch probe via kernel-init exec)

## Verdict

Verdict: FAIL

The dispatch question is UNANSWERED. The probe never ran: 0 T723_PROBE lines in
service-flight-10. SP1 (direct driver call) and SP2 (META-EGL dispatch call) are both
unanswered, so no dispatch point is identified and no fix was built. The mount-onto-/system
route the first lane designed is REFUTED by this flight.

## WALL-TYPE-TAG

WALL-TYPE-TAG: RAMDISK-MOUNT-BREAKS-LINKER (guest)

## Rust par bhi lagu

Rust par bhi lagu: haan. The wall is in the guest's APEX filesystem layout, not in QEMU:
the flown system image's /system/bin/linker64 is a symlink into /apex, which does not exist
before Android init runs apexd. Any emulator (QEMU or the Rust emulator) that mounts this
system image onto /system from the pre-init ramdisk context breaks the dynamic linker the
same way. The fix routes named below apply to both engines.

## Dispatch answer

UNANSWERED. The probe (t723-probe, 68,792 B, SHA 4b275e48) never printed a single line.
No SP1 direct-call result, no SP2 META-EGL result, no resolved address, no dup rc, no
DROP_FAIL / RESOLVE_FAIL / GETPROCADDRESS_NULL. The brief's Step 3a stop rule applies:
probe never ran → STOP, file FAIL findings naming the exact first failing link, do NOT run
the full flight, do NOT build a fix on a guessed dispatch point.

CALL-REACHABILITY (recorded before any patch work, as the brief requires): the probe can
NOT reach the dispatch chain from the ramdisk context via the mount-onto-/system route —
that route is refuted by this flight. The routes that would run it are named in
"Successor direction" below (Option A recommended).

## Flight

service-flight-10 — 00:15:13Z to 00:15:28Z (00:15:13–00:15:28 AEDT 21 Nov 2026).
Host 15.02 s, guest 4.765 s, QEMU exit code 0, stop reason "QEMU process exited with
returncode 0". UART 395 lines, 28,254 B, SHA b0dbacf6ad8acb07824ac0cf57635fa907246899d763a3929aa38ec9f80e0b32.
The guest kernel-panicked at guest 4.761032 s; the flight ended there, not at the 850 s
guest target (the supervisor's target was 850 s; QEMU exited early on the panic — this is
the flight's outcome, not a variance from the brief).

Inputs (all SHA-verified before launch; DTB initrd-end re-asserted 0x1133416d1 =
0x110000000 + 53,745,361):
- kernel Image 70d3859d (20,396,544 B)
- QEMU 8e5c46dc (31,498,864 B)
- DTB eb86dc37 (2,049 B)
- CPIO b183380d (53,745,361 B)
- system.img af40403a (2,059,816,960 B)
- vendor.img 5e0607ad (134,217,728 B)
- supervisor scripts/supervisor_t735.py 7110033a; analyzer scripts/analyze_t735_probe.py b90b2bb2;
  rebuilder rebuild_cpio_t735.py 9b3bf0ec; early-ramdisk b6301e09
- Disk gate OPEN at launch: 16,156,444 KiB free (93%), gate 9,769,216 KiB.

## Counts (analyzer: flights/service-flight-10/analysis-t735.txt, exit 0)

- T723_PROBE lines: 0 (start 0, DONE 0, SP1 DONE 0, SP2 DONE 0, SP3 DONE 0,
  DROP_FAIL 0, RESOLVE_FAIL 0, GETPROCADDRESS_NULL 0, dup rc 0, resolved-address 0).
- T734_KERNEL_OK: 1 (UART L194, guest 3.962264 s). [T734] kernel-init probe finished rc=0
  (L196). The exec-path control FIRED — the kernel-init exec path works; the failure is
  not in the exec path and not in the probe binary.
- [T735] hook lines: 6 — mount /system (vda) rc=0 (L345); mount /vendor (loop1) FAILED
  rc=127 (L347); mount debugfs FAILED rc=127 (L348); mount proc FAILED rc=127 (L349);
  t723-probe finished rc=126 (L351); kernel-init probe done; handing off to Android init
  (L372).
- ELOOP ("Too many symbolic links encountered"): /t723-probe (L350), /t493-probe.sh
  (L373), /t501-keystore2-dump.sh (L374), /init (L375).
- "/system/bin/toybox: inaccessible or not found": 4 lines (L353, 354, 355, 370).
- Kernel panic: 1 (L376, guest 4.761032 s — "Kernel panic - not syncing: Attempted to kill
  init! exitcode=0x00007e00", PID 1 Comm: early-ramdisk). Panic/Oops/BUG: 1.
- T720_PROBE DONE status=PASS: 1 (L224). T733_PROBE_OK: 0. T728_PROBE_OK: 0.
- sys.boot_completed: 0. T721 fence-log: 0. simplefb: 0. dup-fail: 0 (rate 0.000/1000 s —
  meaningless: the guest died at 4.76 s before any presenting; reported as void, not a pass).
- FATAL EXCEPTION: 0. EPERM: 0. avc: denied: 0. Watchdog KILLING: 0.

## Holds

- T720 fence source: HELD (T720_PROBE DONE status=PASS 1, L224 — the fence source still works).
- T734 exec-path control: HELD (T734_KERNEL_OK 1, guest 3.96 s).
- T721 fence-log, simplefb, sys.boot_completed, dup-fail rate: VOID, not failed. The guest
  died at 4.76 s, before Android init ever ran, so none of them could fire. sys.boot_completed
  is 0 because the guest never booted — this is a consequence of the panic, not a boot-chain
  failure. TRIGGER-FIRED-FIRST: no trigger fired (no Android init), so no leg-2 claim is made.

## Exact first failing link

First failing link: `[T735] mount /vendor (loop1) FAILED rc=127` (UART L347) — the first
command after the successful /system mount that fails.

Root cause: `[T735] mount /system (vda) rc=0` (UART L345). Mounting the system image onto
/system replaces the ramdisk's working dynamic linker with the image's dangling symlink,
so every dynamically-linked exec after L345 fails.

## Mechanism (proven from evidence gathered in this lane)

1. The ramdisk's /system/bin/linker64 is a REAL file (2,052,848 B; CPIO listing from the box,
   fresh). Before the mount, dynamically-linked binaries run fine: the shell starts, the
   T734 probe finishes rc=0, the T720 probe passes, and four toybox invocations succeed
   (linker warnings UART L225–341 show toybox loading its libs from the ramdisk's
   /system/lib64).
2. The flown system image's /system/bin/linker64 is a SYMLINK, 38 bytes, fast-link dest
   "/apex/com.android.runtime/bin/linker64" (box debugfs stat, fresh). /apex does NOT exist
   in the ramdisk context: the T735 CPIO contains no apex entry at all (box cpio -t listing,
   fresh), the script never creates /apex (it creates only /data/misc/apexdata), and apexd
   has not run pre-init. The image's own /apex is an EMPTY directory at the image root
   (box debugfs, fresh) — it is not under /system, so mounting the image onto /system does
   not expose it; and /apex/com.android.runtime/bin does not exist in the image.
3. readelf on the CPIO members (fresh): toybox INTERP /system/bin/linker64 (NEEDED
   libcrypto/libz/liblog/libprocessgroup/libselinux/libc/libm/libdl); sh INTERP
   /system/bin/linker64 (NEEDED libc.so); t723-probe INTERP /system/bin/linker64 (NEEDED
   libEGL.so); /init INTERP /system/bin/bootstrap/linker64 (NEEDED libbase/libcrypto/
   libcutils/libext4_utils/libfs_mgr/liblog/libselinux/libc/libm/libdl and others).
   In the image, /system/bin/bootstrap/linker64 is a real file (2,053,192 B).
4. After the mount, every dynamically-linked exec fails (L347–349 rc=127, L350 probe
   rc=126, L373–375 ELOOP, panic L376). A STATIC binary still runs: /loop-node-keeper
   started (L371, "[T431] static loop-node keeper started"). This proves the failure is
   dynamic-linker (INTERP) resolution, not exec in general and not the mount itself.
5. CPIO unpack is NOT the cause: the script ran to the handoff line (L372), unlike the
   T734 flights-4–7 unpack panic. The DTB initrd-end assertion held (no DTB-INITRD-END-CAP
   recurrence).

Errno note (honest gap, does not change the verdict): the shell (mksh) reports the
post-mount failures as ELOOP for the probe/scripts/init but as ENOENT ("inaccessible or
not found") for toybox, although toybox and the probe share INTERP /system/bin/linker64.
A control test on the sandbox kernel shows a dangling INTERP symlink yields execve ENOENT
(errno 2), a self-loop INTERP yields ELOOP (errno 40), and a real-file INTERP runs. The
exact symlink cycle (or 4.19 binfmt_elf behaviour) producing ELOOP in the guest is NOT
proven from the UART alone — the guest died before any further introspection was possible.
What IS proven, and is sufficient to refute the route, is that the mount hides the working
linker64 behind a symlink whose target does not exist pre-init, and every dynamically-linked
exec after the mount fails.

## Successor direction (for the driver)

The mount-onto-/system route (first lane's Option 1) is REFUTED. Do not re-fly it. Three
candidate fix routes, in recommended order:

- A (RECOMMENDED): mount the system image at a scratch point (e.g. /sysimg, NOT /system)
  and set LD_LIBRARY_PATH=/sysimg/lib64 for the probe exec. The ramdisk's real linker64
  stays live (INTERP resolves), the probe loads the image's libEGL.so (fed36a9b) and its
  deps from /sysimg/lib64, and the /vendor mount then works because toybox stays
  accessible. Minimal change to the proven T734 shape.
- B: extract libEGL.so + all 18 of its NEEDED deps into the CPIO (the first lane found
  11/18 differ ramdisk vs image — extract ALL of them together so the set is consistent);
  no mounts at all; re-patch the DTB for the larger CPIO. Most hermetic.
- C (fallback): SP1-only from the ramdisk context — mount /vendor only (before any /system
  mount), run SP1 (direct driver call), defer SP2 with justification. Answers half the
  dispatch question.

No fix was built in this lane: the dispatch point is unidentified (probe never ran), and the
brief forbids building a fix on a guessed dispatch point. Step 3a STOP was applied.

## Variances from the brief

1. The flight ended at guest 4.765 s (kernel panic), not the 850 s guest target — the
   supervisor's target was 850 s; QEMU exited early on the panic. This is the flight's
   outcome under the brief's stop rule, not a deviation.
2. The analyzer's dup-fail rate (0.000/1000 s) is meaningless — the guest died before any
   presenting. Reported as void, not as a pass against the 4.058/6.660 baselines.
3. Disk gate: OPEN at launch (16,156,444 KiB free) but CLOSED again by lane end (7,125,120
   KiB free at 00:34 AEDT 21 Nov, below the 9,769,216 KiB gate — foreign disk drain, same
   pattern as 20 Nov evening). The driver must re-check the gate before any successor flight.
4. The ELOOP-vs-ENOENT errno discrepancy is an explicitly unresolved detail (see Mechanism
   §5). It does not change the verdict, the first failing link, or the successor direction.

## Reflection (FAIL)

Root cause: the first lane's design assumed mounting the system image onto /system would
make the image's libEGL.so visible to the probe while keeping the ramdisk's exec context
intact. That assumption was wrong in a way no static check caught: the mount also replaces
/system/bin/linker64 — the very interpreter every dynamically-linked binary needs — with
the image's APEX symlink, which dangles pre-init. The static checks (CPIO membership,
file sizes, DTB assertion, SHA verification) all passed; the failure only appears at exec
time, in the guest, after the mount. The lesson is that a mount-onto-/system design must
be validated against the INTERP chain, not just the library path: the check that was
missing was "after the mount, can a dynamically-linked binary still exec?" — answerable
only by the flight, which is exactly what the flight proved. The same blind spot class
as T723's RAMDISK-MOUNT-BLIND (the mount hides what the probe needs), now with the
mechanism named: the mount doesn't just blind the probe, it breaks the linker.

What changes: any future mount-based visibility route must (a) name the INTERP each
binary in the exec chain uses, (b) verify that INTERP resolves AFTER the mount, from the
mounted state, before the flight, and (c) prefer a scratch mountpoint (Option A) over
overmounting /system. The analyzer already treats the [T735] hook lines as decisive; a
future lane should also treat "toybox inaccessible" as the tripwire — the first rc=127
after a mount is the signal to stop, not to continue to the probe exec.

What survives: the T734 kernel-init exec path (T734_KERNEL_OK fired), the T720 fence
source (PASS), the full build (CPIO b183380d / DTB eb86dc37 / early-ramdisk b6301e09 /
supervisor 7110033a), the adapted analyzer (b90b2bb2), and the flight evidence
(service-flight-10, UART b0dbacf6). Nothing needs rebuilding for the successor — only the
mount point in the early-ramdisk script changes (Option A) or the CPIO gains the lib set
(Option B).

## Evidence

- Flight: flights/service-flight-10/ (flight-end.json, preflight.json, qemu-command.txt,
  qemu-stderr.log 0 B, supervisor.log, uart.log 395 lines SHA b0dbacf6, uart.log.gz,
  uart.log.numbered.txt, analysis-t735.txt).
- Build: CPIO b183380d, DTB eb86dc37, early-ramdisk b6301e09, supervisor 7110033a,
  analyzer b90b2bb2, rebuilder 9b3bf0ec — all in the worktree, sandbox
  ~/workspace/t735-scratch/, and box /mnt/sdc3/muse/kernel-primary/t735/.
- Mechanism evidence (gathered in this lane, not in the flight): box debugfs stat of the
  image's /system/bin/linker64 (symlink → /apex/com.android.runtime/bin/linker64), box
  debugfs listing of the image's empty root /apex and /system/apex .capex files, box CPIO
  listing (no apex entry), readelf INTERP/NEEDED of toybox/sh/init/t723-probe, sandbox
  control test (dangling INTERP → ENOENT; self-loop INTERP → ELOOP; real-file INTERP →
  runs). Raw notes in the lane's working files; the control-test binaries live in /tmp
  (ephemeral) and are not part of the deliverable set.
