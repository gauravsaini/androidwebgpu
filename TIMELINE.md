# T718 timeline (all times UTC unless marked AEDT; clock/mtime-sourced)

- 2026-11-15T17:30:55Z (04:30:55 AEDT): lane launched (spawn receipt). Wedge trigger 20:30:55Z.
- 2026-11-15T17:31:19Z: Step 0 PASS — Mac disk 95%, 12 Gi avail, >= 8 GiB gate.
- 2026-11-15T17:33-17:37Z (bounded): Step 1 PASS — all input SHAs verified, no network fetch; framework .so files extracted fresh from the vault system image via read-only debugfs.
- 2026-11-15T17:35-17:50Z (bounded): Step 2 framework-first disassembly — flag omission proven from allocator-V2-ndk.so; AllocationResult/NativeHandle layouts and status header verified; 11f24 dead-code hypothesis refuted; PLT probe finds the dead AParcel_readInt64 stub (0x34ab0).
- 2026-11-15T17:51:00Z: STEP2 DONE + PATCH BUILT — fixed binary 3daa5a49 (73 diff bytes, parser footprint identical), disassembly review PASS (all bl/cb targets exact).
- 2026-11-15T17:51:43Z: vendor patched on box (bac3cdc1, inode 731 reused, e2fsck 0/0, SYS_RAWIO + neighbours intact).
- 2026-11-15T17:52:53Z: CPIO (1150c841, 53,379,397 B, only vendor.img member differs) + DTB (d298f5ad, initrd-end 0x1132e8145) built.
- 2026-11-15T17:53-17:57Z (bounded): artifacts transferred box -> sandbox (SHAs verified); 8 T718 scripts adapted, diff-reviewed, py_compile OK, runtime-verified against T716/T717 fixtures (all oracle counts exact).
- 2026-11-15T18:00:34Z: inputs verified on Mac (all 3 artifact SHAs match box + sandbox); scripts verified on Mac.
- 2026-11-15T18:00:49Z: smoke launched (supervisor PID 4268, QEMU PID 4272).
- 2026-11-15T18:05:49Z: smoke ended on the host cap (guest 191.7 s, 13,816 lines, UART 137fb562). REJECT 0, CALL 2, GBA-fail 0, importBuffer x2 — SMOKE PASS, -129 wall broken.
- 2026-11-15T18:08:30Z: flight-01 launched (supervisor PID 4668, QEMU PID 4672).
- 2026-11-15T18:28:47Z: flight-01 mid-check (guest 1216.7 s, UART 19.3 MB, realtime pace).
- 2026-11-15T18:48:34Z: flight-01 ended on the guest target (2403.8 s, 323,607 lines, UART 081a9381). Allocation at scale (604 calls, 695 imports, 0 failures); queueBuffer 0, present 0, Invalid present fence x2241.
- 2026-11-15T18:49:56Z: flight-ab launched (supervisor PID 6353, QEMU PID 6356).
- 2026-11-15T18:50:21Z: flight-ab ended (QEMU self-exit 0, 576 lines, guest 20.19 s, UART 1a9a58fa). Control pattern matches T716 exactly.
- 2026-11-15T18:56:34Z (05:56:34 AEDT 16 Nov 2026): Step 4 complete - findings, outcome, death note, timeline written and SHA-verified; heartbeat CLOSED; lane ends FAIL (present path). Report to driver.
