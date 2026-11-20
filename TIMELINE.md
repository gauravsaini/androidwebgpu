# T738 timeline — unmount probe mounts before handoff (restore T734 device state)

- 03:19:31 AEDT 21 Nov 2026: lane launched (agent 62e4b03d). Disk 100%, 1,544,900 KiB free — gate CLOSED.
- 03:20–03:35: Step 1 DONE — inputs SHA-verified; T737/T736/T734 findings read; handoff-state diff confirmed (T734 f1e68858).
- 03:35–03:39: Step 2 DONE — early-ramdisk-t738 7f4525a9 (unmount block + verify, STOP on failure); CPIO 5d7affd5 (53,749,194 B); DTB 4be60332 (initrd-end 0x1133425ca); supervisor 800d4746; analyzer ea5923ee (regression-checked on flight-12 UART).
- 03:39: disk gate OPEN — 93%, 16,125,236 KiB free. Flight service-flight-13 launching (guest target 850 s, host cap 1100 s).
- 03:40:36–03:41:01: flight service-flight-13 — host 25.03 s, guest 16.485 s, QEMU exit 0. Probe ran (27 T723_PROBE); unmount /sysimg FAILED rc=1 (L542); HANDOFF ABORTED (L593); kernel panic guest 16.479 s.
- 03:41–03:55: Step 4 adjudication DONE — verdict FAIL. New wall INIT-UMOUNT-SELF-BLOCKED (toybox maps /sysimg libraries; the tool is its own blocker). Successor: T739 (scope LD_LIBRARY_PATH to the probe line; verify transitive closure first).
- ~03:47–03:50: sandbox egress-proxy outage — box ssh/rsync + Mac macssh all failed (nc proxy broken pipe / PREFLIGHT_FAIL). Mac + sandbox evidence complete; box sync pending on recovery.
