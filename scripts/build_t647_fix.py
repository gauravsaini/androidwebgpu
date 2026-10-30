#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for T647:
1. Add /apex/com.android.configinfrastructure/javalib/framework-configinfrastructure.jar to BOOTCLASSPATH.
   This provides android.provider.DeviceConfig$OnPropertiesChangedListener to system_server.
2. In ramdisk-images/system.img:
   - Patch inode size at 134292740 (60051 -> 60142).
   - Patch extent at 398934016: replace T642_BCP_OVERRIDE_BLOCK with T647_BCP_OVERRIDE_BLOCK.
3. In t636-diag.sh:
   - Define BOOTCLASSPATH_32_APEX with all 32 jars including configinfrastructure.
   - Enforce BOOTCLASSPATH_32_APEX in /data/system/environ/classpath.
   - Verify configinfrastructure jar accessibility in do_prep.
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

ROOT = Path("/Users/Shared/wt-track647-system-server-deviceconfig-listener")
BASE_IMAGE = ROOT / "guest-inputs/t646-system-server-zygote-sigkill-initramfs.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t647-system-server-deviceconfig-listener-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t647-manifest.json"
FLIGHT_MANIFEST = ROOT / "flights" / "flight-01" / "guest-image-manifest.json"

EXPECTED_BASE_SHA256 = "ad918c0559913d0b9d81a4488c34f86bbbc45b9ae0e285bcb2d88e4751af4629"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"

SYSTEM_IMG_SIZE = 2059816960
INIT_INODE_SIZE_OFFSET = 134292740
INIT_DATA_OFFSET = 398934016
INIT_ALLOCATED_SIZE = 15 * 4096
T646_INITRC_SIZE = 60051
T647_INITRC_SIZE = 60142

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

CONFIGINFRASTRUCTURE_JAR = "/apex/com.android.configinfrastructure/javalib/framework-configinfrastructure.jar"
BOOTCLASSPATH_32_APEX = BOOTCLASSPATH_31_APEX + ":" + CONFIGINFRASTRUCTURE_JAR

T642_BCP_OVERRIDE_BLOCK = (
    b"    exec u:r:init:s0 root root -- /system/bin/sh /data/t636-diag.sh prep\n"
    b"    load_exports /data/system/environ/classpath\n"
    b"    export BOOTCLASSPATH " + BOOTCLASSPATH_31_APEX.encode("ascii") + b"\n"
    b"    write /dev/kmsg T642_FD_ALLOWLIST_FIX_APPLIED\n"
    b"    exec_background u:r:init:s0 root root -- /system/bin/sh /data/t636-diag.sh bg\n"
)

T647_BCP_OVERRIDE_BLOCK = (
    b"    exec u:r:init:s0 root root -- /system/bin/sh /data/t636-diag.sh prep\n"
    b"    load_exports /data/system/environ/classpath\n"
    b"    export BOOTCLASSPATH " + BOOTCLASSPATH_32_APEX.encode("ascii") + b"\n"
    b"    write /dev/kmsg T647_DEVICECONFIG_LISTENER_FIX_APPLIED\n"
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

def build_diag_script(base_script: str) -> bytes:
    script = base_script
    # 1. Replace BOOTCLASSPATH_31_APEX variable name with BOOTCLASSPATH_32_APEX
    script = script.replace("BOOTCLASSPATH_31_APEX", "BOOTCLASSPATH_32_APEX")

    # 2. Append CONFIGINFRASTRUCTURE_JAR to the definition of BOOTCLASSPATH_32_APEX
    old_def_end = 'framework-ondevicepersonalization.jar"'
    new_def_end = f'framework-ondevicepersonalization.jar:{CONFIGINFRASTRUCTURE_JAR}"'
    if old_def_end not in script:
        raise ValueError("Could not find old_def_end in base_script")
    script = script.replace(old_def_end, new_def_end, 1)

    # 3. Add verification mark in do_prep
    anchor = '    if [ -f /apex/com.android.tethering/javalib/framework-connectivity.jar ]; then'
    t647_check = """    # T647: Verify configinfrastructure jar is accessible via /apex/
    if [ -f /apex/com.android.configinfrastructure/javalib/framework-configinfrastructure.jar ]; then
        mark "T647_APEX_CONFIGINFRASTRUCTURE_JAR_OK"
    else
        mark "T647_APEX_CONFIGINFRASTRUCTURE_JAR_MISSING"
    fi

"""
    if anchor not in script:
        raise ValueError("Could not find anchor in base_script for do_prep check")
    script = script.replace(anchor, t647_check + anchor, 1)

    # 4. In do_prep and watch_classpath_source, check for framework-configinfrastructure.jar
    old_grep = 'if ! "$TOY" grep -q "framework-tethering.jar" /data/system/environ/classpath; then'
    new_grep = 'if ! "$TOY" grep -q "framework-configinfrastructure.jar" /data/system/environ/classpath; then'
    script = script.replace(old_grep, new_grep)

    # 5. Update comments/marks
    script = script.replace("T642_ENFORCING_31_JAR_BOOTCLASSPATH", "T647_ENFORCING_32_JAR_BOOTCLASSPATH")
    script = script.replace("31-jar BOOTCLASSPATH", "32-jar BOOTCLASSPATH")

    return script.encode("utf-8")

def main():
    t0 = time.monotonic()
    print("[T647 BUILDER] Starting T647 initramfs build...")

    if not BASE_IMAGE.exists():
        sys.exit(f"Missing base image: {BASE_IMAGE}")
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.exists():
        sys.exit(f"Missing kernel: {kernel_path}")

    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        sys.exit(f"Kernel SHA mismatch: expected {EXPECTED_KERNEL_SHA256}, got {kernel_sha}")
    print(f"[T647 BUILDER] Verified canonical Image SHA: {kernel_sha}")

    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        sys.exit(f"Base image SHA mismatch: expected {EXPECTED_BASE_SHA256}, got {base_sha}")
    print(f"[T647 BUILDER] Verified T646 base image SHA: {base_sha}")

    # Verify size calculations
    delta = len(T647_BCP_OVERRIDE_BLOCK) - len(T642_BCP_OVERRIDE_BLOCK)
    if delta != 91:
        raise ValueError(f"Unexpected delta: {delta} != 91")
    new_calc_size = T646_INITRC_SIZE + delta
    if new_calc_size != T647_INITRC_SIZE:
        raise ValueError(f"Size mismatch: {new_calc_size} != {T647_INITRC_SIZE}")
    if new_calc_size > INIT_ALLOCATED_SIZE:
        raise ValueError(f"Exceeds extent: {new_calc_size} > {INIT_ALLOCATED_SIZE}")

    print(f"[T647 BUILDER] init.rc size will change: {T646_INITRC_SIZE} -> {T647_INITRC_SIZE} ({INIT_ALLOCATED_SIZE - T647_INITRC_SIZE} bytes headroom)")

    # Read extracted base diag script
    diag_script_path = ROOT / "scripts/extracted_t646_diag.sh"
    with open(diag_script_path, "r", encoding="utf-8") as f:
        base_diag = f.read()

    new_diag_bytes = build_diag_script(base_diag)
    print(f"[T647 BUILDER] Prepared updated t636-diag.sh ({len(new_diag_bytes)} bytes)")

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    print(f"[T647 BUILDER] Streaming CPIO archive from {BASE_IMAGE}...")
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
                print(f"[T647 BUILDER] Found system.img ({filesize} bytes), applying patch...")
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
                if orig_inode_size != T646_INITRC_SIZE:
                    raise AssertionError(f"Expected orig inode size {T646_INITRC_SIZE}, got {orig_inode_size}")

                new_size_bytes = struct.pack("<I", T647_INITRC_SIZE)
                fout.write(new_size_bytes)
                new_system_sha.update(new_size_bytes)
                print(f"[T647 BUILDER] Patched inode size: {orig_inode_size} -> {T647_INITRC_SIZE}")

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
                if T642_BCP_OVERRIDE_BLOCK not in orig_initrc:
                    raise ValueError("T642_BCP_OVERRIDE_BLOCK anchor not found in extent!")

                new_initrc = orig_initrc.replace(T642_BCP_OVERRIDE_BLOCK, T647_BCP_OVERRIDE_BLOCK, 1)
                if len(new_initrc) != T647_INITRC_SIZE:
                    raise AssertionError(f"new_initrc length {len(new_initrc)} != {T647_INITRC_SIZE}")

                new_extent = new_initrc + b"\x00" * (INIT_ALLOCATED_SIZE - len(new_initrc))
                if len(new_extent) != INIT_ALLOCATED_SIZE:
                    raise AssertionError(f"Extent size changed: {len(new_extent)} != {INIT_ALLOCATED_SIZE}")

                fout.write(new_extent)
                new_system_sha.update(new_extent)
                print(f"[T647 BUILDER] Patched init.rc extent successfully ({orig_inode_size} -> {T647_INITRC_SIZE} bytes).")

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
                print(f"[T647 BUILDER] Replacing t636-diag.sh ({filesize} -> {len(new_diag_bytes)} bytes)...")
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
        "track": "T647",
        "description": "Fix system_server DeviceConfig.OnPropertiesChangedListener NoClassDefFoundError: add framework-configinfrastructure.jar to BOOTCLASSPATH",
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "base_image": str(BASE_IMAGE),
        "base_image_sha256": base_sha,
        "output_image": str(OUTPUT_IMAGE),
        "output_image_sha256": output_sha,
        "output_image_bytes": output_bytes,
        "orig_system_sha256": orig_system_sha.hexdigest(),
        "new_system_sha256": new_system_sha.hexdigest(),
        "build_elapsed_seconds": round(elapsed, 2),
        "bootclasspath_entries_count": 32,
        "pinned_missing_class": "android.provider.DeviceConfig$OnPropertiesChangedListener",
        "pinned_providing_jar": CONFIGINFRASTRUCTURE_JAR,
        "bootclasspath_32_apex": BOOTCLASSPATH_32_APEX,
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
            "base_t646_initramfs": {
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

    print(f"[T647 BUILDER] Build complete in {elapsed:.2f}s!")
    print(f"[T647 BUILDER] Output image: {OUTPUT_IMAGE}")
    print(f"[T647 BUILDER] Output SHA-256: {output_sha} ({output_bytes} bytes)")
    print(f"[T647 BUILDER] Manifest written to {MANIFEST_FILE} and {FLIGHT_MANIFEST}")

if __name__ == "__main__":
    main()
