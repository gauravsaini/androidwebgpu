#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for T642: get system_server past the fd allowlist check."""

from __future__ import annotations

import gzip
import hashlib
import json
import os
import struct
import sys
import time
from pathlib import Path

ROOT = Path("/Users/Shared/wt-track642-system-server-fd-allowlist")
BASE_IMAGE = ROOT / "guest-inputs/t641-systemserviceregistry-preload-initramfs.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t642-system-server-fd-allowlist-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t642-manifest.json"
FLIGHT_MANIFEST = ROOT / "flights" / "flight-01" / "guest-image-manifest.json"

EXPECTED_BASE_SHA256 = "c6328c409056aa0afb307fa5afc8d8bc9998a60879de92278c49b7ae09f43298"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"

SYSTEM_IMG_SIZE = 2059816960
INIT_INODE_SIZE_OFFSET = 134292740
INIT_DATA_OFFSET = 398934016
INIT_ALLOCATED_SIZE = 15 * 4096
T641_INITRC_SIZE = 61195
T642_INITRC_SIZE = 60051

BOOTCLASSPATH_20 = (
    "/apex/com.android.art/javalib/core-oj.jar:"
    "/apex/com.android.art/javalib/core-libart.jar:"
    "/apex/com.android.art/javalib/okhttp.jar:"
    "/apex/com.android.art/javalib/bouncycastle.jar:"
    "/apex/com.android.art/javalib/apache-xml.jar:"
    "/system/framework/framework.jar:"
    "/system/framework/framework-graphics.jar:"
    "/system/framework/ext.jar:"
    "/system/framework/telephony-common.jar:"
    "/system/framework/voip-common.jar:"
    "/system/framework/ims-common.jar:"
    "/apex/com.android.i18n/javalib/core-icu4j.jar:"
    "/apex/com.android.btservices/javalib/framework-bluetooth.jar:"
    "/apex/com.android.conscrypt/javalib/conscrypt.jar:"
    "/apex/com.android.devicelock/javalib/framework-devicelock.jar:"
    "/apex/com.android.healthfitness/javalib/framework-healthfitness.jar:"
    "/apex/com.android.os.statsd/javalib/framework-statsd.jar:"
    "/apex/com.android.sdkext/javalib/framework-sdkextensions.jar:"
    "/apex/com.android.virt/javalib/framework-virtualization.jar:"
    "/system/system_ext/apex/com.android.tethering/javalib/framework-connectivity.jar"
)

NEW_11_JARS = [
    "/system/system_ext/apex/com.android.tethering/javalib/framework-tethering.jar",
    "/system/system_ext/apex/com.android.tethering/javalib/framework-connectivity-t.jar",
    "/system/system_ext/apex/com.android.appsearch/javalib/framework-appsearch.jar",
    "/system/system_ext/apex/com.android.wifi/javalib/framework-wifi.jar",
    "/system/system_ext/apex/com.android.media/javalib/updatable-media.jar",
    "/system/system_ext/apex/com.android.permission/javalib/framework-permission-s.jar",
    "/system/system_ext/apex/com.android.scheduling/javalib/framework-scheduling.jar",
    "/system/system_ext/apex/com.android.adservices/javalib/framework-sdksandbox.jar",
    "/system/system_ext/apex/com.android.adservices/javalib/framework-adservices.jar",
    "/system/system_ext/apex/com.android.uwb/javalib/framework-uwb.jar",
    "/system/system_ext/apex/com.android.ondevicepersonalization/javalib/framework-ondevicepersonalization.jar",
]

APPEND_11_JARS = ":".join(NEW_11_JARS)
BOOTCLASSPATH_31_LEGACY = BOOTCLASSPATH_20 + ":" + APPEND_11_JARS

T641_BCP_OVERRIDE_BLOCK = (
    b"    exec - root root -- /system/bin/sh -c \"/system/bin/toybox sed -i 's|export BOOTCLASSPATH .*|&:" + APPEND_11_JARS.encode("ascii") + b"|' /data/system/environ/classpath\"\n"
    b"    load_exports /data/system/environ/classpath\n"
    b"    export BOOTCLASSPATH " + BOOTCLASSPATH_31_LEGACY.encode("ascii") + b"\n"
    b"    write /dev/kmsg T641_BOOTCLASSPATH_FIX_APPLIED\n"
    b"    exec_background u:r:init:s0 root root -- /system/bin/sh /data/t636-diag.sh\n"
)

# In T642, all 12 APEX jars are routed through /apex/com.android.<pkg>/javalib/<jar>.jar
BOOTCLASSPATH_31_APEX = (
    "/apex/com.android.art/javalib/core-oj.jar:"
    "/apex/com.android.art/javalib/core-libart.jar:"
    "/apex/com.android.art/javalib/okhttp.jar:"
    "/apex/com.android.art/javalib/bouncycastle.jar:"
    "/apex/com.android.art/javalib/apache-xml.jar:"
    "/system/framework/framework.jar:"
    "/system/framework/framework-graphics.jar:"
    "/system/framework/ext.jar:"
    "/system/framework/telephony-common.jar:"
    "/system/framework/voip-common.jar:"
    "/system/framework/ims-common.jar:"
    "/apex/com.android.i18n/javalib/core-icu4j.jar:"
    "/apex/com.android.btservices/javalib/framework-bluetooth.jar:"
    "/apex/com.android.conscrypt/javalib/conscrypt.jar:"
    "/apex/com.android.devicelock/javalib/framework-devicelock.jar:"
    "/apex/com.android.healthfitness/javalib/framework-healthfitness.jar:"
    "/apex/com.android.os.statsd/javalib/framework-statsd.jar:"
    "/apex/com.android.sdkext/javalib/framework-sdkextensions.jar:"
    "/apex/com.android.virt/javalib/framework-virtualization.jar:"
    "/apex/com.android.tethering/javalib/framework-connectivity.jar:"
    "/apex/com.android.tethering/javalib/framework-tethering.jar:"
    "/apex/com.android.tethering/javalib/framework-connectivity-t.jar:"
    "/apex/com.android.appsearch/javalib/framework-appsearch.jar:"
    "/apex/com.android.wifi/javalib/framework-wifi.jar:"
    "/apex/com.android.media/javalib/updatable-media.jar:"
    "/apex/com.android.permission/javalib/framework-permission-s.jar:"
    "/apex/com.android.scheduling/javalib/framework-scheduling.jar:"
    "/apex/com.android.adservices/javalib/framework-sdksandbox.jar:"
    "/apex/com.android.adservices/javalib/framework-adservices.jar:"
    "/apex/com.android.uwb/javalib/framework-uwb.jar:"
    "/apex/com.android.ondevicepersonalization/javalib/framework-ondevicepersonalization.jar"
)

T642_BCP_OVERRIDE_BLOCK = (
    b"    exec u:r:init:s0 root root -- /system/bin/sh /data/t636-diag.sh prep\n"
    b"    load_exports /data/system/environ/classpath\n"
    b"    export BOOTCLASSPATH " + BOOTCLASSPATH_31_APEX.encode("ascii") + b"\n"
    b"    write /dev/kmsg T642_FD_ALLOWLIST_FIX_APPLIED\n"
    b"    exec_background u:r:init:s0 root root -- /system/bin/sh /data/t636-diag.sh bg\n"
)

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

def build():
    t0 = time.monotonic()
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    (ROOT / "flights" / "flight-01").mkdir(parents=True, exist_ok=True)

    print("[T642 BUILDER] Verifying base inputs...")
    base_sha = sha256_file(BASE_IMAGE)
    print(f"  Base image SHA-256: {base_sha}")
    if base_sha != EXPECTED_BASE_SHA256:
        raise ValueError(f"Base image SHA mismatch: {base_sha} != {EXPECTED_BASE_SHA256}")

    kernel_sha = sha256_file(ROOT / "guest-inputs/Image")
    print(f"  Kernel Image SHA-256: {kernel_sha}")
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        raise ValueError(f"Kernel SHA mismatch: {kernel_sha} != {EXPECTED_KERNEL_SHA256}")

    # Read diagnostic script content
    diag_script_path = ROOT / "scripts/diag_script_content.sh"
    with open(diag_script_path, "rb") as f:
        diag_script_content = f.read()

    print(f"[T642 BUILDER] T641 block size: {len(T641_BCP_OVERRIDE_BLOCK)} bytes")
    print(f"[T642 BUILDER] T642 block size: {len(T642_BCP_OVERRIDE_BLOCK)} bytes")
    calc_inode_size = T641_INITRC_SIZE - len(T641_BCP_OVERRIDE_BLOCK) + len(T642_BCP_OVERRIDE_BLOCK)
    if calc_inode_size != T642_INITRC_SIZE:
        raise AssertionError(f"Calculated inode size {calc_inode_size} != {T642_INITRC_SIZE}")
    if calc_inode_size > INIT_ALLOCATED_SIZE:
        raise AssertionError(f"New init.rc size {calc_inode_size} exceeds allocated extent {INIT_ALLOCATED_SIZE}")

    print(f"[T642 BUILDER] Inode size will change from {T641_INITRC_SIZE} to {T642_INITRC_SIZE} ({INIT_ALLOCATED_SIZE - T642_INITRC_SIZE} bytes headroom)")

    print(f"[T642 BUILDER] Streaming CPIO archive from {BASE_IMAGE}...")
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
                print(f"[T642 BUILDER] Found system.img ({filesize} bytes), applying patch...")
                if filesize != SYSTEM_IMG_SIZE:
                    raise ValueError(f"system.img size mismatch: {filesize} != {SYSTEM_IMG_SIZE}")

                fout.write(hdr)
                fout.write(name_bytes)
                fout.write(b"\0" * pad_name)

                # 1. Stream up to INIT_INODE_SIZE_OFFSET
                rem = INIT_INODE_SIZE_OFFSET
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    fout.write(chunk)
                    orig_system_sha.update(chunk)
                    new_system_sha.update(chunk)
                    rem -= len(chunk)

                # 2. Patch inode size
                orig_size_bytes = fin.read(4)
                orig_system_sha.update(orig_size_bytes)
                orig_inode_size = struct.unpack("<I", orig_size_bytes)[0]
                if orig_inode_size != T641_INITRC_SIZE:
                    raise AssertionError(f"Expected orig inode size {T641_INITRC_SIZE}, got {orig_inode_size}")

                new_size_bytes = struct.pack("<I", T642_INITRC_SIZE)
                fout.write(new_size_bytes)
                new_system_sha.update(new_size_bytes)
                print(f"[T642 BUILDER] Patched inode size: {orig_inode_size} -> {T642_INITRC_SIZE}")

                # 3. Stream up to INIT_DATA_OFFSET
                rem = INIT_DATA_OFFSET - (INIT_INODE_SIZE_OFFSET + 4)
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    fout.write(chunk)
                    orig_system_sha.update(chunk)
                    new_system_sha.update(chunk)
                    rem -= len(chunk)

                # 4. Patch data extent (61440 bytes)
                orig_extent = fin.read(INIT_ALLOCATED_SIZE)
                orig_system_sha.update(orig_extent)
                orig_initrc = orig_extent[:orig_inode_size]
                if T641_BCP_OVERRIDE_BLOCK not in orig_initrc:
                    raise ValueError("T641_BCP_OVERRIDE_BLOCK anchor not found in extent!")

                new_initrc = orig_initrc.replace(T641_BCP_OVERRIDE_BLOCK, T642_BCP_OVERRIDE_BLOCK, 1)
                if len(new_initrc) != T642_INITRC_SIZE:
                    raise AssertionError(f"new_initrc length {len(new_initrc)} != {T642_INITRC_SIZE}")

                new_extent = new_initrc + b"\x00" * (INIT_ALLOCATED_SIZE - len(new_initrc))
                if len(new_extent) != INIT_ALLOCATED_SIZE:
                    raise AssertionError(f"Extent size changed: {len(new_extent)} != {INIT_ALLOCATED_SIZE}")

                fout.write(new_extent)
                new_system_sha.update(new_extent)
                print(f"[T642 BUILDER] Patched init.rc extent successfully ({orig_inode_size} -> {T642_INITRC_SIZE} bytes).")

                # 5. Stream remainder of system.img
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
                print(f"[T642 BUILDER] Replacing t636-diag.sh with updated T642 diagnostic script...")
                # Skip original content and padding
                fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                # Format new entry
                new_entry = format_cpio_entry("t636-diag.sh", diag_script_content)
                fout.write(new_entry)

            else:
                # Copy verbatim
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
    elapsed = time.monotonic() - t0

    manifest = {
        "track": "T642",
        "description": "Fix system_server fd allowlist abort: bind-mount APEXes and point BOOTCLASSPATH to /apex/...",
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "base_image": str(BASE_IMAGE),
        "base_image_sha256": base_sha,
        "output_image": str(OUTPUT_IMAGE),
        "output_image_sha256": output_sha,
        "output_image_bytes": OUTPUT_IMAGE.stat().st_size,
        "orig_system_sha256": orig_system_sha.hexdigest(),
        "new_system_sha256": new_system_sha.hexdigest(),
        "build_elapsed_seconds": round(elapsed, 2),
        "bootclasspath_entries_count": 31,
        "allowlist_mechanism": "FileDescriptorAllowlist::IsAllowed in libandroid_runtime.so requires StartsWith('/apex/') && EndsWith('.jar') && EndsWith(Dirname(path), '/javalib')",
        "bootclasspath_31_apex": BOOTCLASSPATH_31_APEX,
        "images": {
            "kernel": {
                "path": str(ROOT / "guest-inputs/Image"),
                "sha256": kernel_sha,
            },
            "initramfs": {
                "path": str(OUTPUT_IMAGE),
                "sha256": output_sha,
                "bytes": OUTPUT_IMAGE.stat().st_size,
            },
            "base_t641_initramfs": {
                "path": str(BASE_IMAGE),
                "sha256": base_sha,
            },
        },
    }

    with open(MANIFEST_FILE, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
    with open(FLIGHT_MANIFEST, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)

    print(f"[T642 BUILDER] Build complete in {elapsed:.1f}s.")
    print(f"[T642 BUILDER] Output fix image: {OUTPUT_IMAGE}")
    print(f"[T642 BUILDER] Output SHA-256:   {output_sha}")
    print(f"[T642 BUILDER] Output size:      {OUTPUT_IMAGE.stat().st_size} bytes")


if __name__ == "__main__":
    build()
