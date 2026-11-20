# Death note — T738 (FAIL)

Flight service-flight-13 ended at guest 16.485 s (host 25.03 s, QEMU exit 0)
when the early-ramdisk script ABORTED the handoff by design: `umount /sysimg`
failed rc=1, the verification correctly reported the state DIRTY, and the
brief's STOP rule fired (exit 1 → kernel panic "Attempted to kill init").

First failing link: `[T738] unmount /sysimg (vda) FAILED rc=1` (UART L542).

Wall: INIT-UMOUNT-SELF-BLOCKED (guest, NEW in T738). The unmount tool is
toybox, and toybox maps five libraries from /sysimg/system/lib64/
(libcrypto, libz, liblog, libprocessgroup, libselinux — readelf-verified
NEEDED, observed loading from /sysimg in 60 linker lines) because the script
exports LD_LIBRARY_PATH=/sysimg/system/lib64:/sysimg/lib64 (line 90) to
every child. A process cannot unmount a filesystem it has files mapped from
— the tool is its own blocker. /vendor and debugfs unmounted cleanly (no
live mappings there); the verification printed the still-mounted
`/dev/vda /sysimg ext4 ro,relatime 0 0` (L589) and aborted correctly.

The dispatch answer stands, third consecutive flight: SP1/SP2 both dup -1
against the IMAGE libEGL (/sysimg/system/lib64/libEGL.so, L384); dispatch
forwards to the stub at base+0x5cb0; dup -1 is the sw_sync timeline EACCES
as uid 1000. The T737 wall INIT-SYSTEM-MOUNT-LOOP0-UNBOUND is NOT closed —
the handoff state was never restored, so init never tried /system from vda.

Reflection (binding): the unmount tool's own library mappings are part of
the handoff device state. Scope library paths the way mounts are scoped —
set them on the one command that needs them, never exported for the whole
script. Verify the umount tool's NEEDED set against the mount it undoes
before the flight.

Successor: T739 — scope LD_LIBRARY_PATH to the probe line only (remove the
line-90 export); FIRST verify the transitive closure on the ramdisk path
(libnetd_client.so is absent from the ramdisk's /system/lib64/ — 13 of 14
libraries present); if the closure does not resolve cleanly, use a static
umount binary. Capture the umount stderr on the UART next time.

Evidence: findings-t738.md, outcome.txt (FAIL), flights/service-flight-13/
(uart.log 8796fb47, 613 lines, 46,953 B).
