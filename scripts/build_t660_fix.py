#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T660:
system-server-phase550-jobstatus-mediastore-images-media-noclassdeffounderror (Chain A)

PINNED MECHANISM:
In T642/T647, a hardcoded BOOTCLASSPATH override (BOOTCLASSPATH_32_APEX) was introduced in
system.img init.rc and t636-diag.sh. It contained only 32 jars and omitted
/apex/com.android.mediaprovider/javalib/framework-mediaprovider.jar. Even though derive_classpath
read /apex/com.android.mediaprovider/etc/classpaths/bootclasspath.pb and generated a 35-jar
BOOTCLASSPATH including framework-mediaprovider.jar, init.rc and t636-diag.sh prep overrode
BOOTCLASSPATH with the hardcoded 32-jar string before Zygote started, causing Zygote and
system_server to inherit a BOOTCLASSPATH without framework-mediaprovider.jar.
Consequently, when com.android.server.job.controllers.JobStatus.<clinit> ran during
DiskStatsLoggingService, ART threw NoClassDefFoundError on android.provider.MediaStore$Images$Media
and marked JobStatus defective. At Phase 550 (PHASE_ACTIVITY_MANAGER_READY),
NetworkWatchlistService$Lifecycle.onBootPhase called JobScheduler.schedule, causing ART to reject
re-init of JobStatus and crash system_server fatally.

FIX:
Supply /apex/com.android.mediaprovider/javalib/framework-mediaprovider.jar on BOOTCLASSPATH in
both system.img init.rc and t636-diag.sh, unblocking MediaStore$Images$Media resolution,
allowing JobStatus to initialize cleanly, and advancing system_server past Phase 550.

Base Image (T659): SHA-256 fe1809ffbb3ee91252a009442c8956264df1bcac022dd731d86c7555cea2eb6c
Canonical Kernel:  SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
Outputs:
  guest-image/t660-system-server-phase550-jobstatus-mediastore-images-media-noclassdeffounderror-initramfs.cpio.gz
"""

from __future__ import annotations

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

ROOT = Path("/Users/Shared/wt-track660-system-server-phase550-jobstatus-mediastore-images-media-noclassdeffounderror")
BASE_IMAGE = ROOT / "guest-inputs/t659-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t660-system-server-phase550-jobstatus-mediastore-images-media-noclassdeffounderror-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t660-manifest.json"

EXPECTED_BASE_SHA256 = "fe1809ffbb3ee91252a009442c8956264df1bcac022dd731d86c7555cea2eb6c"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"

SYSTEM_IMG_SIZE = 2596683776
INIT_DATA_OFFSET = 398934016
INIT_ALLOCATED_SIZE = 61440
INIT_INODE_SIZE_OFFSET = 134292740
ORIG_INITRC_SIZE = 60142

OLD_BCP_BLOCK = (
    b":/apex/com.android.configinfrastructure/javalib/framework-configinfrastructure.jar\n"
    b"    start installd\n"
    b"write /dev/kmsg T652_FIX_APPLIED"
)
NEW_BCP_BLOCK = (
    b":/apex/com.android.configinfrastructure/javalib/framework-configinfrastructure.jar"
    b":/apex/com.android.mediaprovider/javalib/framework-mediaprovider.jar\n"
    b"    start installd\n"
    b"write /dev/kmsg T660_FIX_APPLIED"
)
DELTA = len(NEW_BCP_BLOCK) - len(OLD_BCP_BLOCK)
NEW_INITRC_SIZE = ORIG_INITRC_SIZE + DELTA

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


def check_disk_space() -> float:
    stat = os.statvfs("/System/Volumes/Data")
    free_bytes = stat.f_bavail * stat.f_frsize
    free_gib = free_bytes / (1024 ** 3)
    if free_gib < 2.0:
        raise RuntimeError(
            f"DISK HYGIENE: free disk space {free_gib:.2f} GiB is under 2.0 GiB threshold! Self-throttling."
        )
    return free_gib


def patch_diag_script(base_script: str) -> bytes:
    script = base_script

    # 1. Update BOOTCLASSPATH definition
    old_bcp_end = '/apex/com.android.configinfrastructure/javalib/framework-configinfrastructure.jar"'
    new_bcp_end = '/apex/com.android.configinfrastructure/javalib/framework-configinfrastructure.jar:/apex/com.android.mediaprovider/javalib/framework-mediaprovider.jar"'
    if old_bcp_end not in script:
        raise ValueError("Could not find old_bcp_end anchor in base diag script")
    script = script.replace(old_bcp_end, new_bcp_end, 1)

    script = script.replace('BOOTCLASSPATH_32_APEX', 'BOOTCLASSPATH_33_APEX')
    script = script.replace('framework-configinfrastructure.jar', 'framework-mediaprovider.jar')
    script = script.replace('T647_ENFORCING_32_JAR_BOOTCLASSPATH', 'T660_ENFORCING_33_JAR_BOOTCLASSPATH')

    # 2. Add T660 verification block
    t659_applied_anchor = '    mark "T659_FIX_APPLIED"\n'
    t660_check = """    # T660: MediaStore / framework-mediaprovider BOOTCLASSPATH verification
    mark "T660_BOOTCLASSPATH_PREP"
    if [ -f /apex/com.android.mediaprovider/javalib/framework-mediaprovider.jar ] || [ -f /system/system_ext/apex/com.android.mediaprovider/javalib/framework-mediaprovider.jar ]; then
        mark "T660_MEDIAPROVIDER_JAR_PRESENT"
    else
        mark "T660_MEDIAPROVIDER_JAR_MISSING"
    fi
    mark "T660_FIX_APPLIED"
"""
    if t659_applied_anchor not in script:
        raise ValueError("Could not find anchor 'mark \"T659_FIX_APPLIED\"' in base diag script")
    script = script.replace(t659_applied_anchor, t659_applied_anchor + t660_check, 1)

    return script.encode("utf-8")


def main():
    t0 = time.monotonic()
    print("[T660 BUILDER] Starting build for Track T660...")
    free_gib = check_disk_space()
    print(f"[T660 BUILDER] Disk check: {free_gib:.2f} GiB free.")

    # 1. Verify inputs
    if not BASE_IMAGE.exists():
        raise FileNotFoundError(f"Base image missing: {BASE_IMAGE}")
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.exists():
        raise FileNotFoundError(f"Kernel missing: {kernel_path}")

    base_sha = sha256_file(BASE_IMAGE)
    kernel_sha = sha256_file(kernel_path)
    print(f"[T660 BUILDER] Base image SHA-256: {base_sha}")
    print(f"[T660 BUILDER] Kernel SHA-256:     {kernel_sha}")

    if base_sha != EXPECTED_BASE_SHA256:
        raise ValueError(f"Base image SHA mismatch: {base_sha} != {EXPECTED_BASE_SHA256}")
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        raise ValueError(f"Kernel SHA mismatch: {kernel_sha} != {EXPECTED_KERNEL_SHA256}")

    # Check hardlink count
    base_stat = os.stat(BASE_IMAGE)
    print(f"[T660 BUILDER] Base image link count: {base_stat.st_nlink} (inode {base_stat.st_ino})")
    if base_stat.st_nlink < 2:
        print("[T660 BUILDER] Warning: Base image link count is < 2")

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    # 2. Stream through base image and rebuild cpio.gz
    print(f"[T660 BUILDER] Streaming through base image to create {PARTIAL_IMAGE}...")
    orig_entries = 0
    patched_system_img = False
    patched_diag = False

    with gzip.open(BASE_IMAGE, "rb") as fin, gzip.open(PARTIAL_IMAGE, "wb", compresslevel=6) as fout:
        while True:
            hdr = fin.read(110)
            if not hdr or len(hdr) < 110:
                break
            if hdr[:6] != b"070701":
                raise ValueError(f"Invalid CPIO header magic: {hdr[:6]}")

            ino = int(hdr[6:14], 16)
            mode = int(hdr[14:22], 16)
            uid = int(hdr[22:30], 16)
            gid = int(hdr[30:38], 16)
            nlink = int(hdr[38:46], 16)
            mtime = int(hdr[46:54], 16)
            filesize = int(hdr[54:62], 16)
            devmajor = int(hdr[62:70], 16)
            devminor = int(hdr[70:78], 16)
            rdevmajor = int(hdr[78:86], 16)
            rdevminor = int(hdr[86:94], 16)
            namesize = int(hdr[94:102], 16)
            check = int(hdr[102:110], 16)

            name_bytes = fin.read(namesize)
            name_str = name_bytes[:-1].decode("latin1")
            pad_name = pad4(110 + namesize)
            if pad_name:
                fin.read(pad_name)

            orig_entries += 1

            if name_str == "ramdisk-images/system.img":
                print(f"[T660 BUILDER] Found system.img ({filesize} bytes), applying init.rc patch...")
                if filesize != SYSTEM_IMG_SIZE:
                    raise ValueError(f"system.img size mismatch: {filesize} != {SYSTEM_IMG_SIZE}")

                fout.write(hdr)
                fout.write(name_bytes)
                fout.write(b"\0" * pad_name)

                # Step A: Stream up to INIT_INODE_SIZE_OFFSET
                rem = INIT_INODE_SIZE_OFFSET
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    fout.write(chunk)
                    rem -= len(chunk)

                # Step B: Patch inode size (4 bytes)
                orig_size_bytes = fin.read(4)
                orig_inode_size = struct.unpack("<I", orig_size_bytes)[0]
                if orig_inode_size != ORIG_INITRC_SIZE:
                    raise AssertionError(f"Expected orig inode size {ORIG_INITRC_SIZE}, got {orig_inode_size}")
                new_size_bytes = struct.pack("<I", NEW_INITRC_SIZE)
                fout.write(new_size_bytes)
                print(f"[T660 BUILDER] Patched inode size: {orig_inode_size} -> {NEW_INITRC_SIZE}")

                # Step C: Stream up to INIT_DATA_OFFSET
                rem = INIT_DATA_OFFSET - (INIT_INODE_SIZE_OFFSET + 4)
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    fout.write(chunk)
                    rem -= len(chunk)

                # Step D: Patch extent (INIT_ALLOCATED_SIZE)
                orig_extent = fin.read(INIT_ALLOCATED_SIZE)
                if OLD_BCP_BLOCK not in orig_extent:
                    raise ValueError("OLD_BCP_BLOCK anchor not found in init.rc extent!")
                orig_initrc = orig_extent[:ORIG_INITRC_SIZE]
                new_initrc = orig_initrc.replace(OLD_BCP_BLOCK, NEW_BCP_BLOCK, 1)
                new_extent = new_initrc + orig_extent[ORIG_INITRC_SIZE + DELTA:]
                if len(new_extent) != INIT_ALLOCATED_SIZE:
                    raise AssertionError(f"Extent size mismatch: {len(new_extent)} != {INIT_ALLOCATED_SIZE}")
                fout.write(new_extent)
                print(f"[T660 BUILDER] Patched init.rc extent ({ORIG_INITRC_SIZE} -> {NEW_INITRC_SIZE} bytes).")

                # Step E: Stream remainder of system.img
                rem = filesize - (INIT_DATA_OFFSET + INIT_ALLOCATED_SIZE)
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    fout.write(chunk)
                    rem -= len(chunk)

                pad_data = pad4(filesize)
                if pad_data:
                    fout.write(fin.read(pad_data))
                patched_system_img = True

            elif name_str == "t636-diag.sh":
                print(f"[T660 BUILDER] Found t636-diag.sh ({filesize} bytes), patching...")
                raw_script = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                new_script_bytes = patch_diag_script(raw_script.decode("utf-8"))
                new_entry = format_cpio_entry("t636-diag.sh", new_script_bytes, mode=mode, ino=ino)
                fout.write(new_entry)
                patched_diag = True
                print(f"[T660 BUILDER] Patched t636-diag.sh: {filesize} -> {len(new_script_bytes)} bytes.")

            elif name_str == "TRAILER!!!":
                trailer_entry = format_cpio_entry("TRAILER!!!", b"", mode=0, ino=0)
                fout.write(trailer_entry)
                break

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

    if not patched_system_img:
        raise RuntimeError("Failed to patch system.img: entry not found!")
    if not patched_diag:
        raise RuntimeError("Failed to patch t636-diag.sh: entry not found!")

    # Atomic rename
    PARTIAL_IMAGE.rename(OUTPUT_IMAGE)
    fix_sha = sha256_file(OUTPUT_IMAGE)
    fix_size = OUTPUT_IMAGE.stat().st_size
    elapsed = time.monotonic() - t0
    print(f"[T660 BUILDER] Built fix initramfs in {elapsed:.1f}s:")
    print(f"  Path:   {OUTPUT_IMAGE}")
    print(f"  Size:   {fix_size} bytes")
    print(f"  SHA256: {fix_sha}")

    # Write manifest
    manifest = {
        "track": "T660",
        "description": "system-server-phase550-jobstatus-mediastore-images-media-noclassdeffounderror fix",
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "base_image": {
            "path": str(BASE_IMAGE),
            "sha256": base_sha,
            "st_nlink": base_stat.st_nlink,
        },
        "kernel": {
            "path": str(kernel_path),
            "sha256": kernel_sha,
        },
        "fix_image": {
            "path": str(OUTPUT_IMAGE),
            "sha256": fix_sha,
            "size_bytes": fix_size,
        },
        "modifications": [
            "system.img init.rc: Added :/apex/com.android.mediaprovider/javalib/framework-mediaprovider.jar to export BOOTCLASSPATH",
            "system.img inode 694: Updated size 60142 -> 60210",
            "t636-diag.sh: Updated BOOTCLASSPATH_32_APEX -> BOOTCLASSPATH_33_APEX containing framework-mediaprovider.jar",
            "t636-diag.sh: Updated classpath watchers and added T660 verification block",
        ],
    }
    with open(MANIFEST_FILE, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
    print(f"[T660 BUILDER] Wrote manifest to {MANIFEST_FILE}")


if __name__ == "__main__":
    main()
