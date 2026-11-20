# DEATH NOTE — Track T735 (recovery 1)

## What died

service-flight-10, and with it the T735 probe flight. The guest kernel-panicked at guest
4.761032 s ("Kernel panic - not syncing: Attempted to kill init! exitcode=0x00007e00"),
15.02 s after QEMU started. The T723 EGL-dispatch probe never ran (0 T723_PROBE lines).
Verdict: FAIL — the dispatch question (does RenderEngine's eglDupNativeFenceFDANDROID
reach the driver stub at 0x5cb0?) is unanswered.

## When

Flight 00:15:13–00:15:28 AEDT 21 Nov 2026. Adjudicated 00:35–00:55 AEDT 21 Nov 2026
(lane agent 5a34039c, recovery 1; brief a73acc92; wedge 03:12 AEDT 21 Nov).

## Why

The early-ramdisk script mounted the system image onto /system (UART L345, rc=0). That
mount replaced the ramdisk's real /system/bin/linker64 (2,052,848 B) with the image's
symlink → /apex/com.android.runtime/bin/linker64, a path that does not exist pre-init
(no /apex in the ramdisk CPIO; apexd never ran; the image's own /apex is an empty dir at
the image root, not exposed by the mount). Every dynamically-linked exec after the mount
failed: /vendor, debugfs and proc mounts rc=127 (toybox inaccessible), /t723-probe ELOOP
rc=126, /t493-probe.sh and /t501-keystore2-dump.sh ELOOP, /init ELOOP → panic. A static
binary (/loop-node-keeper) still ran, proving the failure is dynamic-linker resolution,
not exec in general. Wall: RAMDISK-MOUNT-BREAKS-LINKER (guest).

First failing link: `[T735] mount /vendor (loop1) FAILED rc=127` (UART L347).
Root cause: `[T735] mount /system (vda) rc=0` (UART L345).

## What survives

- The T734 kernel-init exec path: T734_KERNEL_OK fired (L194, guest 3.96 s). Not the
  exec path, not the probe binary, not the CPIO unpack.
- The T720 fence source: T720_PROBE DONE status=PASS (L224).
- The full build, SHA-verified: CPIO b183380d (53,745,361 B), DTB eb86dc37 (initrd-end
  0x1133416d1 asserted), early-ramdisk b6301e09, supervisor 7110033a, analyzer b90b2bb2,
  rebuilder 9b3bf0ec.
- The flight evidence: flights/service-flight-10/ (UART b0dbacf6, 395 lines) + analysis.
- The mechanism proof: box debugfs (image linker64 symlink, empty image /apex),
  box CPIO listing (no apex), readelf INTERP/NEEDED, sandbox control test
  (dangling INTERP → ENOENT; loop → ELOOP; real file → runs).

## Successor

Do NOT re-fly the mount-onto-/system route — it is refuted. Recommended: Option A —
mount the system image at a scratch point (/sysimg, not /system) + LD_LIBRARY_PATH for
the probe exec, keeping the ramdisk's real linker64 live. Alternatives: Option B —
extract libEGL.so + all 18 NEEDED deps into the CPIO; Option C — SP1-only, /vendor mount
only, SP2 deferred with justification. See findings-t735.md "Successor direction".

## Reflection

Same blind-spot class as T723's RAMDISK-MOUNT-BLIND: a mount validated only for library
visibility, never for the INTERP chain. The missing check was "after the mount, can a
dynamically-linked binary still exec?" — answerable only by the flight. Future
mount-based routes must name each binary's INTERP and verify it resolves from the mounted
state before flying, and must prefer a scratch mountpoint over overmounting /system.
The ELOOP-vs-ENOENT errno discrepancy (probe ELOOP, toybox ENOENT, same INTERP) is an
explicitly unresolved detail — see findings-t735.md Mechanism §5 — and does not change
the verdict or the successor direction.
