# DEATH NOTE — Track T737

Verdict: FAIL. service-flight-12 ended at guest 11.965 s: first-stage init
failed to mount /system and rebooted to the bootloader.

Wall: INIT-SYSTEM-MOUNT-LOOP0-UNBOUND (guest, NEW in T737). Init attempted
/system from /dev/block/loop0 — a loop device with no backing file attached
(the script attaches only loop1=vendor, loop2=userdata) — instead of its
fstab device /dev/block/vda, because vda was already mounted at /sysimg by
the early-ramdisk script at the handoff. Superblock read failed (I/O error,
dev loop0, sector 2), mount failed EINVAL, init rebooted to bootloader.

What died: the full 850 s flight (guest 11.965 s, not 850 s); the
surfaceflinger-context confirmation of dup -1; the timeline-access fix-route
test. Nothing else died: the probe RAN (27 T723_PROBE, SP1/SP2/SP3 DONE),
the LD_LIBRARY_PATH mechanism is PROVEN (L383: image libEGL loaded), the
/proc scoping is PROVEN (L392–393 + clean first-stage start), T720/T734
controls HELD.

Successor (T738): unmount the probe's mounts (/sysimg, /vendor, debugfs)
after the probe exec, before `exec /init` — restoring exactly T734's
handoff device state, which is proven to boot. Alternative: losetup vda
onto loop0 read-only. Then re-fly the full 850 s flight and test the
timeline-access fix route.

Reflection: see findings-t737.md § Reflection. The defect class is the
same as T735/T736: a mount added for the probe was left mounted at the
handoff without asking what init expects there. The fix is mechanical from
here: every probe mount gets a matching unmount, and the handoff device
state is diffed against the last booting flight at the Step 2 exit.
