#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for T648:
1. Pin permissions manager mechanism:
   PMS.getRequiredPermissionControllerLPr() queries for Intent(ACTION_MANAGE_PERMISSIONS) + CATEGORY_DEFAULT.
   com.android.permissioncontroller is provided by PermissionController.apk in com.android.permission.
   Because apexd failed to activate com.android.permission (compressed capex on non-f2fs /data),
   PMS never scanned /apex/com.android.permission.
2. In ramdisk-images/system.img:
   - Patch directory block of /system/system_ext/priv-app (block 458818 at offset 1879318528):
     Add PermissionController (ino 4393) and SafetyCenterResources (ino 4395) so PMS scans
     PermissionController.apk as a system_ext privileged app.
   - Patch init.rc mark at offset 398934016 (T647 -> T648).
3. In t636-diag.sh:
   - In do_prep, verify PermissionController.apk presence and add fallback bind-mount if needed.
4. Stream CPIO archive into exactly ONE fix image:
   guest-image/t648-system-server-packagemanager-no-permission-controller-initramfs.cpio.gz.
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

ROOT = Path("/Users/Shared/wt-track648-system-server-packagemanager-no-permission-controller")
BASE_IMAGE = ROOT / "guest-inputs/t647-system-server-deviceconfig-listener-initramfs.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t648-system-server-packagemanager-no-permission-controller-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t648-manifest.json"
FLIGHT_MANIFEST = ROOT / "flights" / "flight-01" / "guest-image-manifest.json"

EXPECTED_BASE_SHA256 = "9333d37efd516d3b4ed614b6755b5b9c88e9d5b114cb4e4105f338c5ac9ecbef"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"

SYSTEM_IMG_SIZE = 2059816960
INIT_INODE_SIZE_OFFSET = 134292740
INIT_DATA_OFFSET = 398934016
INIT_ALLOCATED_SIZE = 15 * 4096
T647_INITRC_SIZE = 60142
T648_INITRC_SIZE = 60142  # Same size

SYSTEM_EXT_PRIVAPP_DIR_OFFSET = 458818 * 4096  # 1879318528
DIR_BLOCK_SIZE = 4096

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


def patch_system_ext_privapp_dir(block_bytes: bytes) -> bytes:
    blk = bytearray(block_bytes)
    # Check WallpaperCropper at offset 228
    ino, rec_len, name_len, file_type = struct.unpack("<IHBB", blk[228:236])
    name = blk[236:236+name_len].decode("ascii", errors="replace")
    if name != "WallpaperCropper" or ino != 7018:
        raise ValueError(f"Expected WallpaperCropper at 228, got {name} ino {ino}")
    if rec_len != 3868:
        raise ValueError(f"Expected rec_len 3868 at 228, got {rec_len}")

    # 1. Update WallpaperCropper rec_len to 24 (8 hdr + 16 name)
    struct.pack_into("<H", blk, 228 + 4, 24)

    # 2. Add PermissionController at 252 (8 hdr + 20 name = 28 bytes)
    name1 = b"PermissionController"
    hdr1 = struct.pack("<IHBB", 4393, 28, len(name1), 2)
    blk[252:252+8] = hdr1
    blk[252+8:252+8+len(name1)] = name1

    # 3. Add SafetyCenterResources at 280 (fills remaining block: 4096 - 280 = 3816 bytes)
    name2 = b"SafetyCenterResources"
    rec2 = DIR_BLOCK_SIZE - 280
    hdr2 = struct.pack("<IHBB", 4395, rec2, len(name2), 2)
    blk[280:280+8] = hdr2
    blk[280+8:280+8+len(name2)] = name2
    # Zero remaining space
    used = 280 + 8 + len(name2)
    blk[used:DIR_BLOCK_SIZE] = b"\x00" * (DIR_BLOCK_SIZE - used)

    return bytes(blk)


def build_diag_script(base_script: str) -> bytes:
    script = base_script

    # Anchor for do_prep checks
    anchor = '    if [ -f /apex/com.android.configinfrastructure/javalib/framework-configinfrastructure.jar ]; then'
    t648_check = """    # T648: Verify PermissionController is present in /system/system_ext/priv-app/
    if [ -f /system/system_ext/priv-app/PermissionController/PermissionController.apk ]; then
        mark "T648_PERMISSION_CONTROLLER_APK_EXISTS"
    else
        mark "T648_PERMISSION_CONTROLLER_APK_MISSING_TRYING_FALLBACK"
        "$TOY" mount -o remount,rw /system 2>/dev/null || :
        "$TOY" mkdir -p /system/system_ext/priv-app/PermissionController 2>/dev/null || :
        "$TOY" mount -o bind /system/system_ext/apex/com.android.permission/priv-app/PermissionController@UQ1A.231205.015 /system/system_ext/priv-app/PermissionController 2>/dev/null || :
        if [ -f /system/system_ext/priv-app/PermissionController/PermissionController.apk ]; then
            mark "T648_PERMISSION_CONTROLLER_BIND_MOUNTED_OK"
        else
            mark "T648_PERMISSION_CONTROLLER_BIND_MOUNT_FAILED"
        fi
    fi

    if [ -f /system/system_ext/priv-app/SafetyCenterResources/SafetyCenterResources.apk ]; then
        mark "T648_SAFETY_CENTER_RESOURCES_APK_EXISTS"
    else
        "$TOY" mount -o remount,rw /system 2>/dev/null || :
        "$TOY" mkdir -p /system/system_ext/priv-app/SafetyCenterResources 2>/dev/null || :
        "$TOY" mount -o bind /system/system_ext/apex/com.android.permission/priv-app/SafetyCenterResources@UQ1A.231205.015 /system/system_ext/priv-app/SafetyCenterResources 2>/dev/null || :
    fi

"""
    if anchor not in script:
        raise ValueError("Could not find anchor in base_script for do_prep check")
    script = script.replace(anchor, t648_check + anchor, 1)

    return script.encode("utf-8")


def main():
    t0 = time.monotonic()
    print("[T648 BUILDER] Starting T648 initramfs build...")

    if not BASE_IMAGE.exists():
        sys.exit(f"Missing base image: {BASE_IMAGE}")
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.exists():
        sys.exit(f"Missing kernel: {kernel_path}")

    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        sys.exit(f"Kernel SHA mismatch: expected {EXPECTED_KERNEL_SHA256}, got {kernel_sha}")
    print(f"[T648 BUILDER] Verified canonical Image SHA: {kernel_sha}")

    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        sys.exit(f"Base image SHA mismatch: expected {EXPECTED_BASE_SHA256}, got {base_sha}")
    print(f"[T648 BUILDER] Verified T647 base image SHA: {base_sha}")

    # Read extracted base diag script
    diag_script_path = ROOT / "scripts/extracted_t647_diag.sh"
    with open(diag_script_path, "r", encoding="utf-8") as f:
        base_diag = f.read()

    new_diag_bytes = build_diag_script(base_diag)
    print(f"[T648 BUILDER] Prepared updated t636-diag.sh ({len(new_diag_bytes)} bytes)")

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    print(f"[T648 BUILDER] Streaming CPIO archive from {BASE_IMAGE}...")
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
                print(f"[T648 BUILDER] Found system.img ({filesize} bytes), applying patches...")
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
                old_mark = b"write /dev/kmsg T647_DEVICECONFIG_LISTENER_FIX_APPLIED\n"
                new_mark = b"write /dev/kmsg T648_PERMISSION_CONTROLLER_FIX_APPLIED\n"
                if old_mark not in orig_extent:
                    raise ValueError("T647 mark anchor not found in init.rc extent!")
                new_extent = orig_extent.replace(old_mark, new_mark, 1)
                if len(new_extent) != INIT_ALLOCATED_SIZE:
                    raise AssertionError(f"Extent size changed: {len(new_extent)} != {INIT_ALLOCATED_SIZE}")
                fout.write(new_extent)
                new_system_sha.update(new_extent)
                print(f"[T648 BUILDER] Patched init.rc mark (T647 -> T648).")

                # 3. Stream up to SYSTEM_EXT_PRIVAPP_DIR_OFFSET
                rem = SYSTEM_EXT_PRIVAPP_DIR_OFFSET - (INIT_DATA_OFFSET + INIT_ALLOCATED_SIZE)
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    fout.write(chunk)
                    orig_system_sha.update(chunk)
                    new_system_sha.update(chunk)
                    rem -= len(chunk)

                # 4. Patch /system/system_ext/priv-app directory block (4096 bytes)
                orig_dir_blk = fin.read(DIR_BLOCK_SIZE)
                orig_system_sha.update(orig_dir_blk)
                new_dir_blk = patch_system_ext_privapp_dir(orig_dir_blk)
                if len(new_dir_blk) != DIR_BLOCK_SIZE:
                    raise AssertionError(f"Dir block size changed: {len(new_dir_blk)} != {DIR_BLOCK_SIZE}")
                fout.write(new_dir_blk)
                new_system_sha.update(new_dir_blk)
                print(f"[T648 BUILDER] Patched /system/system_ext/priv-app directory block with PermissionController.")

                # 5. Stream remainder of system.img
                rem = filesize - (SYSTEM_EXT_PRIVAPP_DIR_OFFSET + DIR_BLOCK_SIZE)
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
                print(f"[T648 BUILDER] Replacing t636-diag.sh ({filesize} -> {len(new_diag_bytes)} bytes)...")
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
    elapsed = time.monotonic() - t0

    manifest = {
        "track": "T648",
        "description": "Fix system_server PackageManagerService getRequiredPermissionControllerLPr(): wire PermissionController into /system/system_ext/priv-app",
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "base_image": str(BASE_IMAGE),
        "base_image_sha256": base_sha,
        "output_image": str(OUTPUT_IMAGE),
        "output_image_sha256": output_sha,
        "output_image_bytes": output_bytes,
        "orig_system_sha256": orig_system_sha.hexdigest(),
        "new_system_sha256": new_system_sha.hexdigest(),
        "build_elapsed_seconds": round(elapsed, 2),
        "pinned_mechanism": {
            "component": "com.android.permissioncontroller.permission.ui.ManagePermissionsActivity",
            "package": "com.android.permissioncontroller",
            "intent_action": "android.intent.action.MANAGE_PERMISSIONS",
            "intent_category": "android.intent.category.DEFAULT",
            "providing_apk": "/system/system_ext/apex/com.android.permission/priv-app/PermissionController@UQ1A.231205.015/PermissionController.apk",
            "wiring_target": "/system/system_ext/priv-app/PermissionController",
            "reason_for_empty_set": "apexd failed to activate compressed com.android.permission.capex due to FS_IOC_GETFLAGS failure on non-f2fs /data; PMS only queries active apexes from apexd and standard partition folders, so PermissionController was never scanned",
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
            },
            "base_t647_initramfs": {
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

    print(f"[T648 BUILDER] Build complete in {elapsed:.2f}s!")
    print(f"[T648 BUILDER] Output image: {OUTPUT_IMAGE}")
    print(f"[T648 BUILDER] Output SHA-256: {output_sha} ({output_bytes} bytes)")
    print(f"[T648 BUILDER] Manifest written to {MANIFEST_FILE} and {FLIGHT_MANIFEST}")


if __name__ == "__main__":
    main()
