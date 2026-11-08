# DEATH NOTE — T704

Lane: Track T704 — Reconnect the guest to the external system.img (Mac agyx, fix the embedded-image bypass so the T702 patch + T703 de-shadow actually reach the guest, then fly for the first queued buffer + first present dispatch)
Status: CLOSED — FAIL (Honest FAIL: `sys.boot_completed=1` not observed (0); Option 1 blocked because `CONFIG_VIRTIO_BLK` is `# CONFIG_VIRTIO_BLK is not set` in Route B kernel config; Option 2 executed to rebuild fix initramfs with patched embedded `ramdisk-images/system.img` and byte-aligned DTB; guest mounted patched embedded image to `/dev/loop0` as `/system`; `odrefresh` failed with exit 81 due to `dex2oat` failure on `/system/framework/services.jar` (bad checksum 5255acfa, expected 0d4f3e31) caused by ext4 `shared_blocks` deduplication mutation from Homebrew `debugfs write`; `ZygoteInit` crashed repeatedly with SIGSEGV in `art::ArtField::GetDeclaringClass`; `system_server` did not fork; SurfaceFlinger held 0 queued buffers, 0 present dispatch within 2400 s window)
UTC close: 2026-11-08T12:30:00Z
Worktree: /Users/Shared/wt-track704-embedded-system-img-fix
Branch: t704 (from T703 commit 9470697)

## What ended

Track T704 was opened to resolve the embedded-image bypass discovered in Track T703:
In T703, the driver verified that guest userspace boots via `early-ramdisk` which loop-mounts the embedded `ramdisk-images/system.img` (2,596,683,776 bytes) inside `t699-systemserver-fb0-fix.cpio.gz` to `/dev/loop0`, while the external virtio-blk block device (`/dev/vda`) was never mounted. Consequently, T702's DEX patch and T703's AOT shadow deletion never reached the guest.

Track T704 was tasked with reconnecting the guest to the patched artifacts via two paths:
- **Option 1 (Recommended)**: Point `early-ramdisk`'s `/dev/loop0` at `/dev/vda` and drop the embedded image from the CPIO.
- **Option 2 (Fallback)**: Patch the embedded `ramdisk-images/system.img` inside the initramfs CPIO directly with T702's DEX patch and T703's AOT deletion, recompress with `compresslevel=9`, and re-align the DTB `linux,initrd-end` byte-exactly.

1. **Step 0 — Mac Disk Gate: PASS**:
   - `df -h /System/Volumes/Data` reported 10 GiB available (10,522 MB), strictly satisfying the $\ge$ 8 GiB threshold. No WAIT-DISK.
2. **Step 1 — Input Verification & Feasibility Gate: PASS**:
   - All 10 staged inputs and inherited markdown reports SHA-verified byte-for-byte.
   - Evaluated Option 1 feasibility gate: checked `CONFIG_VIRTIO_BLK` in `extracted_kernel.config` (SHA `d840310a…`). Verbatim result: `# CONFIG_VIRTIO_BLK is not set`.
   - Because the Route B kernel does not have built-in virtio-blk support, `/dev/vda` is never probed or created by the kernel. Option 1 was conclusively blocked without a kernel rebuild.
   - Per specification, Option 2 was selected as primary.
3. **Step 2 — Initramfs Rebuild & DTB Alignment (Option 2): PASS**:
   - Extracted embedded `ramdisk-images/system.img` (2,596,683,776 B) and applied T702's signed patched APK (`8a28e1db…`, 25,654,202 B) and deleted the AOT directory (`oat/arm64` and `oat`).
   - Clean `e2fsck -f -y` pass (exit code 0). Patched embedded image SHA-256: `85e74aaf83c9393cc8d75e38b0fe61ae6c037d4a37563ab7cf5dff4979589ddc`.
   - Rebuilt `guest-inputs/t704-systemserver-fb0-fix.cpio.gz` (size 957,349,225 bytes, SHA-256 `95861d2a777b819e16b687ac98f49bced1b355dc48244cd616db3f427b9c5d17`).
   - Decompiled DTB, re-aligned `linux,initrd-end` to `<0x01 0x490ffd69>` (`0x110000000 + 957349225 = 0x1490ffd69`), and compiled `guest-inputs/t704-modified.dtb` (size 1,920 bytes, SHA-256 `5e98d2acbfed69522cc7efa65aa0eadd0dc419c846795b1d2a7eafe22935963c`).
   - All 4 pre-flight proofs verified.
4. **Step 3a — 120 s Smoke Flight: PASS**:
   - Flown detached: host 125.40 s, guest 123.42 s, exit code 0, 4,987 lines, SHA `df1297f5…`.
   - Intended mount proven: `[   28.338390] init: [libfs_mgr] __mount(source=/dev/block/loop0,target=/system,type=ext4)=0: Success`.
5. **Step 3c — Canonical A/B Control (`flight-ab`): PASS**:
   - Flown on canonical kernel `05c43917…` for 240 s guest target (host 245.61 s, guest 242.30 s, exit code 0, 13,394 lines, SHA `403ac117…`).
   - Replicated canonical mount_all failure baseline at guest 43.05 s (`mkdir() failed on /cache/backup_stage: Read-only file system`), confirming harness baseline stability.
6. **Step 3b — Flight-01 (Route B Kernel, 2400 s Target)**:
   - Flown on Route B kernel `3f45272f…` with rebuilt initramfs and re-aligned DTB.
   - Guest successfully attached and mounted the patched embedded `/system` image.

## Why the verdict is FAIL

Under the proving criteria of Track T704:
1. **`sys.boot_completed=1` Not Observed**:
   - `sys.boot_completed` remained unset throughout the flight window.
2. **`system_server` Did Not Fork**:
   - ART's on-device compiler (`odrefresh`) attempted to compile bootclasspath and system server jars via `dex2oat64`.
   - Mainline bootclasspath compiled cleanly.
   - When compiling `/system/framework/services.jar`, `dex2oat64` failed immediately:
     `01-01 00:01:55.684  2666  2666 E dex2oat64: Failed to verify /system/framework/services.jar: Failure to verify dex file '/system/framework/services.jar': Bad checksum (5255acfa, expected 0d4f3e31)`
   - `odrefresh` terminated with exit code 81 (partial failure).
   - When `app_process64` launched `ZygoteInit.main`, ART encountered an inconsistent boot/services state and repeatedly crashed with `SIGSEGV` (code 1, SEGV_MAPERR, fault addr `0x0`) in `art::ArtField::GetDeclaringClass` at `art::ClassLinker::InitializeClass`.
   - Because Zygote was stuck in a crash loop, `forkSystemServer()` was never executed.
3. **0 Launcher3 Crashes & 0 Queued Buffers**:
   - `com.android.launcher3` never ran (0 crashes observed).
   - SurfaceFlinger held `queueBuffer`: 0.
   - SurfaceFlinger dispatched 0 `presentDisplay`, 0 `validateDisplay`, 0 `presentOrValidate`, and 0 Composer `executeCommands`.

## Groundbreaking Root Cause Discovery: The ext4 `shared_blocks` Dedup Mutation

Forensic investigation into why `/system/framework/services.jar` failed verification uncovered a deep filesystem mechanics discovery:

1. **Android's Read-Only ext4 Filesystems Use `shared_blocks`**:
   - In `debugfs -R "stats" system.img`, the superblock features explicitly include:
     `Filesystem features: ext_attr dir_index filetype extent sparse_super large_file huge_file uninit_bg dir_nlink extra_isize shared_blocks`
   - Android GSI build tooling deduplicates identical 4KB blocks across the system image. Block 145414 (holding the repeating fill pattern `0x0119` with SHA-256 `4ea21150…`) was shared between `system/framework/services.jar` (blocks 2422 and 2423) and `Launcher3QuickStep.apk` (blocks 1726, 2028, 2883, 4151).
2. **Homebrew `debugfs write` Ignores Block Sharing**:
   - When T702's procedure (`debugfs -w -R "write ..."`) was used to replace `Launcher3QuickStep.apk`, `debugfs`'s naive block allocator clobbered block 145414 with string table data from the new APK.
   - Because block 145414 was shared with `services.jar`, modifying it in-place silently corrupted `classes.dex` inside `services.jar`.
   - In T702 and T703, this corruption went undetected because the guest never mounted the external `system.img`.
   - In T704, when Option 2 applied the same `debugfs write` procedure to the embedded `ramdisk-images/system.img`, block 145414 was mutated inside the embedded image as well.
   - When the guest finally mounted the embedded image in T704, `dex2oat64` read `services.jar`, detected that its Adler-32 checksum did not match the header, and aborted with exit code 1.

## Final artifacts

- `findings-t704.md` — full flight findings with verbatim evidence citations, line numbers, and markers.
- `outcome-t704.txt` — single word `FAIL`.
- `DEATH_NOTE-T704.md` — this death certificate.
- `progress-t704.txt` — Steps 0–4 progress log.
- `heartbeat.json` — status and timestamp record.
- `scripts/build_t704_fix.py` — initramfs rebuild and DTB re-alignment script.
- `scripts/supervisor_t704.py` — flight supervisor script.
- `scripts/launch_flight_01.py` — Flight-01 launch script.
- `scripts/launch_flight_ab.py` — Flight-AB launch script.
- `guest-inputs/t704-systemserver-fb0-fix.cpio.gz` — rebuilt fix initramfs (957,349,225 B, SHA `95861d2a…`).
- `guest-inputs/t704-modified.dtb` — byte-aligned DTB (1,920 B, SHA `5e98d2ac…`).
- `flights/flight-01/` — Route B flight records (`flight-end.json`, `uart.log`, `uart.log.gz`, `uart.log.numbered.txt`, `preflight.json`, `qemu-command.txt`).
- `flights/flight-ab/` — Canonical A/B control flight records (`flight-end.json`, `uart.log`, `uart.log.gz`, `uart.log.numbered.txt`, `preflight.json`, `qemu-command.txt`).

## What is not claimed

- No frame commit claimed: SurfaceFlinger did not dispatch present or validate to Composer 2.1 within the 2400 s window, and zero pixels reached `/dev/fb0`.
- Snapshot byte differences were not claimed or used as frame commit proof (per T673 standing rule).
- TEN-PASSES was NOT run in this lane (one Route B flight to 2400 s + one A/B control flight to 240 s). Recorded honestly; no 10/10 claim.

## Handoff to Next Lane (Track T705)

Track T704 proved the embedded-image bypass is real, demonstrated how to rebuild and DTB-align the initramfs, and isolated the exact `shared_blocks` corruption mechanism that blocked `services.jar`.

The exact remediation for Track T705 is:
1. **Primary Solution (Option 1 with Kernel Rebuild)**: Rebuild the Route B kernel with `CONFIG_VIRTIO_BLK=y` enabled (in `extracted_kernel.config` change `# CONFIG_VIRTIO_BLK is not set` to `CONFIG_VIRTIO_BLK=y`). This enables `/dev/vda` at early-ramdisk time. `early-ramdisk` can then mount `/dev/vda` directly and drop `/ramdisk-images/system.img` completely from the CPIO.
2. **Alternative Solution (Option 2 with Safe In-Place / Ext4 Loop Injection)**: If patching the image, do NOT use macOS Homebrew `debugfs write`. Instead, mount the ext4 image via a true Linux loop device (on the box via `boxssh`) where the Linux ext4 driver properly unshares deduplicated `shared_blocks` on copy-on-write, preserving `services.jar` checksum integrity.
