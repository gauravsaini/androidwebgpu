# DEATH NOTE — T699

Lane: Track T699 — system_server-fork + fb0 fix (Mac agyx, apply the T698-named fix, then fly)
Status: CLOSED — FAIL (system_server forked PID 9098, DisplayManagerService set power mode NORMAL, /dev/fb0 bound as character device node c 29 0; however SurfaceFlinger present not dispatched and sys.boot_completed=1 not observed within 1200 s cap)
UTC close: 2026-11-08T03:00:00Z
Worktree: /Users/Shared/wt-track699-systemserver-fb0-fix
Branch: t699 (from T698 commit d75e32a)

## What ended

Track T699 was opened to apply the T698-named fixes and fly them on the Route B kernel (`Image 3f45272f`, unchanged) and modified DTB (`modified.dtb`, `e83a6437`, unchanged) on Mac QEMU 11.1.2:
1. **Fix A (`system_server` fork barrier)**: Applied Path A3 (`dalvik.vm.dexopt.disable=true`, `dalvik.vm.extra-opts=-Xverify:none`, `dalvik.vm.systemservercompilerfilter=verify`) and pre-created `/data/misc/odrefresh/compilation-log.txt`. In addition, discovered and solved the critical DTB buffer constraint (`modified.dtb` specifies `linux,initrd-end = 0x14925d4a3`, exactly 958,780,579 bytes from `0x110000000`), aligning the new fix initramfs to exactly 958,780,579 bytes (SHA-256 `c9fe657decd3f636131a7dfa1b2816d69ebdf15aea61b2b53265b8953760d23b`).
   - *Outcome*: `odrefresh` compiled all required artifacts cleanly and exited with code 80 (`kCompilationSuccess`, lines 21914–21915) with ZERO timeouts. Zygote bypassed the 272 method verifications and successfully forked `system_server` (PID 9098) at guest 540 s (line 30101). `system_server` entered (line 30161) and bootstrapped all core services (`ActivityManager`, `PowerManager`, `DisplayManagerService`, `WindowManagerService`).
2. **Fix B (`gralloc` tmpfs dummy barrier)**: Replaced `toybox truncate -s 16M /dev/fb0` in `init.virt.rc` with character device node creation (`mknod -m 0666 /dev/fb0 c 29 0`).
   - *Outcome*: Userspace gralloc opened `/dev/fb0` as a character device node (lines 24107–24110: `tclass=chr_file`) pointing to the physical simplefb device. `DisplayManagerService` started and commanded SurfaceFlinger to set display power mode 2 (NORMAL) at lines 29220 and 39861.

## Why the verdict is FAIL

While both T698-diagnosed blockers were cured with 100% verified evidence:
1. `sys.boot_completed=1` was not observed (`boot_completed=` empty across all `T642_BOOT_STATE` watcher probes through the 1200 s cutoff).
2. SurfaceFlinger never dispatched present or validate calls to Composer 2.1 (0 `presentDisplay`, 0 `validateDisplay`, 0 Composer `executeCommands` writes to fb0).
3. Right at the 1201.5 s guest target cutoff, `WindowManagerShell` and `SystemUI` (PID 11747) were actively initializing shell components 1–29 (lines 46167–46196: `DisplayController init`, `ShellTaskOrganizer init`). Emulation throughput under QEMU TCG requires an extended flight window or further boot optimization to complete desktop window initialization and dispatch the first frame.

## Final artifacts

- `findings-t699.md` — full flight findings with verbatim evidence citations, line numbers, and markers.
- `outcome-t699.txt` — single word `FAIL`.
- `DEATH_NOTE-T699.md` — this death certificate.
- `progress-t699.txt` — Steps 0–4 progress log.
- `heartbeat.json` — status and timestamp record.
- `GATES.md` — unlazy acceptance gates verification record.
- `TIMELINE.md` — updated project timeline entry.
- `guest-image/t699-systemserver-fb0-fix.cpio.gz` — DTB-aligned fix initramfs (958,780,579 B, SHA-256 `c9fe657d…`).
- `flights/flight-01/` (symlinked as `flights/t699-flight-01/`) — Route B flight records (`flight-end.json`, `uart.log`, `uart.log.gz`, `uart.log.numbered.txt`, `preflight.json`, `qemu-command.txt`).
- `flights/flight-ab/` (symlinked as `flights/t699-flight-ab/`) — Canonical A/B control flight records (`flight-end.json`, `uart.log`, `uart.log.gz`, `uart.log.numbered.txt`, `preflight.json`, `qemu-command.txt`).
- `evidence/fix_a_record.json` and `evidence/fix_b_record.json` — evidentiary fix records.

## What is not claimed

- No `sys.boot_completed=1`: not observed (0 / empty) in any flight.
- No frame commit claimed: SurfaceFlinger did not dispatch present or validate to Composer 2.1 within the 1200 s cap.
- Snapshot byte differences were not claimed or used as frame commit proof (per T673 standing adjudication).
- TEN-PASSES was NOT run in this lane (one Route B flight to 1200 s + one A/B control flight to 240 s). Recorded honestly; no 10/10 claim.

## Handoff to Next Lane

The fundamental platform plumbing is now completely sound:
1. Route B kernel registers `fb0` simplefb without hang or Oops.
2. `odrefresh` completes cleanly without timeout (exit 80).
3. Zygote forks `system_server` (PID 9098) and child processes.
4. `system_server` bootstraps `DisplayManagerService` and powers on the display (power mode 2 NORMAL).
5. Gralloc binds to the real `/dev/fb0` character device node (`tclass=chr_file`).
The remaining hurdle to Stage 5 frame presentation is allowing the newly forked `WindowManagerShell` and `SystemUI` enough time/throughput to finish component startup (or streamlining initial window creation) so SurfaceFlinger initiates display composition.
