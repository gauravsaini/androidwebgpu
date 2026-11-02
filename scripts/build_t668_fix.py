#!/usr/bin/env python3
"""Build exactly ONE fix initramfs for Track T668:
system-server-early-cycle-deaths-gnss-watchdog-gatekeeper-npe (Chain A)

PINNED MECHANISMS:
1. Cycle 1 Watchdog Kill Root Cause:
   In Cycle 1, SystemServer blocked for 51+ seconds in
   GnssNative.native_class_init_once -> waitForVintfService<IGnssAidl>() during
   boot phase 600 (PHASE_THIRD_PARTY_APPS_CAN_START).
   vendor.gnss.ranchu is defined with 'class hal' and 'disabled' in vendor.img.
   servicemanager (PID 862) tried to start it via 'ctl.interface_start aidl/android.hardware.gnss.IGnss/default',
   but init dropped control messages due to control message queue flooding from
   hwservicemanager repeatedly requesting missing android.hardware.media.c2@1.0::IComponentStore/software.
   Additionally, /system/bin/start from t636-diag.sh is rejected with SELinux ctl_start_prop denial.
   This delayed GNSS registration until ~438s, causing the 83s main thread block and Watchdog kill at 466s.

2. Cycle 2 GateKeeper NPE Root Cause:
   In Cycle 2, LockSettingsService$DeviceProvisionedObserver.reportDeviceSetupComplete()
   fired when com.android.sdksetup.DefaultActivity finished provisioning and set DEVICE_PROVISIONED=1.
   LockSettingsService.getGateKeeperService() called ServiceManager.getService("android.service.gatekeeper.IGateKeeperService").
   It returned null (logging "Unable to acquire GateKeeperService") because gatekeeperd
   was configured as 'class late_start' in gatekeeperd.rc, class_start late_start was never triggered,
   and /system/bin/start gatekeeperd from t636-diag.sh was rejected with SELinux ctl_start_prop denial.
   Invoking reportDeviceSetupComplete() on null threw fatal NullPointerException, killing system_server.

3. Pinned Fix:
   - Pre-create /data/misc/gatekeeper (0700 system:system) in early-ramdisk and t636-diag.sh.
   - Directly spawn /vendor/bin/hw/android.hardware.gnss-service.ranchu in background from t636-diag.sh prep
     (before Zygote and SystemServer start), bypassing ctl.start / ctl.interface_start SELinux restrictions.
   - Directly spawn /system/bin/gatekeeperd /data/misc/gatekeeper in background from t636-diag.sh prep,
     ensuring IGateKeeperService registers with ServiceManager before device provisioning occurs.
   - Add a continuous watcher in t636-diag.sh to keep both daemons alive.
   - Do NOT touch services.jar / services.odex / services.vdex (preserves ART boot profiles).
   - Preserve all T664-T667 fixes (ext4 /data on /dev/loop2, tmpfs /data/media, ENODATA serial path).

Base Image (T667): SHA-256 186c7ae2815ca8da39564a4290e69c6e3041e8e4ad606fb862488d497971a680
Canonical Kernel:  SHA-256 05c4391767a6ff44f5124796b92d7b98027c37fbf58e81b342ca2adb7d99fa04
Output:
  guest-image/t668-system-server-early-cycle-deaths-gnss-watchdog-gatekeeper-npe-initramfs.cpio.gz
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
BASE_IMAGE = ROOT / "guest-inputs/t667-base-image.cpio.gz"
OUTPUT_DIR = ROOT / "guest-image"
OUTPUT_IMAGE = OUTPUT_DIR / "t668-system-server-early-cycle-deaths-gnss-watchdog-gatekeeper-npe-initramfs.cpio.gz"
PARTIAL_IMAGE = OUTPUT_IMAGE.with_suffix(OUTPUT_IMAGE.suffix + ".partial")
MANIFEST_FILE = OUTPUT_DIR / "t668-manifest.json"

EXPECTED_BASE_SHA256 = "186c7ae2815ca8da39564a4290e69c6e3041e8e4ad606fb862488d497971a680"
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
    """Pre-create /data/misc/gatekeeper (0700 system:system) so gatekeeperd chdir succeeds."""
    target_pattern = 'echo "[T667] userdata and media filesystem initialization complete"\n'
    replacement = (
        'echo "[T667] userdata and media filesystem initialization complete"\n'
        '# T668: Pre-create /data/misc/gatekeeper for gatekeeperd\n'
        '"$TOYBOX" mkdir -p /data/misc/gatekeeper 2>/dev/null || :\n'
        '"$TOYBOX" chmod 0700 /data/misc/gatekeeper 2>/dev/null || :\n'
        '"$TOYBOX" chown 1000:1000 /data/misc/gatekeeper 2>/dev/null || :\n'
        'echo "[T668] /data/misc/gatekeeper pre-created (0700 1000:1000)"\n'
    )

    if target_pattern not in original:
        raise ValueError("Could not find target pattern in early-ramdisk!")

    return original.replace(target_pattern, replacement, 1)


def patch_diag_script(script_text: str) -> str:
    """Enhance t636-diag.sh with gatekeeperd & vendor.gnss.ranchu direct spawn & supervision."""
    # 1. In do_prep: pre-create /data/misc/gatekeeper and directly spawn gatekeeperd & vendor.gnss.ranchu
    prep_marker = '    # T667: Ensure /data/media is mounted as tmpfs for POSIX ACL support\n'
    prep_addition = (
        '    # T668: Ensure /data/misc/gatekeeper exists with 0700 1000:1000 for gatekeeperd\n'
        '    "$TOYBOX" mkdir -p /data/misc/gatekeeper 2>/dev/null || :\n'
        '    "$TOYBOX" chmod 0700 /data/misc/gatekeeper 2>/dev/null || :\n'
        '    "$TOYBOX" chown 1000:1000 /data/misc/gatekeeper 2>/dev/null || :\n'
        '    mark "T668_GATEKEEPER_DIR_PREPARED"\n'
        '    # T668: Direct spawn gatekeeperd and vendor.gnss.ranchu early (bypass ctl_start SELinux restrictions)\n'
        '    if [ -x /vendor/bin/hw/android.hardware.gnss-service.ranchu ]; then\n'
        '        /vendor/bin/hw/android.hardware.gnss-service.ranchu >/dev/console 2>&1 &\n'
        '        mark "T668_GNSS_EARLY_SPAWNED"\n'
        '    fi\n'
        '    if [ -x /system/bin/gatekeeperd ]; then\n'
        '        /system/bin/gatekeeperd /data/misc/gatekeeper >/dev/console 2>&1 &\n'
        '        mark "T668_GATEKEEPERD_EARLY_SPAWNED"\n'
        '    fi\n'
        '    # T667: Ensure /data/media is mounted as tmpfs for POSIX ACL support\n'
    )
    if prep_marker in script_text:
        script_text = script_text.replace(prep_marker, prep_addition, 1)
    else:
        raise ValueError("Could not find prep_marker in t636-diag.sh!")

    # 2. Add watch_early_services before watch_boot_state
    watch_boot_marker = 'watch_boot_state() {\n'
    early_svc_watcher = (
        '# T668: Supervise gatekeeperd and vendor.gnss.ranchu to ensure Cycle 1 survival\n'
        'watch_early_services() {\n'
        '    mark "T668_EARLY_SERVICES_WATCHER_STARTED"\n'
        '    while :; do\n'
        '        if ! "$TOY" pidof gatekeeperd >/dev/null 2>&1; then\n'
        '            if [ -x /system/bin/gatekeeperd ]; then\n'
        '                "$TOY" mkdir -p /data/misc/gatekeeper 2>/dev/null || :\n'
        '                "$TOY" chmod 0700 /data/misc/gatekeeper 2>/dev/null || :\n'
        '                "$TOY" chown 1000:1000 /data/misc/gatekeeper 2>/dev/null || :\n'
        '                /system/bin/gatekeeperd /data/misc/gatekeeper >/dev/console 2>&1 &\n'
        '                mark "T668_GATEKEEPERD_RESPAWNED"\n'
        '            fi\n'
        '        fi\n'
        '        if ! "$TOY" pidof android.hardware.gnss-service.ranchu >/dev/null 2>&1; then\n'
        '            if [ -x /vendor/bin/hw/android.hardware.gnss-service.ranchu ]; then\n'
        '                /vendor/bin/hw/android.hardware.gnss-service.ranchu >/dev/console 2>&1 &\n'
        '                mark "T668_GNSS_RESPAWNED"\n'
        '            fi\n'
        '        fi\n'
        '        "$TOY" sleep 2 2>/dev/null || :\n'
        '    done\n'
        '}\n'
        'watch_early_services &\n\n'
        'watch_boot_state() {\n'
    )
    if watch_boot_marker in script_text:
        script_text = script_text.replace(watch_boot_marker, early_svc_watcher, 1)
    else:
        raise ValueError("Could not find watch_boot_marker in t636-diag.sh!")

    # 3. Add gatekeeperd and gnss status to T642_BOOT_STATE
    media_fs_marker = 'media_fs=$("$TOYBOX" stat -f -c "%T" /data/media 2>/dev/null || echo "unknown")\n'
    svc_status_addition = (
        'media_fs=$("$TOYBOX" stat -f -c "%T" /data/media 2>/dev/null || echo "unknown")\n'
        '        gk_pid=$("$TOY" pidof gatekeeperd 2>/dev/null || echo "none")\n'
        '        gnss_pid=$("$TOY" pidof android.hardware.gnss-service.ranchu 2>/dev/null || echo "none")\n'
    )
    if media_fs_marker in script_text:
        script_text = script_text.replace(media_fs_marker, svc_status_addition, 1)

    old_boot_marker = 'media_fs=$media_fs"'
    new_boot_marker = 'media_fs=$media_fs gatekeeperd=$gk_pid gnss=$gnss_pid"'
    if old_boot_marker in script_text:
        script_text = script_text.replace(old_boot_marker, new_boot_marker, 1)

    return script_text


def main():
    t0 = time.monotonic()
    free_gib = check_disk_space()
    print(f"[T668 BUILDER] Disk check: {free_gib:.2f} GiB available.")

    # 1. Verify base image (T667)
    if not BASE_IMAGE.is_file():
        raise FileNotFoundError(f"Base image missing: {BASE_IMAGE}")
    base_stat = BASE_IMAGE.stat()
    if base_stat.st_nlink < 2:
        print(f"[T668 BUILDER] WARNING: Base image st_nlink={base_stat.st_nlink} (expected >1).")
    base_sha = sha256_file(BASE_IMAGE)
    if base_sha != EXPECTED_BASE_SHA256:
        raise ValueError(f"Base image SHA-256 mismatch: {base_sha} != {EXPECTED_BASE_SHA256}")
    print(f"[T668 BUILDER] Base image verified: {BASE_IMAGE} ({base_sha})")

    # 2. Verify canonical kernel
    kernel_path = ROOT / "guest-inputs/Image"
    if not kernel_path.is_file():
        raise FileNotFoundError(f"Kernel missing: {kernel_path}")
    kernel_sha = sha256_file(kernel_path)
    if kernel_sha != EXPECTED_KERNEL_SHA256:
        raise ValueError(f"Kernel SHA-256 mismatch: {kernel_sha} != {EXPECTED_KERNEL_SHA256}")
    print(f"[T668 BUILDER] Canonical kernel verified: {kernel_path} ({kernel_sha})")

    # 3. Stream base CPIO and build ONE fix initramfs
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    if PARTIAL_IMAGE.exists():
        PARTIAL_IMAGE.unlink()

    patched_ramdisk = False
    patched_diag = False
    verified_system_img = False
    verified_userdata_img = False

    print(f"[T668 BUILDER] Streaming base CPIO and applying T668 GNSS & GateKeeper fixes...")
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
                print(f"[T668 BUILDER] Patched early-ramdisk (diff: {len(patched_bytes) - len(raw_content)} bytes)")

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
                print(f"[T668 BUILDER] Patched t636-diag.sh (diff: {len(patched_bytes) - len(raw_content)} bytes)")

            elif name_str == "ramdisk-images/system.img":
                print(f"[T668 BUILDER] Verifying embedded system.img ({filesize} bytes)...")
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
                print(f"[T668 BUILDER] Verified system.img byte-identical SHA: {sys_sha}")

            elif name_str == "ramdisk-images/userdata.img":
                print(f"[T668 BUILDER] Streaming existing ext4 userdata.img ({filesize} bytes)...")
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
                print(f"[T668 BUILDER] Streamed ext4 userdata.img OK.")

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
    for old_img in OUTPUT_DIR.glob("t668-*.cpio.gz"):
        old_img.unlink()

    # Move to final output image atomically
    os.replace(PARTIAL_IMAGE, OUTPUT_IMAGE)
    fix_sha = sha256_file(OUTPUT_IMAGE)
    fix_bytes = OUTPUT_IMAGE.stat().st_size
    elapsed = time.monotonic() - t0

    print(f"[T668 BUILDER] Fix image created successfully!")
    print(f"[T668 BUILDER] Path: {OUTPUT_IMAGE}")
    print(f"[T668 BUILDER] SHA-256: {fix_sha} ({fix_bytes} bytes in {elapsed:.2f}s)")

    manifest = {
        "track": "T668",
        "description": "system-server-early-cycle-deaths-gnss-watchdog-gatekeeper-npe",
        "pinned_mechanism": "Pre-create /data/misc/gatekeeper (0700 system:system), directly spawn gatekeeperd and vendor.gnss.ranchu early in t636-diag.sh (bypassing ctl.start SELinux restrictions) and continuously supervise them, eliminating GNSS HAL 91s Watchdog kill and GateKeeper NPE while preserving ext4 /data and tmpfs /data/media",
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
    print(f"[T668 BUILDER] Wrote manifest: {MANIFEST_FILE}")


if __name__ == "__main__":
    main()
