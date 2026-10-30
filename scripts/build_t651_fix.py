#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for T651:
1. Pinned SDK sandbox mechanism:
   PMS.<init> calls getRequiredSdkSandboxPackageName(computer), which queries:
   computer.queryIntentServicesInternal(new Intent("com.android.sdksandbox.SdkSandboxService"), null, MATCH_SYSTEM_ONLY, 0, myUid, false).
   It asserts matches.size() == 1.
   Because apexd failed to activate com.android.adservices.capex on non-f2fs /data (FS_IOC_GETFLAGS ioctl failure),
   PMS never scanned /apex/com.android.adservices/app.
   SdkSandbox.apk is located at /system/system_ext/apex/com.android.adservices/app/SdkSandbox@UQ1A.231205.015/SdkSandbox.apk
   (directory inode 3665, apk inode 3666).
2. In ramdisk-images/system.img:
   - Patch directory block of /system/system_ext/priv-app (block 458818 at offset 1879318528):
     Shrink ExtServices (ino 4105 at 312) rec_len from 3784 to 24 bytes.
     Add SdkSandbox (ino 3665 at offset 336, rec_len 3760) so PMS scans
     SdkSandbox.apk as a system_ext app providing com.android.sdksandbox.
   - Patch init.rc mark at offset 398934016 (T650 -> T651, exactly 55 bytes).
3. In t636-diag.sh:
   - In do_prep, verify SdkSandbox.apk presence and add fallback bind-mount if needed.
4. Stream CPIO archive into exactly ONE fix image:
   guest-image/t651-system-server-packagemanager-missing-sdk-sandbox-initramfs.cpio.gz.
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

ROOT = Path("/Users/Shared/wt-track651-system-server-packagemanager-missing-sdk-sandbox")
BASE_IMAGE = ROOT / "guest-inputs/t650-system-server-packagemanager-missing-services-extension-initramfs.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t651-system-server-packagemanager-missing-sdk-sandbox-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t651-manifest.json"
FLIGHT_MANIFEST = ROOT / "flights" / "flight-01" / "guest-image-manifest.json"

EXPECTED_BASE_SHA256 = "5cff40e94e4c67465f5e74d64a41a26ee2ecf94d93f0ec88944b22e91b2489f7"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"

SYSTEM_IMG_SIZE = 2059816960
INIT_INODE_SIZE_OFFSET = 134292740
INIT_DATA_OFFSET = 398934016
INIT_ALLOCATED_SIZE = 15 * 4096
T650_INITRC_SIZE = 60142
T651_INITRC_SIZE = 60142  # Same size: 55-byte mark swap

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
    # Check ExtServices at offset 312
    ino, rec_len, name_len, file_type = struct.unpack("<IHBB", blk[312:320])
    name = blk[320:320+name_len].decode("ascii", errors="replace")
    if name != "ExtServices" or ino != 4105:
        raise ValueError(f"Expected ExtServices at 312, got {name} ino {ino}")
    if rec_len != 3784:
        raise ValueError(f"Expected rec_len 3784 at 312, got {rec_len}")

    # 1. Update ExtServices rec_len to 24 (8 hdr + 11 name + 5 pad)
    struct.pack_into("<H", blk, 312 + 4, 24)

    # 2. Add SdkSandbox at 336 (fills remaining block: 4096 - 336 = 3760 bytes)
    name_sdk = b"SdkSandbox"
    rec_sdk = DIR_BLOCK_SIZE - 336
    hdr_sdk = struct.pack("<IHBB", 3665, rec_sdk, len(name_sdk), 2)
    blk[336:336+8] = hdr_sdk
    blk[336+8:336+8+len(name_sdk)] = name_sdk

    # 3. Zero remaining space
    used = 336 + 8 + len(name_sdk)
    blk[used:DIR_BLOCK_SIZE] = b"\x00" * (DIR_BLOCK_SIZE - used)

    # Validate full directory parsing
    pos = 0
    entries = []
    while pos < DIR_BLOCK_SIZE:
        e_ino, e_rec, e_nlen, e_type = struct.unpack("<IHBB", blk[pos:pos+8])
        if e_rec == 0:
            raise ValueError(f"Zero rec_len at pos {pos}")
        e_name = blk[pos+8:pos+8+e_nlen].decode("ascii", errors="replace")
        entries.append((pos, e_ino, e_rec, e_name))
        pos += e_rec
    if pos != DIR_BLOCK_SIZE:
        raise AssertionError(f"Total directory entries length {pos} != {DIR_BLOCK_SIZE}")

    print(f"[T651 BUILDER] Validated patched directory entries: {entries[-4:]}")
    return bytes(blk)


def build_diag_script(base_script: str) -> bytes:
    script = base_script

    # Anchor for do_prep checks
    anchor = '    # T650: Verify ExtServices is present in /system/system_ext/priv-app/'
    t651_check = """    # T651: Verify SdkSandbox is present in /system/system_ext/priv-app/
    if [ -f /system/system_ext/priv-app/SdkSandbox/SdkSandbox.apk ]; then
        mark "T651_SDKSANDBOX_APK_EXISTS"
    else
        mark "T651_SDKSANDBOX_APK_MISSING_TRYING_FALLBACK"
        "$TOY" mount -o remount,rw /system 2>/dev/null || :
        "$TOY" mkdir -p /system/system_ext/priv-app/SdkSandbox 2>/dev/null || :
        "$TOY" mount -o bind /system/system_ext/apex/com.android.adservices/app/SdkSandbox@UQ1A.231205.015 /system/system_ext/priv-app/SdkSandbox 2>/dev/null || :
        if [ -f /system/system_ext/priv-app/SdkSandbox/SdkSandbox.apk ]; then
            mark "T651_SDKSANDBOX_BIND_MOUNTED_OK"
        else
            mark "T651_SDKSANDBOX_BIND_MOUNT_FAILED"
        fi
    fi

"""
    if anchor not in script:
        raise ValueError("Could not find anchor in base_script for do_prep check")
    script = script.replace(anchor, t651_check + anchor, 1)

    return script.encode("utf-8")


def main():
    t0 = time.monotonic()
    print("[T651 BUILDER] Starting T651 initramfs build...")

    if not BASE_IMAGE.exists():
        sys.exit(f"Missing base image: {BASE_IMAGE}")
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.exists():
        sys.exit(f"Missing kernel: {kernel_path}")

    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        sys.exit(f"Kernel SHA mismatch: expected {EXPECTED_KERNEL_SHA256}, got {kernel_sha}")
    print(f"[T651 BUILDER] Verified canonical Image SHA: {kernel_sha}")

    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        sys.exit(f"Base image SHA mismatch: expected {EXPECTED_BASE_SHA256}, got {base_sha}")
    print(f"[T651 BUILDER] Verified T650 base image SHA: {base_sha}")

    # Read extracted base diag script
    diag_script_path = ROOT / "scripts/extracted_t650_diag.sh"
    with open(diag_script_path, "r", encoding="utf-8") as f:
        base_diag = f.read()

    new_diag_bytes = build_diag_script(base_diag)
    print(f"[T651 BUILDER] Prepared updated t636-diag.sh ({len(new_diag_bytes)} bytes)")

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    print(f"[T651 BUILDER] Streaming CPIO archive from {BASE_IMAGE}...")
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
                print(f"[T651 BUILDER] Found system.img ({filesize} bytes), applying patches...")
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
                old_mark = b"write /dev/kmsg T650_SERVICES_EXTENSION_FIX_APPLIED   \n"
                new_mark = b"write /dev/kmsg T651_SDK_SANDBOX_FIX_APPLIED          \n"
                if old_mark not in orig_extent:
                    raise ValueError("T650 mark anchor not found in init.rc extent!")
                new_extent = orig_extent.replace(old_mark, new_mark, 1)
                if len(new_extent) != INIT_ALLOCATED_SIZE:
                    raise AssertionError(f"Extent size changed: {len(new_extent)} != {INIT_ALLOCATED_SIZE}")
                fout.write(new_extent)
                new_system_sha.update(new_extent)
                print(f"[T651 BUILDER] Patched init.rc mark (T650 -> T651).")

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
                print(f"[T651 BUILDER] Patched /system/system_ext/priv-app directory block with SdkSandbox.")

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
                print(f"[T651 BUILDER] Replacing t636-diag.sh ({filesize} -> {len(new_diag_bytes)} bytes)...")
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
        "track": "T651",
        "description": "Fix system_server PackageManagerService getRequiredSdkSandboxPackageName(): wire SdkSandbox into /system/system_ext/priv-app",
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
            "service_interface": "com.android.sdksandbox.SdkSandboxService",
            "package": "com.android.sdksandbox",
            "providing_apk": "/system/system_ext/apex/com.android.adservices/app/SdkSandbox@UQ1A.231205.015/SdkSandbox.apk",
            "providing_apk_inode": 3666,
            "providing_dir_inode": 3665,
            "wiring_target": "/system/system_ext/priv-app/SdkSandbox",
            "reason_for_missing_package": "apexd failed to activate compressed com.android.adservices.capex due to FS_IOC_GETFLAGS failure on non-f2fs /data; PMS only queries active apexes from apexd and standard partition folders, so SdkSandbox.apk was never scanned",
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
            "base_t650_initramfs": {
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

    print(f"[T651 BUILDER] Build complete in {elapsed:.2f}s!")
    print(f"[T651 BUILDER] Output image: {OUTPUT_IMAGE}")
    print(f"[T651 BUILDER] Output SHA-256: {output_sha} ({output_bytes} bytes, inode {output_inode})")
    print(f"[T651 BUILDER] Manifest written to {MANIFEST_FILE} and {FLIGHT_MANIFEST}")


if __name__ == "__main__":
    main()
