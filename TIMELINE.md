# TIMELINE — Track T740 (all times AEDT, 21 Nov 2026)

- **05:38** — Lane launched (agent `e7f12434`, brief `9efda8f4`, worktree
  `/Users/Shared/wt-track740-longer-window`, branch t740, base 902c272). Disk
  gate OPEN (26,325,112 KiB free, 88%). Wedge 08:38.
- **05:56** — Step 0: disk re-checked, 23,654,800 KiB free, 89% — OPEN.
  Step 1: all 23 guest-inputs re-hashed, all MATCH; T739/T738/T737/T736/T734
  findings read in full. Hard gate PASSED.
- **~06:10–06:25** — Step 2: early-ramdisk-t740 built (`6804341a`, kill block
  only, no export); CPIO rebuilt (`724dff6c`, 53,753,289 B); DTB re-patched
  (`6b6a118d`, initrd-end 0x1133435c9); embedded vendor verified `5e0607ad`;
  supervisor `f07de12c` + analyzer `2e90935e` import-tested; box sync done
  (CPIO routed Mac→box via sandbox pipe after the sandbox rsync hit quota).
  Branch: INVESTIGATION-ONLY for the fix build.
- **06:22:26** — Flight service-flight-15 launched (supervisor PID 87228,
  QEMU PID 87291, GDB 1269; disk 22,111,368 KiB free, 90% — OPEN).
- **06:23:36** — Early check: T723_PROBE 27, T734_KERNEL_OK (guest 3.89 s),
  [T740] kill both PIDs (201/202), all three [T739] unmounts rc=0, handoff
  verified, init first stage started (guest 11.0 s).
- **~06:33** — Disk 20,420,696 KiB free (91%), still OPEN. Guest kernel-time
  reading stalled at 344.8 s for ~3 polls during the logcat-heavy dex2oat
  phase (QEMU 98.3% CPU, logcat advancing — lagging indicator, not a stall);
  guest caught up (1094.9 s at host 1097.3 s by 06:40).
- **06:42:31** — Flight-15 ENDED: "Guest target reached (1202.1s >= 1200s)",
  host 1202.60 s, guest 1202.06 s, QEMU exit 0. UART 26,242 lines, 3,505,378 B,
  SHA `0befc1fe`. **Boot completed: sys.boot_completed first fired guest
  1175.836 s (12 lines).** System_server watchdog-killed at ~guest 935 s and
  restarted (pid 2323), completing boot. T721 fence created (fd=10 tl=9) +
  simplefb mapped (guest ~890 s). RenderEngine dup-fail 0 (VOID — only 26 s
  post-boot).
- **~06:47** — Analyzer run on flight-15 (exit 0); all counts verified by
  byte recount on the sandbox UART copy.
- **07:00:13** — Flight service-flight-16 launched (mechanism test 1: file
  chmod 0666; supervisor PID 14616, QEMU PID 14620, GDB 1270).
- **07:02:25** — Flight-16 ENDED: host 130.26 s, guest 126.741 s, QEMU exit 0.
  UART 3,968 lines, 577,113 B, SHA `56354691`. **Result: SP3 still fd=-13 —
  the file-chmod route FAILED.** File was already 0644; SP3 opens O_RDONLY;
  no avc denial against the probe.
- **07:00–07:16** — Diagnosis: probe source checked on the box (SP3 opens
  O_RDONLY, line 548); flight-15's first 600 lines contain NO avc denial for
  the probe; the barrier must be on the open path, not the file. Mech2 built:
  early-ramdisk-t740-mech2 (`3c9aa19d`, adds `ls -ld` of the parent dirs +
  `chmod 0755` on them), CPIO `1f8c00f2`, DTB `534f4c39`.
- **07:17:40** — Flight service-flight-17 launch attempt 1 FAILED on a
  supervisor checksum refusal (lane typed a fabricated DTB SHA tail; the gate
  caught it; no QEMU started). Corrected from the supervisor's error message
  and re-launched (supervisor PID 21028, QEMU PID 21041, GDB 1271).
- **07:19:45** — Flight-17 ENDED: host 125.27 s, guest 123.601 s, QEMU exit 0.
  UART 3,817 lines, 548,912 B, SHA `10f6365a`. **Result: MECHANISM PASSES —
  `/sys/kernel/debug` was drwx------ (0700); after chmod 0755, SP3 opens the
  timeline (fd=4), SP1 dup rc=5, SP2 dup rc=7; all three sub-probes mint real
  signaled sw_sync fences.**
- **~07:23–07:25** — Analyzer runs on flights 16/17 (exit 0); byte recounts
  on the sandbox copies match the parser exactly; all files synced to sandbox
  + box; findings/outcome/death note/timeline/progress/heartbeat written.
  Lane ends: verdict FAIL (clause 3 unmeasured), mechanism PASSED.
