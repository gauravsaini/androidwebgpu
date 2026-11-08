# DEATH NOTE — T703

Lane: Track T703 — Delete the AOT shadow (Mac agyx, remove pre-compiled Launcher3 odex/vdex so the patched APK actually executes, then fly for the first queued buffer + first present dispatch)
Status: CLOSED — FAIL (Honest FAIL: `sys.boot_completed=1` observed and executed at guest 1421.42 s; AOT shadow deleted and verified absent in external `guest-inputs/system.img`, but guest `early-ramdisk` mounts embedded `ramdisk-images/system.img` from inside the fix initramfs, leaving Launcher3 crashing 53× with DotRenderer NPE; SurfaceFlinger held 0 queued buffers, 0 present dispatch within 2400 s window)
UTC close: 2026-11-08T09:30:00Z
Worktree: /Users/Shared/wt-track703-delete-aot-shadow
Branch: t703 (from T702 commit 50d8567)

## What ended

Track T703 was opened to execute the exact remediation named by Track T702: delete the pre-compiled AOT artifacts (`Launcher3QuickStep.odex` and `Launcher3QuickStep.vdex`) from `system/system_ext/priv-app/Launcher3QuickStep/oat/arm64` on the repacked system image, forcing ART to execute the patched `classes7.dex` inside `Launcher3QuickStep.apk`, cure the DotRenderer NPE crash loop, and fly for the first queued buffer (`queueBuffer > 0`) and first SurfaceFlinger present dispatch.

1. **AOT Shadow Removal & Integrity Verification Completed**:
   - Preserved T702's repacked system image untouched (`8cab6e89…`, unlinked hardlink and cloned via APFS `cp -c`).
   - Using `debugfs`, deleted `.odex`, `.vdex`, `oat/arm64`, and `oat` directories from `guest-inputs/system.img`.
   - Verified filesystem integrity clean with `e2fsck -f -y` (exit code 0).
   - Produced de-shadowed `guest-inputs/system.img` (SHA-256 `afc0add0a3585b54831836ed051a5e98e59ba9317f6e5b570be4bb2b4d720fe7`, 2,059,816,960 bytes).
   - Preflight proof confirmed 100% absence of `oat` tree and 100% byte-exact preservation of T702's signed patched APK (`8a28e1db…`, `classes7.dex` dexdump clean, patched opcodes verified at `0x199a10`, `0x199a90`, and `0x19b112`).

2. **Flight-01 (2400 s Target Window)**:
   - Successfully flew on Mac QEMU 11.1.2 for 2405.51 host seconds (2403.97 guest seconds, exit code 0).
   - `sys.boot_completed=1` was reached and processed by init at guest 1421.416780 s (line 72427).
   - Display services initialized, SurfaceFlinger declared `Boot is finished` twice (lines 44368, 73788), and Composer 2.1 received `BC_FREE_BUFFER` ×211.

3. **Canonical Kernel Control (`flight-ab`)**:
   - Flown on canonical kernel `05c43917…` for 240 s guest target (host 245.64 s, guest 243.91 s).
   - Replicated canonical mount_all failure at guest ~19 s / 43.32 s (line 1343: `mkdir() failed on /cache/recovery: Read-only file system`), confirming the external image did not perturb canonical baseline behavior.

## Why the verdict is FAIL

Under the proving criteria of Track T703:
1. **Launcher3 Crash Loop Persisted (55 Crashes Observed)**:
   - `com.android.launcher3` crashed 55 distinct times (2 early `DeadSystemException` before boot completion, lines 49181–49185; 53 distinct DotRenderer NPE crashes, lines 77582 to 135592, PIDs 12655 through 18306).
   - ART skipped dexopt on `Launcher3QuickStep.apk` (line 33180: `ArtService: Dexopt result: ... status=SKIPPED ...`).
2. **0 Queued Buffers & 0 Present Dispatches**:
   - SurfaceFlinger held `queueBuffer`: 0.
   - SurfaceFlinger dispatched 0 `presentDisplay`, 0 `validateDisplay`, 0 `presentOrValidate`, and 0 Composer `executeCommands`.

## Groundbreaking Architectural Discovery: The Embedded Ramdisk Image Barrier

Forensic extraction of `guest-inputs/t699-systemserver-fb0-fix.cpio.gz` resolved the mystery of why neither T702's DEX patch nor T703's AOT shadow deletion took effect in guest runtime:

- **The `early-ramdisk` script inside the CPIO initramfs**:
  ```bash
  "$TOYBOX" losetup -r /dev/loop0 /ramdisk-images/system.img
  "$TOYBOX" losetup -r /dev/loop1 /ramdisk-images/vendor.img
  "$TOYBOX" mknod -m 0600 /dev/block/loop0 b 7 0
  ```
- **The `fstab.virt` mount file inside the CPIO initramfs**:
  ```
  /dev/block/loop0 /system ext4 ro,barrier=1 wait,first_stage_mount
  ```
- **The Reality**: Android first-stage init mounts `/dev/block/loop0`, which `early-ramdisk` attached to `/ramdisk-images/system.img` (2,596,683,776 bytes, SHA-256 `7527d1cff3161c51e0cf65cbb6dca06076fb8dfba8fa632cb6f7b116fb4f56f1`) *embedded directly within the CPIO archive*.
- The QEMU parameter `-drive if=none,id=t702system,file=guest-inputs/system.img,format=raw,readonly=on -device virtio-blk-device,drive=t702system` creates `/dev/vda`, which the guest initramfs NEVER mounts!
- The embedded `/ramdisk-images/system.img` still contains the original unpatched `Launcher3QuickStep.apk` and its pre-compiled `.odex`/`.vdex` files.

## Final artifacts

- `findings-t703.md` — full flight findings with verbatim evidence citations, line numbers, and markers.
- `outcome-t703.txt` — single word `FAIL`.
- `DEATH_NOTE-T703.md` — this death certificate.
- `progress-t703.txt` — Steps 0–4 progress log.
- `heartbeat.json` — status and timestamp record.
- `TIMELINE.md` — updated project timeline entry.
- `GATES.md` — verified gate checklist.
- `scripts/supervisor_t703.py` — adapted supervisor script for T703 flights.
- `scripts/launch_t703.py` — launch script.
- `guest-inputs/system.img` — verified de-shadowed external system image (SHA-256 `afc0add0…`).
- `flights/flight-01/` — Route B flight records (`flight-end.json`, `uart.log`, `uart.log.gz`, `uart.log.numbered.txt`, `preflight.json`, `qemu-command.txt`).
- `flights/flight-ab/` — Canonical A/B control flight records (`flight-end.json`, `uart.log`, `uart.log.gz`, `uart.log.numbered.txt`, `preflight.json`, `qemu-command.txt`).

## What is not claimed

- No frame commit claimed: SurfaceFlinger did not dispatch present or validate to Composer 2.1 within the 2400 s window, and zero pixels reached `/dev/fb0`.
- Snapshot byte differences were not claimed or used as frame commit proof (per T673 standing rule).
- TEN-PASSES was NOT run in this lane (one Route B flight to 2400 s + one A/B control flight to 240 s). Recorded honestly; no 10/10 claim.

## Handoff to Next Lane (Track T704)

Track T703 discovered the exact architectural root cause: The guest does not mount the external virtio-blk `system.img`; it mounts `/ramdisk-images/system.img` embedded inside the initramfs CPIO archive.

The exact next fix for Track T704 is:
- **Option 1 (Recommended)**: Modify `early-ramdisk` in `t699-systemserver-fb0-fix.cpio.gz` to point `/dev/loop0` (or `/dev/block/loop0`) to `/dev/vda` (the external QEMU virtio-blk device), and drop `/ramdisk-images/system.img` from the CPIO archive (updating `modified.dtb` `linux,initrd-end` to match). This will immediately connect the de-shadowed and patched external `system.img` to the guest.
- **Option 2**: Apply the T702 DEX patch and T703 AOT shadow removal directly to the embedded `/ramdisk-images/system.img` inside the initramfs CPIO archive, re-compress, and align size to `modified.dtb`.
