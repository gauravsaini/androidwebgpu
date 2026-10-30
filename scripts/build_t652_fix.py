#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for T652:
1. Pinned SettingsProvider SQLiteCantOpenDatabaseException mechanism:
   In SystemServer.startOtherServices() at line 1551, ActivityManagerService.installSystemProviders()
   initializes SettingsProvider.
   SettingsProvider is DirectBootAware, accessing its database at
   /data/user_de/0/com.android.providers.settings/databases/settings.db.
   During boot, /data/user_de/0 was prepared by vold (lines 3859, 3865, 3866),
   but package data directories were never created by installd because installd
   was never started (class main was under 'on nonencrypted', which was never triggered
   because early-ramdisk mounted tmpfs on /data without invoking do_mount_all).
   PackageManagerService.<init> attempted to create app data directories via
   Installer.createAppDataBatched(), but timed out after 10s (line 20852) with
   InstallerException: time out waiting for the installer to be ready.
   Consequently, /data/user_de/0/com.android.providers.settings was never created.
   ContextImpl.getDatabasePath() attempted mkdir("/data/user_de/0/com.android.providers.settings/databases"),
   which failed with ENOENT (line 28339), causing SQLiteConnection.nativeOpen() to fail with
   code 14 SQLITE_CANTOPEN (line 28341), crashing system_server (line 28344 am_crash).
2. In ramdisk-images/system.img:
   - Patch init.rc at offset 398934016:
     Replace: b"write /dev/kmsg T651_SDK_SANDBOX_FIX_APPLIED          \\n" (55 bytes)
     With:    b"start installd\\nwrite /dev/kmsg T652_FIX_APPLIED        \\n" (55 bytes)
     This instructs init to launch the installd daemon during post-fs-data.
3. In t636-diag.sh:
   - In do_prep:
     Start installd (/system/bin/start installd).
     Pre-create /data/user_de/0/com.android.providers.settings/databases with
     ownership 1000:1000 (system:system) and permissions 0700.
     Emit diagnostic marker T652_SETTINGSPROVIDER_DATABASES_DIR_EXISTS.
4. Stream CPIO archive into exactly ONE fix image:
   guest-image/t652-system-server-startotherservices-settingsprovider-sqlite-cantopen-initramfs.cpio.gz.
"""

from __future__ import annotations

import gzip
import hashlib
import json
import os
import struct
import sys
import time
from pathlib import Path

ROOT = Path("/Users/Shared/wt-track652-system-server-startotherservices-settingsprovider-sqlite-cantopen")
BASE_IMAGE = ROOT / "guest-inputs/t651-system-server-packagemanager-missing-sdk-sandbox-initramfs.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t652-system-server-startotherservices-settingsprovider-sqlite-cantopen-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t652-manifest.json"
FLIGHT_MANIFEST = ROOT / "flights" / "flight-01" / "guest-image-manifest.json"

EXPECTED_BASE_SHA256 = "811fcb03e4e23305cc5bd18997e73ee9c5549dd0669b5ac1b2df629af0469ec7"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"

SYSTEM_IMG_SIZE = 2059816960
INIT_DATA_OFFSET = 398934016
INIT_ALLOCATED_SIZE = 15 * 4096

CHUNK = 8 * 1024 * 1024


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


def build_diag_script(base_script: str) -> bytes:
    script = base_script

    # Anchor for do_prep checks
    anchor = '    # T651: Verify SdkSandbox is present in /system/system_ext/priv-app/'
    t652_check = """    # T652: Start installd and ensure SettingsProvider DE data directory
    mark "T652_PREP_INSTALLD_AND_SETTINGSPROVIDER_DE"
    /system/bin/start installd 2>/dev/null || :
    "$TOY" sleep 0.2 2>/dev/null || :

    # Ensure /data/user_de/0/com.android.providers.settings/databases exists
    # SettingsProvider runs as system (UID 1000, GID 1000)
    "$TOY" mkdir -p /data/user_de/0/com.android.providers.settings/databases 2>/dev/null || :
    "$TOY" chown -R 1000:1000 /data/user_de/0/com.android.providers.settings 2>/dev/null || :
    "$TOY" chmod 0700 /data/user_de/0/com.android.providers.settings 2>/dev/null || :
    "$TOY" chmod 0700 /data/user_de/0/com.android.providers.settings/databases 2>/dev/null || :

    # Also prepare user/0 CE directory just in case
    "$TOY" mkdir -p /data/user/0/com.android.providers.settings/databases 2>/dev/null || :
    "$TOY" chown -R 1000:1000 /data/user/0/com.android.providers.settings 2>/dev/null || :

    if [ -d /data/user_de/0/com.android.providers.settings/databases ]; then
        mark "T652_SETTINGSPROVIDER_DATABASES_DIR_EXISTS"
    else
        mark "T652_SETTINGSPROVIDER_DATABASES_DIR_FAILED"
    fi

"""
    if anchor not in script:
        raise ValueError("Could not find anchor in base_script for do_prep check")
    script = script.replace(anchor, t652_check + anchor, 1)

    return script.encode("utf-8")


def main():
    t0 = time.monotonic()
    print("[T652 BUILDER] Starting T652 initramfs build...")

    if not BASE_IMAGE.exists():
        sys.exit(f"Missing base image: {BASE_IMAGE}")
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.exists():
        sys.exit(f"Missing kernel: {kernel_path}")

    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        sys.exit(f"Kernel SHA mismatch: expected {EXPECTED_KERNEL_SHA256}, got {kernel_sha}")
    print(f"[T652 BUILDER] Verified canonical Image SHA: {kernel_sha}")

    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        sys.exit(f"Base image SHA mismatch: expected {EXPECTED_BASE_SHA256}, got {base_sha}")
    print(f"[T652 BUILDER] Verified T651 base image SHA: {base_sha}")

    # Read extracted base diag script
    diag_script_path = ROOT / "scripts/extracted_t651_diag.sh"
    with open(diag_script_path, "r", encoding="utf-8") as f:
        base_diag = f.read()

    new_diag_bytes = build_diag_script(base_diag)
    print(f"[T652 BUILDER] Prepared updated t636-diag.sh ({len(new_diag_bytes)} bytes)")

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    print(f"[T652 BUILDER] Streaming CPIO archive from {BASE_IMAGE}...")
    orig_system_sha = hashlib.sha256()
    new_system_sha = hashlib.sha256()

    with gzip.open(BASE_IMAGE, "rb") as fin, gzip.open(PARTIAL_IMAGE, "wb", compresslevel=6) as fout:
        while True:
            hdr = fin.read(110)
            if not hdr or len(hdr) < 110:
                break
            if hdr[:6] != b"070701":
                raise ValueError(f"Invalid CPIO header magic: {hdr[:6]}")

            namesize = int(hdr[94:102], 16)
            filesize = int(hdr[54:62], 16)
            name_bytes = fin.read(namesize)
            name_str = name_bytes[:-1].decode("latin1")
            pad_name = pad4(110 + namesize)
            if pad_name:
                fin.read(pad_name)

            if name_str == "ramdisk-images/system.img":
                print(f"[T652 BUILDER] Found system.img ({filesize} bytes), applying init.rc patch...")
                if filesize != SYSTEM_IMG_SIZE:
                    raise ValueError(f"system.img size mismatch: {filesize} != {SYSTEM_IMG_SIZE}")

                fout.write(hdr)
                fout.write(name_bytes)
                fout.write(b"\0" * pad_name)

                # 1. Stream up to INIT_DATA_OFFSET
                rem = INIT_DATA_OFFSET
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    fout.write(chunk)
                    orig_system_sha.update(chunk)
                    new_system_sha.update(chunk)
                    rem -= len(chunk)

                # 2. Patch init.rc extent (61440 bytes)
                orig_extent = fin.read(INIT_ALLOCATED_SIZE)
                orig_system_sha.update(orig_extent)
                old_mark = b"write /dev/kmsg T651_SDK_SANDBOX_FIX_APPLIED          \n"
                new_mark = b"start installd\nwrite /dev/kmsg T652_FIX_APPLIED       \n"
                if len(old_mark) != len(new_mark):
                    raise AssertionError(f"Mark length mismatch: {len(old_mark)} != {len(new_mark)}")
                if old_mark not in orig_extent:
                    raise ValueError("T651 mark anchor not found in init.rc extent!")
                new_extent = orig_extent.replace(old_mark, new_mark, 1)
                if len(new_extent) != INIT_ALLOCATED_SIZE:
                    raise AssertionError(f"Extent size changed: {len(new_extent)} != {INIT_ALLOCATED_SIZE}")
                fout.write(new_extent)
                new_system_sha.update(new_extent)
                print(f"[T652 BUILDER] Patched init.rc mark (T651 -> T652 with start installd).")

                # 3. Stream remainder of system.img
                rem = filesize - (INIT_DATA_OFFSET + INIT_ALLOCATED_SIZE)
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    fout.write(chunk)
                    orig_system_sha.update(chunk)
                    new_system_sha.update(chunk)
                    rem -= len(chunk)

                pad_data = pad4(filesize)
                if pad_data:
                    fout.write(fin.read(pad_data))

            elif name_str == "t636-diag.sh":
                print(f"[T652 BUILDER] Replacing t636-diag.sh ({filesize} -> {len(new_diag_bytes)} bytes)...")
                fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)
                new_entry = format_cpio_entry("t636-diag.sh", new_diag_bytes)
                fout.write(new_entry)

            else:
                fout.write(hdr)
                fout.write(name_bytes)
                fout.write(b"\0" * pad_name)
                rem = filesize
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    fout.write(chunk)
                    rem -= len(chunk)
                pad_data = pad4(filesize)
                if pad_data:
                    fout.write(fin.read(pad_data))

            if name_str == "TRAILER!!!":
                break

    os.replace(PARTIAL_IMAGE, OUTPUT_IMAGE)
    output_sha = sha256_file(OUTPUT_IMAGE)
    output_bytes = os.path.getsize(OUTPUT_IMAGE)
    output_inode = OUTPUT_IMAGE.stat().st_ino
    elapsed = time.monotonic() - t0

    manifest = {
        "track": "T652",
        "description": "Fix system_server SettingsProvider SQLiteCantOpenDatabaseException: start installd and ensure /data/user_de/0/com.android.providers.settings/databases",
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "base_image": str(BASE_IMAGE),
        "base_image_sha256": base_sha,
        "base_image_bytes": BASE_IMAGE.stat().st_size,
        "base_image_inode": BASE_IMAGE.stat().st_ino,
        "output_image": str(OUTPUT_IMAGE),
        "output_image_sha256": output_sha,
        "output_image_bytes": output_bytes,
        "output_image_inode": output_inode,
        "orig_system_sha256": orig_system_sha.hexdigest(),
        "new_system_sha256": new_system_sha.hexdigest(),
        "build_elapsed_seconds": round(elapsed, 2),
        "pinned_mechanism": {
            "root_cause": "SettingsProvider database init requires /data/user_de/0/com.android.providers.settings/databases/settings.db. While /data/user_de/0 was prepared by vold (lines 3859, 3865, 3866), the package data directory was never created by installd because installd was never started (class main was under on nonencrypted, which was never triggered due to diskless tmpfs /data). PackageManagerService timed out after 10s connecting to installd (InstallerException line 20852) and skipped creating app data directories. ContextImpl mkdir failed with ENOENT, throwing SQLiteCantOpenDatabaseException code 14 SQLITE_CANTOPEN at line 28341.",
            "fix_components": [
                "1. init.rc: Added 'start installd' at offset 398934016 so init launches installd daemon during post-fs-data",
                "2. t636-diag.sh do_prep: Calls '/system/bin/start installd' and ensures /data/user_de/0/com.android.providers.settings/databases exists with ownership 1000:1000 (system:system) and mode 0700"
            ]
        },
        "images": {
            "kernel": {
                "path": str(kernel_path),
                "sha256": kernel_sha,
            },
            "initramfs": {
                "path": str(OUTPUT_IMAGE),
                "sha256": output_sha,
                "bytes": output_bytes,
                "inode": output_inode,
            },
            "base_t651_initramfs": {
                "path": str(BASE_IMAGE),
                "sha256": base_sha,
            },
        },
    }

    with open(MANIFEST_FILE, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)

    FLIGHT_MANIFEST.parent.mkdir(parents=True, exist_ok=True)
    with open(FLIGHT_MANIFEST, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)

    print(f"[T652 BUILDER] Build complete in {elapsed:.2f}s!")
    print(f"[T652 BUILDER] Output image: {OUTPUT_IMAGE}")
    print(f"[T652 BUILDER] Output SHA-256: {output_sha} ({output_bytes} bytes, inode {output_inode})")
    print(f"[T652 BUILDER] Manifest written to {MANIFEST_FILE} and {FLIGHT_MANIFEST}")


if __name__ == "__main__":
    main()
