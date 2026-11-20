# T739 timeline — scope LD_LIBRARY_PATH to the probe line only

- 04:14:16 AEDT 21 Nov 2026 — Lane launched (agent 41481a8a). Disk gate CLOSED
  (6,341,956 KiB free, 98%). Brief a3b081e5. Wedge 07:14:16 AEDT.
- 04:14–04:25 — Step 1 DONE: findings T738/T737/T736/T734 read in full; all 12
  guest-input SHAs re-verified on the Mac worktree; EMBEDDED-VENDOR RULE:
  CPIO's ramdisk-images/vendor.img = 5e0607ad; QEMU command line confirmed
  from T738 flight-13 preflight.json. No missing/mismatched input.
- 04:25–04:45 — Step 2 DONE: transitive-closure check FIRST (toybox closure =
  13 libs, all in ramdisk /system/lib64/; libnetd_client NOT in toybox's
  closure; backgrounded scripts hold no /sysimg mapping) -> scoped-path route;
  early-ramdisk edited (export removed, umount stderr -> /dev/console,
  hooks retagged [T739]) d75f5658; CPIO rebuilt on the box df2a6c9a
  (53,751,329 B, 241 entries, all untouched byte-identical); DTB re-patched
  e4b0ff79 (initrd-end 0x113342e21); supervisor 8f6cc5ea + analyzer 67b77198
  adapted + import-tested; INTERP chain verified (toybox/sh/probe ->
  /system/bin/linker64 real file; /init -> bootstrap/linker64; t734probe
  static). All 7 build files SHA-verified on the Mac worktree.
- 04:47 — Step 3: disk gate re-checked: 16,820,316 KiB free (93%) — OPEN.
  Launching service-flight-14 (guest target 850 s, host cap 1100 s).
- 04:42:28–04:56:40 — Step 3 DONE: service-flight-14 flew the FULL 850 s
  (guest 851.145 s, host 852.35 s, QEMU exit 0, UART 918ef4b4). Probe RAN
  (27 T723_PROBE, SP1/SP2 dup -1 vs IMAGE libEGL 0x7f9ada97a8); ALL THREE
  unmounts rc=0, handoff verified, init mounted /system from vda (guest
  22.287 s) and switched root (22.293 s). T737 + T738 walls CLOSED.
- 04:57–05:05 — Step 4 adjudication: sys.boot_completed 0 — system_server
  still in OnBootPhase_500 (83/143) at the target; surfaceflinger started
  but never presented; fix route NOT TESTED (blocked, no presenting
  context). T729 wall BOOT-COMPLETED-NEVER-FIRED RECURREND (boot times
  495/790/>851 s on identical inputs). Verdict FAIL per the conjunctive
  PASS contract. Successor: T740 (longer window ~1200 s + kill orphaned
  scripts at the handoff).
