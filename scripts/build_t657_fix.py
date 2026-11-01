#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T657:
system-server-startotherservices-wifi-wifiinjector-npe (Chain A)

1. Pinned Root Cause:
   In SystemServer.startOtherServices() at line 2015:
   SystemServiceManager.startServiceFromJar(
       "com.android.server.wifi.WifiService",
       "/apex/com.android.wifi/javalib/service-wifi.jar"
   )
   Calls WifiService.<init>() -> WifiInjector.<init>(WifiContext) (line 287):
     int threshold = mContext.getResources().getInteger(0x7f05001e);
     // 0x7f05001e: com.android.wifi.resources:integer/config_wifiConfigurationWifiRunnerThresholdInMs

   WifiContext.getResources() delegates to getResourcesApkContext():
     getApkPkgNameForAction("com.android.server.wifi.intent.action.SERVICE_WIFI_RESOURCES_APK", null)
     queries PackageManager.queryIntentActivities(...).
     Crucially, WifiContext filters results:
     resolveInfos.removeIf(ri -> !Environment.isAppInWifiApex(ri.activityInfo.applicationInfo));
     where isAppInWifiApex requires: appInfo.sourceDir.startsWith("/apex/com.android.wifi").

   Root Cause of Failure:
   - On this guest, /data is mounted on tmpfs by early-ramdisk.
   - vendor.img sets ro.apex.updatable=true, so SystemServer uses ApexManagerImpl which queries apexd.
   - apexd failed decompression of all compressed APEXes (.capex) on tmpfs:
     "apexd: Failed to call FS_IOC_GETFLAGS on /data/apex/decompressed/...: Inappropriate ioctl for device".
   - Because com.android.wifi was a .capex (/system/apex/com.android.wifi.capex), it was never activated by apexd.
   - PackageManagerService only scans APEX partitions returned by ApexManager.getActiveApexInfos().
   - Since com.android.wifi was absent from getActiveApexInfos(), PMS never scanned /apex/com.android.wifi/priv-app/.
   - ServiceWifiResources.apk was never registered with PMS.
   - queryIntentActivities returned [], WifiContext logged "Attempted to fetch resources before Wifi Resources APK is loaded!",
     caught IllegalStateException, and returned null Resources.
   - WifiInjector.<init> threw NullPointerException on null.getInteger(0x7f05001e).

2. Pinned Fix:
   - Replace compressed /system/apex/com.android.wifi.capex with authentic uncompressed
     /system/apex/com.android.wifi.apex (extracted from original_apex inside the capex) in system.img.
   - Set security.selinux xattr to "u:object_r:system_file:s0" on /system/apex/com.android.wifi.apex.
   - When apexd scans /system/apex/, it discovers com.android.wifi.apex as an uncompressed APEX,
     loop-mounts its payload without decompression or FS_IOC_GETFLAGS, and marks it active in IApexService.
   - PackageManagerService scans /apex/com.android.wifi/priv-app/ServiceWifiResources@UQ1A.231205.015/ServiceWifiResources.apk.
   - WifiContext resolves ServiceWifiResources from /apex/com.android.wifi, Environment.isAppInWifiApex returns true,
     getResources() returns the valid Resources object, and getInteger(0x7f05001e) reads 100 ms.
   - In t636-diag.sh, pre-provision /data/misc/apexdata/com.android.wifi (0771 system:system) and add verification markers.

3. Outputs:
   guest-image/t657-system-server-startotherservices-wifi-wifiinjector-npe-initramfs.cpio.gz
"""

from __future__ import annotations

import gzip
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path("/Users/Shared/wt-track657-system-server-startotherservices-wifi-wifiinjector-npe")
BASE_IMAGE = ROOT / "guest-inputs/t656-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t657-system-server-startotherservices-wifi-wifiinjector-npe-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t657-manifest.json"

EXPECTED_BASE_SHA256 = "7993ebbfed9f56302456c0b57a10289fccbfe8d53bc32f40a38397845eb528b2"
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

    anchor = '    mark "T656_FIX_APPLIED"\n'
    t657_check = """    # T657: Provision /data/misc/apexdata/com.android.wifi and verify APEX
    mark "T657_PREP_WIFI_APEX"
    "$TOY" mkdir -p /data/misc/apexdata/com.android.wifi 2>/dev/null || :
    "$TOY" chown -R 1000:1000 /data/misc/apexdata/com.android.wifi 2>/dev/null || :
    "$TOY" chmod 0771 /data/misc/apexdata/com.android.wifi 2>/dev/null || :
    if [ -f /system/apex/com.android.wifi.apex ]; then
        mark "T657_SYSTEM_APEX_WIFI_EXISTS"
    else
        mark "T657_SYSTEM_APEX_WIFI_MISSING"
    fi
    if [ -d /apex/com.android.wifi/priv-app ]; then
        mark "T657_WIFI_PRIVAPP_DIR_EXISTS"
    else
        mark "T657_WIFI_PRIVAPP_DIR_PENDING"
    fi
    mark "T657_FIX_APPLIED"
"""
    if anchor not in script:
        raise ValueError("Could not find anchor 'mark \"T656_FIX_APPLIED\"' in base diag script")
    script = script.replace(anchor, anchor + "\n" + t657_check, 1)

    boot_anchor = (
        '        netstats_status="missing"\n'
        '        if [ -d /data/misc/apexdata/com.android.tethering/netstats ]; then\n'
        '            netstats_status="ok"\n'
        '        fi\n'
        '        completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)\n'
        '        mark "T642_BOOT_STATE uptime=$uptime system_server_prop=$ss system_server_pids=$sspids zygote=$zy zypid=$zypid sensors_hal=$sensors_hal netd=$netd_svc netstats=$netstats_status boot_completed=$completed"'
    )
    boot_replace = (
        '        netstats_status="missing"\n'
        '        if [ -d /data/misc/apexdata/com.android.tethering/netstats ]; then\n'
        '            netstats_status="ok"\n'
        '        fi\n'
        '        wifi_apex_status="missing"\n'
        '        if [ -d /apex/com.android.wifi/priv-app ]; then\n'
        '            wifi_apex_status="ok"\n'
        '        fi\n'
        '        completed=$(/system/bin/getprop sys.boot_completed 2>/dev/null)\n'
        '        mark "T642_BOOT_STATE uptime=$uptime system_server_prop=$ss system_server_pids=$sspids zygote=$zy zypid=$zypid sensors_hal=$sensors_hal netd=$netd_svc netstats=$netstats_status wifi_apex=$wifi_apex_status boot_completed=$completed"'
    )
    if boot_anchor in script:
        script = script.replace(boot_anchor, boot_replace, 1)
    else:
        print("[T657 BUILDER] Warning: boot_anchor not found for exact replacement")

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


def prepare_system_img():
    system_img_path = ROOT / "scratch/system.img"
    wifi_apex_path = ROOT / "scratch/com.android.wifi.apex"

    if not wifi_apex_path.exists():
        sys.exit(f"Missing authentic uncompressed APEX: {wifi_apex_path}")

    # Check if system.img already has com.android.wifi.apex
    debugfs = "/opt/homebrew/Cellar/e2fsprogs/1.47.4/sbin/debugfs"
    cmd = [debugfs, "-R", "ls -l system/apex", str(system_img_path)]
    res = subprocess.run(cmd, capture_output=True, text=True)
    if "com.android.wifi.apex" in res.stdout and "com.android.wifi.capex" not in res.stdout:
        print("[T657 BUILDER] scratch/system.img already contains com.android.wifi.apex cleanly.")
        return system_img_path

    print("[T657 BUILDER] Updating scratch/system.img with debugfs...")
    # Remove capex if present
    subprocess.run([debugfs, "-w", "-R", "rm system/apex/com.android.wifi.capex", str(system_img_path)], check=True)
    # Write authentic uncompressed apex
    subprocess.run([debugfs, "-w", "-R", f"write {wifi_apex_path} system/apex/com.android.wifi.apex", str(system_img_path)], check=True)
    # Set SELinux xattr
    subprocess.run([debugfs, "-w", "-R", "ea_set <86> security.selinux u:object_r:system_file:s0", str(system_img_path)], check=True)
    print("[T657 BUILDER] scratch/system.img updated successfully.")
    return system_img_path


def main():
    t0 = time.monotonic()
    print("[T657 BUILDER] Starting T657 initramfs build...")

    free_gib = check_disk_space()
    print(f"[T657 BUILDER] Disk check passed: {free_gib:.2f} GiB free on /System/Volumes/Data.")

    if not BASE_IMAGE.exists():
        sys.exit(f"Missing base image: {BASE_IMAGE}")
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.exists():
        sys.exit(f"Missing kernel: {kernel_path}")

    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        sys.exit(f"Kernel SHA mismatch: expected {EXPECTED_KERNEL_SHA256}, got {kernel_sha}")
    print(f"[T657 BUILDER] Verified canonical Image SHA: {kernel_sha}")

    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        sys.exit(f"Base image SHA mismatch: expected {EXPECTED_BASE_SHA256}, got {base_sha}")
    print(f"[T657 BUILDER] Verified T656 base image SHA: {base_sha}")

    if BASE_IMAGE.stat().st_nlink <= 1:
        sys.exit(f"Base image {BASE_IMAGE} must be hardlinked (st_nlink > 1), got {BASE_IMAGE.stat().st_nlink}")
    print(f"[T657 BUILDER] Base image hardlink verified: link count = {BASE_IMAGE.stat().st_nlink}")

    system_img_path = prepare_system_img()

    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    print(f"[T657 BUILDER] Streaming CPIO archive from {BASE_IMAGE}...")
    diag_patched = False
    system_patched = False

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
                print(f"[T657 BUILDER] Reading and updating t636-diag.sh ({filesize} bytes)...")
                base_diag_bytes = fin.read(filesize)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                new_diag_bytes = build_diag_script(base_diag_bytes.decode("latin1"))
                new_entry = format_cpio_entry("t636-diag.sh", new_diag_bytes)
                fout.write(new_entry)
                diag_patched = True
                print(f"[T657 BUILDER] Replaced t636-diag.sh ({filesize} -> {len(new_diag_bytes)} bytes)")

            elif name_str == "ramdisk-images/system.img":
                print(f"[T657 BUILDER] Replacing ramdisk-images/system.img ({filesize} bytes) with updated system.img...")
                # Skip old system.img data in input
                rem = filesize
                while rem > 0:
                    chunk = fin.read(min(rem, CHUNK))
                    rem -= len(chunk)
                pad_data = pad4(filesize)
                if pad_data:
                    fin.read(pad_data)

                # Write new system.img from scratch/system.img
                new_system_size = system_img_path.stat().st_size
                # Use same mode/ino from original hdr
                ino = int(hdr[6:14], 16)
                mode = int(hdr[14:22], 16)
                # Form new header for system.img
                name_b = name_bytes
                sys_hdr = (
                    f"070701"
                    f"{ino:08x}"
                    f"{mode:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{1:08x}"
                    f"{0:08x}"
                    f"{new_system_size:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{0:08x}"
                    f"{len(name_b):08x}"
                    f"{0:08x}"
                ).encode("ascii")
                fout.write(sys_hdr)
                fout.write(name_b)
                if pad_name:
                    fout.write(b"\0" * pad_name)

                with open(system_img_path, "rb") as fsys:
                    while True:
                        chunk = fsys.read(CHUNK)
                        if not chunk:
                            break
                        fout.write(chunk)
                pad_sys = pad4(new_system_size)
                if pad_sys:
                    fout.write(b"\0" * pad_sys)

                system_patched = True
                print(f"[T657 BUILDER] Replaced ramdisk-images/system.img with {new_system_size} bytes")

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
    if not system_patched:
        raise RuntimeError("Failed to patch ramdisk-images/system.img!")

    os.replace(PARTIAL_IMAGE, OUTPUT_IMAGE)
    output_sha = sha256_file(OUTPUT_IMAGE)
    output_bytes = os.path.getsize(OUTPUT_IMAGE)
    output_inode = OUTPUT_IMAGE.stat().st_ino
    elapsed = time.monotonic() - t0

    manifest = {
        "track": "T657",
        "description": "Fix WifiInjector NPE reading null Resources: supply authentic uncompressed com.android.wifi.apex in /system/apex so apexd activates it and PMS scans ServiceWifiResources.apk",
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

    print(f"[T657 BUILDER] Successfully built {OUTPUT_IMAGE} in {elapsed:.1f}s")
    print(f"[T657 BUILDER] SHA-256: {output_sha} ({output_bytes} bytes, inode {output_inode})")


if __name__ == "__main__":
    main()
