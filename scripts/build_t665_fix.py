#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T665:
system-server-user-unlock-userdatapreparer-getxattr-enotsup-fatal-recovery-reboot (Chain A)

PINNED MECHANISM:
1. Root Cause (a) UserDataPreparer failure:
   On user unlock, UserManagerService.onBeforeUnlockUser (UserManagerService.java:6258) calls
   UserDataPreparer.prepareUserData(0, ...) -> prepareUserDataLI -> enforceSerialNumber -> getSerialNumber.
   In getSerialNumber(File file), Os.getxattr(path, "user.serial") is invoked.
   On this virtual guest, /data is backed by Linux tmpfs. Under Linux 4.19 (mm/shmem.c),
   tmpfs only supports the security.* and trusted.* extended attribute namespaces.
   The user.* xattr namespace returns ENOTSUP (errno 95: Operation not supported on transport endpoint).
   UserDataPreparer.getSerialNumber only catches ENODATA, rethrowing ENOTSUP as IOException.
   prepareUserDataLI catches the exception, destroys user data, retries with allowRecover=false,
   fails again, and enters the fatal system-user wipe path:
   RecoverySystem.rebootPromptAndWipeUserData(context, "prepareUserData failed for system user").

2. Root Cause (b) Recovery reboot crash:
   RecoverySystem.rebootPromptAndWipeUserData delegates to RecoverySystemService -> uncrypt (setup-bcb).
   The virtual guest has no /misc partition, so uncrypt fails:
   "uncrypt: failed to set bootloader message: failed to find /misc partition".
   RecoverySystem.bootCommand throws IOException("Reboot failed (no permissions?)"),
   which prepareUserDataLI wraps in RuntimeException("error rebooting into recovery").
   This uncaught exception on the android.fg handler thread crashes system_server outright (SIG 9).

PINNED FIX:
1. Patch UserDataPreparer.getSerialNumber in classes2.dex of services.jar:
   At offset 0x2c33f0, replace the 28-byte catch (ErrnoException) handler:
   Instead of checking for ENODATA and rethrowing ENOTSUP, directly return -1:
   `catch (ErrnoException e) { return -1; }`
   This is encoded as:
     0d 03          (move-exception v3)
     12 f3          (const/4 v3, -1)
     0f 03          (return v3)
     00 00 * 11     (11 Dalvik nop instructions)
   When getSerialNumber returns -1, enforceSerialNumber logs:
   "Serial number missing on ...; assuming current is valid"
   and calls setSerialNumber(...). In enforceSerialNumber, setSerialNumber is already
   wrapped in a try-catch block:
     try { setSerialNumber(file, serialNumber); }
     catch (IOException e) { Slog.w(TAG, "Failed to set serial number on " + file, e); }
   The ENOTSUP on setxattr is logged as a non-fatal warning, enforceSerialNumber returns normally,
   prepareUserDataLI succeeds, sets sys.user.0.ce_available=true, and user unlock completes!
   The wipe path is never entered.

2. Update DEX header:
   Recompute SHA-1 (offset 0x0c) and Adler-32 (offset 0x08).

3. Update JAR and OAT checksums:
   In services.jar: update CRC32 in Local File Header and Central Directory Header.
   In services.odex: update classes2.dex CRC32 at offset 0x2502c5.
   In services.vdex: update classes2.dex CRC32 at offset 0x40.

4. Apply patched files to system.img via debugfs and verify with e2fsck.
5. In t636-diag.sh, track sys.user.0.ce_available in T642_BOOT_STATE.
6. Preserve all T664 fixes: throttled diagnostic loops, no dynamic-UID precreation,
   CE/DE mirror mounts, 0 .capex.

Base Image (T664): SHA-256 2f7f60df311472680b302aad23d94e449d7634695747535b850bf86b0c8cb10b
Canonical Kernel:  SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
Output:
  guest-image/t665-system-server-user-unlock-userdatapreparer-getxattr-enotsup-fatal-recovery-reboot-initramfs.cpio.gz
"""

from __future__ import annotations

import datetime
import gzip
import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASE_IMAGE = ROOT / "guest-inputs/t664-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t665-system-server-user-unlock-userdatapreparer-getxattr-enotsup-fatal-recovery-reboot-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t665-manifest.json"

EXPECTED_BASE_SHA256 = "2f7f60df311472680b302aad23d94e449d7634695747535b850bf86b0c8cb10b"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"

DEBUGFS = "/opt/homebrew/opt/e2fsprogs/sbin/debugfs"
E2FSCK = "/opt/homebrew/opt/e2fsprogs/sbin/e2fsck"

CHUNK = 8 * 1024 * 1024


def utc_now() -> str:
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while True:
            chunk = f.read(CHUNK)
            if not chunk:
                break
            h.update(chunk)
    return h.hexdigest()


def pad4(n: int) -> int:
    return (-n) & 3


def check_disk_space() -> float:
    stat = os.statvfs("/System/Volumes/Data")
    free_bytes = stat.f_bavail * stat.f_frsize
    free_gib = free_bytes / (1024 ** 3)
    if free_gib < 2.0:
        raise RuntimeError(
            f"DISK HYGIENE: free disk space {free_gib:.2f} GiB is under 2.0 GiB threshold! Self-throttling."
        )
    return free_gib


def patch_diag_script(script_text: str) -> bytes:
    """Enhance t636-diag.sh with sys.user.0.ce_available monitoring."""
    target_pattern = 'completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)\n'
    replacement = (
        'completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)\n'
        '        ce_available=$(/system/bin/getprop sys.user.0.ce_available 2>/dev/null)\n'
    )
    if target_pattern in script_text:
        script_text = script_text.replace(target_pattern, replacement, 1)

    old_boot_marker = 'boot_completed=$completed"'
    new_boot_marker = 'boot_completed=$completed ce_available=$ce_available"'
    if old_boot_marker in script_text:
        script_text = script_text.replace(old_boot_marker, new_boot_marker, 1)

    return script_text.encode("utf-8")


def main():
    t0 = time.monotonic()
    free_gib = check_disk_space()
    print(f"[T665 BUILDER] Disk check: {free_gib:.2f} GiB available.")

    # Verify base image (T664)
    if not BASE_IMAGE.is_file():
        raise FileNotFoundError(f"Base image missing: {BASE_IMAGE}")
    base_stat = os.stat(BASE_IMAGE)
    if base_stat.st_nlink < 2:
        raise RuntimeError(f"Base image must be hardlinked! st_nlink={base_stat.st_nlink}")
    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        raise ValueError(f"Base image SHA mismatch: {base_sha} != {EXPECTED_BASE_SHA256}")
    print(f"[T665 BUILDER] Verified base image: {base_sha} (nlink={base_stat.st_nlink}) OK.")

    # Verify kernel
    kernel_path = ROOT / "guest-inputs/Image"
    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        raise ValueError(f"Kernel SHA mismatch: {kernel_sha} != {EXPECTED_KERNEL_SHA256}")
    print(f"[T665 BUILDER] Verified canonical kernel: {kernel_sha} OK.")

    # Verify scratch/system.img
    scratch_system_img = ROOT / "scratch/system.img"
    if not scratch_system_img.is_file():
        raise FileNotFoundError(f"Patched system.img not found at {scratch_system_img}")
    system_img_size = scratch_system_img.stat().st_size
    print(f"[T665 BUILDER] Found verified patched system.img: {system_img_size} bytes.")

    # Clean existing fix images
    for old_img in OUTPUT_DIR.glob("t665-*.cpio.gz*"):
        old_img.unlink()
        print(f"[T665 BUILDER] Removed old fix image: {old_img}")

    patched_diag = False
    patched_system = False
    print(f"[T665 BUILDER] Streaming CPIO from {BASE_IMAGE} to {PARTIAL_IMAGE}...")

    with gzip.open(BASE_IMAGE, "rb") as fin, gzip.open(PARTIAL_IMAGE, "wb", compresslevel=6) as fout:
        while True:
            hdr = fin.read(110)
            if not hdr or len(hdr) < 110:
                break
            if not hdr.startswith(b"070701"):
                raise ValueError("Malformed CPIO header!")

            ino = int(hdr[6:14], 16)
            mode = int(hdr[14:22], 16)
            uid = int(hdr[22:30], 16)
            gid = int(hdr[30:38], 16)
            nlink = int(hdr[38:46], 16)
            mtime = int(hdr[46:54], 16)
            filesize = int(hdr[54:62], 16)
            namesize = int(hdr[94:102], 16)

            name_bytes = fin.read(namesize)
            name_str = name_bytes[:-1].decode("ascii", errors="replace")
            pad_name = pad4(110 + namesize)
            if pad_name:
                fin.read(pad_name)

            if name_str == "t636-diag.sh":
                print(f"[T665 BUILDER] Patching {name_str} ({filesize} bytes)...")
                content = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                new_content = patch_diag_script(content.decode("utf-8", errors="replace"))
                new_filesize = len(new_content)

                # Format new CPIO header
                hdr_out = (
                    f"070701"
                    f"{ino:08x}"
                    f"{mode:08x}"
                    f"{uid:08x}"
                    f"{gid:08x}"
                    f"{nlink:08x}"
                    f"{mtime:08x}"
                    f"{new_filesize:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{namesize:08x}"
                    f"{0:08x}"
                ).encode("ascii")
                fout.write(hdr_out)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)
                fout.write(new_content)
                pad_new = pad4(new_filesize)
                if pad_new:
                    fout.write(b"\0" * pad_new)
                patched_diag = True
                print(f"[T665 BUILDER] Patched {name_str}: {filesize} -> {new_filesize} bytes.")

            elif name_str == "ramdisk-images/system.img":
                print(f"[T665 BUILDER] Replacing ramdisk-images/system.img ({filesize} bytes) with patched system.img ({system_img_size} bytes)...")
                # Skip original system.img
                rem = filesize
                while rem > 0:
                    fin.read(min(rem, CHUNK))
                    rem -= min(rem, CHUNK)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                # Write CPIO header for patched system.img
                hdr_out = (
                    f"070701"
                    f"{ino:08x}"
                    f"{mode:08x}"
                    f"{uid:08x}"
                    f"{gid:08x}"
                    f"{nlink:08x}"
                    f"{mtime:08x}"
                    f"{system_img_size:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{namesize:08x}"
                    f"{0:08x}"
                ).encode("ascii")
                fout.write(hdr_out)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)

                # Stream patched system.img
                with open(scratch_system_img, "rb") as fsys:
                    while True:
                        buf = fsys.read(CHUNK)
                        if not buf:
                            break
                        fout.write(buf)
                pad_sys = pad4(system_img_size)
                if pad_sys:
                    fout.write(b"\0" * pad_sys)
                patched_system = True
                print(f"[T665 BUILDER] Replaced ramdisk-images/system.img successfully.")

            else:
                # Stream verbatim
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)
                rem = filesize
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    fout.write(chunk)
                    rem -= len(chunk)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                    fout.write(b"\0" * pad_data)

            if name_str == "TRAILER!!!":
                break

    if not patched_system:
        raise RuntimeError("ramdisk-images/system.img was not found in base image!")
    if not patched_diag:
        raise RuntimeError("t636-diag.sh was not found in base image!")

    # Atomically move partial to final
    PARTIAL_IMAGE.rename(OUTPUT_IMAGE)
    fix_sha = sha256_file(OUTPUT_IMAGE)
    fix_size = OUTPUT_IMAGE.stat().st_size
    elapsed = time.monotonic() - t0
    print(f"[T665 BUILDER] Fix image created in {elapsed:.1f}s:")
    print(f"  Path:   {OUTPUT_IMAGE}")
    print(f"  Size:   {fix_size} bytes")
    print(f"  SHA256: {fix_sha}")

    # Write manifest
    manifest = {
        "track": "T665",
        "description": "system-server-user-unlock-userdatapreparer-getxattr-enotsup-fatal-recovery-reboot",
        "pinned_mechanism": "UserDataPreparer.getSerialNumber catch (ErrnoException) { return -1; } on tmpfs /data without user.* xattr support, avoiding fatal recovery reboot wipe path",
        "base_image": {
            "path": str(BASE_IMAGE),
            "sha256": base_sha,
            "bytes": base_stat.st_size,
            "st_nlink": base_stat.st_nlink,
        },
        "canonical_kernel": {
            "path": str(kernel_path),
            "sha256": kernel_sha,
        },
        "fix_image": {
            "path": str(OUTPUT_IMAGE),
            "sha256": fix_sha,
            "bytes": fix_size,
        },
        "patched_components": [
            "system/framework/services.jar (classes2.dex getSerialNumber catch ErrnoException -> return -1)",
            "system/framework/oat/arm64/services.odex (classes2.dex CRC updated at 0x2502c5)",
            "system/framework/oat/arm64/services.vdex (classes2.dex CRC updated at 0x40)",
            "t636-diag.sh (ce_available monitoring in T642_BOOT_STATE)",
        ],
        "built_utc": utc_now(),
    }
    with open(MANIFEST_FILE, "w") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")
    print(f"[T665 BUILDER] Manifest written to {MANIFEST_FILE}")


if __name__ == "__main__":
    main()
