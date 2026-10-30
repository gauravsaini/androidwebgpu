#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T656:
system-server-startotherservices-networkstatsservice-apexdata-write (Chain A)

1. Pinned Root Cause:
   In SystemServer.startOtherServices() at line 1988:
   SystemServiceManager.startServiceFromJar(
       "com.android.server.NetworkStatsServiceInitializer",
       "/apex/com.android.tethering/javalib/service-connectivity.jar"
   )
   Calls NetworkStatsServiceInitializer.maybeCreateNetworkStatsService() ->
   NetworkStatsService.create() ->
   NetworkStatsService.<init>() (line 590) ->
   NetworkStatsService$Dependencies.getOrCreateStatsDir() (line 662):
     ApexEnvironment.getApexEnvironment("com.android.tethering").getDeviceProtectedDataDir()
     returns /data/misc/apexdata/com.android.tethering
     statsDataDir = /data/misc/apexdata/com.android.tethering/netstats
     if (statsDataDir.exists() || statsDataDir.mkdirs()) { return statsDataDir; }
     throw new IllegalStateException("Cannot write into stats data directory: " + statsDataDir);

   Root Cause of Failure:
   - On this guest, /data/misc/apexdata exists with mode 0711 root:root (created by init.rc).
   - system_server runs as UID 1000 (system), which lacks write permissions to /data/misc/apexdata.
   - The directory /data/misc/apexdata/com.android.tethering was absent because apexd decompression
     failed on tmpfs /data (FS_IOC_GETFLAGS ioctl error), APEXes were flattened and bind-mounted,
     and neither apexd nor vold_prepare_subdirs provisions device-level apexdata directories.
   - When statsDataDir.mkdirs() attempted mkdir("/data/misc/apexdata/com.android.tethering"),
     it failed with EACCES, causing IllegalStateException.

2. Pinned Fix:
   In t636-diag.sh:
   Pre-create /data/misc/apexdata/com.android.tethering/netstats with ownership 1000:1000 (system:system)
   and mode 0771 for com.android.tethering and 0770 for netstats.
   When NetworkStatsService runs, statsDataDir.exists() evaluates to true and the service
   initializes successfully.

3. Outputs:
   guest-image/t656-system-server-startotherservices-networkstatsservice-apexdata-write-initramfs.cpio.gz
"""

from __future__ import annotations

import gzip
import hashlib
import json
import os
import sys
import time
from pathlib import Path

ROOT = Path("/Users/Shared/wt-track656-system-server-startotherservices-networkstatsservice-apexdata-write")
BASE_IMAGE = ROOT / "guest-inputs/t655-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t656-system-server-startotherservices-networkstatsservice-apexdata-write-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t656-manifest.json"

EXPECTED_BASE_SHA256 = "8b2d3e547f083f67d244bfa3d1c1286ea8dbda21fe0d04da5746514828ef6a0a"
EXPECTED_KERNEL_SHA256 = "05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04"

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

    anchor = '    mark "T655_FIX_APPLIED"\n'
    t656_check = """    # T656: Provision /data/misc/apexdata/com.android.tethering/netstats
    mark "T656_PREP_NETSTATS_DIR"
    "$TOY" mkdir -p /data/misc/apexdata/com.android.tethering/netstats 2>/dev/null || :
    "$TOY" chown -R 1000:1000 /data/misc/apexdata/com.android.tethering 2>/dev/null || :
    "$TOY" chmod 0771 /data/misc/apexdata/com.android.tethering 2>/dev/null || :
    "$TOY" chmod 0770 /data/misc/apexdata/com.android.tethering/netstats 2>/dev/null || :
    if [ -d /data/misc/apexdata/com.android.tethering/netstats ]; then
        mark "T656_NETSTATS_DIR_PROVISIONED_OK"
    else
        mark "T656_NETSTATS_DIR_PROVISION_FAILED"
    fi
    mark "T656_FIX_APPLIED"
"""
    if anchor not in script:
        raise ValueError("Could not find anchor 'mark \"T655_FIX_APPLIED\"' in base diag script")
    script = script.replace(anchor, anchor + "\n" + t656_check, 1)

    boot_anchor = (
        'sensors_hal=$(/system/bin/getprop init.svc.vendor.sensors-hal-multihal 2>/dev/null)\n'
        '        netd_svc=$(/system/bin/getprop init.svc.netd 2>/dev/null)\n'
        '        completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)'
    )
    boot_replace = (
        'sensors_hal=$(/system/bin/getprop init.svc.vendor.sensors-hal-multihal 2>/dev/null)\n'
        '        netd_svc=$(/system/bin/getprop init.svc.netd 2>/dev/null)\n'
        '        netstats_status="missing"\n'
        '        if [ -d /data/misc/apexdata/com.android.tethering/netstats ]; then\n'
        '            netstats_status="ok"\n'
        '        fi\n'
        '        completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)'
    )
    if boot_anchor in script:
        script = script.replace(boot_anchor, boot_replace, 1)
        script = script.replace(
            'sensors_hal=$sensors_hal netd=$netd_svc boot_completed=$completed"',
            'sensors_hal=$sensors_hal netd=$netd_svc netstats=$netstats_status boot_completed=$completed"',
            1
        )

    return script.encode("utf-8")


def check_disk_space() -> float:
    stat = os.statvfs("/System/Volumes/Data")
    free_bytes = stat.f_bavail * stat.f_frsize
    free_gib = free_bytes / (1024 ** 3)
    if free_gib < 2.0:
        raise RuntimeError(
            f"DISK HYGIENE: free disk space {free_gib:.2f} GiB is under 2.0 GiB threshold! Self-throttling."
        )
    return free_gib


def main():
    t0 = time.monotonic()
    print("[T656 BUILDER] Starting T656 initramfs build...")

    free_gib = check_disk_space()
    print(f"[T656 BUILDER] Disk check passed: {free_gib:.2f} GiB free on /System/Volumes/Data.")

    if not BASE_IMAGE.exists():
        sys.exit(f"Missing base image: {BASE_IMAGE}")
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.exists():
        sys.exit(f"Missing kernel: {kernel_path}")

    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        sys.exit(f"Kernel SHA mismatch: expected {EXPECTED_KERNEL_SHA256}, got {kernel_sha}")
    print(f"[T656 BUILDER] Verified canonical Image SHA: {kernel_sha}")

    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        sys.exit(f"Base image SHA mismatch: expected {EXPECTED_BASE_SHA256}, got {base_sha}")
    print(f"[T656 BUILDER] Verified T655 base image SHA: {base_sha}")

    if BASE_IMAGE.stat().st_nlink <= 1:
        sys.exit(f"Base image {BASE_IMAGE} must be hardlinked (st_nlink > 1), got {BASE_IMAGE.stat().st_nlink}")
    print(f"[T656 BUILDER] Base image hardlink verified: link count = {BASE_IMAGE.stat().st_nlink}")

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    print(f"[T656 BUILDER] Streaming CPIO archive from {BASE_IMAGE}...")
    diag_patched = False

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

            if name_str == "t636-diag.sh":
                print(f"[T656 BUILDER] Reading and updating t636-diag.sh ({filesize} bytes)...")
                base_diag_bytes = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                new_diag_bytes = build_diag_script(base_diag_bytes.decode("latin1"))
                new_entry = format_cpio_entry("t636-diag.sh", new_diag_bytes)
                fout.write(new_entry)
                diag_patched = True
                print(f"[T656 BUILDER] Replaced t636-diag.sh ({filesize} -> {len(new_diag_bytes)} bytes)")

            else:
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

    if not diag_patched:
        raise RuntimeError("Failed to patch t636-diag.sh!")

    os.replace(PARTIAL_IMAGE, OUTPUT_IMAGE)
    output_sha = sha256_file(OUTPUT_IMAGE)
    output_bytes = os.path.getsize(OUTPUT_IMAGE)
    output_inode = OUTPUT_IMAGE.stat().st_ino
    elapsed = time.monotonic() - t0

    manifest = {
        "track": "T656",
        "description": "Fix NetworkStatsService netstats apexdata write failure: pre-provision /data/misc/apexdata/com.android.tethering/netstats (0770/0771 1000:1000) in t636-diag.sh",
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "base_image": str(BASE_IMAGE),
        "base_image_sha256": base_sha,
        "base_image_bytes": BASE_IMAGE.stat().st_size,
        "kernel_image": str(kernel_path),
        "kernel_image_sha256": kernel_sha,
        "fix_image": str(OUTPUT_IMAGE),
        "fix_image_sha256": output_sha,
        "fix_image_bytes": output_bytes,
        "fix_image_inode": output_inode,
        "elapsed_seconds": round(elapsed, 2),
    }

    with open(MANIFEST_FILE, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")

    print(f"[T656 BUILDER] Successfully built {OUTPUT_IMAGE} in {elapsed:.1f}s")
    print(f"[T656 BUILDER] SHA-256: {output_sha} ({output_bytes} bytes, inode {output_inode})")


if __name__ == "__main__":
    main()
