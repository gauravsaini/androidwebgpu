#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T667:
vold-posix-acl-enotsup-user-unlock-android-fg-fatal (Chain A)

PINNED MECHANISM:
1. Root Cause:
   In T666, ext4 userdata was loop-mounted on /data via /dev/loop2, resolving the T665 UserDataPreparer
   getxattr ENOTSUP wipe path and keeping system.img byte-identical to preserve ART boot profile signatures.
   However, user unlock failed at vold:
     "vold: Failed to set default ACL on /data/media/0: Operation not supported on transport endpoint"
   Kernel config inspection of the canonical Linux 4.19 kernel (05c43917...) proves that:
     # CONFIG_EXT4_FS_POSIX_ACL is not set
     CONFIG_EXT4_FS=y
     CONFIG_TMPFS_POSIX_ACL=y
     CONFIG_TMPFS_XATTR=y
     CONFIG_FS_POSIX_ACL=y
   Because CONFIG_EXT4_FS_POSIX_ACL is omitted, ext4_xattr_handlers does not include POSIX ACL handlers.
   Calling setxattr("/data/media/0", "system.posix_acl_default", ...) returns EOPNOTSUPP (errno 95).
   StorageManagerService throws ServiceSpecificException, causing UserDataPreparer.prepareUserDataLI to
   trigger user destruction and a recovery reboot attempt, crashing system_server with an android.fg FATAL.

2. Pinned Fix Point (Filesystem/Mount Arrangement ONLY):
   Fix by the pinned mechanism at the filesystem/mount layer without touching services.jar or reverting
   /data to tmpfs:
   - Mount tmpfs on /data/media in early-ramdisk (and verify/mount in t636-diag.sh).
   - Under Linux 4.19 with CONFIG_TMPFS_POSIX_ACL=y, every tmpfs superblock automatically gets SB_POSIXACL,
     and shmem_xattr_handlers includes shmem_posix_acl_default_handler.
   - Calling setxattr("system.posix_acl_default") on /data/media/0 succeeds natively with return code 0.
   - All other /data directories remain on the ext4 mount (/dev/loop2), preserving the clean ENODATA
     serial path and preventing any regression of prior stages.

Base Image (T666): SHA-256 34463d5c23b13e1f58707462236bbfbc43f326249da3a578c9a01576c25a2a6c
Canonical Kernel:  SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
Output:
  guest-image/t667-vold-posix-acl-enotsup-user-unlock-android-fg-fatal-initramfs.cpio.gz
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
BASE_IMAGE = ROOT / "guest-inputs/t666-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t667-vold-posix-acl-enotsup-user-unlock-android-fg-fatal-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t667-manifest.json"

EXPECTED_BASE_SHA256 = "34463d5c23b13e1f58707462236bbfbc43f326249da3a578c9a01576c25a2a6c"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"
EXPECTED_SYSTEM_IMG_SHA256 = "7527d1cf03a65f90a971c9b1b3f77b024b7df9addbc2b868648df415a7777a08"

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


def format_cpio_entry(name: str, content: bytes, mode: int = 0o100755, ino: int = 89999) -> bytes:
    name_bytes = name.encode("ascii") + b"\0"
    namesize = len(name_bytes)
    filesize = len(content)
    hdr = (
        f"070701"
        f"{ino:08x}"
        f"{mode:08x}"
        f"{0:08x}"
        f"{0:08x}"
        f"{1:08x}"
        f"{0:08x}"
        f"{filesize:08x}"
        f"{0:08x}"
        f"{0:08x}"
        f"{0:08x}"
        f"{0:08x}"
        f"{namesize:08x}"
        f"{0:08x}"
    ).encode("ascii")

    out = bytearray()
    out.extend(hdr)
    out.extend(name_bytes)
    pad_hdr = pad4(len(out))
    out.extend(b"\0" * pad_hdr)
    out.extend(content)
    pad_data = pad4(filesize)
    out.extend(b"\0" * pad_data)
    return bytes(out)


def patch_early_ramdisk(original: str) -> str:
    """Mount tmpfs on /data/media to enable POSIX default ACL support via CONFIG_TMPFS_POSIX_ACL."""
    target_pattern = 'echo "[T666] userdata filesystem initialization complete"\n'
    replacement = (
        'echo "[T666] userdata filesystem initialization complete"\n'
        '# T667: Pinned filesystem/mount arrangement fix: mount tmpfs on /data/media\n'
        '# Canonical Linux 4.19 kernel lacks CONFIG_EXT4_FS_POSIX_ACL=y, causing setxattr of\n'
        '# system.posix_acl_default on ext4 /data/media/0 to fail with EOPNOTSUPP.\n'
        '# With CONFIG_TMPFS_POSIX_ACL=y, tmpfs supports POSIX ACLs natively.\n'
        '"$TOYBOX" mkdir -p /data/media 2>/dev/null || :\n'
        '"$TOYBOX" mount -t tmpfs -o rw,nosuid,nodev,mode=0770,uid=1023,gid=1023 tmpfs /data/media 2>/dev/null || :\n'
        '"$TOYBOX" chmod 0770 /data/media 2>/dev/null || :\n'
        '"$TOYBOX" chown 1023:1023 /data/media 2>/dev/null || :\n'
        'echo "[T667] tmpfs mounted on /data/media"\n'
        'echo "[T667] userdata and media filesystem initialization complete"\n'
    )

    if target_pattern not in original:
        raise ValueError("Could not find target pattern in early-ramdisk!")

    return original.replace(target_pattern, replacement, 1)


def patch_diag_script(script_text: str) -> str:
    """Enhance t636-diag.sh with /data/media tmpfs mount verification and monitoring."""
    # 1. In do_prep: ensure /data/media is mounted as tmpfs
    prep_marker = '"$TOYBOX" chmod 0700 /data/system_ce/0 2>/dev/null || :\n'
    prep_addition = (
        '"$TOYBOX" chmod 0700 /data/system_ce/0 2>/dev/null || :\n'
        '    # T667: Ensure /data/media is mounted as tmpfs for POSIX ACL support\n'
        '    if ! "$TOYBOX" mountpoint -q /data/media 2>/dev/null; then\n'
        '        "$TOYBOX" mkdir -p /data/media 2>/dev/null || :\n'
        '        "$TOYBOX" mount -t tmpfs -o rw,nosuid,nodev,mode=0770,uid=1023,gid=1023 tmpfs /data/media 2>/dev/null || :\n'
        '        "$TOYBOX" chmod 0770 /data/media 2>/dev/null || :\n'
        '        "$TOYBOX" chown 1023:1023 /data/media 2>/dev/null || :\n'
        '        mark "T667_TMPFS_DATA_MEDIA_MOUNTED"\n'
        '    else\n'
        '        mark "T667_TMPFS_DATA_MEDIA_ALREADY_MOUNTED"\n'
        '    fi\n'
    )
    if prep_marker in script_text:
        script_text = script_text.replace(prep_marker, prep_addition, 1)

    # 2. Add media_fs monitoring to T642_BOOT_STATE
    data_fs_pattern = 'data_fs=$("$TOYBOX" stat -f -c "%T" /data 2>/dev/null || echo "unknown")\n'
    media_fs_addition = (
        'data_fs=$("$TOYBOX" stat -f -c "%T" /data 2>/dev/null || echo "unknown")\n'
        '        media_fs=$("$TOYBOX" stat -f -c "%T" /data/media 2>/dev/null || echo "unknown")\n'
    )
    if data_fs_pattern in script_text:
        script_text = script_text.replace(data_fs_pattern, media_fs_addition, 1)

    old_boot_marker = 'data_fs=$data_fs"'
    new_boot_marker = 'data_fs=$data_fs media_fs=$media_fs"'
    if old_boot_marker in script_text:
        script_text = script_text.replace(old_boot_marker, new_boot_marker, 1)

    return script_text


def main():
    t0 = time.monotonic()
    free_gib = check_disk_space()
    print(f"[T667 BUILDER] Disk check: {free_gib:.2f} GiB available.")

    # 1. Verify base image (T666)
    if not BASE_IMAGE.is_file():
        raise FileNotFoundError(f"Base image missing: {BASE_IMAGE}")
    base_stat = BASE_IMAGE.stat()
    if base_stat.st_nlink < 2:
        print(f"[T667 BUILDER] WARNING: Base image st_nlink={base_stat.st_nlink} (expected >1). Re-verifying hardlink...")
    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        raise ValueError(f"Base image SHA-256 mismatch: {base_sha} != {EXPECTED_BASE_SHA256}")
    print(f"[T667 BUILDER] Base image verified: {BASE_IMAGE} ({base_sha})")

    # 2. Verify canonical kernel
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.is_file():
        raise FileNotFoundError(f"Kernel missing: {kernel_path}")
    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        raise ValueError(f"Kernel SHA-256 mismatch: {kernel_sha} != {EXPECTED_KERNEL_SHA256}")
    print(f"[T667 BUILDER] Canonical kernel verified: {kernel_path} ({kernel_sha})")

    # 3. Stream base CPIO and build ONE fix initramfs
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    patched_ramdisk = False
    patched_diag = False
    verified_system_img = False
    verified_userdata_img = False

    print(f"[T667 BUILDER] Streaming base CPIO and applying T667 filesystem/mount arrangement fix...")
    with gzip.open(BASE_IMAGE, "rb") as fin, gzip.open(PARTIAL_IMAGE, "wb", compresslevel=6) as fout:
        while True:
            hdr = fin.read(110)
            if len(hdr) < 110 or not hdr.startswith(b"070701"):
                break

            namesize = int(hdr[94:102], 16)
            filesize = int(hdr[54:62], 16)
            name_bytes = fin.read(namesize)
            name_str = name_bytes[:-1].decode("latin1", errors="replace")
            pad_name = pad4(110 + namesize)
            if pad_name:
                fin.read(pad_name)

            if name_str == "TRAILER!!!":
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)
                break

            if name_str == "early-ramdisk":
                raw_content = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                orig_text = raw_content.decode("utf-8")
                patched_text = patch_early_ramdisk(orig_text)
                patched_bytes = patched_text.encode("utf-8")
                entry = format_cpio_entry("early-ramdisk", patched_bytes, mode=0o100755, ino=89001)
                fout.write(entry)
                patched_ramdisk = True
                print(f"[T667 BUILDER] Patched early-ramdisk (diff: {len(patched_bytes) - len(raw_content)} bytes)")

            elif name_str == "t636-diag.sh":
                raw_content = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                orig_text = raw_content.decode("utf-8")
                patched_text = patch_diag_script(orig_text)
                patched_bytes = patched_text.encode("utf-8")
                entry = format_cpio_entry("t636-diag.sh", patched_bytes, mode=0o100755, ino=89002)
                fout.write(entry)
                patched_diag = True
                print(f"[T667 BUILDER] Patched t636-diag.sh (diff: {len(patched_bytes) - len(raw_content)} bytes)")

            elif name_str == "ramdisk-images/system.img":
                print(f"[T667 BUILDER] Verifying embedded system.img ({filesize} bytes)...")
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)

                h_sys = hashlib.sha256()
                rem = filesize
                while rem > 0:
                    c = fin.read(min(rem, CHUNK))
                    if not c:
                        raise EOFError("Unexpected EOF reading system.img data!")
                    h_sys.update(c)
                    fout.write(c)
                    rem -= len(c)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                    fout.write(b"\0" * pad_data)

                sys_sha = h_sys.hexdigest()
                if sys_sha != EXPECTED_SYSTEM_IMG_SHA256:
                    raise ValueError(f"system.img SHA-256 mismatch: {sys_sha} != {EXPECTED_SYSTEM_IMG_SHA256}")
                verified_system_img = True
                print(f"[T667 BUILDER] Verified system.img byte-identical SHA: {sys_sha}")

            elif name_str == "ramdisk-images/userdata.img":
                print(f"[T667 BUILDER] Streaming existing ext4 userdata.img ({filesize} bytes)...")
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)

                rem = filesize
                while rem > 0:
                    c = fin.read(min(rem, CHUNK))
                    if not c:
                        raise EOFError("Unexpected EOF reading userdata.img data!")
                    fout.write(c)
                    rem -= len(c)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                    fout.write(b"\0" * pad_data)
                verified_userdata_img = True
                print(f"[T667 BUILDER] Streamed ext4 userdata.img OK.")

            else:
                # Stream other files unchanged
                fout.write(hdr)
                fout.write(name_bytes)
                if pad_name:
                    fout.write(b"\0" * pad_name)
                rem = filesize
                while rem > 0:
                    c = fin.read(min(rem, CHUNK))
                    if not c:
                        raise EOFError(f"Unexpected EOF reading {name_str} data!")
                    fout.write(c)
                    rem -= len(c)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                    fout.write(b"\0" * pad_data)

    if not patched_ramdisk:
        raise RuntimeError("early-ramdisk was not patched!")
    if not patched_diag:
        raise RuntimeError("t636-diag.sh was not patched!")
    if not verified_system_img:
        raise RuntimeError("ramdisk-images/system.img was not verified!")
    if not verified_userdata_img:
        raise RuntimeError("ramdisk-images/userdata.img was not verified!")

    # Clean old cpio.gz in guest-image
    for old_img in OUTPUT_DIR.glob("t667-*.cpio.gz"):
        old_img.unlink()

    # Move to final output image atomically
    os.replace(PARTIAL_IMAGE, OUTPUT_IMAGE)
    fix_sha = sha256_file(OUTPUT_IMAGE)
    fix_bytes = OUTPUT_IMAGE.stat().st_size
    elapsed = time.monotonic() - t0

    print(f"[T667 BUILDER] Fix image created successfully!")
    print(f"[T667 BUILDER] Path: {OUTPUT_IMAGE}")
    print(f"[T667 BUILDER] SHA-256: {fix_sha} ({fix_bytes} bytes in {elapsed:.2f}s)")

    manifest = {
        "track": "T667",
        "description": "vold-posix-acl-enotsup-user-unlock-android-fg-fatal",
        "pinned_mechanism": "Mount tmpfs on /data/media in early-ramdisk with native POSIX ACL support (CONFIG_TMPFS_POSIX_ACL=y), allowing vold SetDefaultAcl to succeed natively while preserving ext4 /data mount on /dev/loop2 and user.serial ENODATA semantics",
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
        "system_img_sha256": EXPECTED_SYSTEM_IMG_SHA256,
        "fix_image": {
            "path": str(OUTPUT_IMAGE),
            "sha256": fix_sha,
            "bytes": fix_bytes,
        },
        "built_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }
    with open(MANIFEST_FILE, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")
    print(f"[T667 BUILDER] Wrote manifest: {MANIFEST_FILE}")


if __name__ == "__main__":
    main()
