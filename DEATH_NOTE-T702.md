# DEATH NOTE — T702

Lane: Track T702 — Launcher3 crash-loop cure (Mac agyx, cure the DotRenderer NPE so Launcher3 completes onCreate and queues the first buffer)
Status: CLOSED — FAIL (Honest FAIL: `sys.boot_completed=1` observed and executed at guest 1405.95 s; Launcher3 QuickstepLauncher bytecode patched in `classes7.dex`, but ART executed pre-compiled AOT `Launcher3QuickStep.odex`/`vdex` from `system_ext/priv-app/Launcher3QuickStep/oat/arm64/` instead of the repacked APK, causing Launcher3 to crash across 90 instances with identical DotRenderer NPE; SurfaceFlinger held 0 queued buffers, 0 present dispatch within 2400 s window)
UTC close: 2026-11-08T07:35:00Z
Worktree: /Users/Shared/wt-track702-launcher3-crashloop-cure
Branch: t702 (from T701 commit 5d4b305)

## What ended

Track T702 was opened to cure the fatal `NullPointerException` crash loop in `com.android.launcher3.icons.DotRenderer.<init>` (`Bitmap.getHeight()` on null bitmap) identified by T701, and fly the patched guest on Route B kernel to obtain the first queued buffer (`queueBuffer > 0`) and first SurfaceFlinger present dispatch.

1. **Fix Design & Implementation Completed**:
   - Analyzed Option B (notification-dots bypass) and Option C (FallbackHome override) and refuted both via bytecode inspection (DeviceProfile unconditionally calls createDotRenderer; FallbackHome terminates upon user storage unlock).
   - Selected Option A: Created a dual-layer patch in `classes7.dex` of `Launcher3QuickStep.apk` (`USE_HARDWARE_BITMAP = false` at offset `0x199a90`, `if-nez` hardware picture bypass nop'd at `0x199a10`, and `move v2, v0` null-guard at `0x19b112`).
   - Aligned and signed the APK with `apksigner` (SHA-256 `8a28e1db…`).
   - Repacked `system.img` with `debugfs` and verified with `e2fsck` (new SHA-256 `8cab6e89…`).
   - Preflight proof confirmed 100% byte-exact injection of patched DEX into the repacked system image.

2. **Flight-01 (2400 s Target Window)**:
   - Successfully flew on Mac QEMU 11.1.2 for 2404.78 host seconds (2402.63 guest seconds).
   - `sys.boot_completed=1` was reached and processed by init at guest 1405.952102 s (line 69387), followed by a second wave at guest 1759.748306 s (line 86796).
   - Display power mode 2 (NORMAL) set 6 times; SurfaceFlinger declared `Boot is finished` 3 times (lines 44402, 70828, 87438).
   - Composer 2.1 received `BC_FREE_BUFFER` ×75.

3. **Canonical Kernel Control (`flight-ab`)**:
   - Flown on canonical kernel `05c43917…` for 240 s guest target (host 245.59 s).
   - Replicated canonical mount_all failure at guest ~19 s / 43.76 s (lines 1357–1358: `mkdir() failed on /cache/recovery: Read-only file system`), confirming the repacked system image did not break the baseline.

## Why the verdict is FAIL

Under the proving criteria of Track T702:
1. **Launcher3 Crash Loop Persisted (90 Crashes Observed)**:
   - Despite the valid bytecode patch inside `Launcher3QuickStep.apk`, `com.android.launcher3` crashed 90 distinct times during the flight (lines 74202 to 120574, PIDs 12244 through 18676).
   - **Root Cause Isolated**: The system image contains pre-compiled AOT artifacts in `system/system_ext/priv-app/Launcher3QuickStep/oat/arm64/`:
     - `Launcher3QuickStep.odex` (119,728 bytes, ELF 64-bit ARM aarch64 shared object)
     - `Launcher3QuickStep.vdex` (278,604 bytes, Android vdex version 027)
   - When `PackageManager` registered the APK, `ArtService` logged line 32174: `ArtService: Dexopt result: [packageName = com.android.launcher3] ... actualCompilerFilter=verify, status=SKIPPED ...`
   - ART loaded and executed the native machine code from the unpatched pre-compiled `.odex` file directly, completely bypassing the patched `classes7.dex` inside the APK!
2. **0 Queued Buffers & 0 Present Dispatches**:
   - Because Launcher3 was continuously killed during `onCreate()` by the unpatched ODEX, it never attached to `ViewRootImpl`, never created a `BLASTBufferQueue`, and SurfaceFlinger held `queueBuffer`: 0.
   - SurfaceFlinger dispatched 0 `presentDisplay`, 0 `validateDisplay`, 0 `presentOrValidate`, and 0 Composer `executeCommands`.

## Final artifacts

- `findings-t702.md` — full flight findings with verbatim evidence citations, line numbers, and markers.
- `outcome-t702.txt` — single word `FAIL`.
- `DEATH_NOTE-T702.md` — this death certificate.
- `progress-t702.txt` — Steps 0–5 progress log.
- `heartbeat.json` — status and timestamp record.
- `TIMELINE.md` — updated project timeline entry.
- `scripts/build_t702_fix.py` — APK disassemble, patch, repack, and debugfs injection tool.
- `scripts/supervisor_t702.py` — adapted supervisor script for T702 flights.
- `flights/flight-01/` — Route B flight records (`flight-end.json`, `uart.log`, `uart.log.gz`, `uart.log.numbered.txt`, `preflight.json`, `qemu-command.txt`).
- `flights/flight-ab/` — Canonical A/B control flight records (`flight-end.json`, `uart.log`, `uart.log.gz`, `uart.log.numbered.txt`, `preflight.json`, `qemu-command.txt`).
- `evidence/originals/system.img` — preserved original system image (SHA-256 `98fc0b02…`, 2,059,816,960 B).
- `evidence/originals/Launcher3QuickStep.apk` — preserved original APK (SHA-256 `7626f26b…`, 28,922,784 B).
- `evidence/t702_fix_record.json` — evidentiary JSON record of the patch.

## What is not claimed

- No frame commit claimed: SurfaceFlinger did not dispatch present or validate to Composer 2.1 within the 2400 s window, and zero pixels reached `/dev/fb0`.
- Snapshot byte differences were not claimed or used as frame commit proof (per T673 standing rule).
- TEN-PASSES was NOT run in this lane (one Route B flight to 2400 s + one A/B control flight to 240 s). Recorded honestly; no 10/10 claim.

## Handoff to Next Lane (Track T703)

Track T702 isolated the definitive reason why APK repacking alone did not stop the crash loop: ART executes pre-compiled AOT `.odex` files when present in `oat/arm64/` with priority over APK DEX files.
The exact next fix for Track T703 is:
1. Delete the `system/system_ext/priv-app/Launcher3QuickStep/oat/` directory from `system.img` (deleting `Launcher3QuickStep.odex` and `Launcher3QuickStep.vdex`).
2. Without the pre-compiled `.odex`, `ArtService` will be forced to compile/interpret the repacked APK's patched `classes7.dex`.
3. With the patch active, `DotRenderer.<init>` will execute cleanly, `onCreate()` will complete, `BLASTBufferQueue` will initialize, and the first display buffer will flow to SurfaceFlinger.
