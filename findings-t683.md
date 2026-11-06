Verdict: FAIL

# Track T683 — Composer 2.1 flight recovery

**First failing link:** guest Composer traffic did not yield guest-origin proof of a successful present or a frame committed to fb0. The Composer ELF was installed, the guest mounted the embedded vendor image, and the service process ran. The separate first-cycle boot bar also failed: system_server and user CE remained unavailable, and `sys.boot_completed=1` was not observed.

WALL-TYPE-TAG: guest type-1 on FAIL
Rust par bhi lagu: haan — the same guest Composer 2.1 HIDL interface is required regardless of implementation language.
sys.boot_completed: not observed (cycle 1; the last guest T642 probe at uptime 1175.91s has an empty `boot_completed=` field and no system_server PID).

## Step 0 — Mac disk gate

PASS. `df -h /System/Volumes/Data` reported 21 GiB available, above the 8 GiB gate (228 GiB total, 181 GiB used, 90% capacity).

## Step 1 — Input verification

PASS. All five staged input hashes and sizes matched the brief. The streamed CPIO table listed `ramdisk-images/vendor.img` at exactly 134,217,728 bytes.

| Input | SHA-256 | Size |
|---|---|---:|
| `Image` | `05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04` | 23,073,280 B |
| `minimal-virt-fixed.dtb` | `f5f7e340dcc22f6b76d4c7139a7aac2d9cebf6f2ea2c7c6832a0181d3d9c85b0` | 1,465 B |
| `system.img` | `98fc0b02f5ae9788da0bf4f1a73d472df6d384080a1d323c5878d57a08c6433d` | 2,059,816,960 B |
| T681 Composer ELF | `ddc1e4dd01b91df543be1a83a065308662cf92ee6babd774f6eb014c4a59b132` | 19,941,040 B |
| Base initramfs | `afb2991f59288d1de1b9855b6cbcb6e8050bde3e9a23d2391d3f17c892a3d022` | 954,450,678 B |

## Step 2 — Replacement vendor image

PASS. I extracted only the `ramdisk-images/vendor.img` member and edited a copy of that original ext4 filesystem. The service was replaced at the vendor-filesystem path `/bin/hw/android.hardware.graphics.composer@2.1-service`, which is guest path `/vendor/bin/hw/android.hardware.graphics.composer@2.1-service` after the `/vendor` mount.

The installed payload hash matches the staged T681 ELF. The file is mode `0755`, uid `0`, gid `2000`, with the original SELinux xattr `u:object_r:hal_graphics_composer_default_exec:s0`. The resulting 134,217,728-byte vendor image has SHA-256 `bed3d6e989e16c8bbd7dd7e0208753c3a0d8e3407c96d291cd778ff43a7daaf1`. `e2fsck -fn` reported a clean filesystem.

## Step 3 — Fix initramfs

PASS. Exactly one gzip-compressed newc fix initramfs was built: [artifacts-t683/fix-t683.cpio.gz](artifacts-t683/fix-t683.cpio.gz). The stream transform preserved all other CPIO entries and metadata and replaced the same-sized vendor member. `gzip -t` passed; the resulting CPIO table still lists `ramdisk-images/vendor.img` at 134,217,728 bytes.

- Size: 958,780,579 B
- SHA-256: `8fb3c9dea65b0e6fdd06a2fad92c5292d71d3429fdaf8d27568953384948dd70`

## Step 4 — QEMU flight

PASS after a harness adjustment. QEMU was version 11.1.2 (binary SHA-256 `8e5c46dc6752c36afbb706f933e72fef835e35f48881a63cadb06c6d29bac869`). The flight reused the repository's T670 supervisor for its 1200-second guest timing and full UART capture. The command passed the canonical kernel, DTB, the fix initramfs, and the verified external `system.img` as a read-only virtio block device. The exact command is in `artifacts-t683/flight-t670/qemu-command.txt`.

The first T673/QMP harness launch failed before guest boot because QEMU could not bind a Unix socket under the worktree. Exact stderr:

```text
qemu-system-aarch64: -qmp unix:/Users/Shared/wt-track683-composer-2-1-flight-recovery/artifacts-t683/flight/qmp.sock,server,nowait: Failed to bind socket to /Users/Shared/wt-track683-composer-2-1-flight-recovery/artifacts-t683/flight/qmp.sock: Operation not permitted
```

That attempt exited `1` after 5.02 host seconds with no guest time and no UART bytes. I preserved it in `artifacts-t683/flight/`. I then used the existing no-QMP T670 supervisor; no second fix image was built. The required guest window completed at 1201.196 guest seconds. Host elapsed time was 1203.34 seconds. QEMU exited `0` after the supervisor stopped it at the planned target; its stderr records the expected signal-15 termination.

The successful full UART log is `artifacts-t683/flight-t670/uart.log` (7,652,601 B, 50,043 lines, SHA-256 `73cfd8d61e6c992b913e3c83975a314ea0f39cd3d41c505464d70aa55b5dbe70`). The T670 harness records guest logcat messages in the UART stream; it does not produce a separate logcat export.

The command attached the staged external `system.img` read-only. The guest early-ramdisk also attaches the base CPIO's embedded `ramdisk-images/system.img` as loop0 and mounts that image on `/system`; that embedded member is 2,596,683,776 bytes. The UART proves the embedded loop0 mount, so I do not claim that the separate external disk supplied the guest's `/system`.

## Step 5 — Guest adjudication

### Guest consumption and service

The guest kernel printed the `androidboot.t683_fix_initramfs_sha256` value matching the fix image at UART line 26. Guest line 395 says `[T552] guest attached embedded images to loop0/loop1`; lines 606–610 show loop0 mounted on `/system` and loop1 mounted as ext4. The base fstab maps loop1 to `/vendor`.

The Composer ELF did execute in the guest: UART lines 46319–46394 carry PID 20236 under tag `android.hardware.graphics.composer@2.1-service`, including `Finished read/write`, a 76-byte write, and `BR_TRANSACTION_COMPLETE`. This establishes that the installed ELF loaded and exchanged binder traffic. The init service's short name is not printed in the serial log; the base init service entry points to this executable.

The guest linker accepted the ELF through the device's `/system/bin/linker64`; no Composer-specific `CANNOT LINK EXECUTABLE`, missing-library fatal, or linker load failure was observed. Generic early-boot linker-config warnings did appear before the later `T644_LINKER_CONFIG_VERIFIED_OK` marker. The guest API property was not independently read, so the API-34 unwinder-tag requirement is adjudicated only by successful runtime loading, not by an observed SDK value.

The guest ran SELinux permissive (`androidboot.selinux=permissive`, `enforcing=0`). I found no Composer-specific `/dev/mem` AVC denial, but `/dev/mem` access itself was not proven. The T681 ELF's exact SimpleFB physical address and stride were not observed. The supplied DTB has no framebuffer node, and the guest's `/dev/fb0` setup is a `toybox truncate -s 16M` regular file on tmpfs, not proof of hardware framebuffer writes. For example, UART line 1158 reports an AVC denial for the toybox write to `/dev/fb0` on `dev="tmpfs"`; the later allocator denial at line 41184 is also for that tmpfs file. These are not frame-commit evidence.

SurfaceFlinger logged `Using HWComposer service: default` and `SurfaceFlinger's main thread ready to run` (lines 46290–46294). The Composer process's generic binder dumps and `BR_TRANSACTION_COMPLETE` responses do not identify a successful `present` call. No guest log contains a successful `presentOrValidate`, `presentDisplay`, `setClientTarget`, or guest-side fb0 commit/write. The QMP startup attempt produced no snapshots; no snapshot bytes are used as evidence. **Frame commit: FAIL / not proven.**

One guest log also says `servicemanager: Could not find android.hardware.graphics.composer3.IComposer/default in the VINTF manifest` at guest time 684.041. SurfaceFlinger subsequently selected the HWC default, and the Composer 2.1 process ran, but there is still no present-success evidence.

### First-cycle boot bar

FAIL. The last guest T642 probe, at uptime 1175.91 seconds (UART lines 49951–49952), says `user_de=missing`, `mirror_de=missing`, `mirror_ce=missing`, has blank system_server properties/PIDs and an empty `boot_completed=` field, while zygote is running. No `sys.boot_completed=1` line appears. The guest remained alive until the supervisor's planned stop, but the cycle did not complete system_server startup, user unlock, or CE availability. No system Watchdog kill or system-process FATAL was found. The early `keystore2` Watchdog reports are its own maintenance-thread diagnostics, not Android Watchdog kills.

### T664–T682 regression check

- **Boot path:** Only the vendor CPIO member payload changed. The embedded system member, `fstab.virt`, early-ramdisk, and first-stage loop mount flow were preserved. The guest reached SurfaceFlinger, but the first-cycle boot bar failed as described above.
- **ACL / CE:** No data image or ACL-related init path was changed. The guest never reached user CE unlock, so the end-to-end ACL/CE gate is unverified; I found no `ENOTSUP` line in the flight UART.
- **media.c2:** The T667–T670 pre-registration and respawn arrangement was not changed. Guest logs continue to show `T670_MEDIASWCODEC_RESPAWNED` and `IComponentStore/software` registrations. They repeat through the flight, so media.c2 remains active but repeatedly respawns; I do not claim it is healthy or attribute that existing behavior to the vendor-only replacement.
- **Toolchain/link lineage:** The staged T681 ELF was used byte-for-byte at its recorded SHA-256. No SimpleFB ELF, boot-classpath JAR, or Composer toolchain/link lineage was modified.

This was one full guest flight; no 10-pass reproducibility claim is made. The OKF branch/log were updated. `tools/validate_okf.py` is absent, so the optional OKF validator could not be run.

## Evidence files

- Built vendor image: `artifacts-t683/vendor.img`
- Fix initramfs: `artifacts-t683/fix-t683.cpio.gz`
- Successful flight: `artifacts-t683/flight-t670/` (UART, numbered UART, compressed UART, QEMU stderr, command, preflight, and flight-end record)
- Initial QMP bind failure: `artifacts-t683/flight/`
- Step heartbeat: `progress-t683.txt` and `heartbeat.json`
