# T737 timeline

- 02:03:04 AEDT Sat 21 Nov 2026 — lane launched (agent `199419f7`, brief `7b13f42f`, wedge 05:03 AEDT). Disk gate CLOSED at launch (800,496 KiB free, 100%).
- 02:03–02:14 — Step 1 DONE: inputs SHA-verified; T736/T735/T734 findings read in full; LD_LIBRARY_PATH mechanism RESOLVED (image lib64 is at /sysimg/system/lib64, not /sysimg/lib64); libEGL byte-identity confirmed (dup fn 56 B identical; eglGetProcAddress logic-identical, 2 address operands shifted 0x10).
- 02:14–02:32 — Step 2 DONE: early-ramdisk `32b1430c` (proc scoping + corrected LD_LIBRARY_PATH); CPIO `266deb91` (53,747,156 B); DTB `c075b453` (initrd-end 0x113341dd4); supervisor `eed00206`; analyzer `ea9df2fb` (import-tested vs flight-11 UART: certified counts reproduced).
- 02:35 — Mac disk REOPENED on its own: 18,390,440 KiB free (92%), gate OPEN. (Earlier rsync to the Mac failed once on a transient proxy broken pipe; Mac reachable via macssh throughout.)
- 02:40–02:43 — Staged to the Mac worktree, all SHAs verified: early-ramdisk `32b1430c`, supervisor `eed00206`, analyzer `ea9df2fb`, DTB `c075b453`, CPIO `266deb91` (via box→Mac pipe; sandbox mirror skipped on quota — box copy is canonical).
- 02:44 — Step 3: disk gate OPEN (16,198,872 KiB free at 02:43). service-flight-12 LAUNCHING (full 850 s guest target, 1100 s host cap).
- 02:45 — Step 3: service-flight-12 FLOWN (host 15.02 s, guest 11.965 s, QEMU exit 0). Probe RAN (27 T723_PROBE); SP1/SP2 dup -1 vs the IMAGE's libEGL (L383); /proc scoping PROVEN (L392–393).
- 02:45–02:52 — Step 4: guest aborted — init tried /system from unbound /dev/block/loop0 (L601–605) and rebooted to bootloader (L607). NEW guest wall INIT-SYSTEM-MOUNT-LOOP0-UNBOUND. T734 comparison PROVEN (fstab says vda; T734 mounted vda; T737 handoff left vda/loop1/debugfs mounted). Full flight NOT run; surfaceflinger context un-runnable; no fix route tested.
- 02:56–02:58 — Step 5: findings filed (Verdict FAIL), outcome FAIL (4f8e9e45), death note written. Lane complete 02:58 AEDT 21 Nov, inside the 05:03 wedge. Successor: T738 (unmount probe mounts before handoff).
