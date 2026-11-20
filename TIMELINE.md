# T736 timeline — EGL-dispatch probe via Option A

- **00:54 AEDT Sat 21 Nov 2026** — Lane launched (agent `2e7308b1`, brief `f806a630`, wedge 03:54). Disk gate CLOSED at launch (1,873,920 KiB free, 100%).
- **~00:59** — Disk 119,684 KiB free (foreign drain).
- **~01:00** — Disk gate REOPENED on its own: 21,371,952 KiB free (90%).
- **01:07** — Step 2 DONE. Early-ramdisk `a467a546`; CPIO `da6186e5` (53,745,730 B); DTB `bc44f717`; supervisor `verify_inputs()` passed. Disk 21,391,344 KiB free. Branch: INVESTIGATION-ONLY.
- **~01:08–01:12** — Box sync: CPIO piped Mac→box (SHA `da6186e5` verified); small files rsynced (DTB `bc44f717`, early-ramdisk `a467a546`, supervisor `a4db25c0`, analyzer `ad77e7a4` — all match).
- **01:13:13** — Disk re-checked: 16,610,376 KiB free (93%), gate OPEN. `service-flight-11` launched on the Mac QEMU.
- **01:13:28** — Flight ended. QEMU exit 0; host 15.02 s, guest 11.251 s. UART 446 lines, SHA `d46e2ae1`.
- **01:13–01:20** — Analyzer run (exit 0, REPORT COMPLETE): T723_PROBE 27 lines, SP1/SP2/SP3 DONE 1 each, T734_KERNEL_OK 1, T720 PASS 1. Guest stopped at 11.25 s on the proc-mount wall.
- **01:20–01:35** — Disassembly cross-checks (box): flown driver `34b79da2` (stub `0x5cb0` → trampoline `0x56dc`, sw_sync open + CREATE_FENCE + INC); cpio-unpack driver copy `fafb59a7` REJECTED as not the flown artifact; ramdisk vs image libEGL dispatch code byte-identical.
- **01:38** — Findings written. **Verdict: PASS.** Outcome `PASS`, TIMELINE, CLOSED heartbeat written. Lane complete, inside the 03:54 wedge.
